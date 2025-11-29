use wasmtime::component::{Linker, LinkerInstance};

use crate::proxy::plugins::store::ModuleState;


pub struct PluginHost;

impl PluginHost {

    pub fn register_enviroment(linker: &mut Linker<ModuleState>) -> wasmtime::Result<()> {
        
        wasmtime_wasi::p2::add_to_linker_sync(linker)?;
        Self::register_logger(linker.root().instance("river:request/logger")?)?;

        Ok(())
    }

    fn register_logger(mut logger: LinkerInstance<'_, ModuleState> ) -> wasmtime::Result<()> {

        logger.func_wrap("info", |_, (message, ): (String, )| {
            tracing::info!("WASM LOG: {}", message);
            Ok(())
        })?;

        logger.func_wrap("error", |_, (message, ): (String, )| {
            tracing::error!("WASM LOG: {}", message);
            Ok(())
        })?;

        logger.func_wrap("debug", |_, (message, ): (String, )| {
            tracing::debug!("WASM LOG: {}", message);
            Ok(())
        })?;

        Ok(())
    }

}
