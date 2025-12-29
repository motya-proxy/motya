use motya_macro::{motya_node, Parser};

use crate::kdl::models::{
    definitions::DefinitionsDef, imports::ImportsDef, services::ServicesSectionDef,
    system::SystemDataDef,
};

#[motya_node]
#[derive(Parser, Clone, Debug)]
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
