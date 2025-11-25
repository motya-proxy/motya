use std::collections::{HashMap, HashSet};

use kdl::KdlDocument;

use crate::config::{
        common_types::{
            bad::Bad, connectors::ConnectorsSectionParser, file_server::{FileServerConfig, FileServerSectionParser}, listeners::ListenersSectionParser, path_control::PathControlSectionParser, rate_limiter::RateLimitSectionParser, rules::RulesTable, service::{ServiceSection, ServiceSectionParser}, system_data::{SystemData, SystemDataSectionParser}
        },
        internal::{Config, ProxyConfig},
        kdl::{
            connectors::ConnectorsSection, file_server::FileServerSection, listeners::ListenersSection, path_control::PathControlSection, rate_limiter::RateLimitSection, system_data::SystemDataSection
        },
    };

pub mod utils;
pub mod connectors;
pub mod listeners;
pub mod path_control;
pub mod rate_limiter;
pub mod system_data;
pub mod file_server;

/// This is the primary interface for parsing the document.
impl TryFrom<&KdlDocument> for Config {
    type Error = miette::Error;

    fn try_from(raw_config: &KdlDocument) -> Result<Self, Self::Error> {

        let SystemData {
            threads_per_service,
            daemonize,
            upgrade_socket,
            pid_file,
        } = SystemDataSection::new(raw_config).parse_node(raw_config)?;

        let (basic_proxies, file_servers) = extract_services(threads_per_service, raw_config)?;

        Ok(Config {
            threads_per_service,
            daemonize,
            upgrade_socket,
            pid_file,
            basic_proxies,
            file_servers,
            ..Config::default()
        })
    }
}


/// Extract all services from the top level document
fn extract_services(
    threads_per_service: usize,
    doc: &KdlDocument,
) -> miette::Result<(Vec<ProxyConfig>, Vec<FileServerConfig>)> {
    let service_node = utils::required_child_doc(doc, doc, "services")?;
    let services = utils::wildcard_argless_child_docs(doc, service_node)?;

    let proxy_node_set =
        HashSet::from(["listeners", "connectors", "path-control", "rate-limiting"]);
    let file_server_node_set = HashSet::from(["listeners", "file-server"]);

    let mut proxies = vec![];
    let mut file_servers = vec![];

    for (name, service) in services {
        // First, visit all of the children nodes, and make sure each child
        // node only appears once. This is used to detect duplicate sections
        let mut fingerprint_set: HashSet<&str> = HashSet::new();
        for ch in service.nodes() {
            let name = ch.name().value();
            let dupe = !fingerprint_set.insert(name);
            if dupe {
                return Err(Bad::docspan(format!("Duplicate section: '{name}'!"), doc, &ch.span()).into());
            }
        }

        // Now: what do we do with this node?
        if fingerprint_set.is_subset(&proxy_node_set) {
            // If the contained nodes are a strict subset of proxy node config fields,
            // then treat this section as a proxy node
            proxies.push(extract_service(threads_per_service, doc, name, service)?);
        } else if fingerprint_set.is_subset(&file_server_node_set) {
            // If the contained nodes are a strict subset of the file server config
            // fields, then treat this section as a file server node
            file_servers.push(FileServerSection::new(doc, name).parse_node(service)?);
        } else {
            // Otherwise, we're not sure what this node is supposed to be!
            //
            // Obtain the superset of ALL potential nodes, which is essentially
            // our configuration grammar.
            let superset: HashSet<&str> = proxy_node_set
                .union(&file_server_node_set)
                .cloned()
                .collect();

            // Then figure out what fields our fingerprint set contains that
            // is "novel", or basically fields we don't know about
            let what = fingerprint_set
                .difference(&superset)
                .copied()
                .collect::<Vec<&str>>()
                .join(", ");

            // Then inform the user about the reason for our discontent
            return Err(Bad::docspan(
                format!("Unknown configuration section(s): '{what}'"),
                doc,
                &service.span(),
            )
            .into());
        }
    }

    if proxies.is_empty() && file_servers.is_empty() {
        return Err(Bad::docspan("No services defined", doc, &service_node.span()).into());
    }

    Ok((proxies, file_servers))
}



