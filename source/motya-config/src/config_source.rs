use std::path::PathBuf;

use kdl::KdlDocument;
use miette::Result;

use crate::common_types::error::ConfigError;

#[allow(async_fn_in_trait)]
pub trait ConfigSource: Send + Sync + Default + Clone {
    async fn collect(&self, entry_path: PathBuf) -> Result<Vec<(KdlDocument, String)>>;

    async fn collect_lossy(
        &self,
        entry_path: PathBuf,
    ) -> (Vec<(KdlDocument, String)>, ConfigError) {
        match self.collect(entry_path).await {
            Ok(docs) => (docs, ConfigError::default()),
            Err(report) => {
                let err = if let Some(e) = report.downcast_ref::<ConfigError>() {
                    e.clone()
                } else {
                    ConfigError::default()
                };
                (vec![], err)
            }
        }
    }
}
