use std::collections::BTreeMap;

use motya_config::common_types::value::Value;
use pingora::{Error, Result};

pub trait FromValue: Sized {
    fn from_value(v: Value) -> Result<Self>;
}

macro_rules! impl_from_value {
    ($($t:ty => $variant:ident),*) => {
        $(
            impl FromValue for $t {
                fn from_value(v: Value) -> Result<Self> {
                    match v {
                        Value::$variant(x) => Ok(x),
                        _ => Err(Error::new_str(concat!("Expected ", stringify!($variant)))),
                    }
                }
            }
        )*
    };
}

impl_from_value!(
    String => String,
    i128 => Integer,
    bool => Bool,
    f64 => Float
);

pub trait ConfigMapExt {
    fn take_val<T: FromValue>(&mut self, key: &str) -> Result<Option<T>>;
}

impl ConfigMapExt for BTreeMap<String, Value> {
    fn take_val<T: FromValue>(&mut self, key: &str) -> Result<Option<T>> {
        self.remove(key)
            .map(|v| {
                T::from_value(v).map_err(|e| {
                    tracing::error!("Field '{key}' has invalid type: {e:?}");
                    e
                })
            })
            .transpose()
    }
}

pub trait RequiredValueExt<T> {
    fn required(self, key: &str) -> Result<T>;
}

impl<T> RequiredValueExt<T> for Option<T> {
    fn required(self, key: &str) -> Result<T> {
        self.ok_or_else(|| {
            tracing::error!("Missing required configuration key: '{key}'");
            Error::new_str("Missing configuration field")
        })
    }
}
