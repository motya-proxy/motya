use motya_macro::{motya_node, Parser};

#[motya_node]
#[derive(Parser, Clone, Debug)]
pub struct ImportPath {
    #[node(node_name)]
    pub value: String,
}

#[derive(Parser, Clone, Debug, Default)]
#[node(name = "imports")]
pub struct ImportsDef {
    #[node(dynamic_child)]
    pub paths: Vec<ImportPath>,
}
