use std::{convert::TryFrom, str::FromStr};

use motya_config::common_types::balancer::BalancerConfig;

use crate::proxy::key_selector::KeySelector;

impl TryFrom<BalancerConfig> for KeySelector {
    type Error = String;

    fn try_from(conf: BalancerConfig) -> Result<Self, Self::Error> {
        let mut strategies = Vec::new();

        strategies.push(conf.source);

        if let Some(fallback_str) = conf.fallback {
            strategies.push(fallback_str);
        }

        Ok(KeySelector {
            extraction_strategies: strategies,
            transforms: conf.transforms,
        })
    }
}
