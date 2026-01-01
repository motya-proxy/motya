use crate::{common_types::key_template::KeyTemplate, kdl::schema::definitions::ValueKind};

pub trait KdlValueInfo {
    fn value_kind() -> ValueKind;
}

macro_rules! impl_value_info {
    ($kind:ident => $($ty:ty),+ $(,)?) => {
        $(
            impl KdlValueInfo for $ty {
                fn value_kind() -> ValueKind {
                    ValueKind::$kind
                }
            }
        )+
    };
}

macro_rules! impl_typed_value_info {
    ($label:expr => $($ty:ty),+ $(,)?) => {
        $(
            impl KdlValueInfo for $ty {
                fn value_kind() -> ValueKind {
                    ValueKind::TypedString($label.to_string())
                }
            }
        )+
    };
}

impl_value_info!(String => String, &str);
impl_value_info!(Bool   => bool);
impl_value_info!(Int    =>
    i8, i16, i32, i64, i128, isize,
    u8, u16, u32, u64, u128, usize,
    std::num::NonZeroUsize, std::num::NonZeroU32, std::num::NonZeroI32
);
impl_value_info!(Float  => f32, f64);

impl_typed_value_info!("fqdn" => fqdn::FQDN);
impl_typed_value_info!("path" => std::path::PathBuf);
impl_typed_value_info!("socket-addr" => std::net::SocketAddr, std::net::IpAddr);
impl_typed_value_info!("uri" => http::Uri);
impl_typed_value_info!("path-query" => http::uri::PathAndQuery);
impl_typed_value_info!("duration" => humantime::Duration, std::time::Duration);
impl_typed_value_info!("key-template" => KeyTemplate);
