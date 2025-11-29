use std::collections::HashMap;
use std::sync::Arc;
use futures_util::future::join_all;
use wasmtime::{Engine, Store, component::{Component, Linker}};
use miette::{Context, Result, miette};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxView, WasiView};
use wasmtime_wasi_io::IoView;
use crate::{config::common_types::definitions::{DefinitionsTable, PluginSource}, proxy::{filters::registry::{FilterInstance, FilterRegistry}, plugins::{g, host::PluginHost, module::WasmModuleFilter}}};

use super::loader::PluginLoader;

pub struct WasmArtifact {
    pub name: String,
    component: Component,
    engine: Engine,
}

pub struct WasmPluginStore {
    artifacts: HashMap<String, Arc<WasmArtifact>>,
}

impl WasmPluginStore {
    pub async fn compile(table: &DefinitionsTable) -> Result<Self> {
        
        let engine = Engine::default();

        let futures = table.plugins.iter().map(|(name, def)| {
            let engine = engine.clone();
            let name = name.clone();
            let source = def.source.clone();

            async move {
                let artifact = WasmPluginStore::create_artifact(name.clone(), &source, &engine).await?;
                Ok::<_, miette::Report>((name, Arc::new(artifact)))
            }
        });

        
        let results = join_all(futures).await;

        let mut artifacts = HashMap::new();
        for res in results {
            let (name, artifact) = res?;
            artifacts.insert(name, artifact);
        }

        tracing::info!("WasmPluginFactory initialized with {} plugins", artifacts.len());

        Ok(Self { artifacts })
    }

    
    pub fn register_into(&self, registry: &mut FilterRegistry) {
        for (name, artifact) in &self.artifacts {
            
            let artifact_ref = artifact.clone();

            let name = name.clone();
            registry.register_factory(&name.clone(), Box::new(move |_| {
                
                let filter = Self::create_module(
                    &artifact_ref
                ).map_err(|e| 
                    pingora::Error::new(
                        pingora::ErrorType::Custom("Can't create wasm module")).more_context(format!("artifact name: '{name}'. error: {e}")
                    )
                )?;

                Ok(FilterInstance::Action(Box::new(filter)))
            }));
        }
    }

    pub async fn create_artifact(name: String, source: &PluginSource, engine: &Engine) -> Result<WasmArtifact> {
        tracing::debug!("Preparing plugin '{}'...", name);

        PluginLoader::check_availability(source).await
            .wrap_err_with(|| format!("Availability check failed for plugin '{}'", name))?;

        let bytes = PluginLoader::fetch_bytes(source).await
            .wrap_err_with(|| format!("Download failed for plugin '{}'", name))?;

        tracing::debug!("Compiling plugin '{}' ({} bytes)...", name, bytes.len());

        let component = Component::from_binary(engine, &bytes)
            .map_err(|err| miette!("{err}"))?;

        tracing::info!("Plugin '{}' loaded and compiled successfully", name);

        Ok(WasmArtifact {
            name,
            component,
            engine: engine.clone(),
        })
    }

    pub fn create_module(artifact: &WasmArtifact) -> wasmtime::Result<WasmModuleFilter> {
        
        let mut builder = WasiCtx::builder();
        
        let mut store: Store<ModuleState> = Store::new(&artifact.engine, 
            ModuleState {
                ctx: builder.build(),
                table: ResourceTable::new(),
            }
        );

        let mut linker: Linker<ModuleState> = Linker::new(&artifact.engine);

        PluginHost::register_enviroment(&mut linker)?;

        let instance = g::FilterWorld::instantiate(&mut store, &artifact.component, &linker)?;

        Ok(WasmModuleFilter::new(
            store.into(),
            instance,
        ))
    }
}

pub struct ModuleState {
    pub ctx: WasiCtx,
    pub table: ResourceTable,
}


impl WasiView for ModuleState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.ctx, table: &mut self.table }
    }
}

