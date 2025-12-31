use std::sync::Arc;

use dashmap::DashMap;
use motya_config::{common_types::definitions_table::DefinitionsTable, loader::ConfigLoader};
use ropey::Rope;
use tower_lsp::{Client, lsp_types::Url};

use crate::{diagnostics::DiagnosticConverter, loader::LspConfigSource};

#[derive(Debug)]
pub struct Backend {
    pub client: Client,
    pub documents: Arc<DashMap<Url, Rope>>, 
}

impl Backend {
    pub async fn validate(&self, uri: Url) {
        
        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return, 
        };

        let source = LspConfigSource {
            documents: self.documents.clone(),
        };
        let loader = ConfigLoader::new(source);
        let mut defs = DefinitionsTable::new_with_global();

        let (_, error) = loader.load_lossy(Some(path.clone()), &mut defs).await;

        let converter = DiagnosticConverter::new(self.documents.clone());
        let diagnostics = converter.errors_to_diagnostics(error, &uri);

        self.client.publish_diagnostics(uri, diagnostics, None).await;

    }
}