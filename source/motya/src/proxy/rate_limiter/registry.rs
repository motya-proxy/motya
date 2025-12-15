use std::{collections::HashMap, sync::Arc};

use motya_config::common_types::{
    definitions_table::DefinitionsTable, rate_limiter::StorageConfig,
};

use crate::proxy::rate_limiter::storage::{MemoryStorage, RateLimitStorage};

#[derive(Clone, Default)]
pub struct StorageRegistry {
    storages: HashMap<String, Arc<dyn RateLimitStorage>>,
}

impl StorageRegistry {
    pub async fn new(table: &DefinitionsTable) -> miette::Result<Self> {
        let mut storages = HashMap::new();

        for (name, config) in table.get_storages() {
            let storage: Arc<dyn RateLimitStorage> = match config {
                StorageConfig::Memory {
                    max_keys,
                    cleanup_interval,
                } => {
                    let mem = MemoryStorage::new(*max_keys as u64, *cleanup_interval);
                    Arc::new(mem)
                }

                StorageConfig::Redis {
                    addresses: _,
                    password: _,
                    timeout: _,
                } => {
                    tracing::warn!(
                        "Redis storage '{}' configured but implementation is pending",
                        name
                    );
                    continue;
                }
            };

            storages.insert(name.clone(), storage);
        }

        Ok(Self { storages })
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn RateLimitStorage>> {
        self.storages.get(name).cloned()
    }
}
