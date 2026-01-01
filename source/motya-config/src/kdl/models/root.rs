use motya_macro::{motya_node, NodeSchema, Parser};

use crate::kdl::{
    models::{
        definitions::DefinitionsDef, imports::ImportsDef, services::ServicesSectionDef,
        system::SystemDataDef,
    },
};

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(root)]
pub struct RootDef {
    #[node(child)]
    pub system: Option<SystemDataDef>,

    #[node(child)]
    pub includes: Option<ImportsDef>,

    #[node(child)]
    pub definitions: Option<DefinitionsDef>,

    #[node(child)]
    pub services: Vec<ServicesSectionDef>,
}

#[derive(Parser, Clone, Debug, Default)]
#[node(root, ignore_unknown)]
pub struct PartialParsedRoot {
    #[node(child)]
    pub imports: Option<ImportsDef>,
}
