use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use pprof::ProfilerGuardBuilder;
use std::fs::File;
use std::hint::black_box;
use std::io::Write;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use motya::app_context::AppContext;
use motya_config::cli::cli_struct::Cli;
use reqwest::Client;
use tempfile::NamedTempFile;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};
use tokio::runtime::Runtime;
use std::sync::atomic::{AtomicUsize, Ordering};

const KETAMA_CONFIG_TEMPLATE: &str = r#"
services {
    KetamaService {
        listeners {
            "127.0.0.1:__PROXY_PORT__"
        }
        connectors {
            load-balance {
                selection "Ketama" {
                    key "${header-x-my-custom-id}:${cookie-session-id}" \
                        fallback="${client-ip}"
                    
                    transforms-order {
                        lowercase
                        remove-query-params
                        truncate length="64"
                        
                    }
                    
                    algorithm name="__HASH_ALGO__" seed="__SEED__"
                    
                }
            }
            
            section "/" {
                proxy {
                    __BACKEND_LIST__
                }
            }
        }
    }
}
"#;

struct BenchmarkSetup {
    proxy_url: String,
    client: Client,
    request_counter: AtomicUsize,
}

impl BenchmarkSetup {
    async fn new(hash_algo: &str, seed: Option<u32>) -> Self {
        
        println!("Starting 50 backend servers for {}...", hash_algo);
        let mut backend_servers = Vec::new();
        let mut backend_addresses = Vec::new();
        
        for i in 0..50 {
            let server = MockServer::start().await;
            let resp = ResponseTemplate::new(200)
                .set_body_string(format!("OK from backend {}", i))
                .insert_header("Content-Type", "application/json");
            Mock::given(any()).respond_with(resp.clone()).mount(&server).await;
            
            backend_servers.push(server);
            backend_addresses.push(format!("{}:{}", 
                backend_servers[i].address().ip(),
                backend_servers[i].address().port()));
        }
        
        
        let proxy_port = {
            use std::net::TcpListener;
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);
            port
        };
        
        let backend_config = backend_addresses
            .iter()
            .map(|addr| format!("                    server \"{}\"", addr))
            .collect::<Vec<String>>()
            .join("\n");
        
        let config_str = KETAMA_CONFIG_TEMPLATE
            .replace("__PROXY_PORT__", &proxy_port.to_string())
            .replace("__BACKEND_LIST__", &backend_config)
            .replace("__HASH_ALGO__", hash_algo)
            .replace("__SEED__", &seed.unwrap_or(0).to_string());
        
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", config_str).unwrap();
        let config_path = file.path().to_path_buf();
        
        
        println!("Starting Motya proxy with {} on port {}...", hash_algo, proxy_port);
        let cli = Cli {
            validate_configs: false,
            threads_per_service: None,
            config_entry: Some(config_path),
            daemonize: false,
            upgrade: false,
            pidfile: None,
            upgrade_socket: None,
            command: None,
        };
        
