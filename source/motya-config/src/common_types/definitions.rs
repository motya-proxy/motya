use fqdn::FQDN;
use std::{collections::BTreeMap, path::PathBuf};

use crate::common_types::{
    key_template::{HashOp, KeyTemplate, TransformOp},
    rate_limiter::RateLimitPolicy,
    value::Value,
};

#[derive(Debug, Clone, PartialEq)]
pub enum ChainItem {
    Filter(ConfiguredFilter),
    RateLimiter(RateLimitPolicy),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FilterChain {
    pub items: Vec<ChainItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfiguredFilter {
    pub name: FQDN,
    pub args: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginDefinition {
    pub name: FQDN,
    pub source: PluginSource,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PluginSource {
    File(PathBuf),
    Url(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BalancerConfig {
    pub source: KeyTemplate,
    pub fallback: Option<KeyTemplate>,
    pub algorithm: HashOp,
    pub transforms: Vec<TransformOp>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamedFilterChain {
    pub name: String,
    pub chain: FilterChain,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Modificator {
    Chain(NamedFilterChain),
}
