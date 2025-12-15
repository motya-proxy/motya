use std::time::Duration;

use crate::common_types::key_template::{KeyTemplate, TransformOp};

#[derive(Debug, Clone, PartialEq)]
pub enum StorageConfig {
    Memory {
        max_keys: usize,
        cleanup_interval: Duration,
    },
    Redis {
        addresses: Vec<String>,
        password: Option<String>,
        timeout: Option<Duration>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RateLimitPolicy {
    pub name: String,
    pub algorithm: String,
    pub storage_key: String,
    pub transforms: Vec<TransformOp>,
    pub key_template: KeyTemplate,
    pub rate_req_per_sec: f64,
    pub burst: usize,
}
