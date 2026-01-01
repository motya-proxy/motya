use std::collections::BTreeMap;

use fqdn::FQDN;
use motya_macro::{motya_node, NodeSchema, Parser};

use crate::kdl::{
    models::{key_profile::KeyDef, transforms_order::TransformsOrderDef},
    parser::typed_value::TypedValue,
};

#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "filter")]
pub struct ConfiguredFilterDef {
    #[node(arg)]
    pub name: FQDN,
    #[node(all_props)]
    pub params: BTreeMap<String, TypedValue>,
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
pub enum ChainItemDef {
    #[node(name = "filter")]
    Filter(ConfiguredFilterDef),
    #[node(name = "rate-limit")]
    RateLimit(RateLimitDef),
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "use-chain")]
pub enum UseChainDef {
    Reference {
        #[node(arg)]
        name: String,
    },

    Inline {
        #[node(dynamic_child)]
        items: Vec<ChainItemDef>,
    },
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "rate-limit")]
pub enum RateLimitDef {
    Reference(
        #[node(arg)]
        #[err(name = "ref")]
        String,
    ),
    Inline {
        #[node(child)]
        algorithm: String,

        #[node(child, name = "storage")]
        storage_key: String,

        #[node(child, name = "key")]
        key_template: KeyDef,

        #[node(child, name = "transforms-order")]
        transforms: Option<TransformsOrderDef>,

        #[node(child)]
        burst: usize,

        #[node(child, name = "rate")]
        raw_rate: f64,
    },
}
