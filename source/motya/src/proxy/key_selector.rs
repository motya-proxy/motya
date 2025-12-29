use cookie::Cookie;
use http::uri::PathAndQuery;
use motya_config::common_types::key_template::{HashOp, KeyPart, KeyTemplate, TransformOp};
use pingora_load_balancing::{
    prelude::RoundRobin,
    selection::{consistent::KetamaHashing, FNVHash, Random},
    Backend, LoadBalancer,
};
use smallvec::{Array, SmallVec};
use std::hash::Hasher;
use std::{io::Cursor, net::IpAddr};

pub trait KeySourceContext {
    fn get_header(&self, name: &str) -> Option<&str>;
    fn get_cookie(&self, name: &str) -> Option<Cookie<'_>>;
    fn get_ip(&self) -> Option<IpAddr>;
    fn get_path(&self) -> &PathAndQuery;
}

#[derive(Debug, Clone)]
pub struct KeySelector {
    pub extraction_strategies: Vec<KeyTemplate>,
    pub transforms: Vec<TransformOp>,
}

impl KeySelector {
    pub fn select<C: KeySourceContext, A: Array<Item = u8>>(
        &self,
        ctx: &C,
        buffer: &mut SmallVec<A>,
    ) -> bool {
        buffer.clear();
        let mut extracted = false;

        for strategy in &self.extraction_strategies {
            let start_len = buffer.len();

            for part in &strategy.parts {
                match part {
                    KeyPart::Literal(s) => {
                        buffer.extend_from_slice(s.as_bytes());
                    }
                    KeyPart::Header(name) => {
                        if let Some(val) = ctx.get_header(name) {
                            buffer.extend_from_slice(val.as_bytes());
                        }
                    }
                    KeyPart::Cookie(name) => {
                        if let Some(val) = ctx.get_cookie(name) {
                            buffer.extend_from_slice(val.value().as_bytes());
                        }
                    }
                    KeyPart::UriPath => {
                        buffer.extend_from_slice(ctx.get_path().path().as_bytes());
                    }
                    KeyPart::ClientIp => {
                        if let Some(val) = ctx.get_ip() {
                            buffer.extend_from_slice(val.to_string().as_bytes());
                        }
                    }
                    KeyPart::QueryParams(config_str) => {
                        if let Some(request_query) = ctx.get_path().query() {
                            for required_key in config_str.split('&') {
                                let found_val = request_query.split('&').find_map(|pair| {
                                    let mut parts = pair.splitn(2, '=');
                                    let key = parts.next()?;
                                    let val = parts.next().unwrap_or("");

                                    if key == required_key {
                                        Some(val)
                                    } else {
                                        None
                                    }
                                });

                                if let Some(val) = found_val {
                                    buffer.extend_from_slice(val.as_bytes());
                                }
                            }
                        }
                    }
                    KeyPart::UserAgent => {
                        if let Some(val) = ctx.get_header("user-agent") {
                            buffer.extend_from_slice(val.as_bytes());
                        }
                    }
                }
            }

            if buffer.len() > start_len {
                extracted = true;
                break;
            } else {
                buffer.truncate(start_len);
            }
        }

        for transform in &self.transforms {
            apply_transform(transform, buffer);
        }

        extracted
    }
}

fn apply_transform<A: Array<Item = u8>>(op: &TransformOp, buf: &mut SmallVec<A>) {
    match op {
        TransformOp::Lowercase => {
            for b in buf.iter_mut() {
                b.make_ascii_lowercase();
            }
        }
        TransformOp::Truncate { length } => {
            if buf.len() > length.get() {
                buf.truncate(length.get());
            }
        }
        TransformOp::RemoveQueryParams => {
            if let Some(pos) = buf.iter().position(|&b| b == b'?') {
                buf.truncate(pos);
            }
        }
        _ => {}
    }
}

pub fn hash(op: &HashOp, bytes: &[u8]) -> u64 {
    match op {
        HashOp::XxHash32(seed) => xxhash_rust::xxh32::xxh32(bytes, *seed) as u64,
        HashOp::XxHash64(seed) => xxhash_rust::xxh64::xxh64(bytes, *seed),
        HashOp::Murmur3_32(seed) => {
            let mut cursor = Cursor::new(bytes);
            murmur3::murmur3_32(&mut cursor, *seed).unwrap_or(0) as u64
        }
        HashOp::Fnv1a => {
            let mut hasher = fnv::FnvHasher::default();
            hasher.write(bytes);
            hasher.finish()
        }
    }
}