/// Extracts a single service from the `services` block
fn extract_service(
    threads_per_service: usize,
    doc: &KdlDocument,
    name: &str,
    node: &KdlDocument,
) -> miette::Result<ProxyConfig> {
    ServiceSection::<_>::new(
        &ListenersSection::new(doc), 
        &ConnectorsSection::new(doc, &RulesTable), 
        &PathControlSection::new(doc), 
        &RateLimitSection::new(doc, threads_per_service), 
        name
    ).parse_node(node)
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, net::SocketAddr, num::NonZeroUsize};
    use kdl::{KdlDocument, KdlError};
    use lazy_static::lazy_static;
    use pingora::upstreams::peer::HttpPeer;

    use crate::config::internal::Config;

    pub type Result<T> = miette::Result<T>;

    lazy_static! {
        static ref RESOURCE: kdl::KdlDocument = {
            let kdl_contents = std::fs::read_to_string("./assets/test-config.kdl").unwrap();

            kdl_contents.parse().unwrap_or_else(|e| {
                panic!("Error parsing KDL file: {e:?}");
            })
        };
    }

    // #[test]
    // fn load_test() {
    //     let doc = &*RESOURCE;
    //
    //     let val: crate::config::internal::Config = doc.try_into().unwrap_or_else(|e| {
    //         panic!("Error rendering config from KDL file: {e:?}");
    //     });
    //
    //     let expected = crate::config::internal::Config {
    //         validate_configs: false,
    //         threads_per_service: 8,
    //         basic_proxies: vec![
    //             ProxyConfig {
    //                 name: "Example1".into(),
    //                 listeners: vec![
    //                     ListenerConfig {
    //                         source: crate::config::internal::ListenerKind::Tcp {
    //                             addr: "0.0.0.0:8080".into(),
    //                             tls: None,
    //                             offer_h2: false,
    //                         },
    //                     },
    //                     ListenerConfig {
    //                         source: crate::config::internal::ListenerKind::Tcp {
    //                             addr: "0.0.0.0:4443".into(),
    //                             tls: Some(crate::config::internal::TlsConfig {
    //                                 cert_path: "./assets/test.crt".into(),
    //                                 key_path: "./assets/test.key".into(),
    //                             }),
    //                             offer_h2: true,
    //                         },
    //                     },
    //                 ],
    //                 upstreams: vec![Upstream::Service(HttpPeerOptions { peer: HttpPeer::new(
    //                     "91.107.223.4:443",
    //                     true,
    //                     String::from("onevariable.com"),
    //                 ), prefix_path: "/".parse().unwrap(), target_path: "/".parse().unwrap() })],
    //                 path_control: crate::config::internal::PathControl {
    //                     upstream_request_filters: vec![
    //                         BTreeMap::from([
    //                             ("kind".to_string(), "remove-header-key-regex".to_string()),
    //                             ("pattern".to_string(), ".*(secret|SECRET).*".to_string()),
    //                         ]),
    //                         BTreeMap::from([
    //                             ("key".to_string(), "x-proxy-friend".to_string()),
    //                             ("kind".to_string(), "upsert-header".to_string()),
    //                             ("value".to_string(), "river".to_string()),
    //                         ]),
    //                     ],
    //                     upstream_response_filters: vec![
    //                         BTreeMap::from([
    //                             ("kind".to_string(), "remove-header-key-regex".to_string()),
    //                             ("pattern".to_string(), ".*ETag.*".to_string()),
    //                         ]),
    //                         BTreeMap::from([
    //                             ("key".to_string(), "x-with-love-from".to_string()),
    //                             ("kind".to_string(), "upsert-header".to_string()),
    //                             ("value".to_string(), "river".to_string()),
    //                         ]),
    //                     ],
    //                     request_filters: vec![BTreeMap::from([
    //                         ("kind".to_string(), "block-cidr-range".to_string()),
    //                         (
    //                             "addrs".to_string(),
    //                             "192.168.0.0/16, 10.0.0.0/8, 2001:0db8::0/32".to_string(),
    //                         ),
    //                     ])],
    //                 },
    //                 upstream_options: UpstreamOptions {
    //                     selection: crate::config::internal::SelectionKind::Ketama,
    //                     selector: uri_path_selector,
    //                     health_checks: crate::config::internal::HealthCheckKind::None,
    //                     discovery: crate::config::internal::DiscoveryKind::Static,
    //                 },
    //                 rate_limiting: crate::config::internal::RateLimitingConfig {
    //                     rules: vec![
    //                         AllRateConfig::Multi {
    //                             config: MultiRaterConfig {
    //                                 threads: 8,
    //                                 max_buckets: 4000,
    //                                 max_tokens_per_bucket: NonZeroUsize::new(10).unwrap(),
    //                                 refill_interval_millis: NonZeroUsize::new(10).unwrap(),
    //                                 refill_qty: NonZeroUsize::new(1).unwrap(),
    //                             },
    //                             kind: crate::proxy::rate_limiting::multi::MultiRequestKeyKind::SourceIp,
    //                         },
    //                         AllRateConfig::Multi {
    //                             config: MultiRaterConfig {
    //                                 threads: 8,
    //                                 max_buckets: 2000,
    //                                 max_tokens_per_bucket: NonZeroUsize::new(20).unwrap(),
    //                                 refill_interval_millis: NonZeroUsize::new(1).unwrap(),
    //                                 refill_qty: NonZeroUsize::new(5).unwrap(),
    //                             },
    //                             kind: crate::proxy::rate_limiting::multi::MultiRequestKeyKind::Uri {
    //                                 pattern: RegexShim::new("static/.*").unwrap(),
    //                             },
    //                         },
    //                         AllRateConfig::Single {
    //                             config: crate::proxy::rate_limiting::single::SingleInstanceConfig {
    //                                 max_tokens_per_bucket: NonZeroUsize::new(50).unwrap(),
    //                                 refill_interval_millis: NonZeroUsize::new(3).unwrap(),
    //                                 refill_qty: NonZeroUsize::new(2).unwrap(),
    //                             },
    //                             kind: crate::proxy::rate_limiting::single::SingleRequestKeyKind::UriGroup {
    //                                 pattern: RegexShim::new(r".*\.mp4").unwrap(),
    //                             },
    //                         },
    //                     ],
    //                 },
    //             },
    //             ProxyConfig {
    //                 name: "Example2".into(),
    //                 listeners: vec![ListenerConfig {
    //                     source: crate::config::internal::ListenerKind::Tcp {
    //                         addr: "0.0.0.0:8000".into(),
    //                         tls: None,
    //                         offer_h2: false,
    //                     },
    //                 }],
    //                 upstreams: vec![Upstream::Service(HttpPeerOptions { peer: HttpPeer::new("91.107.223.4:80", false, String::new()), prefix_path: "/".parse().unwrap(), target_path: "/".parse().unwrap() })],
    //                 path_control: crate::config::internal::PathControl {
    //                     upstream_request_filters: vec![],
    //                     upstream_response_filters: vec![],
    //                     request_filters: vec![],
    //                 },
    //                 upstream_options: UpstreamOptions::default(),
    //                 rate_limiting: crate::config::internal::RateLimitingConfig { rules: vec![] },
    //             },
    //         ],
    //         file_servers: vec![FileServerConfig {
    //             name: "Example3".into(),
    //             listeners: vec![
    //                 ListenerConfig {
    //                     source: crate::config::internal::ListenerKind::Tcp {
    //                         addr: "0.0.0.0:9000".into(),
    //                         tls: None,
    //                         offer_h2: false,
    //                     },
    //                 },
    //                 ListenerConfig {
    //                     source: crate::config::internal::ListenerKind::Tcp {
    //                         addr: "0.0.0.0:9443".into(),
    //                         tls: Some(crate::config::internal::TlsConfig {
    //                             cert_path: "./assets/test.crt".into(),
    //                             key_path: "./assets/test.key".into(),
    //                         }),
    //                         offer_h2: true,
    //                     },
    //                 },
    //             ],
    //             base_path: Some(".".into()),
    //         }],
    //         daemonize: false,
    //         pid_file: Some("/tmp/river.pidfile".into()),
    //         upgrade_socket: Some("/tmp/river-upgrade.sock".into()),
    //         upgrade: false,
    //     };
    //
    //     assert_eq!(val.validate_configs, expected.validate_configs);
    //     assert_eq!(val.threads_per_service, expected.threads_per_service);
    //     assert_eq!(val.basic_proxies.len(), expected.basic_proxies.len());
    //     assert_eq!(val.file_servers.len(), expected.file_servers.len());
    //
    //     for (abp, ebp) in val.basic_proxies.iter().zip(expected.basic_proxies.iter()) {
    //         let ProxyConfig {
    //             name,
    //             listeners,
    //             upstream_options,
    //             upstreams,
    //             path_control,
    //             rate_limiting,
    //         } = abp;
    //         assert_eq!(*name, ebp.name);
    //         assert_eq!(*listeners, ebp.listeners);
    //         assert_eq!(*upstream_options, ebp.upstream_options);
    //         upstreams
    //             .iter()
    //             .zip(ebp.upstreams.iter())
    //             .for_each(|(a, e)| {
    //                 let a = match a {
    //                     Upstream::Service(s) => s,
    //                     _ => unreachable!()
    //                 };
    //                 let e = match e {
    //                     Upstream::Service(s) => s,
    //                     _ => unreachable!()
    //                 };
    //                 assert_eq!(a.peer._address, e.peer._address);
    //                 assert_eq!(a.peer.scheme, e.peer.scheme);
    //                 assert_eq!(a.peer.sni, e.peer.sni);
    //             });
    //         assert_eq!(*path_control, ebp.path_control);
    //         assert_eq!(*rate_limiting, ebp.rate_limiting);
    //     }
    //
    //     for (afs, efs) in val.file_servers.iter().zip(expected.file_servers.iter()) {
    //         let FileServerConfig {
    //             name,
    //             listeners,
    //             base_path,
    //         } = afs;
    //         assert_eq!(*name, efs.name);
    //         assert_eq!(*listeners, efs.listeners);
    //         assert_eq!(*base_path, efs.base_path);
    //     }
    // }

    fn err_parse_handler(e: KdlError) -> KdlDocument  {
        panic!("Error parsing KDL file: {e:?}");
    }

    fn err_render_config_handler(e: miette::Error) -> Config {
        panic!("Error rendering config from KDL file: {e:?}");
    }

    const SERVICE_WITH_WASM_MODULE : &str = r#"
    services {
        Example {
            listeners {
                "0.0.0.0:8080"
                "0.0.0.0:4443" cert-path="./assets/test.crt" key-path="./assets/test.key" offer-h2=#true
            }
            connectors {
                proxy "127.0.0.1:8000"
            }
            path-control {
                request-filters {
                    filter kind="module" path="./assets/request_filter.wasm"
                }
            }
        }
    }"#;
    #[test]
    fn service_with_wasm_module() {
        let doc = &SERVICE_WITH_WASM_MODULE.parse().unwrap_or_else(err_parse_handler);
        let val: Config = doc.try_into().unwrap_or_else(err_render_config_handler);
        let request_filters = &val.basic_proxies[0].path_control.request_filters[0];

        dbg!(&request_filters);
        assert_eq!(
            val.basic_proxies[0].path_control.request_filters.len(), 1
        );

    }
    const SERVICE_WITHOUT_CONNECTOR: &str = r#"
    services {
        Example {
            listeners {
                "127.0.0.1:80"
            }
            connectors { }
        }
    }
    "#;
    #[test]
    fn service_without_connector() {
        let doc = &SERVICE_WITHOUT_CONNECTOR.parse().unwrap_or_else(err_parse_handler);
        let val: Result<Config> = doc.try_into();
        let msg = val
            .unwrap_err()
            .help()
            .unwrap()
            .to_string();

        assert!(msg.contains("We require at least one connector"));
    }

    const SERVICE_DUPLICATE_LOAD_BALANCE_SECTIONS: &str = r#"
    services {
        Example {
            listeners {
                "127.0.0.1:80"
            }
            connectors {
                load-balance {
                    selection "Ketama" key="UriPath"
                    discovery "Static"
                    health-check "None"
                }
                load-balance {
                    selection "Ketama" key="UriPath"
                    discovery "Static"
                    health-check "None"
                }
                proxy "127.0.0.1:8000"
            }
        }
    }
    "#;
    #[test]
    fn service_duplicate_load_balance_sections() {
        let doc = &SERVICE_DUPLICATE_LOAD_BALANCE_SECTIONS.parse().unwrap_or_else(err_parse_handler);
        let val: Result<Config> = doc.try_into();

        let msg = val
            .unwrap_err()
            .help()
            .unwrap()
            .to_string();

        assert!(msg.contains("Duplicate 'load-balance' section"));
    }

    const SERVICE_BASE_PATH_NOT_EXIST_TEST: &str = r#"
    services {
        Example {
            listeners {
                "127.0.0.1:80"
            }
            file-server { }
        }
    }
    "#;

    #[test]
    fn service_base_path_not_exist() {
        let doc = &SERVICE_BASE_PATH_NOT_EXIST_TEST.parse().unwrap_or_else(err_parse_handler);
        let val: Config = doc.try_into().unwrap_or_else(err_render_config_handler);
        assert_eq!(val.file_servers.len(), 1);
        assert_eq!(val.file_servers[0].base_path, None);
    }

    const SERVICE_EMPTY_LISTENERS_TEST: &str = r#"
    services {
        Example {
            listeners { }
        }
    }
    "#;

    #[test]
    fn service_empty_listeners() {
        let doc = &SERVICE_EMPTY_LISTENERS_TEST.parse().unwrap_or_else(err_parse_handler);
        let val: Result<Config> = doc.try_into();
        let msg = val
            .unwrap_err()
            .help()
            .unwrap()
            .to_string();

        assert!(msg.contains("nonzero listeners required"));
    }

    const SERVICE_INVALID_NODE_TEST: &str = r#"
    services {
        Example {
            invalid-node { }
        }
    }
    "#;

    #[test]
    fn service_invalid_node() {
        let doc = &SERVICE_INVALID_NODE_TEST.parse().unwrap_or_else(err_parse_handler);
        let val: Result<Config> = doc.try_into();
        let msg = val
            .unwrap_err()
            .help()
            .unwrap()
            .to_string();
        
        assert!(msg.contains("Unknown configuration section(s): 'invalid-node'"));
    }

    
    const DUPLICATE_SERVICE_NODES_TEST: &str = r#"
    services {
        Example {
            listeners { }
            listeners { } 
        }
    }
    "#;

    #[test]
    fn duplicate_services() {
        let doc = &DUPLICATE_SERVICE_NODES_TEST.parse().unwrap_or_else(err_parse_handler);
        let val: Result<Config> = doc.try_into();
        let msg = val
            .unwrap_err()
            .help()
            .unwrap()
            .to_string();
        
        assert!(msg.contains("Duplicate section: 'listeners'!"));
    }

    const EMPTY_TEST: &str = "
    ";

    #[test]
    fn empty() {
        let doc = &EMPTY_TEST.parse().unwrap_or_else(err_parse_handler);
        let val: Result<Config> = doc.try_into();
        assert!(val.is_err());
    }

    
    const SERVICES_EMPTY_TEST: &str = "
        services {

        }
    ";

    #[test]
    fn services_empty() {
        let doc = &SERVICES_EMPTY_TEST.parse().unwrap_or_else(err_parse_handler);
        let val: Result<Config> = doc.try_into();
        assert!(val.is_err());
    }

    /// The most minimal config is single services block
    const ONE_SERVICE_TEST: &str = r#"
    services {
        Example {
            listeners {
                "127.0.0.1:80"
            }
            connectors {
                proxy "127.0.0.1:8000"
            }
        }
    }
    "#;

    // #[test]
    // fn one_service() {
    //     let doc: &::kdl::KdlDocument = &ONE_SERVICE_TEST.parse().unwrap_or_else(err_parse_handler);
    //     let val: Config = doc.try_into().unwrap_or_else(err_render_config_handler);
    //     assert_eq!(val.basic_proxies.len(), 1);
    //     assert_eq!(val.basic_proxies[0].listeners.len(), 1);
    //     assert_eq!(
    //         val.basic_proxies[0].listeners[0].source,
    //         ListenerKind::Tcp {
    //             addr: "127.0.0.1:80".into(),
    //             tls: None,
    //             offer_h2: false,
    //         }
    //     );
    //     let upstream = &val.basic_proxies[0].upstreams[0];
    //     let upstream  = match upstream {
    //         Upstream::Service(s) => s,
    //         _ => unreachable!()
    //     };

    //     assert_eq!(
    //         upstream.peer._address,
    //         ("127.0.0.1:8000".parse::<SocketAddr>().unwrap()).into()
    //     );
    // }
}