use std::sync::Arc;

use miette::{miette, Result};
use motya_config::common_types::{
    key_template::{KeyTemplate, TransformOp},
    rate_limiter::RateLimitPolicy,
};
use smallvec::{Array, SmallVec};

use crate::proxy::{
    context::SessionInfo,
    key_selector::{KeySelector, KeySourceContext},
    rate_limiter::storage::{RateLimitResult, RateLimitStorage},
};

#[derive(Debug, Clone)]
pub struct RateLimiterInstance {
    storage: Arc<dyn RateLimitStorage>,

    selector: KeySelector,

    rate: f64,
    burst: usize,
}

impl RateLimiterInstance {
    pub fn new(policy: RateLimitPolicy, storage: Arc<dyn RateLimitStorage>) -> Self {
        Self {
            storage,
            selector: KeySelector {
                extraction_strategies: vec![policy.key_template],
                transforms: policy.transforms,
            },
            rate: policy.rate_req_per_sec,
            burst: policy.burst,
        }
    }

    pub async fn check(&self, session: &SessionInfo<'_>) -> Result<RateLimitResult> {
        let mut key_buf: SmallVec<[u8; 256]> = SmallVec::new();

        if !self.selector.select(session, &mut key_buf) {
            return Ok(RateLimitResult {
                allowed: true,
                remaining: self.burst,
                reset_after: std::time::Duration::ZERO,
            });
        }

        let key_str = std::str::from_utf8(&key_buf)
            .map_err(|err| miette!("key is not a valid utf-8, reason: {err}"))?;

        self.storage
            .check_and_update(key_str, self.rate, self.burst, 1)
            .await
    }
}
