use crate::{common_types::error::ConfigError, kdl::parser::ctx::ParseContext};

pub trait KdlParsable<S>: Sized {
    fn parse_node(ctx: &ParseContext, state: &S) -> Result<Self, ConfigError>;
}

impl<S, T: KdlParsable<S>> KdlParsable<S> for Box<T> {
    fn parse_node(ctx: &ParseContext, state: &S) -> Result<Self, ConfigError> {
        T::parse_node(ctx, state).map(Box::new)
    }
}
