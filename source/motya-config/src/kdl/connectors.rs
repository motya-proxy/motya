use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::atomic::{AtomicUsize, Ordering},
};

use http::{uri::PathAndQuery, StatusCode};
use miette::Result;

use crate::{
    common_types::{
        balancer::{BalancerConfig, DiscoveryKind, HealthCheckKind, SelectionKind},
        connectors::{
            Connectors, ConnectorsLeaf, HttpPeerConfig, MultiServerUpstreamConfig, RouteMatcher,
            RoutingMode, UpstreamConfig, UpstreamContextConfig, UpstreamServer, ALPN,
        },
        definitions::{ChainItem, ConfiguredFilter, FilterChain, Modificator, NamedFilterChain},
        definitions_table::DefinitionsTable,
        error::ConfigError,
        key_template::{parse_hasher, HashAlgorithm, HashOp, KeyPart},
        rate_limiter::RateLimitPolicy,
        simple_response_type::SimpleResponseConfig,
        value::Value,
    },
    internal::UpstreamOptions,
    kdl::{
        models::{
            chains::{ChainItemDefData, RateLimitDefData, UseChainDef, UseChainDefData},
            connectors::{
                ConnectorLeafDef, ConnectorLeafDefData, ConnectorsDef, LoadBalanceDef,
                ProxyDefData, SectionDef, SelectionAlgDefData, SelectionDef, SelectionDefData,
            },
        },
        parser::{ctx::ParseContext, spanned::Spanned},
    },
};

pub struct ConnectorsLinker<'a> {
    table: &'a DefinitionsTable,
    anon_counter: AtomicUsize,
}

impl<'a> ConnectorsLinker<'a> {
    pub fn new(table: &'a DefinitionsTable) -> Self {
        Self {
            table,
            anon_counter: AtomicUsize::new(0),
        }
    }

    pub fn link(&self, ast: ConnectorsDef) -> (Connectors, ConfigError) {
        let mut errors = ConfigError::default();

        let (data, _) = ast.into_parts();

        let root_nodes = self.compile_sections_recursive(
            data.sections,
            &mut errors,
            "/".parse().unwrap(),
            RouteMatcher::Exact,
        );

        let upstreams = flatten_nodes(root_nodes, &[], &mut errors);

        (Connectors { upstreams }, errors)
    }

    fn compile_sections_recursive(
        &self,
        sections: Vec<SectionDef>,
        errors: &mut ConfigError,
        base_path: PathAndQuery,
        parent_matcher: RouteMatcher,
    ) -> Vec<Spanned<ConnectorsLeaf>> {
        let mut results = Vec::new();

        for section_def in sections {
            let (data, ctx) = section_def.into_parts();

            let next_matcher = match data.routing_mode {
                Some(RoutingMode::Prefix) => RouteMatcher::Prefix,
                Some(RoutingMode::Exact) => RouteMatcher::Exact,
                None => parent_matcher,
            };

            if next_matcher == RouteMatcher::Exact && !data.sections.is_empty() {
                errors.push_report(
                    ctx.err_sections(
                        "A section with 'exact' routing mode cannot contain nested sections",
                    ),
                    &ctx.ctx,
                );
            }

            let path_str = data.path.path();
            let full_path_str = if base_path.path() == "/" && !path_str.starts_with('/') {
                format!("/{}", path_str)
            } else if base_path.path() == "/" {
                path_str.to_string()
            } else {
                format!(
                    "{}/{}",
                    base_path.path().trim_end_matches('/'),
                    path_str.trim_start_matches('/')
                )
            };

            let current_path = match full_path_str.parse::<PathAndQuery>() {
                Ok(p) => p,
                Err(e) => {
                    errors.push_report(
                        ctx.err_path(format!("Bad path joined: {full_path_str}, error: {e}")),
                        &ctx.ctx,
                    );
                    continue;
                }
            };

            let mut section_elements = Vec::new();

            for chain_def in data.chains {
                if let Some(chain_node) = self.compile_use_chain(chain_def, errors, &current_path) {
                    section_elements.push(chain_node);
                }
            }

            if let Some(lb_def) = data.load_balance {
                if let Some(lb_node) = self.compile_load_balance(lb_def, errors) {
                    section_elements.push(lb_node);
                }
            }

            let leaf_node = self.compile_connector_leaf(
                data.leaf,
                &ctx.ctx,
                errors,
                current_path.clone(),
                next_matcher,
            );

            section_elements.push(leaf_node);

            let children_sections =
                self.compile_sections_recursive(data.sections, errors, current_path, next_matcher);

            for child_section in children_sections {
                section_elements.push(child_section);
            }

            results.push(Spanned::new(
                ConnectorsLeaf::Section(section_elements),
                ctx.ctx,
            ));
        }

        results
    }

