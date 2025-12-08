use std::path::PathBuf;
use kdl::KdlDocument;
use miette::Result;


#[allow(async_fn_in_trait)]
pub trait ConfigSource: Send + Sync + Default + Clone {
    async fn collect(&self, entry_path: PathBuf) -> Result<Vec<(KdlDocument, String)>>;
}