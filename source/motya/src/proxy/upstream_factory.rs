use std::collections::BTreeSet;

use futures_util::FutureExt;
use miette::{miette, Result};
use motya_config::{
    common_types::{
        balancer::SelectionKind, connectors::{MultiServerUpstreamConfig, UpstreamConfig, UpstreamContextConfig}, definitions::Modificator, key_template::HashOp
    },
    internal::UpstreamOptions,
};
use pingora::prelude::HttpPeer;
use pingora_load_balancing::{
    discovery,
    prelude::RoundRobin,
    selection::{consistent::KetamaHashing, FNVHash, Random},
    Backend, Backends, LoadBalancer,
};

use crate::proxy::{
    balancer::{Balancer, BalancerType},
    filters::chain_resolver::ChainResolver,
    key_selector::KeySelector,
    upstream_router::UpstreamContext,
};

#[derive(Clone)]
pub struct UpstreamFactory {
    resolver: ChainResolver,
}

impl UpstreamFactory {
    pub fn new(resolver: ChainResolver) -> Self {
        Self { resolver }
    }

    pub async fn create_context(&self, config: UpstreamContextConfig) -> Result<UpstreamContext> {
        let balancer = match &config.upstream {
            UpstreamConfig::Static(_) | UpstreamConfig::Service(_) => None,
            UpstreamConfig::MultiServer(m) => {
                if let Some(lb_options) = config.lb_options {
                    setup_balancer(lb_options, m)?
                } else {
                    None
                }
            }
        };

        let mut chains = Vec::new();

        for modificator in config.chains {
            match modificator {
                Modificator::Chain(named_chain) => {
                    let chain = self.resolver.resolve(&named_chain.name).await?;
                    chains.push(chain);
                }
            }
        }

        let ctx = UpstreamContext {
            balancer,
            upstream: config.upstream,
            chains,
        };

        Ok(ctx)
    }
}

fn setup_balancer(
    lb_options: UpstreamOptions,
    m: &MultiServerUpstreamConfig,
) -> Result<Option<Balancer>, miette::Error> {
    let addrs = m.servers.iter().map(|s| (&s.address, s.weight));
    let mut backends = addrs
        .clone()
        .map(|(addr, weight)| {
            Backend::new_with_weight(&addr.to_string(), weight)
                .expect("never fail because addr is already IpAddr")
        })
        .collect::<Vec<_>>();
    for (backend, (addr, _)) in backends.iter_mut().zip(addrs) {
        assert!(backend
            .ext
            .insert(HttpPeer::new(
                addr,
                //sni is https only
                //https://github.com/cloudflare/pingora/blob/main/docs/user_guide/peer.md
                m.tls_sni.is_some(),
                m.tls_sni.clone().unwrap_or("".to_string())
            ))
            .is_none());
    }
    let disco = discovery::Static::new(BTreeSet::from_iter(backends));
    let balancer_type = match lb_options.selection {
        SelectionKind::FvnHash => {
            BalancerType::FNVHash(LoadBalancer::<FNVHash>::from_backends(Backends::new(disco)))
        }
        SelectionKind::RoundRobin => BalancerType::RoundRobin(
            LoadBalancer::<RoundRobin>::from_backends(Backends::new(disco)),
        ),
        SelectionKind::Random => {
            BalancerType::Random(LoadBalancer::<Random>::from_backends(Backends::new(disco)))
        }
        SelectionKind::KetamaHashing => BalancerType::KetamaHashing(
            LoadBalancer::<KetamaHashing>::from_backends(Backends::new(disco)),
        ),
    };
    match &balancer_type {
        BalancerType::FNVHash(b) => b.update().now_or_never(),
        BalancerType::KetamaHashing(b) => b.update().now_or_never(),
        BalancerType::Random(b) => b.update().now_or_never(),
        BalancerType::RoundRobin(b) => b.update().now_or_never(),
    }
    .expect("static should not block")
    .expect("static should not error");

    let alg = lb_options
        .template
        .as_ref()
        .map(|cfg| cfg.algorithm.clone())
        .unwrap_or(HashOp::XxHash64(0));

    Ok(Some(Balancer {
        selector: lb_options
            .template
            .map(KeySelector::try_from)
            .transpose()
            .map_err(|err| miette!("{err}"))?,
        balancer_type,
        hasher: alg,
    }))
}
