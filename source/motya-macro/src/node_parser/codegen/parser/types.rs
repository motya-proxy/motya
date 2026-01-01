use proc_macro2::TokenStream;

use crate::node_parser::model::{ArgSpec, BaseField, BlockSpec, NameSpec, PropSpec};

pub struct ParseTarget<'a> {
    pub props: &'a [PropSpec],
    pub args: &'a [ArgSpec],
    pub block: &'a BlockSpec,
    pub all_props: &'a Option<BaseField>,
    pub all_args: &'a Option<BaseField>,
    pub node_name: &'a Option<NameSpec>,

    pub ctor_path: TokenStream,

    pub is_tuple: bool,
}
