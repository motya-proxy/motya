use std::path::PathBuf;

use motya_macro::{NodeSchema, Parser};

#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "file-server")]
pub struct FileServerDef {
    #[node(prop)]
    pub root: Option<PathBuf>,
}