        let mut app_ctx = AppContext::bootstrap(cli).await.unwrap();
        let services = app_ctx.build_services().await.unwrap();
        let (mut server, _watcher) = app_ctx.ready();
        server.add_services(services);
        server.bootstrap();
        
        
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            tx.send(()).expect("Failed to send ready signal");
            server.run_forever();
        });

        rx.recv().expect("Server failed to start");

        let proxy_url = format!("http://127.0.0.1:{}", proxy_port);
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(100)
            .build()
            .unwrap();
        
        for i in 0..30 {
            match client.get(&proxy_url).send().await {
                Ok(_) => break,
                Err(e) if i == 29 => panic!("Proxy failed to start: {}", e),
                _ => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
        
        println!("Proxy ready at {}", proxy_url);
        
        Self {
            proxy_url,
            client,
            request_counter: AtomicUsize::new(0)
        }
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    dbg!("start");
    let rt = Runtime::new().unwrap();
    
    
    let setups = rt.block_on(async {
        let xxhash32 = BenchmarkSetup::new("xxhash32", Some(12345)).await;
        let xxhash64 = BenchmarkSetup::new("xxhash64", None).await;
        let murmur3_32 = BenchmarkSetup::new("murmur3_32", Some(42)).await;
        let fnv1a = BenchmarkSetup::new("fnv1a", None).await;
        
        vec![
            ("xxhash32", Arc::new(xxhash32)),
            ("xxhash64", Arc::new(xxhash64)),
            ("murmur3_32", Arc::new(murmur3_32)),
            ("fnv1a", Arc::new(fnv1a)),
        ]
    });
    
    let mut group = c.benchmark_group("ketama_key_extraction_variants");
    group.throughput(Throughput::Elements(1));
    group.sample_size(30);
    group.warm_up_time(Duration::from_secs(2));
    group.measurement_time(Duration::from_secs(8));
    
    let guard = ProfilerGuardBuilder::default()
        .frequency(1000)
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()
        .unwrap();

    
    for (algo_name, setup) in &setups {
        group.bench_function(format!("{}_headers_and_cookies", algo_name), |b| {
            let setup = setup.clone();
            b.to_async(&rt).iter(|| {
                let setup = setup.clone();
                async move {
                    let request_id = setup.request_counter.fetch_add(1, Ordering::Relaxed);
                    let resp = setup.client
                        .get(&setup.proxy_url)
                        .header("X-My-Custom-Id", format!("user-{}", request_id))
                        .header("Cookie", format!("session-id=sess_{}_token", request_id))
                        .send()
                        .await
                        .expect("Request failed");
                    
                    assert_eq!(resp.status(), 200);
                    black_box(resp);
                }
            })
        });
    }
    
    
    for (algo_name, setup) in &setups {
        group.bench_function(format!("{}_fallback_client_ip", algo_name), |b| {
            let setup = setup.clone();
            b.to_async(&rt).iter(|| {
                let setup = setup.clone();
                async move {
                    let request_id = setup.request_counter.fetch_add(1, Ordering::Relaxed);
                    
                    let resp = setup.client
                        .get(&setup.proxy_url)
                        .header("X-My-Custom-Id", format!("ip-user-{}", request_id))
                        
                        .send()
                        .await
                        .expect("Request failed");
                    
                    assert_eq!(resp.status(), 200);
                    black_box(resp);
                }
            })
        });
    }
    
    
    for (algo_name, setup) in &setups {
        group.bench_function(format!("{}_case_variations", algo_name), |b| {
            let setup = setup.clone();
            let cases = vec!["UserSession", "USERSESSION", "usersession", "uSeRsEsSiOn"];
            
            b.to_async(&rt).iter_custom(|iters| {
                let setup = setup.clone();
                let cases = cases.clone();
                async move {
                    let start = std::time::Instant::now();
                    
                    for i in 0..iters {
                        let case_idx = (i as usize) % cases.len();
                        let request_id = setup.request_counter.fetch_add(1, Ordering::Relaxed);
                        
                        let resp = setup.client
                            .get(&setup.proxy_url)
                            .header("X-My-Custom-Id", cases[case_idx])
                            .header("Cookie", format!("session-id=case_test_{}", request_id))
                            .send()
                            .await
                            .expect("Request failed");
                        
                        black_box(resp);
                    }
                    
                    start.elapsed()
                }
            })
        });
    }
    
    
    for (algo_name, setup) in &setups {
        group.bench_function(format!("{}_uri_with_query_params", algo_name), |b| {
            let setup = setup.clone();
            let paths = vec![
                "/api/v1/users?page=1&limit=20",
                "/api/v1/products?category=books&sort=price",
                "/search?q=rust+proxy&filter=recent",
            ];
            
            b.to_async(&rt).iter_custom(|iters| {
                let setup = setup.clone();
                let paths = paths.clone();
                async move {
                    let start = std::time::Instant::now();
                    
                    for i in 0..iters {
                        let path_idx = (i as usize) % paths.len();
                        let request_id = setup.request_counter.fetch_add(1, Ordering::Relaxed);
                        
                        let resp = setup.client
                            .get(format!("{}{}", setup.proxy_url, paths[path_idx]))
                            .header("X-My-Custom-Id", format!("query_test_{}", request_id))
                            .header("Cookie", format!("session-id=q_{}", request_id))
                            .send()
                            .await
                            .expect("Request failed");
                        
                        black_box(resp);
                    }
                    
                    start.elapsed()
                }
            })
        });
    }
    
    
    for (algo_name, setup) in &setups {
        group.bench_function(format!("{}_long_keys_truncate", algo_name), |b| {
            let setup = setup.clone();
            b.to_async(&rt).iter(|| {
                let setup = setup.clone();
                async move {
                    let request_id = setup.request_counter.fetch_add(1, Ordering::Relaxed);
                    
                    let long_session = "a".repeat(100);
                    
                    let resp = setup.client
                        .get(&setup.proxy_url)
                        .header("X-My-Custom-Id", format!("very_long_user_id_{}_with_many_characters", request_id))
                        .header("Cookie", format!("session-id={}", long_session))
                        .send()
                        .await
                        .expect("Request failed");
                    
                    assert_eq!(resp.status(), 200);
                    black_box(resp);
                }
            })
        });
    }
    
    group.finish();
    
    
    let mut comparison_group = c.benchmark_group("hash_algorithms_comparison");
    comparison_group.throughput(Throughput::Elements(1));
    
    for (algo_name, setup) in setups {
        comparison_group.bench_function(algo_name, |b| {
            b.to_async(&rt).iter(|| {
                let setup = setup.clone();
                async move {
                    let request_id = setup.request_counter.fetch_add(1, Ordering::Relaxed);
                    let resp = setup.client
                        .get(&setup.proxy_url)
                        .header("X-My-Custom-Id", format!("bench_{}", request_id))
                        .header("Cookie", format!("session-id=bench_{}", request_id))
                        .send()
                        .await
                        .expect("Request failed");
                    
                    black_box(resp);
                }
            })
        });
    }
    
    comparison_group.finish();

    if let Ok(report) = guard.report().build() {
        let file = File::create("flamegraph.svg").unwrap();
        report.flamegraph(file).unwrap();
    };

}

criterion_group!(balancer, criterion_benchmark);
criterion_main!(balancer);