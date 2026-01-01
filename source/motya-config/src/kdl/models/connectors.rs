use std::net::SocketAddr;

use http::{uri::PathAndQuery, Uri};
use motya_macro::{motya_node, NodeSchema, Parser};

use crate::{
    common_types::{balancer::SelectionKind, connectors::RoutingMode},
    kdl::models::{
        chains::UseChainDef,
        key_profile::{HashAlgDef, KeyDef},
        transforms_order::TransformsOrderDef,
    },
};

// =============================================================================
// ROOT
// =============================================================================

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "connectors")]
pub struct ConnectorsDef {
    #[node(child, name = "section")]
    pub sections: Vec<SectionDef>,
}

// =============================================================================
// SECTION CONTENT
// =============================================================================

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "section")]
pub struct SectionDef {
    #[node(arg)]
    pub path: PathAndQuery,

    #[node(prop, name = "as")]
    pub routing_mode: Option<RoutingMode>,

    #[node(child)]
    pub leaf: ConnectorLeafDef,

    #[node(child, name = "load-balance")]
    pub load_balance: Option<LoadBalanceDef>,

    #[node(child, min = 0)]
    pub chains: Vec<UseChainDef>,

    #[node(child)]
    pub sections: Vec<SectionDef>,
}

// =============================================================================
// LEAF POLYMORPHISM (Proxy OR Return)
// =============================================================================

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
pub enum ConnectorLeafDef {
    #[node(name = "proxy")]
    Proxy(ProxyDef),
    #[node(name = "return")]
    Return(ReturnDef),
}

// =============================================================================
// PROXY (Single OR Multi)
// =============================================================================

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "proxy")]
pub enum ProxyDef {
    Single {
        #[node(arg)]
        url: Uri,

        #[node(prop, name = "tls-sni")]
        tls_sni: Option<String>,
        #[node(prop)]
        proto: Option<String>,
    },

    Multi {
        #[node(child)]
        servers: Vec<UpstreamServerDef>,

        #[node(prop, name = "tls-sni")]
        tls_sni: Option<String>,
        #[node(prop)]
        proto: Option<String>,
    },
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "server")]
pub struct UpstreamServerDef {
    #[node(arg)]
    pub address: SocketAddr,
    #[node(prop)]
    pub weight: Option<usize>,
}

// =============================================================================
// RETURN
// =============================================================================

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "return")]
pub struct ReturnDef {
    #[node(arg)]
    pub code: u16,
    #[node(arg)]
    pub body: Option<String>,
}

// =============================================================================
// LOAD BALANCE & OTHERS
// =============================================================================

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "load-balance")]
pub struct LoadBalanceDef {
    #[node(child, name = "selection")]
    pub selection: Option<SelectionDef>,

    #[node(child, name = "health-check")]
    pub health_check: Option<String>,

    #[node(child, name = "discovery")]
    pub discovery: Option<String>,
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "selection")]
pub enum SelectionDef {
    Simple {
        #[node(arg)]
        kind: SelectionKind,
    },
    Reference {
        #[node(arg)]
        kind: SelectionKind,

        #[node(prop, name = "use-key-profile")]
        profile_ref: String,
    },
    Inline(SelectionAlgDef),
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
pub enum SelectionAlgDef {
    None,
    Inline {
        #[node(arg)]
        kind: SelectionKind,

        #[node(child)]
        key: KeyDef,

        #[node(child)]
        algorithm: HashAlgDef,

        #[node(child, name = "transforms-order")]
        transforms: Option<TransformsOrderDef>,
    },
}
