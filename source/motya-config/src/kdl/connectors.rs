use std::{
    net::SocketAddr,
    sync::atomic::{AtomicUsize, Ordering},
};

use http::{uri::PathAndQuery, StatusCode, Uri};
use motya_macro::validate;

use crate::{
    block_parser,
    common_types::{
        connectors::{
            Connectors, ConnectorsLeaf, HttpPeerConfig, MultiServerUpstreamConfig, RouteMatcher,
            UpstreamConfig, UpstreamContextConfig, UpstreamServer, ALPN,
        },
        definitions::{BalancerConfig, Modificator, NamedFilterChain},
        definitions_table::DefinitionsTable,
        section_parser::SectionParser,
        simple_response_type::SimpleResponseConfig,
    },
    internal::{DiscoveryKind, HealthCheckKind, SelectionKind, UpstreamOptions},
    kdl::{
        chain_parser::ChainParser,
        key_profile_parser::KeyProfileParser,
        parser::{
            block::BlockParser,
            ctx::ParseContext,
            ensures::Rule,
            utils::{OptionTypedValueExt, PrimitiveType},
        },
    },
};

pub struct ConnectorsSection<'a> {
    table: &'a DefinitionsTable,
    anon_counter: AtomicUsize,
}

impl SectionParser<ParseContext<'_>, Connectors> for ConnectorsSection<'_> {
    #[validate(ensure_node_name = "connectors")]
    fn parse_node(&self, ctx: ParseContext) -> miette::Result<Connectors> {
        let mut anonymous_definitions = DefinitionsTable::default();

        let root_nodes = self.parse_connections_node(ctx, &mut anonymous_definitions)?;

        let upstreams = flatten_nodes(root_nodes, &[])?;

        Ok(Connectors {
            upstreams,
            anonymous_definitions,
        })
    }
}

impl<'a> ConnectorsSection<'a> {
    pub fn new(table: &'a DefinitionsTable) -> Self {
        Self {
            table,
            anon_counter: AtomicUsize::new(0),
        }
    }

    pub fn parse_connections_node(
        &self,
        ctx: ParseContext<'_>,
        anonymous_definitions: &mut DefinitionsTable,
    ) -> miette::Result<Vec<ConnectorsLeaf>> {
        self.process_nodes_recursive(
            ctx,
            anonymous_definitions,
            "/".parse().unwrap(),
            RouteMatcher::Exact,
        )
    }

    fn process_nodes_recursive(
        &self,
        ctx: ParseContext<'a>,
        anon_definitions: &mut DefinitionsTable,
        base_path: PathAndQuery,
        matcher: RouteMatcher,
    ) -> miette::Result<Vec<ConnectorsLeaf>> {
        block_parser!(
            ctx,
            leaf: optional_any(&["proxy", "return"]) => |ctx, name| match name {
                "return" => self.extract_static_response(ctx, base_path.clone()),
                "proxy" => self.extract_connector(ctx, base_path.clone(), matcher),
                _ => unreachable!("Guaranteed by BlockParser"),
            },
            lb: optional("load-balance") => |ctx| self.extract_load_balance(ctx, anon_definitions),
            chains: repeated("use-chain") => |ctx| self.extract_chain_usage(ctx, anon_definitions, base_path.clone()),
            sections: repeated("section") => |ctx| self.extract_section(ctx, anon_definitions, base_path.clone(), matcher)
        );

        let mut result = Vec::new();

        if let Some(l) = leaf {
            result.push(l);
        }
        if let Some(l) = lb {
            result.push(l);
        }

        result.extend(chains);
        result.extend(sections);

        Ok(result)
    }