impl IoView for ModuleState {
    fn table(&mut self) -> &mut ResourceTable { &mut self.table }
}


#[cfg(test)]
mod tests {
    use crate::config::common_types::definitions::PluginDefinition;

    use super::*;
    use std::collections::{HashMap, HashSet};
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path};

    const WASM_BYTES: &[u8] = include_bytes!("../../../assets/request_filter.wasm");

    
    fn create_rules_table(plugin_name: &str, source: PluginSource) -> DefinitionsTable {
        let mut plugins = HashMap::new();
        plugins.insert(plugin_name.to_string(), PluginDefinition {
            name: plugin_name.to_string(),
            source,
        });

        DefinitionsTable::new(
            HashSet::new(),
            HashMap::new(),
            plugins
        )
    }

    #[tokio::test]
    async fn test_factory_load_from_url_success() {
        
        let mock_server = MockServer::start().await;

        Mock::given(method("HEAD"))
            .and(path("/filter.wasm"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/filter.wasm"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(WASM_BYTES))
            .mount(&mock_server)
            .await;

        let url = format!("{}/filter.wasm", mock_server.uri());
        let table = create_rules_table("test-plugin", PluginSource::Url(url));

        
        let factory = WasmPluginStore::compile(&table).await.expect("Factory initialization failed");

        assert!(factory.artifacts.contains_key("test-plugin"));
        
        let artifact = factory.artifacts.get("test-plugin").unwrap();
        assert_eq!(artifact.name, "test-plugin");
    }

    #[tokio::test]
    async fn test_factory_load_from_url_404() {
        let mock_server = MockServer::start().await;

        
        Mock::given(method("HEAD"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let url = format!("{}/missing.wasm", mock_server.uri());
        let table = create_rules_table("missing-plugin", PluginSource::Url(url));

        

        let result = WasmPluginStore::compile(&table).await;
        
        assert!(result.is_err());
        let err = result.err().unwrap();
        
        assert!(err.to_string().contains("Availability check failed for plugin 'missing-plugin'"));
    }

    #[tokio::test]
    async fn test_factory_compilation_error_invalid_bytes() {
        let mock_server = MockServer::start().await;

        
        let bad_bytes = b"this is not a wasm binary";

        Mock::given(method("HEAD")).respond_with(ResponseTemplate::new(200)).mount(&mock_server).await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bad_bytes.as_slice()))
            .mount(&mock_server)
            .await;

        let url = format!("{}/bad.wasm", mock_server.uri());
        let table = create_rules_table("bad-plugin", PluginSource::Url(url));

        let result = WasmPluginStore::compile(&table).await;
        
        assert!(result.is_err());
        let err = result.err().unwrap();
        
        assert!(err.to_string().contains("failed to parse WebAssembly module"));
    }

    #[tokio::test]
    async fn test_factory_mixed_sources() {
        
        let mock_server = MockServer::start().await;

        Mock::given(method("HEAD")).respond_with(ResponseTemplate::new(200)).mount(&mock_server).await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(WASM_BYTES))
            .mount(&mock_server)
            .await;

        let url = format!("{}/remote.wasm", mock_server.uri());
        
        
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("local.wasm");
        tokio::fs::write(&file_path, WASM_BYTES).await.unwrap();

        let mut plugins = HashMap::new();
        plugins.insert("remote".to_string(), PluginDefinition {
            name: "remote".to_string(),
            source: PluginSource::Url(url),
        });
        plugins.insert("local".to_string(), PluginDefinition {
            name: "local".to_string(),
            source: PluginSource::File(file_path),
        });

        let table = DefinitionsTable::new(
            HashSet::new(), HashMap::new(), plugins
        );

        let factory = WasmPluginStore::compile(&table).await.expect("Should load mixed sources");
        
        assert!(factory.artifacts.contains_key("remote"));
        assert!(factory.artifacts.contains_key("local"));
    }
}