    fn compile_connector_leaf(
        &self,
        leaf_def: ConnectorLeafDef,
        parent_ctx: &ParseContext,
        errors: &mut ConfigError,
        current_path: PathAndQuery,
        matcher: RouteMatcher,
    ) -> Spanned<ConnectorsLeaf> {
        let (leaf_data, leaf_ctx) = leaf_def.into_parts();

        let leaf_content = match leaf_data {
            ConnectorLeafDefData::Return(ret_def) => {
                let (data, ctx) = ret_def.into_parts();

                let http_code = match StatusCode::from_u16(data.code) {
                    Ok(c) => c,
                    Err(_) => {
                        errors.push_report(ctx.err_code("Invalid HTTP status code"), &ctx.ctx);
                        StatusCode::OK
                    }
                };

                ConnectorsLeaf::Upstream(UpstreamConfig::Static(SimpleResponseConfig {
                    http_code,
                    response_body: data.body.unwrap_or_default(),
                    prefix_path: current_path,
                }))
            }
            ConnectorLeafDefData::Proxy(proxy_def) => {
                let (proxy_data, proxy_ctx) = proxy_def.into_parts();

                let config = match proxy_data {
                    ProxyDefData::Single {
                        url,
                        tls_sni,
                        proto,
                    } => {
                        let host_addr = match url
                            .authority()
                            .and_then(|host| host.as_str().parse::<SocketAddr>().ok())
                        {
                            Some(addr) => addr,
                            None => {
                                errors.push_report(
                                    proxy_ctx.err_self("Not a valid socket address in URL"),
                                    parent_ctx,
                                );
                                SocketAddr::from(([0, 0, 0, 0], 0))
                            }
                        };

                        let (tls, sni, alpn) = match self
                            .resolve_proto_settings(proto.as_deref(), tls_sni.as_deref())
                        {
                            Ok(res) => res,
                            Err(msg) => {
                                errors.push_report(proxy_ctx.err_self(msg), &proxy_ctx.ctx);
                                (false, String::new(), ALPN::H1)
                            }
                        };

                        UpstreamConfig::Service(HttpPeerConfig {
                            peer_address: host_addr,
                            alpn,
                            sni,
                            tls,
                            prefix_path: current_path,
                            target_path: url
                                .path()
                                .parse()
                                .unwrap_or(PathAndQuery::from_static("/")),
                            matcher,
                        })
                    }
                    ProxyDefData::Multi {
                        servers,
                        tls_sni,
                        proto,
                    } => {
                        let mut upstream_servers = Vec::new();
                        for s_def in servers {
                            let (s_data, _) = s_def.into_parts();
                            upstream_servers.push(UpstreamServer {
                                address: s_data.address,
                                weight: s_data.weight.unwrap_or(1),
                            });
                        }

                        let (_tls, sni, alpn) = match self
                            .resolve_proto_settings(proto.as_deref(), tls_sni.as_deref())
                        {
                            Ok(res) => res,
                            Err(msg) => {
                                errors.push_report(proxy_ctx.err_self(msg), &proxy_ctx.ctx);
                                (false, String::new(), ALPN::H1)
                            }
                        };

                        let final_sni = if sni.is_empty() { None } else { Some(sni) };

                        UpstreamConfig::MultiServer(MultiServerUpstreamConfig {
                            servers: upstream_servers,
                            tls_sni: final_sni,
                            alpn,
                            prefix_path: current_path,
                            target_path: PathAndQuery::from_static("/"),
                            matcher,
                        })
                    }
                };
                ConnectorsLeaf::Upstream(config)
            }
        };

        Spanned::new(leaf_content, leaf_ctx.ctx)
    }

    fn compile_load_balance(
        &self,
        lb_def: LoadBalanceDef,
        errors: &mut ConfigError,
    ) -> Option<Spanned<ConnectorsLeaf>> {
        let (data, ctx) = lb_def.into_parts();

        let health_checks = match data.health_check.as_deref() {
            Some("None") | None => HealthCheckKind::None,
            Some(val) => {
                errors.push_report(
                    ctx.err_health_check(format!("Unknown health-check kind: '{val}'")),
                    &ctx.ctx,
                );
                HealthCheckKind::None
            }
        };

        let discovery = match data.discovery.as_deref() {
            Some("Static") | None => DiscoveryKind::Static,
            Some(val) => {
                errors.push_report(
                    ctx.err_discovery(format!("Unknown discovery kind: '{val}'")),
                    &ctx.ctx,
                );
                DiscoveryKind::Static
            }
        };

        let (selection, template) = if let Some(sel_def) = data.selection {
            self.compile_selection(sel_def, errors)
        } else {
            (SelectionKind::RoundRobin, None)
        };

        Some(Spanned::new(
            ConnectorsLeaf::LoadBalance(UpstreamOptions {
                selection,
                template,
                health_checks,
                discovery,
            }),
            ctx.ctx,
        ))
    }

