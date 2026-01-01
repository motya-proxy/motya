use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use async_recursion::async_recursion;
use dashmap::DashMap;
use kdl::KdlDocument;
use miette::NamedSource;
use motya_config::{
    common_types::error::{ConfigError, ParseError},
    config_source::ConfigSource,
    kdl::{
        models::root::PartialParsedRoot,
        parser::{ctx::ParseContext, parsable::KdlParsable},
    },
};
use path_clean::PathClean;
use ropey::Rope;
use tower_lsp::lsp_types::Url;

#[derive(Clone, Default)]
pub struct LspConfigSource {
    pub documents: Arc<DashMap<Url, Rope>>,
}

impl ConfigSource for LspConfigSource {
    async fn collect(&self, entry_path: PathBuf) -> miette::Result<Vec<(KdlDocument, String)>> {
        let (docs, errors) = self.collect_lossy(entry_path).await;

        if !errors.is_empty() {
            Err(miette::Report::new(errors))
        } else {
            Ok(docs)
        }
    }

    async fn collect_lossy(
        &self,
        entry_path: PathBuf,
    ) -> (Vec<(KdlDocument, String)>, ConfigError) {
        let mut runner = Runner {
            documents: self.documents.clone(),
            visited: HashSet::new(),
            found_docs: Vec::new(),
            errors: ConfigError::default(),
        };

        match runner.read_content(&entry_path).await {
            Ok(content) => {
                runner.process_file(entry_path, content).await;
            }
            Err(e) => {
                let name = entry_path.to_string_lossy().to_string();
                let src = NamedSource::new(name, String::new());
                runner.errors.push(ParseError::new(
                    e,
                    None,
                    Some("Check if the entry point file exists".to_string()),
                    src,
                ));
            }
        }

        (runner.found_docs, runner.errors)
    }
}

struct Runner {
    documents: Arc<DashMap<Url, Rope>>,
    visited: HashSet<PathBuf>,
    found_docs: Vec<(KdlDocument, String)>,
    errors: ConfigError,
}

impl Runner {
    #[async_recursion]
    async fn process_file(&mut self, path: PathBuf, content: String) {
        if self.visited.contains(&path) {
            return;
        }
        self.visited.insert(path.clone());

        let name = path.to_string_lossy().to_string();
        let named_source = NamedSource::new(&name, content.clone());

        let doc: KdlDocument = match content.parse() {
            Ok(d) => d,
            Err(kdl_error) => {
                let errors = ParseError::from_kdl_error(kdl_error, named_source);
                for err in errors {
                    self.errors.push(err);
                }
                return;
            }
        };

        let ctx = ParseContext::new(doc.clone(), &name);

        let root_result = PartialParsedRoot::parse_node(&ctx, &());

        match root_result {
            Ok(root) => {
                if let Some(imports) = root.imports {
                    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));

                    for path_node in imports.paths {
                        let (path_str, node_ctx) = path_node.into_parts();
                        let resolved_path = base_dir.join(&path_str.value).clean();

                        if self.visited.contains(&resolved_path) {
                            continue;
                        }

                        match self.read_content(&resolved_path).await {
                            Ok(sub_content) => {
                                self.process_file(resolved_path, sub_content).await;
                            }
                            Err(msg) => {
                                let report = node_ctx.err_value(msg);
                                self.errors.push_report(report, &node_ctx.ctx);
                            }
                        }
                    }
                }
            }
            Err(report) => {
                self.errors.merge(report);
            }
        }

        self.found_docs.push((doc, name));
    }

    async fn read_content(&self, path: &Path) -> Result<String, String> {
        if let Ok(url) = Url::from_file_path(path)
            && let Some(rope) = self.documents.get(&url)
        {
            return Ok(rope.to_string());
        }

        tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))
    }
}
