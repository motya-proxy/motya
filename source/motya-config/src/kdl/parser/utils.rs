use std::{any::type_name, fmt::Display, str::FromStr};

use kdl::KdlValue;
use miette::Result;

use crate::kdl::parser::typed_value::TypedValue;

#[allow(clippy::wrong_self_convention)]
pub trait OptionTypedValueExt {
    fn as_str(self) -> Result<Option<String>>;
    fn as_bool(self) -> Result<Option<bool>>;
    fn as_usize(self) -> Result<Option<usize>>;
    fn parse_as<T>(self) -> Result<Option<T>>
    where
        T: FromStr,
        T::Err: Display;
}

impl OptionTypedValueExt for Option<TypedValue> {
    fn as_str(self) -> Result<Option<String>> {
        match self {
            Some(v) => Ok(Some(v.as_str()?)),
            None => Ok(None),
        }
    }

    fn as_bool(self) -> Result<Option<bool>> {
        match self {
            Some(v) => Ok(Some(v.as_bool()?)),
            None => Ok(None),
        }
    }

    fn as_usize(self) -> Result<Option<usize>> {
        match self {
            Some(v) => Ok(Some(v.as_usize()?)),
            None => Ok(None),
        }
    }
    fn parse_as<T>(self) -> Result<Option<T>>
    where
        T: FromStr,
        T::Err: Display,
    {
        match self {
            Some(v) => Ok(Some(v.parse_as::<T>()?)),
            None => Ok(None),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveType {
    String,
    Integer,
    Float,
    Bool,
    Null,
}

impl std::fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrimitiveType::String => write!(f, "String"),
            PrimitiveType::Integer => write!(f, "Integer"),
            PrimitiveType::Float => write!(f, "Float"),
            PrimitiveType::Bool => write!(f, "Boolean"),
            PrimitiveType::Null => write!(f, "Null"),
        }
    }
}

pub fn get_simple_type_name<T>() -> &'static str {
    type_name::<T>()
        .rsplit("::")
        .next()
        .unwrap_or("UnknownType")
}

pub fn get_kdl_type_name(val: &KdlValue) -> &'static str {
    match val {
        KdlValue::String(_) => "String",
        KdlValue::Integer(_) => "Integer",
        KdlValue::Float(_) => "Float",
        KdlValue::Bool(_) => "Boolean",
        KdlValue::Null => "Null",
    }
}

pub mod macros_helpers {

    use miette::{NamedSource, SourceSpan};

    use crate::{
        common_types::error::{ConfigError, ParseError},
        kdl::parser::{ctx::ParseContext, typed_value::TypedValue},
    };

    #[allow(unused)]
    pub fn to_parse_error(
        e: miette::Report,
        fallback_span: SourceSpan,
        source: NamedSource<String>,
    ) -> ParseError {
        let help = e.help().map(|h| h.to_string());

        let label = e
            .labels()
            .and_then(|mut iter| iter.next())
            .map(|l| *l.inner())
            .unwrap_or(fallback_span);

        ParseError {
            message: e.to_string(),
            label: Some(label),
            help,
            src: source,
        }
    }

    #[allow(unused)]
    pub fn push_report(
        errors: &mut Vec<ParseError>,
        e: miette::Report,
        fallback_span: SourceSpan,
        source: NamedSource<String>,
    ) {
        errors.push(to_parse_error(e, fallback_span, source));
    }

    #[allow(unused)]
    pub fn push_custom(
        errors: &mut Vec<ParseError>,
        msg: impl Into<String>,
        help: Option<String>,
        span: SourceSpan,
        source: NamedSource<String>,
    ) {
        errors.push(ParseError {
            message: msg.into(),
            label: Some(span),
            help,
            src: source,
        });
    }

    #[allow(unused)]
    pub fn parse_input_value<T>(
        fetch_result: miette::Result<Option<TypedValue>>,
        parser_logic: impl FnOnce(TypedValue) -> miette::Result<T>,
        default_val: Option<T>,
        is_required: bool,
        desc: &str,
        ctx: &ParseContext,
        errors: &mut Vec<ParseError>,
    ) -> Option<T> {
        match fetch_result {
            Ok(Some(val_ref)) => match parser_logic(val_ref.clone()) {
                Ok(parsed) => Some(parsed),
                Err(e) => {
                    push_report(errors, e, val_ref.span(), ctx.source().clone());
                    None
                }
            },

            Ok(None) => {
                if let Some(def) = default_val {
                    Some(def)
                } else {
                    if is_required {
                        push_custom(
                            errors,
                            format!("Missing required {}", desc),
                            None,
                            ctx.current_span(),
                            ctx.source().clone(),
                        );
                    }
                    None
                }
            }

            Err(e) => {
                if let Some(def) = default_val {
                    Some(def)
                } else {
                    if is_required {
                        push_report(errors, e, ctx.current_span(), ctx.source().clone());
                    }
                    None
                }
            }
        }
    }

    #[allow(unused)]
    pub fn merge_child_errors(
        parent_errors: &mut Vec<ParseError>,
        child_result: Result<(), ConfigError>,
    ) {
        if let Err(mut cfg_err) = child_result {
            parent_errors.append(&mut cfg_err.errors);
        }
    }
}
