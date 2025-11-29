use std::collections::BTreeMap;

use async_trait::async_trait;
use tokio::sync::Mutex;
use wasmtime::Store;
use crate::proxy::plugins::{store::ModuleState, g};



pub struct Request {
    pub path: String,
    pub method: String,
    pub headers: BTreeMap<String, String>,
}



#[async_trait]
pub trait WasmModuleFilterTrait: Send + Sync {
    async fn call_filter(&self, req: Request) -> wasmtime::Result<bool>;
}

pub struct WasmModuleFilter {
    store: Mutex<Store<ModuleState>>,
    instance: g::FilterWorld
}

#[async_trait]
impl WasmModuleFilterTrait for WasmModuleFilter {
    async fn call_filter(&self, req: Request) -> wasmtime::Result<bool> {
        let mut store = self.store.lock().await;
        let store = &mut *store;
        self.instance.call_filter(store, &req.into())
    }
}

impl From<Request> for g::Request {
    fn from(req: Request) -> Self {
        Self {
            path: req.path,
            method: req.method,
            headers: req.headers.into_iter().map(|(k, v)| {
                g::river::request::r::Pair {
                    name: k,
                    value: v,
                }
            }).collect(),
        }
    }
}


impl WasmModuleFilter {
    pub fn new(store: Mutex<Store<ModuleState>>, instance: g::FilterWorld) -> Self {
        Self { instance, store }
    }
}


#[cfg(test)]
mod tests {

    use wasmtime::Engine;

    use crate::{config::common_types::definitions::PluginSource, proxy::plugins::store::WasmPluginStore};

    use super::*;
    #[tokio::test]
    async fn test_wasm() {
        
        let artifact = WasmPluginStore::create_artifact(
            "example".to_string(), 
            //request_filter.wasm from examples/wasm-module
            &PluginSource::File("./assets/request_filter.wasm".into()), 
            &Engine::default()
        ).await.unwrap();

        let module = WasmPluginStore::create_module(&artifact).unwrap();

        let ping = module
            .call_filter(
                Request { 
                    path: "/something".to_string(),
                    headers: BTreeMap::new(),
                    method: "GET".to_string(),
                }
            ).await.unwrap();

        assert!(!ping);

        let ping = module
            .call_filter(
                Request { 
                    path: "/hubabuba".to_string(),
                    headers: BTreeMap::new(),
                    method: "GET".to_string(),
                }
            ).await.unwrap();

        assert!(ping);
    }
}