#[cfg(test)]
mod execution_tests {
    use super::*;
    use cookie::Cookie;
    use motya_config::common_types::definitions::BalancerConfig;
    use motya_config::common_types::key_template::{
        parse_hasher, parse_transform, HashAlgorithm, KeyTemplate, Transform,
    };
    use smallvec::SmallVec;
    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr};

    // --- Mock Setup ---

    struct MockContext {
        headers: HashMap<String, String>,
        cookies: HashMap<String, Cookie<'static>>,
        ip: Option<IpAddr>,
        uri: PathAndQuery,
    }

    impl MockContext {
        fn new() -> Self {
            Self {
                headers: HashMap::new(),
                cookies: HashMap::new(),
                ip: Some(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))),
                uri: PathAndQuery::from_static("/"),
            }
        }

        fn with_header(mut self, k: &str, v: &str) -> Self {
            self.headers.insert(k.to_lowercase(), v.to_string());
            self
        }

        fn with_cookie(mut self, k: &str, v: &str) -> Self {
            self.cookies
                .insert(k.to_string(), Cookie::new(k.to_string(), v.to_string()));
            self
        }
        fn with_path(mut self, path: PathAndQuery) -> Self {
            self.uri = path;
            self
        }
    }

    impl KeySourceContext for MockContext {
        fn get_header(&self, name: &str) -> Option<&str> {
            self.headers.get(name).map(|s| s.as_str())
        }
        fn get_cookie(&self, name: &str) -> Option<Cookie<'_>> {
            self.cookies.get(name).cloned()
        }
        fn get_ip(&self) -> Option<IpAddr> {
            self.ip
        }
        fn get_path(&self) -> &PathAndQuery {
            &self.uri
        }
    }

    // --- Helpers ---

    fn build_manual_selector(parts: Vec<KeyPart>, transforms: Vec<TransformOp>) -> KeySelector {
        KeySelector {
            extraction_strategies: vec![KeyTemplate { parts }],
            transforms,
        }
    }

    fn selector_from_config(conf: BalancerConfig) -> KeySelector {
        let mut strategies = vec![conf.source];
        if let Some(fb) = conf.fallback {
            strategies.push(fb);
        }

        KeySelector {
            extraction_strategies: strategies,
            transforms: conf.transforms,
        }
    }

    fn make_config(source: &str, transforms_names: Vec<&str>) -> BalancerConfig {
        let parsed_transforms: Vec<TransformOp> = transforms_names
            .into_iter()
            .map(|name| {
                let mut params = HashMap::new();
                if name == "truncate" {
                    params.insert("length".to_string(), "3".to_string());
                }

                let raw_transform = Transform {
                    name: name.to_string(),
                    params,
                };

                parse_transform(&raw_transform).expect("Invalid transform in test")
            })
            .collect();

        let raw_algo = HashAlgorithm {
            name: "xxhash64".to_string(),
            seed: 0,
        };
        let parsed_algo = parse_hasher(&raw_algo).expect("Invalid hasher in test");

        BalancerConfig {
            source: source.parse().expect("Invalid source template"),
            fallback: None,
            transforms: parsed_transforms,
            algorithm: parsed_algo,
        }
    }

    // --- Tests ---

    #[test]
    fn test_case_sensitivity_without_transform() {
        let selector = build_manual_selector(vec![KeyPart::Header("x-id".to_string())], vec![]);

        let hasher = HashOp::XxHash64(0);

        let ctx_upper = MockContext::new().with_header("x-id", "User123");
        let ctx_lower = MockContext::new().with_header("x-id", "user123");

        let mut buf1: SmallVec<[u8; 256]> = SmallVec::new();
        assert!(selector.select(&ctx_upper, &mut buf1));

        let mut buf2: SmallVec<[u8; 256]> = SmallVec::new();
        assert!(selector.select(&ctx_lower, &mut buf2));

        assert_eq!(String::from_utf8(buf1.to_vec()).unwrap(), "User123");
        assert_eq!(String::from_utf8(buf2.to_vec()).unwrap(), "user123");

        let h1 = hash(&hasher, &buf1);
        let h2 = hash(&hasher, &buf2);
        assert_ne!(h1, h2, "Without 'lowercase', hashes MUST differ");
    }

    #[test]
    fn test_query_params_extraction() {
        let selector =
            build_manual_selector(vec![KeyPart::QueryParams("id&type".to_string())], vec![]);

        let ctx = MockContext::new().with_path(PathAndQuery::from_static(
            "/api?garbage=true&type=admin&id=100",
        ));

        let mut buf: SmallVec<[u8; 256]> = SmallVec::new();
        let found = selector.select(&ctx, &mut buf);

        assert!(found);
        assert_eq!(String::from_utf8(buf.to_vec()).unwrap(), "100admin");
    }

    #[test]
    fn test_extraction_basic_headers() {
        let conf = make_config("${header-x-a}---${header-x-b}", vec![]);
        let selector = selector_from_config(conf);

        let ctx = MockContext::new()
            .with_header("x-a", "Hello")
            .with_header("x-b", "World");

        let mut buf: SmallVec<[u8; 256]> = SmallVec::new();
        let found = selector.select(&ctx, &mut buf);

        assert!(found, "Should find headers");
        assert_eq!(String::from_utf8(buf.to_vec()).unwrap(), "Hello---World");
    }

    #[test]
    fn test_query_params_partial_missing() {
        let selector =
            build_manual_selector(vec![KeyPart::QueryParams("id&token".to_string())], vec![]);

        let ctx = MockContext::new().with_path(PathAndQuery::from_static("/?id=555"));

        let mut buf: SmallVec<[u8; 256]> = SmallVec::new();
        selector.select(&ctx, &mut buf);

        assert_eq!(String::from_utf8(buf.to_vec()).unwrap(), "555");
    }

    #[test]
    fn test_fallback_strategy() {
        let mut conf = make_config("${header-x-missing}", vec![]);
        conf.fallback = Some("${cookie-session}".parse().unwrap());

        let selector = selector_from_config(conf);

        let ctx = MockContext::new().with_cookie("session", "my-cookie-id");

        let mut buf: SmallVec<[u8; 256]> = SmallVec::new();
        let found = selector.select(&ctx, &mut buf);

        assert!(found, "Should fallback to cookie");
        assert_eq!(String::from_utf8(buf.to_vec()).unwrap(), "my-cookie-id");
    }

    #[test]
    fn test_primary_success_prevents_fallback() {
        let mut conf = make_config("${header-x-primary}", vec![]);
        conf.fallback = Some("${cookie-session}".parse().unwrap());

        let selector = selector_from_config(conf);

        let ctx = MockContext::new()
            .with_header("x-primary", "MAIN_VALUE")
            .with_cookie("session", "FALLBACK_VALUE");

        let mut buf: SmallVec<[u8; 256]> = SmallVec::new();
        let found = selector.select(&ctx, &mut buf);

        assert!(found, "Key should be found using the primary strategy");

        let result = String::from_utf8(buf.to_vec()).unwrap();

        assert_eq!(result, "MAIN_VALUE");

        assert!(
            !result.contains("FALLBACK_VALUE"),
            "Fallback value should not be present"
        );
    }

    #[test]
    fn test_transform_lowercase_and_stability() {
        let conf = make_config("${header-x-key}", vec!["lowercase"]);
        let selector = selector_from_config(conf);

        let hasher = HashOp::XxHash64(0);

        let ctx_upper = MockContext::new().with_header("x-key", "VALUE");
        let ctx_lower = MockContext::new().with_header("x-key", "value");
        let ctx_mixed = MockContext::new().with_header("x-key", "VaLuE");

        let mut buf1: SmallVec<[u8; 256]> = SmallVec::new();
        selector.select(&ctx_upper, &mut buf1);
        assert_eq!(String::from_utf8(buf1.to_vec()).unwrap(), "value");

        let mut buf2: SmallVec<[u8; 256]> = SmallVec::new();
        selector.select(&ctx_lower, &mut buf2);

        let mut buf3: SmallVec<[u8; 256]> = SmallVec::new();
        selector.select(&ctx_mixed, &mut buf3);

        let h1 = hash(&hasher, &buf1);
        let h2 = hash(&hasher, &buf2);
        let h3 = hash(&hasher, &buf3);

        assert_eq!(h1, h2, "Hashing MUST be consistent");
        assert_eq!(h2, h3, "Hashing MUST be consistent");
    }

    #[test]
    fn test_transform_remove_query_params() {
        let conf = make_config("${header-x-url}", vec!["remove-query-params"]);
        let selector = selector_from_config(conf);

        let ctx = MockContext::new().with_header("x-url", "/path/to/resource?foo=bar&baz=1");

        let mut buf: SmallVec<[u8; 256]> = SmallVec::new();
        selector.select(&ctx, &mut buf);

        assert_eq!(
            String::from_utf8(buf.to_vec()).unwrap(),
            "/path/to/resource"
        );
    }

    #[test]
    fn test_transform_truncate() {
        let conf = make_config("${header-x-long}", vec!["truncate"]);
        let selector = selector_from_config(conf);

        let ctx = MockContext::new().with_header("x-long", "1234567890");

        let mut buf: SmallVec<[u8; 256]> = SmallVec::new();
        selector.select(&ctx, &mut buf);

        assert_eq!(String::from_utf8(buf.to_vec()).unwrap(), "123");
    }

    #[test]
    fn test_missing_key_returns_false() {
        let conf = make_config("${header-missing}", vec![]);
        let selector = selector_from_config(conf);

        let ctx = MockContext::new();

        let mut buf: SmallVec<[u8; 256]> = SmallVec::new();
        let found = selector.select(&ctx, &mut buf);

        assert!(!found, "Should return false if key extraction failed");
        assert!(buf.is_empty());
    }
}
