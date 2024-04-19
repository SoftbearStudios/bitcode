use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::derive::{Decode, Encode};
use crate::length::{LengthDecoder, LengthEncoder};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::num::NonZeroUsize;

#[cfg(feature = "std")]
use core::hash::{BuildHasher, Hash};
#[cfg(feature = "std")]
use std::collections::HashMap;

pub struct MapEncoder<K: Encode, V: Encode> {
    lengths: LengthEncoder,
    keys: K::Encoder,
    values: V::Encoder,
}

// Can't derive since it would bound K + V: Default.
impl<K: Encode, V: Encode> Default for MapEncoder<K, V> {
    fn default() -> Self {
        Self {
            lengths: Default::default(),
            keys: Default::default(),
            values: Default::default(),
        }
    }
}

impl<K: Encode, V: Encode> Buffer for MapEncoder<K, V> {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        self.lengths.collect_into(out);
        self.keys.collect_into(out);
        self.values.collect_into(out);
    }

    fn reserve(&mut self, additional: NonZeroUsize) {
        self.lengths.reserve(additional);
        // We don't know the lengths of the maps, so we can't reserve more.
    }
}

pub struct MapDecoder<'a, K: Decode<'a>, V: Decode<'a>> {
    lengths: LengthDecoder<'a>,
    keys: K::Decoder,
    values: V::Decoder,
}

// Can't derive since it would bound K + V: Default.
impl<'a, K: Decode<'a>, V: Decode<'a>> Default for MapDecoder<'a, K, V> {
    fn default() -> Self {
        Self {
            lengths: Default::default(),
            keys: Default::default(),
            values: Default::default(),
        }
    }
}

impl<'a, K: Decode<'a>, V: Decode<'a>> View<'a> for MapDecoder<'a, K, V> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        self.lengths.populate(input, length)?;
        self.keys.populate(input, self.lengths.length())?;
        self.values.populate(input, self.lengths.length())
    }
}

macro_rules! encode_body {
    ($t:ty) => {
        #[inline(always)]
        fn encode(&mut self, map: &$t) {
            let n = map.len();
            self.lengths.encode(&n);

            if let Some(n) = NonZeroUsize::new(n) {
                self.keys.reserve(n);
                self.values.reserve(n);
                for (k, v) in map {
                    self.keys.encode(k);
                    self.values.encode(v);
                }
            }
        }
    };
}
macro_rules! decode_body {
    ($t:ty) => {
        #[inline(always)]
        fn decode(&mut self) -> $t {
            // BTreeMap::from_iter is faster than BTreeMap::insert since it can add the items in
            // bulk once it ensures they are sorted. They are about equivalent for HashMap.
            (0..self.lengths.decode())
                .map(|_| (self.keys.decode(), self.values.decode()))
                .collect()
        }
    };
}

impl<K: Encode, V: Encode> Encoder<BTreeMap<K, V>> for MapEncoder<K, V> {
    encode_body!(BTreeMap<K, V>);
}
impl<'a, K: Decode<'a> + Ord, V: Decode<'a>> Decoder<'a, BTreeMap<K, V>> for MapDecoder<'a, K, V> {
    decode_body!(BTreeMap<K, V>);
}

#[cfg(feature = "std")]
impl<K: Encode, V: Encode, S> Encoder<HashMap<K, V, S>> for MapEncoder<K, V> {
    encode_body!(HashMap<K, V, S>);
}
#[cfg(feature = "std")]
impl<'a, K: Decode<'a> + Eq + Hash, V: Decode<'a>, S: BuildHasher + Default>
    Decoder<'a, HashMap<K, V, S>> for MapDecoder<'a, K, V>
{
    decode_body!(HashMap<K, V, S>);
}

#[cfg(test)]
mod test {
    use alloc::collections::BTreeMap;
    #[cfg(feature = "std")]
    use std::collections::HashMap;

    fn bench_data<T: FromIterator<(u8, u8)>>() -> T {
        (0..=255).map(|k| (k, 0)).collect()
    }
    #[cfg(feature = "std")]
    crate::bench_encode_decode!(btree_map: BTreeMap<_, _>, hash_map: HashMap<_, _>);
    #[cfg(not(feature = "std"))]
    crate::bench_encode_decode!(btree_map: BTreeMap<_, _>);
}
