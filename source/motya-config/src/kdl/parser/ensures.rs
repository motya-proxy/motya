use std::{net::SocketAddr, str::FromStr};

use kdl::KdlValue;
use miette::Result;

use crate::kdl::parser::{
    ctx::ParseContext,
    utils::{get_kdl_type_name, PrimitiveType},
};

/// Defines validation constraints that can be applied to a KDL node.
#[derive(Debug, Clone, Copy)]
pub enum Rule<'a> {
    ReqChildren,
    /// The node must be a leaf (cannot have a `{ ... }` block).
    NoChildren,
    /// The node cannot have any arguments or properties.
    NoArgs,
    /// Only the specified named properties are allowed.
    OnlyKeys(&'a [&'a str]),
    NoPositionalArgs,
    ExactArgs(usize),
    AtLeastArgs(usize),
    Name(NamePredicate),
    OnlyKeysTyped(&'a [(&'a str, PrimitiveType)]),
}

#[derive(Debug, Clone, Copy)]
pub enum NamePredicate {
    SocketAddr,
    FQDN,
    Custom(fn(&str) -> bool, &'static str),
}

impl<'a> ParseContext<'a> {
    /// Applies a list of validation rules to the current context.
    pub fn validate(&self, rules: &[Rule]) -> Result<()> {
        for rule in rules {
            match rule {
                Rule::NoChildren => self.ensure_no_children()?,
                Rule::NoArgs => self.ensure_no_args()?,
                Rule::OnlyKeys(allowed) => self.ensure_only_keys(allowed)?,
                Rule::NoPositionalArgs => self.ensure_positional_args(0, 0)?,
                Rule::Name(predicate) => self.ensure_name_matches(predicate)?,
                Rule::OnlyKeysTyped(schema) => self.ensure_only_keys_typed(schema)?,
                Rule::ExactArgs(n) => self.ensure_positional_args(*n, *n)?,
                Rule::AtLeastArgs(n) => self.ensure_positional_args(*n, 256)?,
                Rule::ReqChildren => self.ensure_req_children()?,
            }
        }
        Ok(())
    }

    pub fn ensure_only_keys_typed(&self, schema: &[(&str, PrimitiveType)]) -> Result<()> {
        let args = self.args()?;

        for arg in args {
            if let Some(name_node) = arg.name() {
                let key = name_node.value();

                match schema.iter().find(|(k, _)| *k == key) {
                    None => {
                        let allowed_keys: Vec<&str> = schema.iter().map(|(k, _)| *k).collect();
                        return Err(self.error(format!(
                            "Unknown configuration key: '{key}'. Allowed keys are: {:?}",
                            allowed_keys
                        )));
                    }
                    Some((_, expected_type)) => {
                        let value = arg.value();
                        let is_valid = matches!(
                            (expected_type, value),
                            (PrimitiveType::String, KdlValue::String(_))
                                | (PrimitiveType::Integer, KdlValue::Integer(_))
                                | (PrimitiveType::Float, KdlValue::Float(_))
                                | (PrimitiveType::Bool, KdlValue::Bool(_))
                                | (PrimitiveType::Null, KdlValue::Null)
                        );

                        if !is_valid {
                            let actual_type = get_kdl_type_name(value);
                            return Err(self.error(format!(
                                "Invalid type for key '{key}'. Expected {expected_type}, found {actual_type}"
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn ensure_name_matches(&self, predicate: &NamePredicate) -> Result<()> {
        let name = self.name()?;

        match predicate {
            NamePredicate::SocketAddr => {
                if name.parse::<SocketAddr>().is_err() {
                    return Err(self.error(format!(
                        "'{name}' is not a valid socket address. Expected format: 'IP:PORT' (e.g., '127.0.0.1:8080')"
                    )));
                }
            }
            NamePredicate::FQDN => {
                if fqdn::FQDN::from_str(name).is_err() {
                    return Err(self.error(format!(
                        "'{name}' is not a valid FQDN (Fully Qualified Domain Name)"
                    )));
                }
            }
            NamePredicate::Custom(func, err_msg) => {
                if !func(name) {
                    return Err(self.error(format!("Invalid node name '{name}': {err_msg}")));
                }
            }
        }
        Ok(())
    }

    fn ensure_positional_args(&self, min: usize, max: usize) -> Result<()> {
        let args = self.args()?;
        let count = args.iter().filter(|e| e.name().is_none()).count();
        let name = self.name().unwrap_or("node");

        if count < min {
            if min == max {
                return Err(self.error(format!(
                    "Directive '{name}' requires exactly {min} positional argument(s), found {count}"
                )));
            }
            return Err(self.error(format!(
                "Directive '{name}' requires at least {min} positional argument(s), found {count}"
            )));
        }

        if count > max {
            if max == 0 {
                return Err(self.error(format!(
                    "Directive '{name}' does not accept positional arguments (only named properties allowed)"
                )));
            }
            if min == max {
                return Err(self.error(format!(
                    "Directive '{name}' requires exactly {min} positional argument(s), found {count}"
                )));
            }
            return Err(self.error(format!(
                "Directive '{name}' allows at most {max} positional argument(s), found {count}"
            )));
        }

        Ok(())
    }

    pub fn ensure_req_children(&self) -> Result<()> {
        if !self.has_children_block()? {
            return Err(self.error(format!(
                "Directive '{name}' requires a children block {{ ... }}",
                name = self.name()?
            )));
        }
        Ok(())
    }

    /// Enforces that the node is a leaf (no children block).
    pub fn ensure_no_children(&self) -> Result<()> {
        if self.has_children_block()? {
            return Err(self.error(format!(
                "Directive '{name}' must be a leaf node (no children block allowed)",
                name = self.name()?
            )));
        }
        Ok(())
    }

    /// Enforces that only whitelisted keys are present in the arguments.
    pub fn ensure_only_keys(&self, allowed: &[&str]) -> Result<()> {
        let args = self.args()?;
        for arg in args {
            if let Some(name) = arg.name() {
                let key = name.value();
                if !allowed.contains(&key) {
                    return Err(self.error(format!(
                        "Unknown configuration key: '{key}'. Allowed keys are: {:?}",
                        allowed
                    )));
                }
            }
        }
        Ok(())
    }

    /// Enforces that the node has no arguments (positional or named).
    pub fn ensure_no_args(&self) -> Result<()> {
        if !self.args()?.is_empty() {
            let name = self.name().unwrap_or("node");
            return Err(self.error(format!("Directive '{name}' cannot have arguments")));
        }
        Ok(())
    }
}
