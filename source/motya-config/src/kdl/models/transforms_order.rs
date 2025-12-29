use motya_macro::{motya_node, Parser};
use std::num::NonZeroUsize;

use crate::common_types::key_template::TransformOp;

#[motya_node]
#[derive(Parser, Clone, Debug)]
pub enum TransformStepDef {
    #[node(name = "truncate")]
    Truncate {
        #[node(prop)]
        length: NonZeroUsize,
    },

    #[node(name = "lowercase")]
    Lowercase,

    #[node(name = "remove-query-params")]
    RemoveQueryParams,

    #[node(name = "strip-trailing-slash")]
    StripTrailingSlash,
}

#[motya_node]
#[derive(Parser, Clone, Debug)]
#[node(name = "transforms-order")]
pub struct TransformsOrderDef {
    #[node(dynamic_child)]
    pub steps: Vec<TransformStepDef>,
}

#[derive(Parser, Clone, Debug)]
#[node(name = "profile")]
pub struct KeyProfileDef {
    #[node(child)]
    pub transforms: Option<TransformsOrderDef>,
}

impl From<TransformsOrderDef> for Vec<TransformOp> {
    fn from(value: TransformsOrderDef) -> Self {
        let mut steps = Vec::new();

        let (value, _) = value.into_parts();

        for step in value.steps.into_iter() {
            let op = match *step {
                TransformStepDefData::Truncate { length } => TransformOp::Truncate { length },
                TransformStepDefData::Lowercase => TransformOp::Lowercase,
                TransformStepDefData::RemoveQueryParams => TransformOp::RemoveQueryParams,
                TransformStepDefData::StripTrailingSlash => TransformOp::StripTrailingSlash,
            };
            steps.push(op);
        }
        steps
    }
}
