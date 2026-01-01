use std::{
    collections::HashMap,
    fmt::Debug,
    ops::{Range, RangeFrom, RangeFull, RangeTo},
    str::FromStr,
    sync::Arc,
    vec::IntoIter,
};

use fqdn::FQDN;
use kdl::{KdlDocument, KdlEntry, KdlNode};
use miette::{NamedSource, Result, SourceSpan};

use crate::{
    common_types::bad::Bad, kdl::parser::typed_value::TypedValue, var_registry::VarRegistry,
};

#[derive(Debug, Clone)]
pub struct ParseContext {
    doc: Arc<KdlDocument>,
    source_name: Arc<str>,
    current: Current,
    pub(crate) registry: Option<Arc<VarRegistry>>,
}

#[derive(Debug, Clone)]
pub enum Current {
    Document(Arc<KdlDocument>),
    Node(Arc<KdlNode>),
}

impl ParseContext {
    pub fn new_with_registry(
        doc: Arc<KdlDocument>,
        source_name: Arc<str>,
        registry: Arc<VarRegistry>,
    ) -> Self {
        Self {
            current: Current::Document(doc.clone()),
            doc,
            registry: Some(registry),
            source_name,
        }
    }

    #[cfg(test)]
    pub fn new_with_self(doc: KdlDocument) -> Self {
        let arc_doc = Arc::new(doc);
        Self {
            current: Current::Document(Arc::clone(&arc_doc)),
            doc: arc_doc,
            source_name: Arc::from("<unknown>"),
            registry: None,
        }
    }

    pub fn new(doc: KdlDocument, source_name: &str) -> Self {
        let doc = Arc::new(doc);
        Self {
            current: Current::Document(Arc::clone(&doc)),
            doc,
            source_name: Arc::from(source_name),
            registry: None,
        }
    }

    fn derive(&self, current: Current) -> Self {
        Self {
            doc: Arc::clone(&self.doc),
            source_name: Arc::clone(&self.source_name),
            current,
            registry: self.registry.as_ref().map(Arc::clone),
        }
    }

    pub fn source(&self) -> NamedSource<String> {
        NamedSource::new(self.source_name.as_ref(), self.doc.to_string())
    }

    /// Creates a new context for the child block's content.
    pub fn enter_block(&self) -> Result<ParseContext> {
        match &self.current {
            Current::Node(node) => {
                let children = node
                    .children()
                    .map(|c| Arc::new(c.clone()))
                    .ok_or_else(|| {
                        self.error("Expected a children block { ... }, but none found")
                    })?;

                Ok(self.derive(Current::Document(children)))
            }
            Current::Document(_) => {
                Err(self.error("Cannot enter block: current context is already a document root"))
            }
        }
    }

    /// Returns the source span of a specific property by key.
    pub fn prop_span(&self, key: &str) -> Option<SourceSpan> {
        if let Current::Node(node) = &self.current {
            node.entries()
                .iter()
                .find(|e| e.name().map(|nm| nm.value()) == Some(key))
                .map(|e| e.span())
        } else {
            None
        }
    }

    /// Returns the source span of a positional argument by index.
    pub fn arg_span(&self, index: usize) -> Option<SourceSpan> {
        if let Current::Node(node) = &self.current {
            node.entries()
                .iter()
                .filter(|e| e.name().is_none())
                .nth(index)
                .map(|e| e.span())
        } else {
            None
        }
    }

    /// Returns the span of the current node's name.
    pub fn name_span(&self) -> SourceSpan {
        match &self.current {
            Current::Node(node) => node.name().span(),
            Current::Document(doc) => doc.span(),
        }
    }

    pub fn error_with_span(&self, msg: impl Into<String>, span: SourceSpan) -> miette::Error {
        Bad::docspan(msg.into(), &self.doc, &span, &self.source_name).into()
    }

    /// Generates a styled error message pointing to the current span in the source.
    pub fn error(&self, msg: impl Into<String>) -> miette::Error {
        self.error_with_span(msg, self.current_span())
    }

    /// Returns the source span of the current element (Node or Document).
    pub fn current_span(&self) -> SourceSpan {
        match &self.current {
            Current::Document(doc) => doc.span(),
            Current::Node(node) => node.span(),
        }
    }

    /// Returns an iterator over child nodes, each wrapped in a new `ParseContext`.
    pub fn nodes_iter(&self) -> Result<IntoIter<ParseContext>> {
        Ok(self.nodes()?.into_iter())
    }

    /// Returns the name of the current node.
    pub fn name(&self) -> Result<&str> {
        match &self.current {
            Current::Document(_) => Err(self.error("Expected node, but current is a document. This sounds like a bug: attempting to access node name on the root document.")),
            Current::Node(node) => Ok(node.name().value()),
        }
    }

    /// Iterates over child nodes, returning a new `ParseContext` for each child.
    pub fn nodes(&self) -> Result<Vec<ParseContext>> {
        let doc = match &self.current {
            Current::Document(d) => Arc::clone(d),
            Current::Node(n) => {
                let Some(children) = n.children().cloned() else {
                    return Ok(vec![]);
                };
                Arc::new(children)
            }
        };

        Ok(doc
            .nodes()
            .iter()
            .map(|node| self.derive(Current::Node(Arc::new(node.clone()))))
            .collect())
    }

    /// Asserts that the current node has a specific name.
    pub fn expect_name(&self, expected: &str) -> Result<()> {
        if self.name()? == expected {
            Ok(())
        } else {
            Err(self.error(format!("Expected '{expected}', found '{}'", self.name()?)))
        }
    }

    /// Returns the raw slice of arguments/entries for the current node.
    pub fn args(&self) -> Result<&[KdlEntry]> {
        match &self.current {
            Current::Document(_) => Err(self.error("Expected node, but current is a document. This sounds like a bug: attempting to access args on the root document.")),
            Current::Node(node) => Ok(node.entries()),
        }
    }

    /// Retrieves a required named property as a String.
    pub fn string_arg(&self, name: &str) -> Result<String> {
        let val = self
            .opt_prop(name)?
            .ok_or_else(|| self.error(format!("Missing required property: '{name}'")))?;

        Ok(val.as_str()?.to_string())
    }

    /// Retrieves a required named property and parses it as an FQDN.
    pub fn parse_fqdn_arg(&self, name: &str) -> Result<FQDN> {
        let s = self.string_arg(name)?;
        FQDN::from_str(&s).map_err(|e| self.error(format!("Invalid FQDN '{s}': {e}")))
    }

    /// Checks if the current node has an attached children block (e.g., `{ ... }`).
    pub fn has_children_block(&self) -> bool {
        match &self.current {
            Current::Node(n) => n.children().is_some(),
            Current::Document(_) => true,
        }
    }

    /// Retrieves child nodes but returns an error if the block is empty.
    pub fn req_nodes(&self) -> Result<Vec<ParseContext>> {
        let ns = self.nodes()?;
        if ns.is_empty() {
            return Err(self.error(format!(
                "Block '{name}' cannot be empty",
                name = self.name()?
            )));
        }
        Ok(ns)
    }

    /// Retrieves multiple optional properties at once.
    pub fn props<const N: usize>(&self, keys: [&str; N]) -> Result<[Option<TypedValue>; N]> {
        let mut result = std::array::from_fn(|_| None);
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
