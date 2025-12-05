
use std::{collections::{HashMap, HashSet}, hash::Hash, path::PathBuf};
use fqdn::FQDN;

use crate::proxy::filters::generate_registry::load_definitions_table;

/// Definitions Table (Intermediate Representation).
///
/// This structure accumulates all "blueprints" and configurations from all loaded KDL files.
/// It serves as a bridge between the **Parsing Phase** (KDL -> Structs) and the
/// **Compilation Phase** (Structs -> Runtime Objects).
///
/// # Lifecycle:
/// 1. **Aggregation**: The `ConfigLoader` iterates through all files and merges their `definitions`
///    into a single table.
/// 2. **Augmentation**: Service parsers (e.g., [`ConnectorsSection`]) inject anonymous chains
///    (from `use-chain { ... }` blocks) into this table.
/// 3. **Consumption**:
///    - [`WasmPluginStore`] uses the `plugins` field to download/load WASM files.
///    - [`ChainResolver`] uses the `chains` field to instantiate concrete filter objects
///      when building routes.
#[derive(Debug, Default, Clone)]
pub struct DefinitionsTable {
    /// A list of all known filter names (Fully Qualified Domain Names).
    ///
    /// Used for **"Fail Fast" validation**: allows the application to crash with a clear error
    /// *before* the proxy starts if a chain references a filter that was not declared
    /// via `def` or `plugin`.
    available_filters: HashSet<FQDN>,

    /// A library of filter chain configurations.
    ///
    /// Stores the settings (key=value arguments) for sequences of filters.
    /// Contains both explicitly named user chains and automatically generated
    /// (**anonymous**) chains from `connectors` blocks.
    chains: HashMap<String, FilterChain>,

    /// Plugin metadata.
    ///
    /// Stores information on *where* to retrieve the plugin code (File Path or URL),
    /// but does not hold the compiled code itself.
    plugins: HashMap<FQDN, PluginDefinition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FilterChain {
    pub filters: Vec<ConfiguredFilter>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfiguredFilter {
    pub name: FQDN,
    pub args: HashMap<String, String>,
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


impl DefinitionsTable {

    pub fn new(
        available_filters: HashSet<FQDN>,
        chains: HashMap<String, FilterChain>,
        plugins: HashMap<FQDN, PluginDefinition>
    ) -> Self {
        Self { available_filters, chains, plugins }
    }

    pub fn new_with_global() -> Self {
        load_definitions_table()
    }

    pub fn get_chain_by_name(&self, name: &str) -> Option<FilterChain> {
        self.chains.get(name).cloned()
    }

    pub fn insert_filter(&mut self, filter_name: FQDN) -> bool {
        self.available_filters.insert(filter_name)
    }

    pub fn insert_plugin(&mut self, name: FQDN, plugin: PluginDefinition) -> Option<PluginDefinition> {
        self.plugins.insert(name, plugin)
    }

    pub fn insert_chain(&mut self, name: impl Into<String>, chain: FilterChain) -> Option<FilterChain> {
        self.chains.insert(name.into(), chain)
    }
    
    pub fn extend_chain(&mut self, chains: HashMap<String, FilterChain>) {
        self.chains.extend(chains);
    }

    pub fn get_plugins(&self) -> &HashMap<FQDN, PluginDefinition> { &self.plugins }
    pub fn get_available_filters(&self) -> &HashSet<FQDN> { &self.available_filters }
    pub fn get_chains(&self) -> &HashMap<String, FilterChain> { &self.chains }

    pub fn merge(&mut self, other: DefinitionsTable) -> miette::Result<()> {
        
        for filter in other.available_filters {
            self.available_filters.insert(filter);
        }

        for (name, chain) in other.chains {
            if self.chains.contains_key(&name) {
                
                return Err(miette::miette!("Duplicate chain definition across files: '{}'", name));
            }
            self.chains.insert(name, chain);
        }
        
        for (name, plugin) in other.plugins {
            if self.plugins.contains_key(&name) {
                return Err(miette::miette!("Duplicate plugin definition across files: '{}'", name));
            }
            self.plugins.insert(name, plugin);
        }

        Ok(())
    }

}



#[derive(Debug, Clone, PartialEq)]
pub struct NamedFilterChain {
    pub name: String,
    pub chain: FilterChain
}

#[derive(Debug, Clone, PartialEq)]
pub enum Modificator {
    Chain(NamedFilterChain)
}

