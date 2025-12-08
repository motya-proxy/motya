use std::path::PathBuf;
use miette::Result;

use crate::common_types::definitions_table::DefinitionsTable;
use crate::config_source::ConfigSource;
use crate::internal::Config;
use crate::kdl::compiler::ConfigCompiler;

#[allow(async_fn_in_trait)]
pub trait FileConfigLoaderProvider {
    async fn load_entry_point(
        self,
        path: Option<PathBuf>,
        global_definitions: &mut DefinitionsTable,
    ) -> Result<Option<Config>>;
}

#[derive(Clone)]
pub struct ConfigLoader<S: ConfigSource> {
    source: S
}

impl<S: ConfigSource> FileConfigLoaderProvider for ConfigLoader<S> {
    async fn load_entry_point(
        self,
        path: Option<PathBuf>,
        global_definitions: &mut DefinitionsTable,
    ) -> Result<Option<Config>> {
        if let Some(path) = path {
            
            let documents = self.source
                .collect(path)
                .await?;

            let config = ConfigCompiler::new(documents)
                .compile(global_definitions)?;

            Ok(Some(config))
        } else {
            Ok(None)
        }
    }
}

impl<S: ConfigSource> ConfigLoader<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}