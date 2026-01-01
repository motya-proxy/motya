use std::fmt;

use kdl::KdlError;
use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

use crate::kdl::parser::ctx::ParseContext;

#[derive(Debug, Error, Diagnostic, Clone)]
#[error("{message}")]
pub struct ParseError {
    pub message: String,

    #[label("here")]
    pub label: Option<SourceSpan>,

    #[help]
    pub help: Option<String>,

    #[source_code]
    pub src: NamedSource<String>,
}

#[derive(Error, Diagnostic, Default, Clone)]
#[error("Configuration parsing failed with {count} errors")]
pub struct ConfigError {
    pub count: usize,

    #[related]
    pub errors: Vec<ParseError>,
}

impl fmt::Debug for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.errors.is_empty() {
            return f.write_str("ConfigError: No errors");
        }

        fmt::Debug::fmt(&miette::Report::new(self.clone()), f)
    }
}

impl ConfigError {
    pub fn push(&mut self, err: ParseError) {
        self.errors.push(err);
        self.count = self.errors.len();
    }

    pub fn push_report(&mut self, report: miette::Report, ctx: &ParseContext) {
        self.push(ParseError::from_report(report, ctx));
    }

    pub fn merge(&mut self, other: ConfigError) {
        self.errors.extend(other.errors);
        self.count = self.errors.len();
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn from_list(errors: Vec<ParseError>) -> Self {
        Self {
            count: errors.len(),
            errors,
        }
    }
}

impl ParseError {
    pub fn new(
        msg: impl Into<String>,
        label: Option<SourceSpan>,
        help: Option<String>,
        src: NamedSource<String>,
    ) -> Self {
        Self {
            message: msg.into(),
            label,
            help,
            src,
        }
    }

    pub fn from_report(e: miette::Report, ctx: &ParseContext) -> Self {
        let help = e.help().map(|h| h.to_string());

        let label = e
            .labels()
            .and_then(|mut iter| iter.next())
            .map(|l| *l.inner())
            .unwrap_or_else(|| ctx.current_span());

        Self {
            message: e.to_string(),
            label: Some(label),
            help,
            src: ctx.source().clone(),
        }
    }

    pub fn from_kdl_error(e: KdlError, src: NamedSource<String>) -> Vec<Self> {
        e.diagnostics
            .into_iter()
            .map(|d| Self {
                message: d.message.unwrap_or_else(|| "Syntax error".to_string()),
                label: Some(d.span),
                help: d.help,
                src: src.clone(),
            })
            .collect()
    }
}