    fn extract_chain_usage(
        &self,
        ctx: ParseContext<'_>,
        anonymous_definitions: &mut DefinitionsTable,
        path: PathAndQuery,
    ) -> miette::Result<ConnectorsLeaf> {
        if ctx.has_children_block()? {
            ctx.validate(&[Rule::NoArgs])?;

            let chain = ChainParser.parse(
                ctx.enter_block()?,
                Some(&self.anon_counter),
                Some(path.as_str()),
            )?;

            let id = self.anon_counter.fetch_add(1, Ordering::Relaxed);
            let path_slug = path.path().replace('/', "_");
            let generated_name = format!("__anon_{id}_{path_slug}");

            anonymous_definitions.insert_chain(generated_name.clone(), chain.clone());

            Ok(ConnectorsLeaf::Modificator(Modificator::Chain(
                NamedFilterChain {
                    chain,
                    name: generated_name,
                },
            )))
        } else {
            ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1)])?;

            let name = ctx.first()?.as_str()?;

            let chain = self
                .table
                .get_chain_by_name(&name)
                .ok_or_else(|| ctx.error(format!("Chain '{}' not found in definitions", name)))?;

            Ok(ConnectorsLeaf::Modificator(Modificator::Chain(
                NamedFilterChain {
                    chain,
                    name: name.to_string(),
                },
            )))
        }
    }

    fn extract_section(
        &self,
        ctx: ParseContext<'_>,
        anonymous_definitions: &mut DefinitionsTable,
        base_path: PathAndQuery,
        parent_matcher: RouteMatcher,
    ) -> miette::Result<ConnectorsLeaf> {
        ctx.validate(&[
            Rule::ReqChildren,
            Rule::ExactArgs(1),
            Rule::OnlyKeysTyped(&[("as", PrimitiveType::String)]),
        ])?;

        let path_segment = ctx.arg(0)?.as_str()?;
        let mode_arg = ctx.opt_prop("as")?.as_str()?;

        let next_matcher = match mode_arg.as_deref() {
            Some("prefix") => RouteMatcher::Prefix,
            Some("exact") => RouteMatcher::Exact,
            Some(other) => {
                return Err(ctx.error(format!(
                    "Unknown routing mode '{other}'. Use 'prefix' or 'exact'"
                )))
            }
            None => parent_matcher,
        };

        let children_nodes = ctx.req_nodes()?;

        if next_matcher == RouteMatcher::Exact {
            for child in &children_nodes {
                if child.name()? == "section" {
                    return Err(child.error(
                        "A section with 'exact' routing mode cannot contain nested sections.",
                    ));
                }
            }
        }

        let full_path = if base_path.path() == "/" && !path_segment.starts_with('/') {
            format!("/{}", path_segment)
        } else if base_path.path() == "/" {
            path_segment.to_string()
        } else {
            format!(
                "{}/{}",
                base_path.path().trim_end_matches('/'),
                path_segment.trim_start_matches('/')
            )
        };

        let path = full_path
            .parse()
            .map_err(|err| ctx.error(format!("Bad path: {full_path}, error: {err}")))?;

        let block_ctx = ctx.enter_block()?;

        Ok(ConnectorsLeaf::Section(self.process_nodes_recursive(
            block_ctx,
            anonymous_definitions,
            path,
            next_matcher,
        )?))
    }

    fn extract_static_response(
        &self,
        ctx: ParseContext<'a>,
        base_path: PathAndQuery,
    ) -> miette::Result<ConnectorsLeaf> {
        ctx.validate(&[
            Rule::OnlyKeysTyped(&[
                ("code", PrimitiveType::Integer),
                ("response", PrimitiveType::String),
            ]),
            Rule::NoChildren,
            Rule::NoPositionalArgs,
        ])?;

        let [code_opt, response] = ctx.props(["code", "response"])?;

        let response_body = response.as_str()?.unwrap_or_default();

        let http_code = code_opt
            .parse_as::<StatusCode>()?
            .ok_or(ctx.error("invalid http code"))?;

        Ok(ConnectorsLeaf::Upstream(UpstreamConfig::Static(
            SimpleResponseConfig {
                http_code,
                response_body,
                prefix_path: base_path,
            },
        )))
    }

    fn extract_connector(
        &self,
        ctx: ParseContext<'_>,
        base_path: PathAndQuery,
        parent_matcher: RouteMatcher,
    ) -> miette::Result<ConnectorsLeaf> {
        if ctx.has_children_block()? {
            ctx.validate(&[Rule::NoArgs])?;

            let block_ctx = ctx.enter_block()?;
            let mut block = BlockParser::new(block_ctx)?;

            let servers = block.required_repeated("server", |ctx| {
                ctx.validate(&[
                    Rule::NoChildren,
                    Rule::ExactArgs(1),
                    Rule::OnlyKeysTyped(&[("weight", PrimitiveType::Integer)]),
                ])?;

                let address = ctx.first()?.parse_as::<SocketAddr>()?;

                let weight = ctx.opt_prop("weight")?.as_usize()?.unwrap_or(1);

                Ok(UpstreamServer { address, weight })
            })?;

            let tls_sni = block.optional("tls-sni", |ctx| {
                ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1)])?;
                ctx.first()?.as_str()
            })?;

            let proto_str = block.optional("proto", |ctx| {
                ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1)])?;
                ctx.first()?.as_str()
            })?;

            block.exhaust()?;

            let (tls, sni, alpn) =
                self.resolve_proto_settings(&ctx, proto_str.as_deref(), tls_sni.as_deref())?;

            let final_sni = if sni.is_empty() { None } else { Some(sni) };

            Ok(ConnectorsLeaf::Upstream(UpstreamConfig::MultiServer(
                MultiServerUpstreamConfig {
                    servers,
                    tls_sni: final_sni,
                    alpn,
                    prefix_path: base_path,
                    target_path: PathAndQuery::from_static("/"),
                    matcher: parent_matcher,
                },
            )))
        } else {
            ctx.validate(&[
                Rule::ExactArgs(1),
                Rule::OnlyKeysTyped(&[
                    ("tls-sni", PrimitiveType::String),
                    ("proto", PrimitiveType::String),
                ]),
            ])?;

            let uri = ctx.first()?.parse_as::<Uri>()?;

            let host_addr = uri
                .authority()
                .and_then(|host| host.as_str().parse::<SocketAddr>().ok())
                .ok_or(ctx.error("Not a valid socket address"))?;

            let [sni_opt, proto_opt] = ctx.props(["tls-sni", "proto"])?;

            let (tls, sni, alpn) = self.resolve_proto_settings(
                &ctx,
                proto_opt.as_str()?.as_deref(),
                sni_opt.as_str()?.as_deref(),
            )?;

            Ok(ConnectorsLeaf::Upstream(UpstreamConfig::Service(
                HttpPeerConfig {
                    peer_address: host_addr,
                    alpn,
                    sni,
                    tls,
                    prefix_path: base_path,
                    target_path: uri.path().parse().unwrap_or(PathAndQuery::from_static("/")),
                    matcher: parent_matcher,
                },
            )))
        }
    }

    fn extract_load_balance(
        &self,
        ctx: ParseContext<'_>,
        anonymous_definitions: &mut DefinitionsTable,
    ) -> miette::Result<ConnectorsLeaf> {
        ctx.validate(&[Rule::ReqChildren, Rule::NoArgs])?;

        let block_ctx = ctx.enter_block()?;

        block_parser!(block_ctx,
            selection_data: optional("selection") => |ctx| self.parse_selection(ctx, anonymous_definitions),

            health_opt: optional("health-check") => |ctx| {
                ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1), Rule::OnlyKeys(&[])])?;

                match ctx.arg(0)?.as_str()?.as_str() {
                    "None" => Ok(HealthCheckKind::None),
                    val => Err(ctx.error(format!("Unknown health-check kind: '{val}'"))),
                }
            },

            discovery_opt: optional("discovery") => |ctx| {
                ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1), Rule::OnlyKeys(&[])])?;

                match ctx.arg(0)?.as_str()?.as_str() {
                    "Static" => Ok(DiscoveryKind::Static),
                    val => Err(ctx.error(format!("Unknown discovery kind: '{val}'"))),
                }
            }
        );

        let (selection, template) = selection_data.unwrap_or((SelectionKind::RoundRobin, None));
        let health_checks = health_opt.unwrap_or(HealthCheckKind::None);
        let discovery = discovery_opt.unwrap_or(DiscoveryKind::Static);

        Ok(ConnectorsLeaf::LoadBalance(UpstreamOptions {
            selection,
            template,
            health_checks,
            discovery,
        }))
    }

    fn parse_selection(
        &self,
        ctx: ParseContext<'_>,
        anonymous_definitions: &mut DefinitionsTable,
    ) -> miette::Result<(SelectionKind, Option<BalancerConfig>)> {
        ctx.validate(&[
            Rule::ExactArgs(1),
            Rule::OnlyKeysTyped(&[("use-key-profile", PrimitiveType::String)]),
        ])?;

        let selection_kind = ctx.first()?.parse_as::<SelectionKind>()?;

        let profile_ref = ctx.opt_prop("use-key-profile")?.as_str()?;

        let has_block = ctx.has_children_block()?;

        self.validate_selection(
            ctx,
            anonymous_definitions,
            selection_kind,
            profile_ref,
            has_block,
        )
    }

    fn resolve_proto_settings(
        &self,
        ctx: &ParseContext<'_>,
        proto: Option<&str>,
        tls_sni: Option<&str>,
    ) -> miette::Result<(bool, String, ALPN)> {
        let alpn = match proto {
            Some(p) => parse_proto_value(p).map_err(|msg| ctx.error(msg))?,
            None => None,
        };

        match (alpn, tls_sni) {
            (None, None) | (Some(ALPN::H1), None) => Ok((false, String::new(), ALPN::H1)),
            (None, Some(sni)) => Ok((true, sni.to_string(), ALPN::H2H1)),
            (Some(_), None) => Err(ctx.error("'tls-sni' is required for HTTP2 support")),
            (Some(p), Some(sni)) => Ok((true, sni.to_string(), p)),
        }
    }

    fn validate_selection(
        &self,
        ctx: ParseContext<'_>,
        anonymous_definitions: &mut DefinitionsTable,
        selection_kind: SelectionKind,
        profile_ref: Option<String>,
        has_block: bool,
    ) -> Result<(SelectionKind, Option<BalancerConfig>), miette::Error> {
        let key_source = match (profile_ref, has_block) {
            (Some(_), true) => {
                return Err(ctx.error(
                    "Cannot use 'use-key-profile' and define an inline block simultaneously",
                ));
            }
            (Some(name), false) => {
                if let Some(template) = self.table.get_key_templates().get(&name) {
                    Some(template.clone())
                } else {
                    return Err(ctx.error(format!("Key profile '{}' not found", name)));
                }
            }
            (None, true) => {
                let template = KeyProfileParser.parse(ctx.enter_block()?)?;

                let id = self.anon_counter.fetch_add(1, Ordering::Relaxed);
                let generated_name = format!("__anon_key_{id}");

                anonymous_definitions.insert_key_profile(generated_name, template.clone());
                Some(template)
            }
            (None, false) => None,
        };

        match selection_kind {
            SelectionKind::KetamaHashing | SelectionKind::FvnHash => {
                if key_source.is_none() {
                    return Err(ctx.error(format!(
                        "Selection kind '{kind}' requires a key source. Use 'use-key-profile' or provide an inline key configuration block.",
                        kind = ctx.first()?.as_str()?
                    )));
                }
            }
            _ => {}
        }

        Ok((selection_kind, key_source))
    }
}

