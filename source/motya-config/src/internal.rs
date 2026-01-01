use std::path::PathBuf;

use crate::
    common_types::{
        balancer::{BalancerConfig, DiscoveryKind, HealthCheckKind, SelectionKind},
        connectors::Connectors,
        file_server::FileServerConfig,
        listeners::Listeners,
    }
;

/// Motya's internal configuration
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub validate_configs: bool,
    pub threads_per_service: usize,
    pub daemonize: bool,
    pub pid_file: Option<PathBuf>,
    pub upgrade_socket: Option<PathBuf>,
    pub upgrade: bool,
    pub basic_proxies: Vec<ProxyConfig>,
    pub file_servers: Vec<FileServerConfig>,
}

//
// Basic Proxy Configuration
//
#[derive(Clone, Debug, PartialEq)]
pub struct ProxyConfig {
    pub name: String,
    pub listeners: Listeners,
    pub connectors: Connectors,
}

#[derive(Debug, PartialEq, Clone)]
pub struct UpstreamOptions {
    pub selection: SelectionKind,
    pub template: Option<BalancerConfig>,
    pub health_checks: HealthCheckKind,
    pub discovery: DiscoveryKind,
}

impl Default for UpstreamOptions {
    fn default() -> Self {
        Self {
            selection: SelectionKind::RoundRobin,
            template: None,
            health_checks: HealthCheckKind::None,
            discovery: DiscoveryKind::Static,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            validate_configs: false,
            threads_per_service: 8,
            basic_proxies: vec![],
            file_servers: vec![],
            daemonize: false,
            pid_file: None,
            upgrade_socket: Some(PathBuf::from("/tmp/motya-upgrade.sock")),
            upgrade: false,
        }
    }
}
