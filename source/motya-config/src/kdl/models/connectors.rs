use crate::kdl::models::chains::UseChainDef;
use crate::kdl::models::transforms_order::TransformsOrderDef;
use crate::{
    internal::SelectionKind,
    kdl::models::key_profile::{HashAlgDef, KeyDef},
};
use http::uri::PathAndQuery;
use http::Uri;
use miette::miette;
use motya_macro::{motya_node, Parser};
use std::{net::SocketAddr, str::FromStr};

// =============================================================================
// ROOT
// =============================================================================

#[motya_node]
#[derive(Parser, Clone, Debug)]
#[node(name = "connectors")]
pub struct ConnectorsDef {
    #[node(child, name = "section")]
    pub sections: Vec<SectionDef>,
}

// =============================================================================
// SECTION CONTENT
// =============================================================================

#[derive(Clone, Debug)]
pub enum RoutingMode {
    Exact,
    Prefix,
}

impl FromStr for RoutingMode {
    type Err = miette::Report;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "exact" => Ok(RoutingMode::Exact),
            "prefix" => Ok(RoutingMode::Prefix),
            _ => Err(miette!("'exact' | 'prefix' available")),
        }
    }
}

#[motya_node]
#[derive(Parser, Clone, Debug)]
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
#[derive(Parser, Clone, Debug)]
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
#[derive(Parser, Clone, Debug)]
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
#[derive(Parser, Clone, Debug)]
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
#[derive(Parser, Clone, Debug)]
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
#[derive(Parser, Clone, Debug)]
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
#[derive(Parser, Clone, Debug)]
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
#[derive(Parser, Clone, Debug)]
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
