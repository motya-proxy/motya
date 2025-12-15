use std::{path::PathBuf, str::FromStr, time::Duration};

use fqdn::FQDN;
use humantime::parse_duration;
use miette::Result;
use motya_macro::validate;

use crate::{
    block_parser,
    common_types::{
        definitions::{PluginDefinition, PluginSource},
        definitions_table::DefinitionsTable,
        rate_limiter::{RateLimitPolicy, StorageConfig},
        section_parser::SectionParser,
    },
    kdl::{
        chain_parser::ChainParser,
        key_profile_parser::KeyProfileParser,
        key_template::KeyTemplateParser,
        parser::{
            ctx::ParseContext,
            ensures::Rule,
            utils::{OptionTypedValueExt, PrimitiveType},
        },
        rate_limit::RateLimitPolicyParser,
        transforms_order::TransformsOrderParser,
    },
};

pub struct DefinitionsSection;

impl SectionParser<ParseContext<'_>, DefinitionsTable> for DefinitionsSection {
    #[validate(ensure_node_name = "definitions")]
    fn parse_node(&self, ctx: ParseContext) -> miette::Result<DefinitionsTable> {
        self.extract_definitions(ctx)
    }
}

impl DefinitionsSection {
    fn extract_definitions(&self, ctx: ParseContext) -> miette::Result<DefinitionsTable> {
        let mut table = DefinitionsTable::default();

        block_parser!(
            ctx,
            optional("modifiers") => |ctx| self.parse_modifiers(ctx, &mut table),
            optional("plugins") => |ctx| self.parse_plugins(ctx, &mut table),
            optional("key-profiles") => |ctx| self.parse_key_profiles(ctx, &mut table),
            optional("storages") => |ctx| self.parse_storages(ctx, &mut table),
            optional("rate-limits") => |ctx| self.parse_rate_limits(ctx, &mut table)
        );

        Ok(table)
    }

    fn parse_storages(
        &self,
        ctx: ParseContext<'_>,
        table: &mut DefinitionsTable,
    ) -> miette::Result<()> {
        block_parser!(ctx,
            repeated("storage") => |ctx| self.parse_single_storage(ctx, table)
        );
        Ok(())
    }

    fn parse_single_storage(
        &self,
        ctx: ParseContext<'_>,
        table: &mut DefinitionsTable,
    ) -> miette::Result<()> {
        ctx.validate(&[
            Rule::ReqChildren,
            Rule::ExactArgs(1),
            Rule::OnlyKeysTyped(&[("type", PrimitiveType::String)]),
        ])?;

        let name = ctx.first()?.as_str()?;
        let storage_type = ctx.prop("type")?.as_str()?;

        let block_ctx = ctx.enter_block()?;

        let config = match storage_type.as_str() {
            "memory" => self.parse_memory_storage(block_ctx)?,
            "redis" => self.parse_redis_storage(block_ctx)?,
            unknown => {
                return Err(ctx.error(format!(
                    "Unknown storage type: '{}'. Supported: 'memory', 'redis'",
                    unknown
                )))
            }
        };

        if table.insert_storage(name.to_string(), config).is_some() {
            return Err(ctx.error(format!("Duplicate storage definition: '{}'", name)));
        }

        Ok(())
    }

    fn parse_memory_storage(&self, ctx: ParseContext<'_>) -> miette::Result<StorageConfig> {
        block_parser!(ctx,
            max_keys: optional("max-keys") => |ctx| {
                ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1)])?;

