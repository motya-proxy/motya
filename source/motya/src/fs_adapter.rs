use std::path::{Path, PathBuf};

use miette::{IntoDiagnostic, Result};
use motya_config::kdl::fs_loader::AsyncFs;
use tokio::fs;


#[derive(Clone, Default)]
pub struct TokioFs;

impl AsyncFs for TokioFs {
    async fn canonicalize(path: &Path) -> Result<PathBuf> {
        fs::canonicalize(path).await.into_diagnostic()
    }

    async fn read_to_string(path: &Path) -> Result<String> {
        fs::read_to_string(path).await.into_diagnostic()
    }
}