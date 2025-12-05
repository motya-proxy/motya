use std::path::PathBuf;

use crate::common_types::listeners::Listeners;

//
// File Server Configuration
//
#[derive(Debug, Clone, PartialEq)]
pub struct FileServerConfig {
    pub name: String,
    pub listeners: Listeners,
    pub base_path: Option<PathBuf>,
}