                ctx.first()?.as_usize()
            },
            cleanup_interval: optional("cleanup-interval") => |ctx| {
                ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1)])?;
                let s = ctx.first()?.as_str()?;

                parse_duration(&s).map_err(|e| ctx.error(format!("Invalid duration: {}", e)))
            }
        );

        Ok(StorageConfig::Memory {
            max_keys: max_keys.unwrap_or(10000),
            cleanup_interval: cleanup_interval.unwrap_or(Duration::from_secs(60)),
        })
    }

    fn parse_redis_storage(&self, ctx: ParseContext<'_>) -> miette::Result<StorageConfig> {
        block_parser!(ctx,
            addresses: required("addresses") => |ctx| {
                ctx.validate(&[Rule::NoChildren, Rule::AtLeastArgs(1)])?;

                ctx.args_typed()?.iter().map(|v| v.as_str()).collect::<Result<Vec<_>>>()
            },
            password: optional("password") => |ctx| {
                ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1)])?;

                Ok(ctx.first()?.as_str()?.to_string())
            },
            timeout: optional("timeout") => |ctx| {
                ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1)])?;
                let s = ctx.first()?.as_str()?;

                parse_duration(&s).map_err(|e| ctx.error(format!("Invalid duration: {}", e)))
            }
        );

        Ok(StorageConfig::Redis {
            addresses,
            password,
            timeout,
        })
    }

    fn parse_rate_limits(
        &self,
        ctx: ParseContext<'_>,
        table: &mut DefinitionsTable,
    ) -> miette::Result<()> {
        block_parser!(ctx,
            repeated("policy") => |ctx| self.parse_single_policy(ctx, table)
        );
        Ok(())
    }

    fn parse_single_policy(
        &self,
        ctx: ParseContext<'_>,
        table: &mut DefinitionsTable,
    ) -> miette::Result<()> {
        let policy = RateLimitPolicyParser.parse(ctx.clone(), None, None)?;
        let name = policy.name.clone();

        if table.insert_rate_limit(name.clone(), policy).is_some() {
            return Err(ctx.error(format!("Duplicate rate-limit policy: '{}'", name)));
        }

        Ok(())
    }

    fn parse_key_profiles(
        &self,
        ctx: ParseContext<'_>,
        table: &mut DefinitionsTable,
    ) -> miette::Result<()> {
        block_parser!(ctx,
            repeated("namespace") => |ctx| self.parse_key_profile_namespace(ctx, table, ""),
            repeated("template") => |ctx| self.parse_key_profile_template(ctx, table, "")
        );

        Ok(())
    }

    fn parse_key_profile_namespace(
        &self,
        ctx: ParseContext<'_>,
        table: &mut DefinitionsTable,
        prefix: &str,
    ) -> miette::Result<()> {
        ctx.validate(&[Rule::ReqChildren, Rule::ExactArgs(1), Rule::OnlyKeys(&[])])?;

        let sub_name = ctx.first()?.as_str()?;

        let new_prefix = if prefix.is_empty() {
            sub_name
        } else {
            format!("{}.{}", prefix, sub_name)
        };

        block_parser!(ctx.enter_block()?,
            repeated("namespace") => |ctx| self.parse_key_profile_namespace(ctx, table, &new_prefix),
            repeated("template") => |ctx| self.parse_key_profile_template(ctx, table, &new_prefix)
        );

        Ok(())
    }

    fn parse_key_profile_template(
        &self,
        ctx: ParseContext<'_>,
        table: &mut DefinitionsTable,
        namespace_prefix: &str,
    ) -> miette::Result<()> {
        ctx.validate(&[Rule::ReqChildren, Rule::ExactArgs(1), Rule::OnlyKeys(&[])])?;

        let template_name = ctx.first()?.as_str()?;

        let full_name = if namespace_prefix.is_empty() {
            template_name
        } else {
            format!("{}.{}", namespace_prefix, template_name)
        };

        if table.get_key_templates().contains_key(&full_name) {
            return Err(ctx.error(format!("Duplicate key template: '{}'", full_name)));
        }

        let template_config = KeyProfileParser.parse(ctx.enter_block()?)?;

        table.insert_key_profile(full_name, template_config);

        Ok(())
    }

    fn parse_modifiers(
        &self,
        ctx: ParseContext<'_>,
        table: &mut DefinitionsTable,
    ) -> miette::Result<()> {
        block_parser!(ctx,
            repeated("namespace") => |ctx| self.parse_namespace_recursive(ctx, table, ""),
            repeated("chain-filters") => |ctx| self.parse_chain(ctx, table)
        );
        Ok(())
    }

    fn parse_plugins(
        &self,
        ctx: ParseContext<'_>,
        table: &mut DefinitionsTable,
    ) -> miette::Result<()> {
        block_parser!(ctx,
            repeated("plugin") => |ctx| {
                let plugin_def = self.parse_single_plugin(ctx.clone())?;

                if table.get_plugins().contains_key(&plugin_def.name) {
                    return Err(ctx.error(format!("Duplicate plugin definition: '{}'", plugin_def.name)));
                }

                table.insert_plugin(plugin_def.name.clone(), plugin_def);
                Ok(())
            }
        );
        Ok(())
    }

    fn parse_single_plugin(&self, ctx: ParseContext<'_>) -> miette::Result<PluginDefinition> {
        ctx.validate(&[Rule::ReqChildren, Rule::NoArgs])?;

        let block_ctx = ctx.enter_block()?;

        block_parser!(block_ctx,
            name: required("name") => |ctx| {
                ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1)])?;
                ctx.first()?.parse_as::<FQDN>()
            },

            source: required("load") => |ctx| {
                ctx.validate(&[
                    Rule::NoChildren,
                    Rule::NoPositionalArgs,
                    Rule::OnlyKeysTyped(&[
                        ("path", PrimitiveType::String),
                        ("url", PrimitiveType::String)
                    ])
                ])?;

                let [path_opt, url_opt] = ctx.props(["path", "url"])?;

                match (path_opt.as_str()?, url_opt.as_str()?) {
                    (Some(path), None) => Ok(PluginSource::File(PathBuf::from(path))),
                    (None, Some(url)) => Ok(PluginSource::Url(url)),
                    (Some(_), Some(_)) => Err(ctx.error("Duplicate source: provide either 'path' or 'url', not both")),
                    (None, None) => Err(ctx.error("'load' must provide either 'path' or 'url'")),
                }
            }
        );

        Ok(PluginDefinition { name, source })
    }

    fn parse_namespace_recursive(
        &self,
        ctx: ParseContext<'_>,
        table: &mut DefinitionsTable,
        prefix: &str,
    ) -> miette::Result<()> {
        ctx.validate(&[Rule::ReqChildren, Rule::ExactArgs(1), Rule::OnlyKeys(&[])])?;

        let sub_name = ctx.arg(0)?.as_str()?;
        let new_prefix = if prefix.is_empty() {
            sub_name
        } else {
            format!("{}.{}", prefix, sub_name)
        };

        let block_ctx = ctx.enter_block()?;

        block_parser!(block_ctx,
            repeated("namespace") => |ctx| self.parse_namespace_recursive(ctx, table, &new_prefix),

            repeated("def") => |ctx| {
                ctx.validate(&[
                    Rule::NoChildren,
                    Rule::NoPositionalArgs,
                    Rule::OnlyKeysTyped(&[("name", PrimitiveType::String)])
                ])?;

                let def_name = ctx.prop("name")?.as_str()?;

                let fqdn_str = format!("{}.{}", new_prefix, def_name);
                let fqdn = FQDN::from_str(&fqdn_str).map_err(|e| {
                    ctx.error(format!("Resulting FQDN '{}' is invalid: {}", fqdn_str, e))
                })?;

                if !table.insert_filter(fqdn.clone()) {
                    return Err(ctx.error(format!("Duplicate filter definition: '{}'", fqdn)));
                }
                Ok(())
            }
        );

        Ok(())
    }

    fn parse_chain(
        &self,
        ctx: ParseContext<'_>,
        table: &mut DefinitionsTable,
    ) -> miette::Result<()> {
        ctx.validate(&[Rule::ReqChildren, Rule::ExactArgs(1), Rule::OnlyKeys(&[])])?;

        let chain_name = ctx.first()?.as_str()?;

        if table.get_chains().contains_key(&chain_name) {
            return Err(ctx.error(format!("Duplicate chain-filters name: '{}'", chain_name)));
        }

        let chain = ChainParser.parse(ctx.enter_block()?, None, None)?;

        table.insert_chain(chain_name, chain);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use kdl::KdlDocument;

    use super::*;
    use crate::{
        assert_err_contains,
        common_types::{
            connectors::{Connectors, ConnectorsLeaf, RouteMatcher, UpstreamConfig, ALPN},
            definitions::{ChainItem, Modificator},
            key_template::KeyTemplate,
        },
        internal::{DiscoveryKind, HealthCheckKind, SelectionKind},
        kdl::{
            connectors::ConnectorsSection,
            definitions::DefinitionsSection,
            parser::{block::BlockParser, ctx::Current},
        },
    };

    fn parse_config(input: &str) -> miette::Result<Connectors> {
        let doc: KdlDocument = input.parse().unwrap();
        let table = DefinitionsTable::default();

        let ctx = ParseContext::new(&doc, Current::Document(&doc), "test");
        let mut block = BlockParser::new(ctx)?;

        block.required("connectors", |ctx| {
            ConnectorsSection::new(&table).parse_node(ctx)
        })
    }

    fn parse_config_with_defs(defs_input: &str, conn_input: &str) -> miette::Result<Connectors> {
        let defs_doc: KdlDocument = defs_input.parse().unwrap();
        let defs_ctx = ParseContext::new(&defs_doc, Current::Document(&defs_doc), "test");
        let mut defs_block = BlockParser::new(defs_ctx)?;

        let table = defs_block.required("definitions", |ctx| DefinitionsSection.parse_node(ctx))?;

        let conn_doc: KdlDocument = conn_input.parse().unwrap();
        let conn_ctx = ParseContext::new(&conn_doc, Current::Document(&conn_doc), "test");
        let mut conn_block = BlockParser::new(conn_ctx)?;

        conn_block.required("connectors", |ctx| {
            ConnectorsSection::new(&table).parse_node(ctx)
        })
    }

    const DEFS_RATE_LIMIT: &str = r#"
    definitions {
        storages {
            storage "my-redis" type="redis" {
                addresses "127.0.0.1:6379"
                timeout "100ms"
            }
            storage "local" type="memory" {
                max-keys 5000
            }
        }
        rate-limits {
            policy "api-limiter" {
                algorithm "token-bucket"
                storage "my-redis"
                key "${client-ip}"
                rate "100/m"
                burst 20
            }
        }
    }
    "#;

    #[test]
    fn test_parse_rate_limits() {
        let doc: KdlDocument = DEFS_RATE_LIMIT.parse().unwrap();
        let ctx = ParseContext::new(&doc, Current::Document(&doc), "test");
        let mut defs_block = BlockParser::new(ctx).unwrap();

        let defs = defs_block
            .required("definitions", |ctx| DefinitionsSection.parse_node(ctx))
            .unwrap();

        assert!(defs.has_rate_storage("my-redis"));
        assert!(defs.has_rate_storage("local"));

        let policy = defs
            .get_rate_limit("api-limiter")
            .expect("Policy not found");
        assert_eq!(policy.storage_key, "my-redis");
        assert_eq!(policy.burst, 20);
        assert!((policy.rate_req_per_sec - (100.0 / 60.0)).abs() < f64::EPSILON);
    }

    const LOAD_BALANCE_BASIC: &str = r#"
    connectors {
        load-balance {
            selection "RoundRobin"
            health-check "None"
            discovery "Static"
        }
        proxy {
            server "127.0.0.1:8080"
        }
    }
    "#;

    #[test]
    fn test_load_balance_basic() {
        let connectors = parse_config(LOAD_BALANCE_BASIC).expect("Parsing failed");

        assert_eq!(connectors.upstreams.len(), 1);
        let upstream = &connectors.upstreams[0];
        let lb_options = upstream.lb_options.clone().unwrap();
        assert_eq!(lb_options.selection, SelectionKind::RoundRobin);
        assert_eq!(lb_options.health_checks, HealthCheckKind::None);
        assert_eq!(lb_options.discovery, DiscoveryKind::Static);
        assert!(lb_options.template.is_none());
    }

    const LOAD_BALANCE_ALL_SELECTION_TYPES: &str = r#"
    connectors {
        load-balance {
            selection "Random"
        }
        proxy {
            server "127.0.0.1:8080"
        }
    }
    "#;

    #[test]
    fn test_load_balance_random_selection() {
        let connectors = parse_config(LOAD_BALANCE_ALL_SELECTION_TYPES).expect("Parsing failed");

        let upstream = &connectors.upstreams[0];
        let lb_options = upstream.lb_options.clone().unwrap();
        assert_eq!(lb_options.selection, SelectionKind::Random);
    }

    const LOAD_BALANCE_FNV_HASH: &str = r#"
    connectors {
        load-balance {
            selection "FNV" use-key-profile="ip-profile"
        }
        proxy { 
            server "127.0.0.1:8080"
        }
    }
    "#;

    #[test]
    fn test_load_balance_fnv_hash_with_key_profile() {
        // Should fail because "ip-profile" is not defined in the empty table
        let result = parse_config(LOAD_BALANCE_FNV_HASH);
        assert!(result.is_err());
    }

    const DEFS_KEY_PROFILE: &str = r#"
    definitions {
        key-profiles {
            template "ip-profile" {
                key "amogus"
            }
        }
    }
    "#;

    const LOAD_BALANCE_WITH_KEY_PROFILE: &str = r#"
    connectors {
        load-balance {
            selection "FNV" use-key-profile="ip-profile"
        }
        proxy {
            server "127.0.0.1:8080"
        }
    }
    "#;

    #[test]
    fn test_load_balance_with_defined_key_profile() {
        let connectors = parse_config_with_defs(DEFS_KEY_PROFILE, LOAD_BALANCE_WITH_KEY_PROFILE)
            .expect("Parsing failed");

        let upstream = &connectors.upstreams[0];
        let lb_options = upstream.lb_options.clone().unwrap();
        assert_eq!(lb_options.selection, SelectionKind::FvnHash);
        assert!(lb_options.template.is_some());

        let template = lb_options.template.as_ref().unwrap();
        assert_eq!(
            template.source,
            "amogus".to_string().parse::<KeyTemplate>().unwrap()
        );
    }

    const LOAD_BALANCE_HASH_WITHOUT_KEY_SOURCE: &str = r#"
    connectors {
        load-balance {
            selection "Ketama"
        }
        proxy {
            server "127.0.0.1:8080"
        }
    }
    "#;

    #[test]
    fn test_error_hash_selection_without_key_source() {
        let result = parse_config(LOAD_BALANCE_HASH_WITHOUT_KEY_SOURCE);
        assert!(result.is_err());

        let err_msg = result.unwrap_err().help().unwrap().to_string();
        assert!(err_msg.contains("requires a key source"));
    }

    const LOAD_BALANCE_DUPLICATE: &str = r#"
    connectors {
        load-balance {
            selection "RoundRobin"
        }
        load-balance {
            selection "Random"
        }
        proxy {
            server "127.0.0.1:8081"
        }
    }
    "#;

    #[test]
    fn test_error_duplicate_load_balance() {
        let result = parse_config(LOAD_BALANCE_DUPLICATE);
        assert!(result.is_err());

        let err_msg = result.unwrap_err().help().unwrap().to_string();
        assert_err_contains!(err_msg, "Directive 'load-balance' cannot be repeated");
    }

    const MULTI_SERVER_PROXY_CONFIG: &str = r#"
    connectors {
        proxy { 
            tls-sni "onevariable.com" 
            proto "h2-or-h1"
            server "91.107.223.4:443" 
            server "91.107.223.5:443" 
            server "91.107.223.6:443"
        }
    }
    "#;

    #[test]
    fn test_multi_server_proxy_parsing() {
        let connectors = parse_config(MULTI_SERVER_PROXY_CONFIG).expect("Parsing failed");

        assert_eq!(
            connectors.upstreams.len(),
            1,
            "Should have one UpstreamConfig"
        );

        let upstream_config = &connectors.upstreams[0];

        let UpstreamConfig::MultiServer(upstream) = &upstream_config.upstream else {
            panic!(
                "Expected MultiServer upstream, got {:?}",
                upstream_config.upstream
            );
        };

        assert_eq!(
            upstream.servers.len(),
            3,
            "Should parse 3 server directives"
        );
        assert_eq!(
            upstream.tls_sni.as_deref(),
            Some("onevariable.com"),
            "Should parse tls-sni"
        );
        assert_eq!(upstream.alpn, ALPN::H2H1, "Should parse proto h2-or-h1");
        assert_eq!(upstream.prefix_path, "/", "Should have default prefix path");
        assert_eq!(
            upstream.matcher,
            RouteMatcher::Exact,
            "Should have default matcher"
        );

        assert_eq!(
            upstream.servers[0].address,
            "91.107.223.4:443".parse().unwrap()
        );
        assert_eq!(
            upstream.servers[2].address,
            "91.107.223.6:443".parse().unwrap()
        );
    }

    const SINGLE_SERVER_BLOCK_PROXY_CONFIG: &str = r#"
    connectors {
        proxy {
            server "127.0.0.1:8080"
        }
    }
    "#;

    #[test]
    fn test_single_server_block_proxy_parsing() {
        let connectors = parse_config(SINGLE_SERVER_BLOCK_PROXY_CONFIG).expect("Parsing failed");

        let UpstreamConfig::MultiServer(upstream) = &connectors.upstreams[0].upstream else {
            panic!("Expected MultiServer upstream (even for a single server in block)");
        };

        assert_eq!(upstream.servers.len(), 1, "Should parse 1 server directive");
        assert!(
            upstream.tls_sni.is_none(),
            "tls-sni should be None by default"
        );
        assert_eq!(
            upstream.alpn,
            ALPN::H1,
            "ALPN should be H1 by default (no sni/proto)"
        );
        assert_eq!(
            upstream.servers[0].address,
            "127.0.0.1:8080".parse().unwrap()
        );
    }

    const ERROR_DUPLICATE_PROTO: &str = r#"
    connectors {
        proxy {
            proto "h2-or-h1"
            proto "h1-only"
            server "127.0.0.1:8080"
        }
    }
    "#;

    #[test]
    fn test_error_on_duplicate_options() {
        let result = parse_config(ERROR_DUPLICATE_PROTO);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().help().unwrap().to_string();
        crate::assert_err_contains!(err_msg, "Directive 'proto' cannot be repeated");
    }

    const ERROR_NO_SERVERS: &str = r#"
    connectors {
        proxy {
            tls-sni "a.com"
        }
    }
    "#;

    #[test]
    fn test_error_on_no_servers() {
        let result = parse_config(ERROR_NO_SERVERS);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().help().unwrap().to_string();
        crate::assert_err_contains!(
            err_msg,
            "Missing required directive 'server' (at least one expected)"
        );
    }

    const ERROR_BLOCK_WITH_ARG: &str = r#"
    connectors {
        proxy "http://invalid.com" {
            server "127.0.0.1:8080"
        }
    }
    "#;

    #[test]
    fn test_error_block_with_top_arg() {
        let result = parse_config(ERROR_BLOCK_WITH_ARG);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().help().unwrap().to_string();
        crate::assert_err_contains!(err_msg, "Directive 'proxy' cannot have arguments");
    }

    const INVALID_STRICT_NESTING: &str = r#"
    connectors {
        section "/api" as="exact" {
            // This should fail because parent is exact
            section "/v1" {
                return code=200 response="fail"
            }
        }
    }
    "#;

    #[test]
    fn test_strict_section_cannot_have_children() {
        let result = parse_config(INVALID_STRICT_NESTING);
        let err_msg = result.err().unwrap().help().unwrap().to_string();
        crate::assert_err_contains!(
            err_msg,
            "A section with 'exact' routing mode cannot contain nested sections"
        );
    }

    const VALID_STRICT_CONFIG: &str = r#"
    connectors {
        section "/api" as="exact" {
            // Non-section children are allowed
            return code=200 response="OK"
        }
    }
    "#;

    #[test]
    fn test_strict_section_allowed_directives() {
        let result = parse_config(VALID_STRICT_CONFIG);
        assert!(result.is_ok());
    }

    const DEFS_ARGS: &str = r#"
    definitions {
        modifiers {
            chain-filters "defined_with_args" {
                filter name="set-header" key="X-Region" value="EU"
                filter name="log-request"
            }
        }
    }
    "#;

    const ARGS_PARSING_CONN: &str = r#"
    connectors {
        use-chain {
            filter name="rate-limit" rps="100" burst="20"
        }
        use-chain "defined_with_args"
        proxy "http://127.0.0.1:8080"
    }
    "#;

    #[test]
    fn test_filter_args_parsing() {
        let connectors =
            parse_config_with_defs(DEFS_ARGS, ARGS_PARSING_CONN).expect("Parsing failed");
        let upstream = &connectors.upstreams[0];

        assert_eq!(upstream.chains.len(), 2);

        match &upstream.chains[0] {
            Modificator::Chain(named_chain) => {
                assert!(named_chain.name.contains("__anon_"), "Should be anonymous");
                let ChainItem::Filter(filter) = &named_chain.chain.items[0] else {
                    unreachable!()
                };

                assert_eq!(filter.name, "rate-limit");
                assert_eq!(filter.args.len(), 2);
                assert_eq!(filter.args.get("rps").map(|s| s.as_str()), Some("100"));
                assert_eq!(filter.args.get("burst").map(|s| s.as_str()), Some("20"));
            }
        }

        match &upstream.chains[1] {
            Modificator::Chain(named_chain) => {
                assert_eq!(named_chain.name, "defined_with_args");
                assert_eq!(named_chain.chain.items.len(), 2);

                let ChainItem::Filter(filter1) = &named_chain.chain.items[0] else {
                    unreachable!()
                };
                assert_eq!(filter1.name, "set-header");
                assert_eq!(filter1.args.len(), 2);
                assert_eq!(
                    filter1.args.get("key").map(|s| s.as_str()),
                    Some("X-Region")
                );
                assert_eq!(filter1.args.get("value").map(|s| s.as_str()), Some("EU"));

                let ChainItem::Filter(filter2) = &named_chain.chain.items[1] else {
                    unreachable!()
                };
                assert_eq!(filter2.name, "log-request");
                assert!(filter2.args.is_empty(), "Args should be empty");
            }
        }
    }

    const ANONYMOUS_CHAIN_TEST: &str = r#"
    connectors {
        section "/anon" {
            use-chain {
                filter name="logger" level="debug"
            }
            proxy "http://127.0.0.1:8080"
        }
    }
    "#;

    #[test]
    fn test_anonymous_chain() {
        let connectors = parse_config(ANONYMOUS_CHAIN_TEST).expect("Parsing failed");
        let upstream = &connectors.upstreams[0];

        match &upstream.chains[0] {
            Modificator::Chain(named_chain) => {
                assert_eq!(named_chain.chain.items.len(), 1);
                let ChainItem::Filter(filter) = &named_chain.chain.items[0] else {
                    unreachable!()
                };
                assert_eq!(filter.name, "logger");
                let level_arg = filter.args.get("level").expect("Argument 'level' missing");
                assert_eq!(level_arg, "debug");
                assert_eq!(filter.args.len(), 1);
            }
        }
    }

    const DEFS_SIMPLE: &str = r#"
    definitions {
        modifiers {
            chain-filters "security" {
                filter name="block-ip"
                filter name="auth-check"
            }
        }
    }
    "#;

    const SIMPLE_CHAIN_CONN: &str = r#"
    connectors {
        use-chain "security"
        proxy "http://127.0.0.1:8080"
    }
    "#;

    #[test]
    fn test_use_chain_simple() {
        let connectors =
            parse_config_with_defs(DEFS_SIMPLE, SIMPLE_CHAIN_CONN).expect("Parsing failed");
        let upstream = &connectors.upstreams[0];

        assert_eq!(upstream.chains.len(), 1, "Should have 1 rule (the chain)");

        match &upstream.chains[0] {
            Modificator::Chain(named_chain) => {
                assert_eq!(named_chain.chain.items.len(), 2);
                let ChainItem::Filter(filter1) = &named_chain.chain.items[0] else {
                    unreachable!()
                };
                assert_eq!(filter1.name, "block-ip");
                let ChainItem::Filter(filter2) = &named_chain.chain.items[1] else {
                    unreachable!()
                };
                assert_eq!(filter2.name, "auth-check");
            }
        }
    }

    const DEFS_NESTED: &str = r#"
    definitions {
        modifiers {
            chain-filters "global-log" {
                filter name="logger"
            }
            chain-filters "api-protection" {
                filter name="rate-limit"
            }
        }
    }
    "#;

    const NESTED_INHERITANCE_CONN: &str = r#"
    connectors {
        use-chain "global-log"

        section "/api" {
            use-chain "api-protection"
            proxy "http://127.0.0.1:8081"
        }

        section "/public" {
            proxy "http://127.0.0.1:8082"
        }
    }
    "#;

    #[test]
    fn test_use_chain_inheritance() {
        let connectors =
            parse_config_with_defs(DEFS_NESTED, NESTED_INHERITANCE_CONN).expect("Parsing failed");

        let api_upstream = connectors.upstreams.iter()
            .find(|u| match &u.upstream {
                UpstreamConfig::Service(s) => &s.prefix_path,
                _ => panic!("not for this test")
            } == "/api")
            .expect("API upstream not found");

        assert_eq!(api_upstream.chains.len(), 2);
        let Modificator::Chain(r1) = &api_upstream.chains[0];
        let ChainItem::Filter(filter) = &r1.chain.items[0] else {
            unreachable!()
        };
        assert_eq!(filter.name, "logger");
        let Modificator::Chain(r2) = &api_upstream.chains[1];
        let ChainItem::Filter(filter) = &r2.chain.items[0] else {
            unreachable!()
        };
        assert_eq!(filter.name, "rate-limit");

        let public_upstream = connectors.upstreams.iter()
            .find(|u| match &u.upstream {
                UpstreamConfig::Service(s) => &s.prefix_path,
                _ => panic!("not for this test")
            } == "/public")
            .expect("Public upstream not found");

        assert_eq!(public_upstream.chains.len(), 1);
        let Modificator::Chain(r1) = &public_upstream.chains[0];
        let ChainItem::Filter(filter) = &r1.chain.items[0] else {
            unreachable!()
        };
        assert_eq!(filter.name, "logger");
    }

    const DEFS_MULTI: &str = r#"
    definitions {
        modifiers {
            chain-filters "a" { filter name="A" }
            chain-filters "b" { filter name="B" }
        }
    }
    "#;

    const MULTIPLE_CHAINS_CONN: &str = r#"
    connectors {
        section "/combo" {
            use-chain "a"
            use-chain "b"
            proxy "http://127.0.0.1:9000"
        }
    }
    "#;

    #[test]
    fn test_multiple_chains_same_level() {
        let connectors =
            parse_config_with_defs(DEFS_MULTI, MULTIPLE_CHAINS_CONN).expect("Parsing failed");
        let upstream = &connectors.upstreams[0];

        assert_eq!(upstream.chains.len(), 2);

        let Modificator::Chain(r) = &upstream.chains[0];
        let ChainItem::Filter(filter) = &r.chain.items[0] else {
            unreachable!()
        };
        assert_eq!(filter.name, "A");
        let Modificator::Chain(r) = &upstream.chains[1];
        let ChainItem::Filter(filter) = &r.chain.items[0] else {
            unreachable!()
        };
        assert_eq!(filter.name, "B");
    }

    const DEFS_MISSING: &str = r#"
    definitions {
        modifiers {
            chain-filters "exists" { filter name="ok" }
        }
    }
    "#;

    const MISSING_CHAIN_CONN: &str = r#"
    connectors {
        use-chain "GHOST"
        proxy "http://127.0.0.1:8080"
    }
    "#;

    #[test]
    fn test_missing_chain_error() {
        let result = parse_config_with_defs(DEFS_MISSING, MISSING_CHAIN_CONN);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().help().unwrap().to_string();
        assert!(err_msg.contains("Chain 'GHOST' not found in definitions"));
    }

    const CONNECTORS_NESTED_SECTIONS: &str = r#"
    connectors {
        proxy "http://0.0.0.0:8000"
        section "/first" as="prefix" {
            proxy "http://0.0.0.0:8000"
            section "/second" {
                proxy "http://0.0.0.0:8001/something"
            }
        }
    }
    "#;

    #[test]
    fn connectors_with_nested_sections() {
        let doc: KdlDocument = CONNECTORS_NESTED_SECTIONS.parse().unwrap();
        let table = DefinitionsTable::default();
        let mut anon = DefinitionsTable::default();
        let ctx = ParseContext::new(&doc, Current::Document(&doc), "test");
        let mut block = BlockParser::new(ctx).unwrap();

        let nodes = block
            .required("connectors", |ctx| {
                let section = ConnectorsSection::new(&table);
                section.parse_connections_node(ctx, &mut anon)
            })
            .unwrap();

        let ConnectorsLeaf::Upstream(UpstreamConfig::Service(first)) = &nodes[0] else {
            unreachable!()
        };
        let ConnectorsLeaf::Section(second_section) = &nodes[1] else {
            unreachable!()
        };
        let ConnectorsLeaf::Section(third_section) = &second_section[1] else {
            unreachable!()
        };

        assert_eq!(first.prefix_path, "/");
        assert_eq!(first.target_path, "/");

        let ConnectorsLeaf::Upstream(UpstreamConfig::Service(second)) =
            second_section.first().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(second.prefix_path, "/first");
        assert_eq!(second.target_path, "/");

        let ConnectorsLeaf::Upstream(UpstreamConfig::Service(third)) =
            third_section.first().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(third.prefix_path, "/first/second");
        assert_eq!(third.target_path, "/something");
    }

    const CONNECTORS_SECTION_WITH_PATH: &str = r#"
    connectors {
        section "/old-path" {
            proxy "http://0.0.0.0:8000/new-path"
        }
    }
    "#;

    #[test]
    fn service_section_with_path() {
        let connectors = parse_config(CONNECTORS_SECTION_WITH_PATH).unwrap();
        let upstream = &connectors.upstreams[0];
        if let UpstreamConfig::Service(s) = &upstream.upstream {
            assert_eq!(s.prefix_path, "/old-path");
            assert_eq!(s.target_path, "/new-path");
        } else {
            panic!("Expected Service upstream");
        }
    }

    const CONNECTORS_SECTION: &str = r#"
    connectors {
        section "/" {
            proxy "http://0.0.0.0:8000"
        }
    }
    "#;

    #[test]
    fn service_section() {
        let connectors = parse_config(CONNECTORS_SECTION).unwrap();
        let upstream = &connectors.upstreams[0];
        if let UpstreamConfig::Service(s) = &upstream.upstream {
            assert_eq!(
                s.peer_address,
                SocketAddr::V4("0.0.0.0:8000".parse().unwrap())
            );
        } else {
            panic!("Expected Service upstream");
        }
    }

    const CONNECTORS_PROXY: &str = r#"
    connectors {
        proxy "http://0.0.0.0:8000"
    }
    "#;

    #[test]
    fn service_proxy() {
        let connectors = parse_config(CONNECTORS_PROXY).unwrap();
        let upstream = &connectors.upstreams[0];
        if let UpstreamConfig::Service(s) = &upstream.upstream {
            assert_eq!(
                s.peer_address,
                SocketAddr::V4("0.0.0.0:8000".parse().unwrap())
            );
        } else {
            panic!("Expected Service upstream");
        }
    }

    const CONNECTORS_RETURN_SIMPLE_RESPONSE: &str = r#"
    connectors {
        return code=200 response="OK"
    }
    "#;

    #[test]
    fn service_return_simple_response() {
        let connectors = parse_config(CONNECTORS_RETURN_SIMPLE_RESPONSE).unwrap();
        let upstream = &connectors.upstreams[0];
        if let UpstreamConfig::Static(response) = &upstream.upstream {
            assert_eq!(response.http_code, http::StatusCode::OK);
            assert_eq!(response.response_body, "OK");
        } else {
            panic!("Expected Static upstream");
        }
    }
}
