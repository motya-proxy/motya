use motya_macro::{motya_node, NodeSchema, Parser};

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
pub enum AB {
    #[node(name = "A")]
    A,
    #[node(name = "B")]
    B,
}

