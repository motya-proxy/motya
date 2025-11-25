use http::Uri;
use pingora::prelude::HttpPeer;

use crate::config::{
    common_types::rules::Modificator,
    internal::{SimpleResponse, UpstreamOptions},
};

#[derive(Debug, Clone)]
pub struct HttpPeerOptions {
    pub peer: HttpPeer,
    pub prefix_path: Uri,
    pub target_path: Uri
}

#[derive(Debug, Clone)]
pub enum Upstream {
    Service(HttpPeerOptions),
    Static(SimpleResponse)
}

pub enum ConnectorsLeaf {
    Upstream(Upstream),
    Modificator(Modificator),
    LoadBalance(UpstreamOptions),
    Section(Vec<ConnectorsLeaf>)
}

#[derive(Debug, Clone)]
pub struct Connectors { 
    pub upstreams: Vec<UpstreamWithContext>
}

#[derive(Debug, Clone)]
pub struct UpstreamWithContext {
    pub upstream: Upstream,
    pub rules: Vec<Modificator>,
    pub lb_options: UpstreamOptions,
}

pub trait ConnectorsSectionParser<T> {
    fn parse_node(&self, node: &T) -> miette::Result<Connectors>;
}