/// Recursive function to flatten the node tree
fn flatten_nodes(
    nodes: Vec<ConnectorsLeaf>,
    parent_chains: &[Modificator], // Chains inherited from parents
) -> miette::Result<Vec<UpstreamContextConfig>> {
    let mut results = Vec::new();

    // 1. Build context for the current level
    let mut current_chains = parent_chains.to_vec();
    let mut local_lb_options: Option<UpstreamOptions> = None;

    // Separate configuration (chains, lb) from structure (upstreams, sections)
    let mut structure = Vec::new();

    for node in nodes {
        match node {
            ConnectorsLeaf::Modificator(m) => current_chains.push(m),
            ConnectorsLeaf::LoadBalance(lb) => local_lb_options = Some(lb),
            s => structure.push(s),
        }
    }

    // 2. Traverse structural elements
    for node in structure {
        match node {
            ConnectorsLeaf::Upstream(up) => {
                // VALIDATION: Check compatibility if LoadBalance is present
                if local_lb_options.is_some() && !matches!(up, UpstreamConfig::MultiServer(_)) {
                    return Err(miette::miette!(
                        "The 'load-balance' directive can only be applied to 'proxy' blocks with multiple servers (MultiServer). Found incompatible upstream (Static or Single Service) in the same section."
                    ));
                }

                results.push(UpstreamContextConfig {
                    upstream: up,
                    chains: current_chains.clone(),
                    lb_options: local_lb_options.clone(),
                });
            }
            ConnectorsLeaf::Section(children) => {
                let children_flat = flatten_nodes(children, &current_chains)?;
                results.extend(children_flat);
            }
            _ => unreachable!(),
        }
    }

    Ok(results)
}

