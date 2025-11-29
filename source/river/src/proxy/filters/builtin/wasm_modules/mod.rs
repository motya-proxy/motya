use async_trait::async_trait;
use pingora_core::Result;
use pingora_proxy::Session;
use crate::proxy::{RiverContext, filters::types::RequestFilterMod, plugins::module::{self, WasmModuleFilter, WasmModuleFilterTrait}};


#[async_trait]
impl RequestFilterMod for WasmModuleFilter {
    async fn request_filter(&self, session: &mut Session, _ctx: &mut RiverContext) -> Result<bool> {
        
        let req_header = session.downstream_session.req_header(); 
        
        let req = module::Request {
            path: req_header.uri.path().to_string(),                                        // TODO: So many allocations... 
            method: req_header.method.to_string(),                                          // it's like a string factory 
            headers: req_header.headers.iter().map(|(k, v)| {    // that works 24/7
                (k.as_str().to_string(), v.to_str().unwrap_or_default().to_string())
            }).collect(),                                                                   // Memory usage goes brrrrrr
        };

        match self.call_filter(req).await {
            Ok(res) => {
                tracing::info!("Wasm module request filter returned: {res}");
                Ok(res) 
            },
            Err(e) => {
                tracing::error!("Error calling wasm module request filter: {e:?}");
                Err(pingora_core::Error::new_str("Wasm module error"))
            }
        }
    }
}
