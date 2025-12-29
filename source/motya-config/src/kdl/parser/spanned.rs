use crate::kdl::parser::ctx::ParseContext;
use miette::SourceSpan;
use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
};

#[derive(Clone)]
pub struct Spanned<T> {
    pub data: T,
    pub ctx: ParseContext,
}

impl<T> Spanned<T> {
    pub fn new(data: T, ctx: ParseContext) -> Self {
        Self { data, ctx }
    }

    pub fn into_inner(self) -> T {
        self.data
    }
    pub fn span(&self) -> SourceSpan {
        self.ctx.current_span()
    }

    pub fn err_node(&self, msg: impl Into<String>) -> miette::Error {
        self.ctx.error(msg)
    }

    pub fn err_prop(&self, key: &str, msg: impl Into<String>) -> miette::Error {
        let span = self
            .ctx
            .prop_span(key)
            .unwrap_or_else(|| self.ctx.current_span());
        self.ctx.error_with_span(msg, span)
    }

    pub fn err_arg(&self, index: usize, msg: impl Into<String>) -> miette::Error {
        let span = self
            .ctx
            .arg_span(index)
            .unwrap_or_else(|| self.ctx.current_span());
        self.ctx.error_with_span(msg, span)
    }
}

impl<T> Deref for Spanned<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> DerefMut for Spanned<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl<T: Debug> Debug for Spanned<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Spanned").field(&self.data).finish()
    }
}
