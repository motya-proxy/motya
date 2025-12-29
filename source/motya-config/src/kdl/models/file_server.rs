use std::path::PathBuf;

use motya_macro::Parser;

#[derive(Parser, Clone, Debug)]
#[node(name = "file-server")]
pub struct FileServerDef {
    #[node(prop)]
    pub root: Option<PathBuf>,
}
