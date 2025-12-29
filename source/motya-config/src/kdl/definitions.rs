use std::collections::{BTreeMap, HashMap};
use std::time::Duration as StdDuration;

use crate::common_types::key_template::{parse_hasher, HashAlgorithm};
use crate::common_types::value::Value;
use crate::common_types::{
    definitions::{
        ChainItem, ConfiguredFilter, FilterChain, PluginDefinition,
        PluginSource as RuntimePluginSource,
    },
    definitions_table::DefinitionsTable,
    error::ConfigError,
    rate_limiter::{RateLimitPolicy, StorageConfig},
};
use crate::kdl::models::chains::{ChainItemDefData, RateLimitDefData};
use crate::kdl::models::definitions::{
    DefinitionsDef, KeyProfileNamespaceDef, KeyProfileTemplateDef, KeyProfilesSectionDefData,
    ModifiersNamespaceDef, ModifiersSectionDefData, PluginDef, RateLimitPolicyDef, StorageDef,
    StorageDefData,
};

pub struct DefinitionsCompiler;

impl DefinitionsCompiler {
    // TODO: Toposort? Skill issue. Not today, babe.
    pub fn collect_prerequisites(
        &self,
        ast: DefinitionsDef,
        table: &mut DefinitionsTable,
        errors: &mut ConfigError,
    ) {
        let ast = ast.into_inner();

        if let Some(section) = ast.storages {
            let section = section.into_inner();
            self.compile_storages(section.storages, table, errors);
        }

        if let Some(section) = ast.rate_limits {
            let section = section.into_inner();
            self.compile_rate_limits(section.policies, table, errors);
        }
        if let Some(section) = ast.plugins {
            let section = section.into_inner();
            self.compile_plugins(section.plugins, table, errors);
        }
        if let Some(section) = ast.key_profiles {
            let section = section.into_inner();
            self.compile_key_profiles(section, table, errors);
        }
    }

    pub fn compile_modifiers(
        &self,
        section: ModifiersSectionDefData,
        table: &mut DefinitionsTable,
        errors: &mut ConfigError,
    ) {
        for ns in section.namespaces {
            self.compile_modifier_namespace(table, errors, ns, "");
        }

        for chain_def in section.chains {
            let (data, ctx) = chain_def.into_parts();

            if table.get_chains().contains_key(&data.name) {
                errors.push_report(
                    ctx.err_name(format!("Duplicate chain-filters name: '{}'", data.name)),
                    &ctx.ctx,
                );
                continue;
            }

            let mut items = vec![];
            for item in data.filters {
                let (item, _) = item.into_parts();
                match item {
                    ChainItemDefData::Filter(def) => {
                        items.push(ChainItem::Filter(ConfiguredFilter {
                            args: def
                                .params
                                .into_iter()
                                .map(|(k, v)| (k, v.value().into()))
                                .collect::<BTreeMap<String, Value>>(),
                            name: def.name,
                        }));
                    }
                    ChainItemDefData::RateLimit(def) => {
                        let (rl_data, ctx) = def.into_parts();
                        match rl_data {
                            RateLimitDefData::Reference(name) => {
                                if let Some(policy) = table.get_rate_limit(&name) {
                                    items.push(ChainItem::RateLimiter(policy));
                                } else {
                                    errors.push_report(
                                        ctx.err_reference_ref(format!(
                                            "Rate limit policy '{}' not found",
                                            name
                                        )),
                                        &ctx.ctx,
                                    );
                                }
                            }
                            RateLimitDefData::Inline {
                                algorithm,
                                storage_key,
                                key_template,
                                transforms,
                                burst,
                                raw_rate,
                            } => {
                                let (key_template, _) = key_template.into_parts();

                                items.push(ChainItem::RateLimiter(RateLimitPolicy {
                                    name: format!("__anon_rl_{}_{}", data.name, items.len()),
                                    algorithm,
                                    burst,
                                    key_template: key_template.template,
                                    rate_req_per_sec: raw_rate,
                                    storage_key,
                                    transforms: transforms.map(|v| v.into()).unwrap_or_default(),
                                }))
                            }
                        }
                    }
                }
            }
            table.insert_chain(data.name, FilterChain { items });
        }
    }

    fn compile_modifier_namespace(
        &self,
        table: &mut DefinitionsTable,
        errors: &mut ConfigError,
        ns: ModifiersNamespaceDef,
        prefix: &str,
    ) {
        let (data, _ctx) = ns.into_parts();
        let new_prefix = if prefix.is_empty() {
            data.name
        } else {
            format!("{}.{}", prefix, data.name)
        };

        for sub_ns in data.namespaces {
            self.compile_modifier_namespace(table, errors, sub_ns, &new_prefix);
        }

        for def in data.defs {
            let (d, ctx) = def.into_parts();
            let fqdn_str = format!("{}.{}", new_prefix, d.name);

            match fqdn_str.parse::<fqdn::FQDN>() {
                Ok(fqdn) => {
                    if !table.insert_filter(fqdn.clone()) {
                        errors.push_report(
                            ctx.err_self(format!("Duplicate filter definition: '{}'", fqdn)),
                            &ctx.ctx,
                        );
                    }
                }
                Err(e) => {
                    errors.push_report(
                        ctx.err_self(format!("Invalid FQDN '{}': {}", fqdn_str, e)),
                        &ctx.ctx,
                    );
                }
            }
        }
    }

