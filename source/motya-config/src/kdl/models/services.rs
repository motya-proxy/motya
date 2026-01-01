use motya_macro::{motya_node, NodeSchema, Parser};

use crate::kdl::models::{
    connectors::ConnectorsDef, file_server::FileServerDef, listeners::ListenersDef,
};

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "services")]
pub struct ServicesSectionDef {
    #[node(dynamic_child)]
    pub items: Vec<ServiceDef>,
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
pub struct ServiceDef {
    #[node(node_name)]
    pub name: String,

    #[node(child)]
    pub listeners: ListenersDef,

    #[node(child, flatten)]
    pub mode: ServiceMode,
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
pub enum ServiceMode {
    #[node(name = "file-server")]
    FileServer(FileServerDef),
    #[node(name = "connectors")]
    Connectors(ConnectorsDef),
}
