use std::fmt::Display;

use kdl::KdlValue;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum Value {
    String(String),
    Integer(i128),
    Float(f64),
    Bool(bool),
    Null,
}

impl From<KdlValue> for Value {
    fn from(value: KdlValue) -> Self {
        match value {
            KdlValue::String(s) => Self::String(s),
            KdlValue::Integer(i) => Self::Integer(i),
            KdlValue::Float(f) => Self::Float(f),
            KdlValue::Bool(b) => Self::Bool(b),
            KdlValue::Null => Self::Null,
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Self::String(s) => s.to_string(),
            Self::Integer(i) => i.to_string(),
            Self::Float(f) => f.to_string(),
            Self::Bool(b) => b.to_string(),
            Self::Null => "".to_string(),
        };

        f.write_str(&str)
    }
}
