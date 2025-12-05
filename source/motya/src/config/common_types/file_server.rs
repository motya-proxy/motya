use std::path::PathBuf;

use crate::config::common_types::listeners::Listeners;

//
// File Server Configuration
//
#[derive(Debug, Clone, PartialEq)]
pub struct FileServerConfig {
    pub(crate) name: String,
    pub(crate) listeners: Listeners,
    pub(crate) base_path: Option<PathBuf>,
}