fn parse_proto_value(value: &str) -> Result<Option<ALPN>, String> {
    match value {
        "h1-only" => Ok(Some(ALPN::H1)),
        "h2-only" => Ok(Some(ALPN::H2)),
        "h1-or-h2" => {
            tracing::warn!("accepting 'h1-or-h2' as meaning 'h2-or-h1'");
            Ok(Some(ALPN::H2H1))
        }
        "h2-or-h1" => Ok(Some(ALPN::H2H1)),
        other => Err(format!(
            "'proto' should be one of 'h1-only', 'h2-only', or 'h2-or-h1', found '{other}'"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_err_contains;
    use crate::common_types::definitions::ChainItem;
    use crate::common_types::key_template::KeyTemplate;
    use crate::kdl::definitions::DefinitionsSection;
    use crate::kdl::parser::block::BlockParser;
    use crate::kdl::parser::ctx::Current;
    use kdl::KdlDocument;

    /// Helper to parse config when no external definitions are needed
    fn parse_config(input: &str) -> miette::Result<Connectors> {
        let doc: KdlDocument = input.parse().unwrap();
        let table = DefinitionsTable::default();

        let ctx = ParseContext::new(&doc, Current::Document(&doc), "test");
        let mut block = BlockParser::new(ctx)?;

        block.required("connectors", |ctx| {
            ConnectorsSection::new(&table).parse_node(ctx)
        })
    }

    /// Helper to parse config with definitions
    fn parse_config_with_defs(defs_input: &str, conn_input: &str) -> miette::Result<Connectors> {
        // 1. Parse definitions
        let defs_doc: KdlDocument = defs_input.parse().unwrap();
        let defs_ctx = ParseContext::new(&defs_doc, Current::Document(&defs_doc), "test");
        let mut defs_block = BlockParser::new(defs_ctx)?;

        let table = defs_block.required("definitions", |ctx| DefinitionsSection.parse_node(ctx))?;

        // 2. Parse connectors
        let conn_doc: KdlDocument = conn_input.parse().unwrap();
        let conn_ctx = ParseContext::new(&conn_doc, Current::Document(&conn_doc), "test");
        let mut conn_block = BlockParser::new(conn_ctx)?;

        conn_block.required("connectors", |ctx| {
            ConnectorsSection::new(&table).parse_node(ctx)
        })
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
                let ChainItem::Filter(filter) = &named_chain.chain.items[0] else {
                    unreachable!()
                };

                assert_eq!(filter.name, "block-ip");
                let ChainItem::Filter(filter) = &named_chain.chain.items[1] else {
                    unreachable!()
                };
                assert_eq!(filter.name, "auth-check");
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
        let ChainItem::Filter(filter1) = &r1.chain.items[0] else {
            unreachable!()
        };
        assert_eq!(filter1.name, "logger");
        let Modificator::Chain(r2) = &api_upstream.chains[1];
        let ChainItem::Filter(filter2) = &r2.chain.items[0] else {
            unreachable!()
        };
        assert_eq!(filter2.name, "rate-limit");

        let public_upstream = connectors.upstreams.iter()
            .find(|u| match &u.upstream {
                UpstreamConfig::Service(s) => &s.prefix_path,
                _ => panic!("not for this test")
            } == "/public")
            .expect("Public upstream not found");

        assert_eq!(public_upstream.chains.len(), 1);
        let Modificator::Chain(r1) = &public_upstream.chains[0];
        let ChainItem::Filter(filter1) = &r1.chain.items[0] else {
            unreachable!()
        };
        assert_eq!(filter1.name, "logger");
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

        let ChainItem::Filter(filter1) = &r.chain.items[0] else {
            unreachable!()
        };
        assert_eq!(filter1.name, "A");
        let Modificator::Chain(r) = &upstream.chains[1];
        let ChainItem::Filter(filter2) = &r.chain.items[0] else {
            unreachable!()
        };
        assert_eq!(filter2.name, "B");
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
                ConnectorsSection::new(&table).parse_connections_node(ctx, &mut anon)
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
