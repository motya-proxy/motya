use std::fmt::Debug;
use std::net::SocketAddr;

use http::uri::PathAndQuery;

use crate::common_types::{definitions::Modificator, simple_response_type::SimpleResponseConfig};
use crate::internal::UpstreamOptions;
use crate::kdl::parser::spanned::Spanned;

#[derive(Clone, Debug, PartialEq)]
pub enum ALPN {
    H1,
    H2,
    H2H1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RouteMatcher {
    #[default]
    Exact,
    Prefix,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HttpPeerConfig {
    pub peer_address: SocketAddr,
    pub alpn: ALPN,
    pub tls: bool,
    pub sni: String,
    pub prefix_path: PathAndQuery,
    pub target_path: PathAndQuery,
    pub matcher: RouteMatcher,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum UpstreamConfig {
    Service(HttpPeerConfig),
    Static(SimpleResponseConfig),
    MultiServer(MultiServerUpstreamConfig),
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpstreamServer {
    pub address: std::net::SocketAddr,
    pub weight: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MultiServerUpstreamConfig {
    pub servers: Vec<UpstreamServer>,
    pub tls_sni: Option<String>,
    pub alpn: ALPN,
    pub prefix_path: PathAndQuery,
    pub target_path: PathAndQuery,
    pub matcher: RouteMatcher,
}

#[derive(Clone, Debug)]
pub enum ConnectorsLeaf {
    Upstream(UpstreamConfig),
    Modificator(Modificator),
    LoadBalance(UpstreamOptions),
    Section(Vec<Spanned<ConnectorsLeaf>>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Connectors {
    pub upstreams: Vec<UpstreamContextConfig>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpstreamContextConfig {
    pub upstream: UpstreamConfig,
    pub chains: Vec<Modificator>,
    pub lb_options: Option<UpstreamOptions>,
}
