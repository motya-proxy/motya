use async_recursion::async_recursion;
use kdl::KdlDocument;
use miette::{Context, IntoDiagnostic, Result, miette};
use motya_config::common_types::section_parser::SectionParser;
use motya_config::config_source::ConfigSource;
use motya_config::kdl::includes::IncludesSection;
use motya_config::kdl::parser::ctx::{Current, ParseContext};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::utils::normalize_path;

#[derive(Default, Clone)]
pub struct MapCollector {
    files: HashMap<PathBuf, String>,
    visited: HashSet<PathBuf>,
}

impl ConfigSource for MapCollector {
    async fn collect(&self, entry_path: PathBuf) -> Result<Vec<(KdlDocument, String)>> {
        let mut runner = MapCollector {
            files: self.files.clone(),
            visited: HashSet::new(),
        };

        runner.load_recursive(entry_path).await
    }
}

impl MapCollector {
    pub fn new(raw_files: HashMap<String, String>) -> Self {
        let files = raw_files
            .into_iter()
            .map(|(k, v)| (PathBuf::from(k), v))
            .collect();

        Self {
            files,
            visited: HashSet::new(),
        }
    }

    #[async_recursion]
    async fn load_recursive(&mut self, path: PathBuf) -> Result<Vec<(KdlDocument, String)>> {
        if self.visited.contains(&path) {
            return Ok(vec![]);
        }
        self.visited.insert(path.clone());

        let content = self
            .files
            .get(&path)
            .ok_or_else(|| miette!("File not found in snapshot: {:?}", path))?;

        let doc: KdlDocument = content
            .parse()
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to parse KDL: {:?}", path))?;

        let mut docs = Vec::new();

        let name = path.to_string_lossy();

        let includes =
            IncludesSection.parse_node(ParseContext::new(&doc, Current::Document(&doc), &name))?;

        let base_dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));

        for path_str in includes {
            let resolved_path = normalize_path(base_dir, &path_str);

            let mut sub_docs = self.load_recursive(resolved_path).await?;
            docs.append(&mut sub_docs);
        }

        docs.push((doc, name.to_string()));
        Ok(docs)
    }
}
