use crate::coder::{Buffer, Decoder, Encoder, Result, View};
use crate::derive::{Decode, Encode};
use core::num::NonZeroUsize;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

macro_rules! ipvx_addr {
    ($addr: ident, $repr: ident) => {
        impl ConvertFrom<&$addr> for $repr {
            fn convert_from(ip: &$addr) -> Self {
                (*ip).into()
            }
        }

        impl ConvertFrom<$repr> for $addr {
            fn convert_from(bits: $repr) -> Self {
                Self::from(bits)
            }
        }
    };
}

ipvx_addr!(Ipv4Addr, u32);
ipvx_addr!(Ipv6Addr, u128);

pub(crate) type IpAddrConversion = std::result::Result<Ipv4Addr, Ipv6Addr>;

impl ConvertFrom<&IpAddr> for IpAddrConversion {
    fn convert_from(value: &IpAddr) -> Self {
        match value {
            IpAddr::V4(v4) => Ok(*v4),
            IpAddr::V6(v6) => Err(*v6),
        }
    }
}

impl ConvertFrom<IpAddrConversion> for IpAddr {
    fn convert_from(value: IpAddrConversion) -> Self {
        match value {
            Ok(v4) => Self::V4(v4),
            Err(v6) => Self::V6(v6),
        }
    }
}

pub(crate) type SocketAddrV4Conversion = (Ipv4Addr, u16);

impl ConvertFrom<&SocketAddrV4> for SocketAddrV4Conversion {
    fn convert_from(value: &SocketAddrV4) -> Self {
        (*value.ip(), value.port())
    }
}

impl ConvertFrom<SocketAddrV4Conversion> for SocketAddrV4 {
    fn convert_from((ip, port): SocketAddrV4Conversion) -> Self {
        Self::new(ip, port)
    }
}

pub(crate) type SocketAddrV6Conversion = (Ipv6Addr, u16, u32, u32);

impl ConvertFrom<&SocketAddrV6> for SocketAddrV6Conversion {
    fn convert_from(value: &SocketAddrV6) -> Self {
        (
            *value.ip(),
            value.port(),
            value.flowinfo(),
            value.scope_id(),
        )
    }
}

impl ConvertFrom<SocketAddrV6Conversion> for SocketAddrV6 {
    fn convert_from((ip, port, flowinfo, scope_id): SocketAddrV6Conversion) -> Self {
        Self::new(ip, port, flowinfo, scope_id)
    }
}

pub(crate) type SocketAddrConversion = std::result::Result<SocketAddrV4, SocketAddrV6>;

impl ConvertFrom<&SocketAddr> for SocketAddrConversion {
    fn convert_from(value: &SocketAddr) -> Self {
        match value {
            SocketAddr::V4(v4) => Ok(*v4),
            SocketAddr::V6(v6) => Err(*v6),
        }
    }
}

impl ConvertFrom<SocketAddrConversion> for SocketAddr {
    fn convert_from(value: SocketAddrConversion) -> Self {
        match value {
            Ok(v4) => Self::V4(v4),
            Err(v6) => Self::V6(v6),
        }
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
