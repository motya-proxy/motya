use std::{fmt::Debug, future, time::Duration};

use async_trait::async_trait;
use miette::{miette, Result};
use moka::future::Cache;
use tokio::time::Instant;

#[derive(Debug)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub remaining: usize,
    pub reset_after: Duration,
}

#[async_trait]
pub trait RateLimitStorage: Send + Sync + Debug {
    async fn check_and_update(
        &self,
        key: &str,
        rate_per_sec: f64,
        burst: usize,
        cost: u32,
    ) -> Result<RateLimitResult>;
}

#[derive(Debug, Clone)]
struct BucketState {
    tokens: f64,
    last_update: Instant,
    last_allowed: bool,
}

#[derive(Debug)]
pub struct MemoryStorage {
    cache: Cache<String, BucketState>,
}

impl MemoryStorage {
    pub fn new(max_keys: u64, cleanup_interval: Duration) -> Self {
        let cache = Cache::builder()
            .max_capacity(max_keys)
            .time_to_idle(cleanup_interval)
            .build();

        Self { cache }
    }
}

#[async_trait]
impl RateLimitStorage for MemoryStorage {
    async fn check_and_update(
        &self,
        key: &str,
        rate_per_sec: f64,
        burst: usize,
        cost: u32,
    ) -> Result<RateLimitResult> {
        let now = Instant::now();
        let burst_f64 = burst as f64;
        let cost_f64 = cost as f64;

        let new_state = self
            .cache
            .entry_by_ref(key)
            .and_upsert_with(|entry| {
                let mut state = entry.map(|v| v.into_value()).unwrap_or(BucketState {
                    tokens: burst_f64,
                    last_update: now,
                    last_allowed: false,
                });

                let elapsed = now.duration_since(state.last_update).as_secs_f64();
                let added = elapsed * rate_per_sec;

                state.tokens = (state.tokens + added).min(burst_f64);

                let allowed = if state.tokens >= cost_f64 {
                    state.tokens -= cost_f64;
                    true
                } else {
                    false
                };

                future::ready(BucketState {
                    tokens: state.tokens,
                    last_update: now,
                    last_allowed: allowed,
                })
            })
            .await
            .into_value();

        let reset_after = if new_state.last_allowed {
            Duration::ZERO
        } else {
            let needed = cost_f64 - new_state.tokens;
            if rate_per_sec > 0.0 {
                Duration::from_secs_f64(needed / rate_per_sec)
            } else {
                return Err(miette!("rate_per_sec value cannot be less than zero"));
            }
        };

        Ok(RateLimitResult {
            allowed: new_state.last_allowed,
            remaining: new_state.tokens as usize,
            reset_after,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::time::{sleep, Duration};

    use super::*;

    fn create_storage() -> MemoryStorage {
        MemoryStorage::new(1000, Duration::from_secs(60))
    }

    #[tokio::test]
    async fn test_basic_allow() {
        let storage = create_storage();
        let key = "test_client";

        let result = storage.check_and_update(key, 10.0, 5, 1).await.unwrap();

        assert!(result.allowed, "First request should be allowed");
        assert_eq!(result.remaining, 4, "Should consume 1 token");
        assert_eq!(result.reset_after, Duration::ZERO);
    }

    #[tokio::test]
    async fn test_burst_depletion() {
        let storage = create_storage();
        let key = "greedy_client";

        let rate = 1.0;
        let burst = 3;

        for i in 0..burst {
            let res = storage.check_and_update(key, rate, burst, 1).await.unwrap();
            assert!(res.allowed, "Request {} should be allowed", i);
            assert_eq!(res.remaining, burst - 1 - i);
        }

        let res = storage.check_and_update(key, rate, burst, 1).await.unwrap();
        assert!(!res.allowed, "Request exceeding burst should be denied");
        assert_eq!(res.remaining, 0);

        assert!(res.reset_after.as_millis() > 900);
    }

    #[tokio::test]
    async fn test_refill_over_time() {
        let storage = create_storage();
        let key = "patient_client";

        let rate = 10.0;
        let burst = 1;

        let res = storage.check_and_update(key, rate, burst, 1).await.unwrap();
        assert!(res.allowed);
        assert_eq!(res.remaining, 0);

        let res = storage.check_and_update(key, rate, burst, 1).await.unwrap();
        assert!(!res.allowed);

        sleep(Duration::from_millis(150)).await;

        let res = storage.check_and_update(key, rate, burst, 1).await.unwrap();
        assert!(res.allowed, "Token should be refilled after wait");
    }

    #[tokio::test]
    async fn test_concurrency_no_race_condition() {
        let storage = Arc::new(create_storage());
        let key = "concurrent_key";

        let rate = 0.01;
        let burst = 100;
        let cost = 1;

        let mut handles = vec![];

        for _ in 0..burst {
            let s = storage.clone();
            let k = key.to_string();
            handles.push(tokio::spawn(async move {
                s.check_and_update(&k, rate, burst, cost).await.unwrap()
            }));
        }

        let mut success_count = 0;
        for h in handles {
            let res = h.await.unwrap();
            if res.allowed {
                success_count += 1;
            }
        }

        assert_eq!(
            success_count, 100,
            "Exactly 100 requests should pass in parallel"
        );

        let final_res = storage
            .check_and_update(key, rate, burst, cost)
            .await
            .unwrap();
        assert!(!final_res.allowed, "Bucket should be exactly empty");
        assert_eq!(final_res.remaining, 0);
    }

    #[tokio::test]
    async fn test_bucket_overflow_protection() {
        let storage = create_storage();
        let key = "overflow_check";

        let rate = 100.0;
        let burst = 5;

        storage.check_and_update(key, rate, burst, 1).await.unwrap();

        sleep(Duration::from_millis(200)).await;

        let res = storage.check_and_update(key, rate, burst, 0).await.unwrap();

        assert_eq!(res.remaining, 5, "Tokens should be capped at burst size");
    }

    #[tokio::test]
    async fn test_cost_higher_than_one() {
        let storage = create_storage();
        let key = "heavy_request";

        let rate = 0.01;
        let burst = 10;
        let cost = 5;

        let res1 = storage
            .check_and_update(key, rate, burst, cost)
            .await
            .unwrap();
        assert!(res1.allowed);
        assert_eq!(res1.remaining, 5);

        let res2 = storage
            .check_and_update(key, rate, burst, cost)
            .await
            .unwrap();
        assert!(res2.allowed);
        assert_eq!(res2.remaining, 0);

        let res3 = storage
            .check_and_update(key, rate, burst, cost)
            .await
            .unwrap();
        assert!(!res3.allowed);
    }
}
