use std::path::PathBuf;

use fqdn::FQDN;
use humantime::Duration;
use motya_macro::{motya_node, NodeSchema, Parser};

use crate::{
    common_types::key_template::KeyTemplate,
    kdl::models::{
        chains::ChainItemDef,
        key_profile::{HashAlgDef, KeyDef},
        transforms_order::TransformsOrderDef,
    },
};

// =============================================================================
// ROOT DEFINITIONS
// =============================================================================

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "definitions")]
pub struct DefinitionsDef {
    #[node(child)]
    pub modifiers: Option<ModifiersSectionDef>,

    #[node(child)]
    pub plugins: Option<PluginsSectionDef>,

    #[node(child, name = "key-profiles")]
    pub key_profiles: Option<KeyProfilesSectionDef>,

    #[node(child)]
    pub storages: Option<StoragesSectionDef>,

    #[node(child, name = "rate-limits")]
    pub rate_limits: Option<RateLimitsSectionDef>,
}

// =============================================================================
// MODIFIERS SECTION (Chains & Namespaces)
// =============================================================================

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "modifiers")]
pub struct ModifiersSectionDef {
    #[node(child)]
    pub namespaces: Vec<ModifiersNamespaceDef>,

    #[node(child)]
    pub chains: Vec<ChainFiltersDef>,
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "namespace")]
pub struct ModifiersNamespaceDef {
    #[node(arg)]
    pub name: String,

    #[node(child)]
    pub namespaces: Vec<ModifiersNamespaceDef>,

    #[node(child)]
    pub defs: Vec<FilterDefRef>,
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "def")]
pub struct FilterDefRef {
    #[node(prop)]
    pub name: String,
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "chain-filters")]
pub struct ChainFiltersDef {
    #[node(arg)]
    pub name: String,

    #[node(child)]
    pub filters: Vec<ChainItemDef>,
}

// =============================================================================
// PLUGINS SECTION
// =============================================================================

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "plugins")]
pub struct PluginsSectionDef {
    #[node(child)]
    pub plugins: Vec<PluginDef>,
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "plugin")]
pub struct PluginDef {
    #[node(child, flat)]
    pub name: FQDN,

    #[node(child)]
    pub load: PluginLoadDef,
}

#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "load")]
pub struct PluginLoadDef {
    #[node(prop)]
    pub path: Option<PathBuf>,
    #[node(prop)]
    pub url: Option<String>,
}

// =============================================================================
// KEY PROFILES SECTION
// =============================================================================

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "key-profiles")]
pub struct KeyProfilesSectionDef {
    #[node(child)]
    pub namespaces: Vec<KeyProfileNamespaceDef>,

    #[node(child)]
    pub templates: Vec<KeyProfileTemplateDef>,
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "namespace")]
pub struct KeyProfileNamespaceDef {
    #[node(arg)]
    pub name: String,

    #[node(child)]
    pub namespaces: Vec<KeyProfileNamespaceDef>,

    #[node(child)]
    pub templates: Vec<KeyProfileTemplateDef>,
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "template")]
pub struct KeyProfileTemplateDef {
    #[node(arg)]
    pub name: String,

    #[node(child)]
    pub key: KeyDef,

    #[node(child)]
    pub algorithm: HashAlgDef,

    #[node(child, name = "transforms-order")]
    pub transforms: Option<TransformsOrderDef>,
}

// =============================================================================
// STORAGES SECTION
// =============================================================================

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "storages")]
pub struct StoragesSectionDef {
    #[node(child)]
    pub storages: Vec<StorageDef>,
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
pub enum StorageDef {
    #[node(name = "redis")]
    Redis(RedisStorageDef),
    #[node(name = "memory")]
    Memory(MemoryStorageDef),
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
pub struct RedisStorageDef {
    #[node(arg)]
    pub name: String,

    #[node(child)]
    pub addresses: Vec<String>,

    #[node(child)]
    pub password: Option<String>,

    #[node(child, flat)]
    pub timeout: Option<Duration>,
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
pub struct MemoryStorageDef {
    #[node(arg)]
    pub name: String,

    #[node(child, name = "max-keys")]
    pub max_keys: Option<usize>,

    #[node(child, flat, name = "cleanup-interval")]
    pub cleanup_interval: Option<Duration>,
}

// =============================================================================
// RATE LIMITS SECTION
// =============================================================================

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "rate-limits")]
pub struct RateLimitsSectionDef {
    #[node(child, name = "policy")]
    pub policies: Vec<RateLimitPolicyDef>,
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "rate-limit")]
pub struct RateLimitPolicyDef {
    #[node(arg)]
    pub name: String,

    #[node(child)]
    pub algorithm: Option<String>,

    #[node(child, name = "storage")]
    pub storage_ref: Option<String>,

    #[node(child, flat)]
    pub key: KeyTemplate,

    #[node(child, flat)]
    pub rate: Duration,

    #[node(child)]
    pub burst: Option<usize>,

    #[node(child, name = "transforms-order")]
    pub transforms: Option<TransformsOrderDef>,
}
