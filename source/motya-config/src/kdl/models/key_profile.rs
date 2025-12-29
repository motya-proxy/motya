use motya_macro::{motya_node, Parser};

use crate::common_types::key_template::KeyTemplate;
use crate::kdl::models::transforms_order::TransformsOrderDef;

#[motya_node]
#[derive(Parser, Clone, Debug)]
#[node(name = "key")]
pub struct KeyDef {
    #[node(arg)]
    pub template: KeyTemplate,

    #[node(prop)]
    pub fallback: Option<KeyTemplate>,
}

#[motya_node]
#[derive(Parser, Clone, Debug)]
#[node(name = "algorithm")]
pub struct HashAlgDef {
    #[node(prop, default = "\"xxhash64\".to_string()")]
    pub name: String,

    #[node(prop, default = "0")]
    pub seed: usize,
}

#[motya_node]
#[derive(Parser, Clone, Debug)]
#[node(name = "profile")]
pub struct KeyProfileDef {
    #[node(child)]
    pub key: KeyDef,

    #[node(child)]
    pub transforms: Option<TransformsOrderDef>,

    #[node(child)]
    pub algorithm: Option<HashAlgDef>,
}
