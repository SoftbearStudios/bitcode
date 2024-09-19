use super::convert::ConvertFrom;
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
