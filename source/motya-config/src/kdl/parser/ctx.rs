use fqdn::FQDN;
use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use miette::{Result, SourceSpan};
use std::{
    borrow::Cow,
    collections::HashMap,
    fmt::Debug,
    ops::{Range, RangeFrom, RangeFull, RangeTo},
    str::FromStr,
    vec::IntoIter,
};

use crate::{
    common_types::bad::Bad, kdl::parser::typed_value::TypedValue, var_registry::VarRegistry,
};

#[derive(Debug, Clone)]
pub struct ParseContext<'a> {
    doc: &'a KdlDocument,
    source_name: &'a str,
    current: Current<'a>,
    pub(crate) registry: Option<&'a VarRegistry>,
}

#[derive(Debug, Clone)]
pub enum Current<'a> {
    Document(&'a KdlDocument),
    Node(&'a KdlNode, &'a [KdlEntry]),
}

impl<'a> ParseContext<'a> {
    pub fn new_with_registy(
        doc: &'a KdlDocument,
        current: Current<'a>,
        source_name: &'a str,
        registry: &'a VarRegistry,
    ) -> Self {
        Self {
            current,
            doc,
            registry: Some(registry),
            source_name,
        }
    }
    /// Creates a new parsing context from a document and a specific location (node or root).
    pub fn new(doc: &'a KdlDocument, current: Current<'a>, source_name: &'a str) -> Self {
        Self {
            doc,
            source_name,
            current,
            registry: None,
        }
    }

    /// Creates a new context for the child block's content.
    /// Returns an error if the block does not exist.
    pub fn enter_block(&self) -> Result<ParseContext<'a>> {
        match &self.current {
            Current::Node(node, _) => {
                let children = node.children().ok_or_else(|| {
                    self.error("Expected a children block { ... }, but none found")
                })?;

                Ok(ParseContext::new(
                    self.doc,
                    Current::Document(children),
                    self.source_name,
                ))
            }
            Current::Document(_) => {
                Err(self.error("Cannot enter block: current context is already a document root"))
            }
        }
    }

    pub fn error_with_span(&self, msg: impl Into<String>, span: SourceSpan) -> miette::Error {
        Bad::docspan(msg.into(), self.doc, &span, self.source_name).into()
    }

    /// Generates a styled error message pointing to the current span in the source.
    pub fn error(&self, msg: impl Into<String>) -> miette::Error {
        Bad::docspan(msg.into(), self.doc, &self.current_span(), self.source_name).into()
    }

    /// Returns the source span of the current element (Node or Document).
    pub fn current_span(&self) -> SourceSpan {
        match &self.current {
            Current::Document(doc) => doc.span(),
            Current::Node(node, _) => node.span(),
        }
    }

    /// Returns the name of the current node (e.g., "server" in `server "localhost"`).
    /// Returns an error if the context is the Document root.
    pub fn name(&self) -> Result<&str> {
        match &self.current {
            Current::Document(_) => Err(self.error("Expected node, but current is a document")),
            Current::Node(node, _) => Ok(node.name().value()),
        }
    }

    pub fn nodes_iter<'b>(&self) -> Result<IntoIter<ParseContext<'_>>>
    where
        'a: 'b,
    {
        Ok(self.nodes()?.into_iter())
    }
    /// Iterates over child nodes, returning a new `ParseContext` for each child.
    pub fn nodes<'b>(&self) -> Result<Vec<ParseContext<'b>>>
    where
        'a: 'b,
    {
        let doc = match self.current {
            Current::Document(d) => d,
            Current::Node(n, _) => n
                .children()
                .ok_or_else(|| self.error("Expected children block"))?,
        };

        let nodes = doc
            .nodes()
            .iter()
            .map(|node| (node, node.name().value(), node.entries()));

        Ok(nodes
            .map(|(node, _name, args)| ParseContext {
                current: Current::Node(node, args),
                ..self.clone()
            })
            .collect())
    }

    /// Asserts that the current node has a specific name.
    pub fn expect_name(&self, expected: &str) -> Result<()> {
        match &self.current {
            Current::Document(_) => Err(self.error(format!(
                "Expected node '{expected}', but current is a document"
            ))),
            Current::Node(node, _) => {
                if node.name().value() == expected {
                    Ok(())
                } else {
                    Err(self.error(format!(
                        "Expected '{expected}', found '{}'",
                        node.name().value()
                    )))
                }
            }
        }
    }

    /// Returns the raw slice of arguments/entries for the current node.
    pub fn args(&self) -> Result<&[KdlEntry]> {
        match &self.current {
            Current::Document(_) => Err(self.error("Expected node, but current is a document")),
            Current::Node(_, args) => Ok(args),
        }
    }

    /// Retrieves a required named property as a String.
    pub fn string_arg(&self, name: &str) -> Result<String> {
        let entry = self
            .opt_prop(name)?
            .ok_or_else(|| self.error(format!("Missing required argument: '{name}'")))?;

        Ok(entry.as_str()?.to_string())
    }

    /// Retrieves a required named property and parses it as an FQDN.
    pub fn parse_fqdn_arg(&self, name: &str) -> Result<FQDN> {
        let str = self.string_arg(name)?;
        FQDN::from_str(&str).map_err(|err| self.error(format!("Invalid FQDN '{str}': {err}")))
    }

    /// Checks if the current node has an attached children block (e.g., `{ ... }`).
    pub fn has_children_block(&self) -> Result<bool> {
        match &self.current {
            Current::Node(n, _) => Ok(n.children().is_some()),
            Current::Document(_) => Err(self.error("Expected node, but current is a document")),
        }
    }

    /// Retrieves child nodes but returns an error if the block is empty.
    pub fn req_nodes(&self) -> Result<Vec<ParseContext<'_>>> {
        let nodes = self.nodes()?;

        if nodes.is_empty() {
            return Err(self.error(format!(
                "Block '{name}' cannot be empty",
                name = self.name()?
            )));
        }

        Ok(nodes)
    }

    pub fn props<'b, const N: usize>(
        &'a self,
        keys: [&str; N],
    ) -> Result<[Option<TypedValue<'b>>; N]>
    where
        'a: 'b,
    {
        let mut result = [None; N];

        for (i, key) in keys.iter().enumerate() {
            result[i] = self.opt_prop(key)?;
        }

        Ok(result)
    }
}
pub trait SliceRange<T: ?Sized> {
    fn slice<'a>(&self, slice: &'a T) -> Option<&'a T>;
}

