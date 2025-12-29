use crate::config_source::ConfigSource;
use crate::kdl::models::imports::ImportsDef;
use crate::kdl::parser::ctx::ParseContext;
use crate::kdl::parser::parsable::KdlParsable;
use async_recursion::async_recursion;
use kdl::KdlDocument;
use miette::{miette, Context, IntoDiagnostic, Result};
use std::collections::HashSet;
use std::future::Future;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

pub trait AsyncFs: Send + Sync + Clone + Default {
    fn canonicalize(path: &Path) -> impl Future<Output = Result<PathBuf>> + Send;
    fn read_to_string(path: &Path) -> impl Future<Output = Result<String>> + Send;
}

#[derive(Default, Clone)]
pub struct FileCollector<F: AsyncFs> {
    fs: PhantomData<F>,
    documents: Vec<(KdlDocument, String)>,
    visited_paths: HashSet<PathBuf>,
}

impl<F: AsyncFs> ConfigSource for FileCollector<F> {
    async fn collect(&self, entry_path: PathBuf) -> Result<Vec<(KdlDocument, String)>> {
        Self::collect(self.clone(), entry_path).await
    }
}

impl<Fs: AsyncFs> FileCollector<Fs> {
    pub async fn collect(mut self, entry_path: PathBuf) -> Result<Vec<(KdlDocument, String)>> {
        let root_path = Fs::canonicalize(&entry_path)
            .await
            .context("Failed to resolve entry point")?;

        self.load_recursive(root_path).await?;

        Ok(self.documents)
    }

    #[async_recursion]
    async fn load_recursive(&mut self, path: PathBuf) -> Result<()> {
        if self.visited_paths.contains(&path) {
            return Ok(());
        }
        self.visited_paths.insert(path.clone());

        let content = Fs::read_to_string(&path)
            .await
            .wrap_err_with(|| format!("Failed to read file: {:?}", path))?;

        let doc: KdlDocument = content
            .parse()
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to parse KDL: {:?}", path))?;

        let name = path
            .file_name()
            .map(|s| s.to_string_lossy())
            .ok_or_else(|| miette!("It's not a file: {:?}", path))?;

        let raw_includes =
            ImportsDef::parse_node(&ParseContext::new(doc.clone(), &name), &()).unwrap_or_default();

        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));

        for path_str in raw_includes.paths.iter().map(|v| v.value.clone()) {
            let include_path = base_dir.join(path_str);

            self.load_recursive(include_path).await?;
        }

        self.documents.push((doc, name.to_string()));
        Ok(())
    }
}
