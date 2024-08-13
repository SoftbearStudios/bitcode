use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::derive::{Decode, Encode};
use core::net::SocketAddrV4;
use core::num::NonZeroUsize;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

macro_rules! ipvx_addr {
    ($addr: ident) => {
        impl ConvertFrom<&$addr> for [u8; std::mem::size_of::<$addr>()] {
            fn convert_from(ip: &$addr) -> Self {
                ip.octets()
            }
        }

        impl ConvertFrom<[u8; std::mem::size_of::<$addr>()]> for $addr {
            fn convert_from(octets: [u8; std::mem::size_of::<$addr>()]) -> Self {
                Self::from(octets)
            }
        }
    };
}

ipvx_addr!(Ipv4Addr);
ipvx_addr!(Ipv6Addr);

impl ConvertFrom<&IpAddr> for std::result::Result<Ipv4Addr, Ipv6Addr> {
    fn convert_from(value: &IpAddr) -> Self {
        match value {
            IpAddr::V4(v4) => Ok(*v4),
            IpAddr::V6(v6) => Err(*v6),
        }
    }
}

impl ConvertFrom<std::result::Result<Ipv4Addr, Ipv6Addr>> for IpAddr {
    fn convert_from(value: std::result::Result<Ipv4Addr, Ipv6Addr>) -> Self {
        match value {
            Ok(v4) => Self::V4(v4),
            Err(v6) => Self::V6(v6),
        }
    }
}

impl ConvertFrom<&SocketAddrV4> for (Ipv4Addr, u16) {
    fn convert_from(value: &SocketAddrV4) -> Self {
        (*value.ip(), value.port())
    }
}

impl ConvertFrom<(Ipv4Addr, u16)> for SocketAddrV4 {
    fn convert_from((ip, port): (Ipv4Addr, u16)) -> Self {
        Self::new(ip, port)
    }
}

// Like [`From`] but we can implement it ourselves.
pub(crate) trait ConvertFrom<T>: Sized {
    fn convert_from(value: T) -> Self;
}

pub struct ConvertIntoEncoder<T: Encode>(T::Encoder);

// Can't derive since it would bound T: Default.
impl<T: Encode> Default for ConvertIntoEncoder<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<D, T: Encode + for<'a> ConvertFrom<&'a D>> Encoder<D> for ConvertIntoEncoder<T> {
    #[inline(always)]
    fn encode(&mut self, t: &D) {
        self.0.encode(&T::convert_from(t));
    }
}

impl<T: Encode> Buffer for ConvertIntoEncoder<T> {
    fn collect_into(&mut self, out: &mut Vec<u8>) {
        self.0.collect_into(out);
    }
    fn reserve(&mut self, additional: NonZeroUsize) {
        self.0.reserve(additional);
    }
}

/// Decodes a `T` and then converts it with [`ConvertFrom`].
pub struct ConvertFromDecoder<'a, T: Decode<'a>>(T::Decoder);

// Can't derive since it would bound T: Default.
impl<'a, T: Decode<'a>> Default for ConvertFromDecoder<'a, T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<'a, T: Decode<'a>> View<'a> for ConvertFromDecoder<'a, T> {
    fn populate(&mut self, input: &mut &'a [u8], length: usize) -> Result<()> {
        self.0.populate(input, length)
    }
}

impl<'a, F: ConvertFrom<T>, T: Decode<'a>> Decoder<'a, F> for ConvertFromDecoder<'a, T> {
    #[inline(always)]
    fn decode(&mut self) -> F {
        F::convert_from(self.0.decode())
    }
}
