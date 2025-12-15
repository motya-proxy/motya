use std::{fmt::Display, str::FromStr};

use kdl::{KdlEntry, KdlValue};
use miette::Result;

use crate::kdl::parser::{ctx::ParseContext, utils::get_simple_type_name};

#[derive(Debug, Clone, Copy)]
pub struct TypedValue<'a> {
    ctx: &'a ParseContext<'a>,
    entry: &'a KdlEntry,
}

impl<'a> TypedValue<'a> {
    pub fn new(ctx: &'a ParseContext<'a>, entry: &'a KdlEntry) -> Self {
        Self { ctx, entry }
    }

    pub fn name(&self) -> Option<&str> {
        self.entry.name().map(|n| n.value())
    }

    fn try_resolve_variable(&self) -> Result<Option<String>> {
        let type_anno = match self.entry.ty() {
            Some(t) => t.value(),
            None => return Ok(None),
        };

        if !matches!(type_anno, "env" | "var") {
            return Ok(None);
        }

        let var_key = self.entry.value().as_string().ok_or_else(|| {
            self.ctx.error_with_span(
                format!(
                    "Variable key must be a string (e.g. ({})\"KEY\"), found {:?}",
                    type_anno,
                    self.entry.value()
                ),
                self.entry.span(),
            )
        })?;

        let registry = self.ctx.registry.as_ref().ok_or_else(|| {
            self.ctx.error_with_span(
                "Variables are not supported in this context (registry missing)",
                self.entry.span(),
            )
        })?;

        match registry.resolve(var_key, type_anno) {
            Some(val) => Ok(Some(val)),
            None => Err(self.ctx.error_with_span(
                format!("Variable '{}' not found in source '{}'", var_key, type_anno),
                self.entry.span(),
            )),
        }
    }

    pub fn as_str(self) -> Result<String> {
        if let Some(resolved) = self.try_resolve_variable()? {
            return Ok(resolved);
        }

        self.entry
            .value()
            .as_string()
            .map(|s| s.to_string())
            .ok_or_else(|| {
                self.ctx.error_with_span(
                    format!("Expected a string value, found {:?}", self.entry.value()),
                    self.entry.span(),
                )
            })
    }

    pub fn as_usize(self) -> Result<usize> {
        if let Some(resolved) = self.try_resolve_variable()? {
            return resolved.parse::<usize>().map_err(|e| {
                self.ctx.error_with_span(
                    format!("Failed to parse resolved variable '{resolved}' as usize: {e}"),
                    self.entry.span(),
                )
            });
        }

        self.entry
            .value()
            .as_integer()
            .and_then(|i| usize::try_from(i).ok())
            .ok_or_else(|| {
                self.ctx.error_with_span(
                    format!(
                        "Expected a positive integer, found {:?}",
                        self.entry.value()
                    ),
                    self.entry.span(),
                )
            })
    }

    pub fn as_bool(self) -> Result<bool> {
        if let Some(resolved) = self.try_resolve_variable()? {
            return resolved.parse::<bool>().map_err(|e| {
                self.ctx.error_with_span(
                    format!("Failed to parse resolved variable '{resolved}' as bool: {e}"),
                    self.entry.span(),
                )
            });
        }

        self.entry.value().as_bool().ok_or_else(|| {
            self.ctx.error_with_span(
                format!("Expected a boolean, found {:?}", self.entry.value()),
                self.entry.span(),
            )
        })
    }

    pub fn parse_as<T>(self) -> Result<T>
    where
        T: FromStr,
        T::Err: Display,
    {
        let raw_str = self.as_string_lossy()?;
        T::from_str(&raw_str).map_err(|e| {
            let type_name = get_simple_type_name::<T>();
            self.ctx.error_with_span(
                format!("Invalid {type_name} '{raw_str}'. Reason: {e}"),
                self.entry.span(),
            )
        })
    }

    pub fn as_string_lossy(self) -> Result<String> {
        if let Some(resolved) = self.try_resolve_variable()? {
            return Ok(resolved);
        }

        match self.entry.value() {
            KdlValue::String(s) => Ok(s.clone()),
            KdlValue::Integer(i) => Ok(i.to_string()),
            KdlValue::Float(f) => Ok(f.to_string()),
            KdlValue::Bool(b) => Ok(b.to_string()),

            KdlValue::Null => Err(self.ctx.error_with_span(
                "Cannot parse 'null' as a string or number",
                self.entry.span(),
            )),
        }
    }
}

impl<'a> ParseContext<'a> {
    pub fn first<'b>(&'a self) -> Result<TypedValue<'b>>
    where
        'a: 'b,
    {
        let entry = self
            .args()?
            .first()
            .ok_or_else(|| self.error("Missing required first argument"))?;

        Ok(TypedValue::new(self, entry))
    }

    pub fn arg<'b>(&'a self, index: usize) -> Result<TypedValue<'b>>
    where
        'a: 'b,
    {
        let entry = self
            .args()?
            .iter()
            .filter(|e| e.name().is_none())
            .nth(index)
            .ok_or_else(|| {
                self.error(format!(
                    "Missing required argument at position {}",
                    index + 1
                ))
            })?;

        Ok(TypedValue::new(self, entry))
    }

    pub fn args_named_typed<'b>(&'a self) -> Result<Vec<TypedValue<'b>>>
    where
        'a: 'b,
    {
        let entries = self
            .args_typed()?
            .into_iter()
            .filter(|e| e.name().is_some())
            .collect();

        Ok(entries)
    }

    pub fn args_typed<'b>(&'a self) -> Result<Vec<TypedValue<'b>>>
    where
        'a: 'b,
    {
        let entries = self
            .args()?
            .iter()
            .map(|entry| TypedValue::new(self, entry))
            .collect();

        Ok(entries)
    }

    pub fn prop<'b>(&'a self, key: &str) -> Result<TypedValue<'b>>
    where
        'a: 'b,
    {
        let entry = self
            .args()?
            .iter()
            .find(|e| e.name().map(|n| n.value()) == Some(key))
            .ok_or_else(|| self.error(format!("Missing required property '{}'", key)))?;

        Ok(TypedValue::new(self, entry))
    }

    pub fn opt_prop<'b>(&'a self, key: &str) -> Result<Option<TypedValue<'b>>>
    where
        'a: 'b,
    {
        let entry = self
            .args()?
            .iter()
            .find(|e| e.name().map(|n| n.value()) == Some(key));

        Ok(entry.map(|e| TypedValue::new(self, e)))
    }
}
