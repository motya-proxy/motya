use std::collections::BTreeMap;
use crate::config::common_types::definitions::{DefinitionsTable, FilterChain};
use crate::proxy::filters::registry::{FilterRegistry, FilterInstance};
use crate::proxy::{RequestFilterMod, RequestModifyMod, ResponseModifyMod};
use miette::{Context, IntoDiagnostic, Result, miette};

#[derive(Default)]
pub struct RuntimeChain {
    pub actions: Vec<Box<dyn RequestFilterMod>>,
    pub req_mods: Vec<Box<dyn RequestModifyMod>>,
    pub res_mods: Vec<Box<dyn ResponseModifyMod>>,
}


pub struct ChainResolver {
    table: DefinitionsTable,
    registry: FilterRegistry,
}

impl ChainResolver {
    
    pub fn new(table: DefinitionsTable, registry: FilterRegistry) -> Result<Self> {
        for (chain_name, chain) in table.get_chains() {
            for filter in &chain.filters {
                if !registry.contains(&filter.name) {
                    return Err(miette!(
                        "Chain '{}' references unknown filter '{}'. Did you forget to load a plugin?",
                        chain_name,
                        filter.name
                    ));
                }
            }
        }
        
        for filter_name in &table.available_filters {
            if !registry.contains(filter_name) {
                return Err(miette!("Filter '{}' is defined but not registered.", filter_name));
            }
        }

        Ok(Self { table, registry })
    }

    pub fn resolve(&self, chain_name: &str) -> Result<RuntimeChain> {
        let chain_cfg = self.table.get_chains().get(chain_name).ok_or_else(|| {
            miette!("Chain '{}' not found in definitions table", chain_name)
        })?;

        self.build_chain(chain_cfg, chain_name)
    }

    fn build_chain(&self, chain: &FilterChain, context_name: &str) -> Result<RuntimeChain> {
        let mut runtime_chain = RuntimeChain::default();

        for filter_cfg in &chain.filters {
            let settings: BTreeMap<String, String> = filter_cfg.args.iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            let instance = self.registry
                .build(&filter_cfg.name, settings)
                .into_diagnostic()
                .wrap_err_with(|| format!("Failed to build filter '{}' in chain '{}'", filter_cfg.name, context_name))?;

            match instance {
                FilterInstance::Action(f) => runtime_chain.actions.push(f),
                FilterInstance::Request(f) => runtime_chain.req_mods.push(f),
                FilterInstance::Response(f) => runtime_chain.res_mods.push(f),
            }
        }

        Ok(runtime_chain)
    }
}