    fn compile_use_chain(
        &self,
        chain_def: UseChainDef,
        errors: &mut ConfigError,
        path: &PathAndQuery,
    ) -> Option<Spanned<ConnectorsLeaf>> {
        let (chain_data, ctx) = chain_def.into_parts();

        let leaf = match chain_data {
            UseChainDefData::Reference { name } => {
                if let Some(chain) = self.table.get_chain_by_name(&name) {
                    ConnectorsLeaf::Modificator(Modificator::Chain(NamedFilterChain {
                        chain,
                        name,
                    }))
                } else {
                    errors.push_report(
                        ctx.err_reference_name(format!(
                            "Chain '{}' not found in definitions",
                            name
                        )),
                        &ctx.ctx,
                    );
                    return None;
                }
            }
            UseChainDefData::Inline { items } => {
                let mut runtime_items = Vec::new();
                for item in items {
                    let (item_data, _) = item.into_parts();
                    match item_data {
                        ChainItemDefData::Filter(def) => {
                            runtime_items.push(ChainItem::Filter(ConfiguredFilter {
                                args: def
                                    .params
                                    .into_iter()
                                    .map(|(k, v)| (k, v.value().into()))
                                    .collect::<BTreeMap<String, Value>>(),
                                name: def.name,
                            }));
                        }
                        ChainItemDefData::RateLimit(def) => {
                            let (rl_data, rl_ctx) = def.into_parts();
                            match rl_data {
                                RateLimitDefData::Reference(name) => {
                                    if let Some(policy) = self.table.get_rate_limit(&name) {
                                        runtime_items.push(ChainItem::RateLimiter(policy));
                                    } else {
                                        errors.push_report(
                                            rl_ctx.err_reference_ref(format!(
                                                "Rate limit policy '{}' not found in definitions",
                                                name
                                            )),
                                            &rl_ctx.ctx,
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
                                    let key_template = key_template.into_inner();

                                    runtime_items.push(ChainItem::RateLimiter(RateLimitPolicy {
                                        name: format!("__anon_rl_conn_{}", runtime_items.len()),
                                        algorithm,
                                        burst,
                                        key_template: key_template.template,
                                        rate_req_per_sec: raw_rate,
                                        storage_key,
                                        transforms: transforms
                                            .map(|v| v.into())
                                            .unwrap_or_default(),
                                    }))
                                }
                            }
                        }
                    }
                }
                let chain = FilterChain {
                    items: runtime_items,
                };
                let id = self.anon_counter.fetch_add(1, Ordering::Relaxed);
                let path_slug = path.path().replace('/', "_");
                let generated_name = format!("__anon_{id}_{path_slug}");

                ConnectorsLeaf::Modificator(Modificator::Chain(NamedFilterChain {
                    chain,
                    name: generated_name,
                }))
            }
        };

        Some(Spanned::new(leaf, ctx.ctx))
    }

    fn resolve_proto_settings(
        &self,
        proto: Option<&str>,
        tls_sni: Option<&str>,
    ) -> Result<(bool, String, ALPN), String> {
        let alpn = match proto {
            Some(p) => Some(parse_proto_value(p)?),
            None => None,
        };
        match (alpn, tls_sni) {
            (None, None) | (Some(ALPN::H1), None) => Ok((false, String::new(), ALPN::H1)),
            (None, Some(sni)) => Ok((true, sni.to_string(), ALPN::H2H1)),
            (Some(_), None) => Err("'tls-sni' is required for HTTP2 support".to_string()),
            (Some(p), Some(sni)) => Ok((true, sni.to_string(), p)),
        }
    }

    fn compile_selection(
        &self,
        sel_def: SelectionDef,
        errors: &mut ConfigError,
    ) -> (SelectionKind, Option<BalancerConfig>) {
        let (sel_data, sel_ctx) = sel_def.into_parts();

        match sel_data {
            SelectionDefData::Reference { kind, profile_ref } => {
                if let Some(t) = self.table.get_key_templates().get(&profile_ref) {
                    (kind, Some(t.clone()))
                } else {
                    errors.push_report(
                        sel_ctx.err_reference_profile_ref(format!(
                            "Key profile '{}' not found",
                            profile_ref
                        )),
                        &sel_ctx.ctx,
                    );
                    (kind, None)
                }
            }
            SelectionDefData::Simple { kind } => (kind, None),
            SelectionDefData::Inline(alg_wrapper) => {
                let (alg_data, _) = alg_wrapper.into_parts();

                match alg_data {
                    SelectionAlgDefData::None => (SelectionKind::RoundRobin, None),
                    SelectionAlgDefData::Inline {
                        kind,
                        key,
                        algorithm,
                        transforms,
                    } => {
                        let (key_def, key_ctx) = key.into_parts();
                        let source = key_def.template;
                        let fallback = key_def.fallback;

                        let (alg_def, alg_inner_ctx) = algorithm.into_parts();

                        let runtime_alg_struct = HashAlgorithm {
                            name: alg_def.name,
                            seed: alg_def.seed,
                        };

                        let alg_op = match parse_hasher(&runtime_alg_struct) {
                            Ok(op) => op,
                            Err(e) => {
                                errors.push_report(alg_inner_ctx.err_self(e), &alg_inner_ctx.ctx);
                                HashOp::XxHash64(0)
                            }
                        };

                        let mut transform_ops = Vec::new();
                        if let Some(tr_def) = transforms {
                            transform_ops = tr_def.into();
                        }

                        match kind {
                            SelectionKind::KetamaHashing | SelectionKind::FvnHash => {
                                let is_effectively_empty = source.parts.is_empty()
                                    || (source.parts.len() == 1
                                        && matches!(source.parts[0], KeyPart::Literal(ref s) if s.is_empty()));

                                if is_effectively_empty {
                                    errors.push_report(
                                        key_ctx.err_self(format!(
                                            "Selection kind '{:?}' requires a non-empty key source",
                                            kind
                                        )),
                                        &key_ctx.ctx,
                                    );
                                }
                            }
                            _ => {}
                        }

                        let config = BalancerConfig {
                            source,
                            fallback,
                            algorithm: alg_op,
                            transforms: transform_ops,
                        };

                        (kind, Some(config))
                    }
                }
            }
        }
    }
}
fn flatten_nodes(
    nodes: Vec<Spanned<ConnectorsLeaf>>,
    base_parent_chains: &[Modificator],
    errors: &mut ConfigError,
) -> Vec<UpstreamContextConfig> {
    let mut results = Vec::new();

    let mut block_chains = base_parent_chains.to_vec();
    let mut block_lb_options: Option<Spanned<UpstreamOptions>> = None;
    let mut block_elements = Vec::new();

    for node in nodes {
        match &node.data {
            ConnectorsLeaf::Modificator(m) => {
                block_chains.push(m.clone());
            }
            ConnectorsLeaf::LoadBalance(lb) => {
                if block_lb_options.is_some() {
                    errors.push_report(
                        node.err_node("Duplicate 'load-balance' directive"),
                        &node.ctx,
                    );
                }
                block_lb_options = Some(Spanned::new(lb.clone(), node.ctx.clone()));
            }
            _ => {
                block_elements.push(node);
            }
        }
    }

    for node in block_elements {
        match &node.data {
            ConnectorsLeaf::Upstream(up) => {
                if let Some(ref lb_span) = block_lb_options {
                    if !matches!(up, UpstreamConfig::MultiServer(_)) {
                        errors.push_report(
                            lb_span.err_node("'load-balance' requires multiple servers"),
                            &lb_span.ctx,
                        );
                    }
                }

                results.push(UpstreamContextConfig {
                    upstream: up.clone(),
                    chains: block_chains.clone(),
                    lb_options: block_lb_options.as_ref().map(|s| s.data.clone()),
                });
            }
            ConnectorsLeaf::Section(children) => {
                let children_flat = flatten_nodes(children.clone(), base_parent_chains, errors);
                results.extend(children_flat);
            }
            _ => unreachable!(),
        }
    }

    results
}

fn parse_proto_value(value: &str) -> Result<ALPN, String> {
    match value {
        "h1-only" => Ok(ALPN::H1),
        "h2-only" => Ok(ALPN::H2),
        "h1-or-h2" | "h2-or-h1" => Ok(ALPN::H2H1),
        other => Err(format!(
            "'proto' should be 'h1-only', 'h2-only', or 'h2-or-h1', found '{other}'"
        )),
    }
}
