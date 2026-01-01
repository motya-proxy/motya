use motya_macro::{motya_node, NodeSchema, Parser};

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
pub struct ImportPath {
    #[node(node_name)]
    pub value: String,
}

#[derive(Parser, Clone, Debug, Default, NodeSchema)]
#[node(name = "imports")]
pub struct ImportsDef {
    #[node(dynamic_child)]
    pub paths: Vec<ImportPath>,
}
