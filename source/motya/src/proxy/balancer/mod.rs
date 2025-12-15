use motya_config::common_types::key_template::HashOp;
use pingora_load_balancing::{
    prelude::RoundRobin,
    selection::{consistent::KetamaHashing, FNVHash, Random},
    Backend, LoadBalancer,
};
use smallvec::SmallVec;

use crate::proxy::key_selector::{hash, KeySelector, KeySourceContext};

pub mod key_selector_builder;

pub struct Balancer {
    pub selector: Option<KeySelector>,
    pub balancer_type: BalancerType,
    pub hasher: HashOp,
}

impl Balancer {
    pub fn select_backend<C: KeySourceContext>(&self, ctx: &C) -> Option<Backend> {
        if let Some(selector) = &self.selector {
            let mut buffer: SmallVec<[u8; 256]> = SmallVec::new();

            let key = if selector.select(ctx, &mut buffer) {
                hash(&self.hasher, &buffer)
            } else {
                hash(&HashOp::XxHash64(0), &[])
            };

            self.select(&key.to_le_bytes())
        } else {
            self.select(&0u64.to_le_bytes())
        }
    }

    fn select(&self, key: &[u8]) -> Option<Backend> {
        match &self.balancer_type {
            BalancerType::FNVHash(b) => b.select(key, 256),
            BalancerType::Random(b) => b.select(key, 256),
            BalancerType::KetamaHashing(b) => b.select(key, 256),
            BalancerType::RoundRobin(b) => b.select(key, 256),
        }
    }
}

pub enum BalancerType {
    RoundRobin(LoadBalancer<RoundRobin>),
    Random(LoadBalancer<Random>),
    FNVHash(LoadBalancer<FNVHash>),
    KetamaHashing(LoadBalancer<KetamaHashing>),
}
