use super::ctx::ParseContext;
use miette::Result;
use std::collections::HashMap;

pub struct BlockParser {
    ctx: ParseContext,
    children: HashMap<String, Vec<ParseContext>>,
}

impl BlockParser {
    /// Initializes the parser by consuming the children of the provided context.
    pub fn new(ctx: ParseContext) -> Result<Self> {
        let mut children: HashMap<String, Vec<ParseContext>> = HashMap::new();

        for child in ctx.nodes()? {
            let name = child.name()?;
            children.entry(name.to_string()).or_default().push(child);
        }

        Ok(Self { ctx, children })
    }

    /// Creates a BlockParser, passes it to the provided function,
    /// and ensures all directives are consumed (exhausted) after execution.
    pub fn enter<F, T>(ctx: ParseContext, f: F) -> Result<T>
    where
        F: FnOnce(&mut BlockParser) -> Result<T>,
    {
        let mut parser = Self::new(ctx)?;
        let result = f(&mut parser)?;
        parser.exhaust()?;
        Ok(result)
    }

    /// Requires exactly one directive with the given `name`.
    ///
    /// Errors if:
    /// - The directive is missing.
    /// - The directive appears multiple times.
    pub fn required<T, F>(&mut self, name: &str, f: F) -> Result<T>
    where
        F: FnOnce(ParseContext) -> Result<T>,
    {
        self.optional(name, f)?.ok_or_else(|| {
            self.ctx
                .error(format!("Missing required directive '{name}'"))
        })
    }

    /// Allows zero or one directive with the given `name`.
    ///
    /// Errors if:
    /// - The directive appears multiple times.
    pub fn optional<T, F>(&mut self, name: &str, f: F) -> Result<Option<T>>
    where
        F: FnOnce(ParseContext) -> Result<T>,
    {
        match self.children.remove(name) {
            Some(mut nodes) if nodes.len() == 1 => Ok(Some(f(nodes.pop().unwrap())?)),
            Some(nodes) => {
                let first = &nodes[0];
                Err(first.error(format!("Directive '{name}' cannot be repeated")))
            }
            None => Ok(None),
        }
    }
    /// Requires at least one directive with the given `name`.
    ///
    /// Errors if:
    /// - The directive is missing.
    pub fn required_repeated<T, F>(&mut self, name: &str, mut f: F) -> Result<Vec<T>>
    where
        F: FnMut(ParseContext) -> Result<T>,
    {
        let results = self.repeated(name, &mut f)?;

        if results.is_empty() {
            return Err(self.ctx.error(format!(
                "Missing required directive '{name}' (at least one expected)"
            )));
        }

        Ok(results)
    }

    /// Allows zero or more directives with the given `name`.
    /// Returns a vector of results.
    pub fn repeated<T, F>(&mut self, name: &str, mut f: F) -> Result<Vec<T>>
    where
        F: FnMut(ParseContext) -> Result<T>,
    {
        let mut results = Vec::new();
        if let Some(nodes) = self.children.remove(name) {
            for node in nodes {
                results.push(f(node)?);
            }
        }
        Ok(results)
    }

    /// Requires exactly one directive from the provided list of `names`.
    ///
    /// The closure `f` receives the `ParseContext` AND the name of the matched directive.
    ///
    /// Errors if:
    /// - None of the directives are present.
    /// - More than one distinct directive from the list is present (conflict).
    /// - The matched directive appears multiple times (duplicate).
    pub fn required_any<T, F>(&mut self, names: &[&str], f: F) -> Result<T>
    where
        F: FnOnce(ParseContext, &str) -> Result<T>,
    {
        self.optional_any(names, f)?.ok_or_else(|| {
            self.ctx
                .error(format!("Block must contain exactly one of: {:?}", names))
        })
    }

    /// Allows zero or one directive from the provided list of `names`.
    ///
    /// The closure `f` receives the `ParseContext` AND the name of the matched directive.
    ///
    /// Errors if:
    /// - More than one distinct directive from the list is present (conflict).
    /// - The matched directive appears multiple times (duplicate).
    pub fn optional_any<T, F>(&mut self, names: &[&str], f: F) -> Result<Option<T>>
    where
        F: FnOnce(ParseContext, &str) -> Result<T>,
    {
        let present_keys: Vec<&str> = names
            .iter()
            .filter(|&&name| self.children.contains_key(name))
            .copied()
            .collect();

        if present_keys.is_empty() {
            return Ok(None);
        }

        if present_keys.len() > 1 {
            let key1 = present_keys[0];
            let key2 = present_keys[1];

            let node2 = &self.children[key2][0];

            return Err(node2.error(format!(
                "Directive '{key2}' conflicts with '{key1}' (mutually exclusive)"
            )));
        }

        let matched_key = present_keys[0];

        self.optional(matched_key, |ctx| f(ctx, matched_key))
    }
    pub fn exhaust(self) -> Result<()> {
        if let Some((name, nodes)) = self.children.into_iter().next() {
            let first = &nodes[0];
            return Err(first.error(format!("Unknown directive: '{name}'")));
        }
        Ok(())
    }
}

#[macro_export]
macro_rules! block_parser {
    ( $ctx:expr, $($rest:tt)* ) => {
        let mut _bp = $crate::kdl::parser::block::BlockParser::new($ctx)?;

        $crate::block_parser!(@step _bp, $($rest)*);
        _bp.exhaust()?;
    };

    (@step $bp:ident, $var:ident : $method:ident ( $($arg:expr),* ) => $handler:expr $(, $($tail:tt)*)?) => {
        let $var = $bp.$method($($arg),*, $handler)?;
        $crate::block_parser!(@step $bp, $($($tail)*)?);
    };

    (@step $bp:ident, $method:ident ( $($arg:expr),* ) => $handler:expr $(, $($tail:tt)*)?) => {
        $bp.$method($($arg),*, $handler)?;
        $crate::block_parser!(@step $bp, $($($tail)*)?);
    };

    (@step $bp:ident, ) => {};
}