impl<T> SliceRange<[T]> for Range<usize> {
    fn slice<'a>(&self, slice: &'a [T]) -> Option<&'a [T]> {
        slice.get(self.start..self.end)
    }
}

impl<T> SliceRange<[T]> for RangeFrom<usize> {
    fn slice<'a>(&self, slice: &'a [T]) -> Option<&'a [T]> {
        slice.get(self.start..)
    }
}

impl<T> SliceRange<[T]> for RangeTo<usize> {
    fn slice<'a>(&self, slice: &'a [T]) -> Option<&'a [T]> {
        slice.get(..self.end)
    }
}

impl<T> SliceRange<[T]> for RangeFull {
    fn slice<'a>(&self, slice: &'a [T]) -> Option<&'a [T]> {
        Some(slice)
    }
}

pub trait HashMapValidationExt {
    fn ensure_only_keys(
        self,
        allowed: &[&str],
        doc: &KdlDocument,
        span: &SourceSpan,
        source_name: &str,
    ) -> miette::Result<Self>
    where
        Self: Sized;
}

impl<V> HashMapValidationExt for HashMap<&str, V> {
    fn ensure_only_keys(
        self,
        allowed: &[&str],
        doc: &KdlDocument,
        span: &SourceSpan,
        source_name: &str,
    ) -> miette::Result<Self> {
        if let Some(bad_key) = self.keys().find(|k| !allowed.contains(k)) {
            return Err(Bad::docspan(
                format!(
                    "Unknown configuration key: '{bad_key}'. Allowed keys are: {:?}",
                    allowed
                ),
                doc,
                span,
                source_name,
            )
            .into());
        }

        Ok(self)
    }
}