    fn compile_storages(
        &self,
        items: Vec<StorageDef>,
        table: &mut DefinitionsTable,
        errors: &mut ConfigError,
    ) {
        for storage_def in items {
            let (data, ctx) = storage_def.into_parts();

            let (name, config) = match data {
                StorageDefData::Redis(inner) => {
                    let inner = inner.into_inner();
                    (
                        inner.name,
                        StorageConfig::Redis {
                            addresses: inner.addresses,
                            password: inner.password,
                            timeout: inner.timeout.map(|d| d.into()),
                        },
                    )
                }
                StorageDefData::Memory(inner) => {
                    let inner = inner.into_inner();
                    (
                        inner.name,
                        StorageConfig::Memory {
                            max_keys: inner.max_keys.unwrap_or(10000),
                            cleanup_interval: inner
                                .cleanup_interval
                                .map(|d| d.into())
                                .unwrap_or(StdDuration::from_secs(60)),
                        },
                    )
                }
            };

            if table.insert_storage(name.clone(), config).is_some() {
                errors.push_report(
                    ctx.err_self(format!("Duplicate storage definition: '{}'", name)),
                    &ctx.ctx,
                );
            }
        }
    }

    fn compile_rate_limits(
        &self,
        items: Vec<RateLimitPolicyDef>,
        table: &mut DefinitionsTable,
        errors: &mut ConfigError,
    ) {
        for policy_def in items {
            let (data, ctx) = policy_def.into_parts();

            let policy = RateLimitPolicy {
                name: data.name.clone(),
                algorithm: data.algorithm.unwrap_or_else(|| "token_bucket".to_string()),
                storage_key: data.storage_ref.unwrap_or_default(),
                key_template: data.key,
                rate_req_per_sec: data.rate.as_secs_f64(),
                burst: data.burst.unwrap_or(1),
                transforms: data.transforms.map(|v| v.into()).unwrap_or_default(),
            };

            if table
                .insert_rate_limit(policy.name.clone(), policy)
                .is_some()
            {
                errors.push_report(
                    ctx.err_self(format!("Duplicate rate-limit policy: '{}'", data.name)),
                    &ctx.ctx,
                );
            }
        }
    }

    fn compile_plugins(
        &self,
        items: Vec<PluginDef>,
        table: &mut DefinitionsTable,
        errors: &mut ConfigError,
    ) {
        for plugin_node in items {
            let (data, ctx) = plugin_node.into_parts();

            if table.get_plugins().contains_key(&data.name) {
                errors.push_report(
                    ctx.err_self(format!("Duplicate plugin definition: '{}'", data.name)),
                    &ctx.ctx,
                );
                continue;
            }

            let source = match (data.load.path, data.load.url) {
                (Some(path), None) => RuntimePluginSource::File(path),
                (None, Some(url)) => RuntimePluginSource::Url(url),
                _ => {
                    errors.push_report(
                        ctx.err_load(format!(
                            "Plugin '{}' must have exactly one load source (path OR url)",
                            data.name
                        )),
                        &ctx.ctx,
                    );
                    continue;
                }
            };

            table.insert_plugin(
                data.name.clone(),
                PluginDefinition {
                    name: data.name,
                    source,
                },
            );
        }
    }

    fn compile_key_profiles(
        &self,
        section: KeyProfilesSectionDefData,
        table: &mut DefinitionsTable,
        errors: &mut ConfigError,
    ) {
        for ns in section.namespaces {
            self.compile_key_profile_namespace(table, errors, ns, "");
        }
        for tmpl in section.templates {
            self.compile_key_profile_template(table, errors, tmpl, "");
        }
    }

    fn compile_key_profile_namespace(
        &self,
        table: &mut DefinitionsTable,
        errors: &mut ConfigError,
        ns: KeyProfileNamespaceDef,
        prefix: &str,
    ) {
        let (data, _ctx) = ns.into_parts();
        let new_prefix = if prefix.is_empty() {
            data.name
        } else {
            format!("{}.{}", prefix, data.name)
        };

        for sub_ns in data.namespaces {
            self.compile_key_profile_namespace(table, errors, sub_ns, &new_prefix);
        }
        for tmpl in data.templates {
            self.compile_key_profile_template(table, errors, tmpl, &new_prefix);
        }
    }

    fn compile_key_profile_template(
        &self,
        table: &mut DefinitionsTable,
        errors: &mut ConfigError,
        tmpl: KeyProfileTemplateDef,
        prefix: &str,
    ) {
        let (data, ctx) = tmpl.into_parts();

        let full_name = if prefix.is_empty() {
            data.name.clone()
        } else {
            format!("{}.{}", prefix, data.name)
        };

        if table.get_key_templates().contains_key(&full_name) {
            errors.push_report(
                ctx.err_self(format!("Duplicate key template: '{}'", full_name)),
                &ctx.ctx,
            );
        }

        let (key_def, _key_ctx) = data.key.into_parts();
        let source = key_def.template;
        let fallback = key_def.fallback;

        // =====================================================================
        // 2. HASH ALGORITHM
        // =====================================================================
        let (alg_def, alg_ctx) = data.algorithm.into_parts();

        let runtime_alg_struct = HashAlgorithm {
            name: alg_def.name,
            seed: alg_def.seed,
        };

        let algorithm = match parse_hasher(&runtime_alg_struct) {
            Ok(op) => op,
            Err(e) => {
                errors.push_report(alg_ctx.err_self(e), &alg_ctx.ctx);
                return;
            }
        };

        // =====================================================================
        // 3. TRANSFORMS
        // =====================================================================
        let mut transforms = Vec::new();

        if let Some(trans_order_def) = data.transforms {
            transforms = trans_order_def.into();
        }

        // =====================================================================
        // 4. INSERT
        // =====================================================================
        let config = crate::common_types::definitions::BalancerConfig {
            source,
            fallback,
            algorithm,
            transforms,
        };

        table.insert_key_profile(full_name, config);
    }
}
