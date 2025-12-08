use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use fqdn::fqdn;
use pprof::ProfilerGuardBuilder;
use reqwest::Client;
use std::{
    fs::File,
    sync::{mpsc, Arc},
    thread,
    time::Duration,
};

use std::collections::HashMap;
use wiremock::{
    matchers::{any, header, method},
    Mock, ResponseTemplate,
};

use http::uri::PathAndQuery;
use motya_config::{
    common_types::{
        connectors::{ALPN, Connectors, HttpPeerConfig, UpstreamConfig, UpstreamContextConfig},
        definitions::{ConfiguredFilter, FilterChain, Modificator, NamedFilterChain},
        definitions_table::DefinitionsTable,
        listeners::{ListenerConfig, ListenerKind, Listeners},
    },
    internal::{Config, ProxyConfig},
};
use pingora::{prelude::HttpPeer, server::Server};
use wiremock::MockServer;

use motya::{app_context::{pingora_opt, pingora_server_conf}, proxy::{
    filters::{chain_resolver::ChainResolver, generate_registry::load_registry},
    motya_proxy_service,
}};

async fn setup_filters() -> String {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(wiremock::matchers::path("/test"))
        .and(header("X-Test", "TEST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    Mock::given(any())
        .respond_with(ResponseTemplate::new(400))
        .mount(&mock_server)
        .await;

    let mut definitions_table = DefinitionsTable::default();
    let registry = load_registry(&mut definitions_table);

    let chain = FilterChain {
        filters: vec![ConfiguredFilter {
            name: fqdn!("motya.request.upsert-header"),
            args: HashMap::from([
                ("key".to_string(), "X-Test".to_string()),
                ("value".to_string(), "TEST".to_string()),
            ]),
        }],
    };

    definitions_table.insert_chain("insert", chain.clone());

    let resolver = ChainResolver::new(definitions_table, Arc::new(registry.into()))
        .await
        .unwrap();

    let config = Config::default();
    let proxy_port = {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    };
    let proxy_addr = format!("127.0.0.1:{proxy_port}");
    let url = format!("http://127.0.0.1:{proxy_port}/test");

    let chains = (1..100)
        .map(|_| {
            Modificator::Chain(NamedFilterChain {
                name: "insert".to_string(),
                chain: chain.clone(),
            })
        })
        .collect::<Vec<_>>();

    let proxy = ProxyConfig {
        connectors: Connectors {
            upstreams: vec![UpstreamContextConfig {
                lb_options: Default::default(),
                chains,
                upstream: UpstreamConfig::Service(HttpPeerConfig {
                    peer_address: *mock_server.address(),
                    alpn: ALPN::H1,
                    prefix_path: PathAndQuery::from_static("/test"),
                    target_path: PathAndQuery::from_static("/"),

                    matcher: Default::default(),
                }),
            }],
            anonymous_definitions: Default::default(),
        },
        listeners: Listeners {
            list_cfgs: vec![ListenerConfig {
                source: ListenerKind::Tcp {
                    addr: proxy_addr.to_string(),
                    offer_h2: false,
                    tls: None,
                },
            }],
        },
        name: "TestServer".to_string(),
    };

    let mut app_server =
        Server::new_with_opt_and_conf(pingora_opt(&config), pingora_server_conf(&config));

    let (proxy_service, _) = motya_proxy_service(proxy, resolver, &app_server)
        .await
        .unwrap();

    app_server.bootstrap();
    app_server.add_services(vec![proxy_service]);

    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let (_mock_server, app_server) = (mock_server, app_server);

        tx.send(()).expect("Failed to send ready signal");
        app_server.run_forever();
    });

    rx.recv().expect("Server failed to start");

    url
}

fn criterion_benchmark(c: &mut Criterion) {
    let url = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(setup_filters());

    let client = Client::builder()
        .pool_idle_timeout(Duration::from_secs(10))
        .pool_max_idle_per_host(100)
        .build()
        .unwrap();

    let url = Box::leak(Box::new(url.as_str()));
    
    let mut group = c.benchmark_group("proxy_throughput");
    
    group.throughput(Throughput::Elements(1));

    let guard = ProfilerGuardBuilder::default()
        .frequency(1000)
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()
        .unwrap();

    group.bench_function("http_proxy_throughput", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let resp = client
                    .get(*url)
                    .send()
                    .await
                    .expect("Failed to send request");
                
                assert_eq!(resp.status(), 200);
            });
    });

    group.finish(); 
    
    if let Ok(report) = guard.report().build() {
        let file = File::create("flamegraph.svg").unwrap();
        report.flamegraph(file).unwrap();
    };
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
