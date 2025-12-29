use std::{net::SocketAddr, path::PathBuf};

use miette::Report;
use motya_macro::Parser;

use crate::common_types::system_data::{
    ConfigProvider, FilesProviderConfig, HttpProviderConfig, S3ProviderConfig, SystemData,
};

#[derive(Parser, Clone, Debug)]
pub enum ConfigProviderDef {
    #[node(name = "files")]
    Files {
        #[node(prop)]
        watch: Option<bool>,
    },

    #[node(name = "s3")]
    S3 {
        #[node(prop)]
        bucket: String,
        #[node(prop)]
        key: String,
        #[node(prop)]
        region: String,
        #[node(prop)]
        interval: Option<String>,
        #[node(prop)]
        endpoint: Option<String>,
    },

    #[node(name = "http")]
    Http {
        #[node(prop)]
        address: SocketAddr,
        #[node(prop)]
        path: http::uri::PathAndQuery,
        #[node(prop)]
        persist: Option<bool>,
    },
}

#[derive(Parser, Clone, Debug)]
#[node(name = "providers")]
pub struct ProvidersContainerDef {
    #[node(dynamic_child, min = 1, max = 1)]
    pub providers: Vec<ConfigProviderDef>,
}

#[derive(Parser, Clone, Debug)]
#[node(name = "system")]
pub struct SystemDataDef {
    #[node(child, name = "threads-per-service")]
    pub tps: Option<usize>,

    #[node(child, name = "daemonize")]
    pub daemonize: Option<bool>,

    #[node(child, flat, name = "upgrade-socket")]
    pub upgrade: Option<PathBuf>,

    #[node(child, flat, name = "pid-file")]
    pub pid: Option<PathBuf>,

    #[node(child)]
    pub providers: Option<ProvidersContainerDef>,
}

impl TryFrom<SystemDataDef> for SystemData {
    type Error = Report;

    fn try_from(def: SystemDataDef) -> Result<Self, Self::Error> {
        let data = def;

        let provider = if let Some(container_data) = data.providers {
            container_data
                .providers
                .into_iter()
                .next()
                .map(|provider_def| match provider_def {
                    ConfigProviderDef::Files { watch } => {
                        ConfigProvider::Files(FilesProviderConfig {
                            watch: watch.unwrap_or(false),
                        })
                    }
                    ConfigProviderDef::S3 {
                        bucket,
                        key,
                        region,
                        interval,
                        endpoint,
                    } => ConfigProvider::S3(S3ProviderConfig {
                        bucket,
                        key,
                        region,
                        interval: interval.unwrap_or_else(|| "60s".to_string()),
                        endpoint,
                    }),
                    ConfigProviderDef::Http {
                        address,
                        path,
                        persist,
                    } => ConfigProvider::Http(HttpProviderConfig {
                        address,
                        path,
                        persist: persist.unwrap_or(false),
                    }),
                })
        } else {
            None
        };

        Ok(SystemData {
            threads_per_service: data.tps.unwrap_or(8),
            daemonize: data.daemonize.unwrap_or(false),
            upgrade_socket: data.upgrade,
            pid_file: data.pid,
            provider,
        })
    }
}
