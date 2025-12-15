use regex::Regex;
use std::sync::OnceLock;
use std::{collections::HashMap, str::FromStr};

#[derive(Debug, Clone, PartialEq)]
pub enum KeyPart {
    Literal(String),
    UriPath,
    ClientIp,
    UserAgent,
    Header(String),
    Cookie(String),
    QueryParams(String),
}
#[derive(Debug, Clone, PartialEq)]
pub enum TransformOp {
    Lowercase,
    RemoveQueryParams,
    StripTrailingSlash,
    Truncate { length: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub enum HashOp {
    XxHash32(u32),
    XxHash64(u64),
    Murmur3_32(u32),
    Fnv1a,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HashAlgorithm {
    pub name: String,
    pub seed: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transform {
    pub name: String,
    pub params: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyTemplate {
    pub parts: Vec<KeyPart>,
}

impl FromStr for KeyTemplate {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl KeyTemplate {
    pub fn new(template: &str) -> Result<Self, String> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new(r"\$\{([^}]+)\}").unwrap());

        let mut parts = Vec::new();
        let mut last_pos = 0;

        for caps in re.captures_iter(template) {
            let m = caps.get(0).unwrap();
            let var_body = &caps[1];

            if m.start() > last_pos {
                parts.push(KeyPart::Literal(template[last_pos..m.start()].to_string()));
            }

            let part = parse_variable(var_body)?;
            parts.push(part);

            last_pos = m.end();
        }

        if last_pos < template.len() {
            parts.push(KeyPart::Literal(template[last_pos..].to_string()));
        }

        if parts.is_empty() {
            parts.push(KeyPart::Literal(String::new()));
        }

        Ok(Self { parts })
    }
}

fn parse_variable(var: &str) -> Result<KeyPart, String> {
    match var {
        "uri-path" => Ok(KeyPart::UriPath),
        "client-ip" => Ok(KeyPart::ClientIp),
        "user-agent" => Ok(KeyPart::UserAgent),
        s if s.starts_with("header-") => {
            let name = s.strip_prefix("header-").unwrap().to_lowercase();
            if name.is_empty() {
                return Err("Empty header name".to_string());
            }
            Ok(KeyPart::Header(name))
        }
        s if s.starts_with("cookie-") => {
            let name = s.strip_prefix("cookie-").unwrap().to_string();
            if name.is_empty() {
                return Err("Empty cookie name".to_string());
            }
            Ok(KeyPart::Cookie(name))
        }
        s if s.starts_with("query?") => {
            let name = s.strip_prefix("query?").unwrap().to_string();
            if name.is_empty() {
                return Err("Empty query param name".to_string());
            }
            Ok(KeyPart::QueryParams(name))
        }
        unknown => Err(format!("Unknown variable: {}", unknown)),
    }
}

pub fn parse_transform(t: &Transform) -> Result<TransformOp, String> {
    match t.name.as_str() {
        "lowercase" => Ok(TransformOp::Lowercase),
        "remove-query-params" => Ok(TransformOp::RemoveQueryParams),
        "strip-trailing-slash" => Ok(TransformOp::StripTrailingSlash),
        "truncate" => {
            let len_str = t
                .params
                .get("length")
                .ok_or("Missing length param for truncate")?;
            let length = len_str.parse::<usize>().map_err(|_| "Invalid length")?;
            Ok(TransformOp::Truncate { length })
        }
        _ => Err(format!("Unknown transform: {}", t.name)),
    }
}

pub fn parse_hasher(algo: &HashAlgorithm) -> Result<HashOp, String> {
    match algo.name.as_str() {
        "xxhash32" => Ok(HashOp::XxHash32(
            algo.seed.try_into().map_err(|err| format!("{err}"))?,
        )),
        "xxhash64" => Ok(HashOp::XxHash64(
            algo.seed.try_into().map_err(|err| format!("{err}"))?,
        )),
        "murmur3_32" => Ok(HashOp::Murmur3_32(
            algo.seed.try_into().map_err(|err| format!("{err}"))?,
        )),
        "fnv1a" => Ok(HashOp::Fnv1a),
        _ => Err(format!("Unknown hash algorithm: {}", algo.name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_transform(name: &str, params: Vec<(&str, &str)>) -> Transform {
        let mut map = HashMap::new();
        for (k, v) in params {
            map.insert(k.to_string(), v.to_string());
        }
        Transform {
            name: name.to_string(),
            params: map,
        }
    }

    #[test]
    fn test_parse_simple_literals_and_vars() {
        let template_str = "prefix-${client-ip}-suffix";
        let template = KeyTemplate::new(template_str).expect("Should parse successfully");

        assert_eq!(template.parts.len(), 3);
        assert_eq!(template.parts[0], KeyPart::Literal("prefix-".to_string()));
        assert_eq!(template.parts[1], KeyPart::ClientIp);
        assert_eq!(template.parts[2], KeyPart::Literal("-suffix".to_string()));
    }

    #[test]
    fn test_parse_headers_and_cookies() {
        let template_str = "${header-x-my-custom-id}:${cookie-session-id}";
        let template = KeyTemplate::new(template_str).unwrap();

        assert_eq!(template.parts.len(), 3);
        assert_eq!(
            template.parts[0],
            KeyPart::Header("x-my-custom-id".to_string())
        );
        assert_eq!(template.parts[1], KeyPart::Literal(":".to_string()));
        assert_eq!(template.parts[2], KeyPart::Cookie("session-id".to_string()));
    }

    #[test]
    fn test_parse_query_params() {
        let template_str = "${query?search_term}";
        let template = KeyTemplate::new(template_str).unwrap();

        assert_eq!(template.parts.len(), 1);
        assert_eq!(
            template.parts[0],
            KeyPart::QueryParams("search_term".to_string())
        );
    }

    #[test]
    fn test_known_variables() {
        let template_str = "${uri-path}|${user-agent}";
        let template = KeyTemplate::new(template_str).unwrap();

        assert_eq!(template.parts[0], KeyPart::UriPath);
        assert_eq!(template.parts[1], KeyPart::Literal("|".to_string()));
        assert_eq!(template.parts[2], KeyPart::UserAgent);
    }

    #[test]
    fn test_empty_template_gives_empty_literal() {
        let template = KeyTemplate::new("").unwrap();
        assert_eq!(template.parts.len(), 1);
        assert_eq!(template.parts[0], KeyPart::Literal("".to_string()));
    }

    #[test]
    fn test_unknown_variable() {
        let template_str = "${what-is-this}";
        let res = KeyTemplate::new(template_str);

        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Unknown variable: what-is-this");
    }

    #[test]
    fn test_transforms_parsing() {
        let t_lower = create_transform("lowercase", vec![]);
        assert_eq!(parse_transform(&t_lower), Ok(TransformOp::Lowercase));

        let t_remove = create_transform("remove-query-params", vec![]);
        assert_eq!(
            parse_transform(&t_remove),
            Ok(TransformOp::RemoveQueryParams)
        );

        let t_strip = create_transform("strip-trailing-slash", vec![]);
        assert_eq!(
            parse_transform(&t_strip),
            Ok(TransformOp::StripTrailingSlash)
        );

        let t_trunc = create_transform("truncate", vec![("length", "64")]);
        match parse_transform(&t_trunc) {
            Ok(TransformOp::Truncate { length }) => assert_eq!(length, 64),
            res => panic!("Expected Truncate {{ 64 }}, got {:?}", res),
        }
    }

    #[test]
    fn test_truncate_missing_or_invalid_param() {
        let t_missing = create_transform("truncate", vec![]);
        let res_missing = parse_transform(&t_missing);
        assert!(res_missing.is_err());
        assert_eq!(
            res_missing.unwrap_err(),
            "Missing length param for truncate"
        );

        let t_invalid = create_transform("truncate", vec![("length", "foo")]);
        let res_invalid = parse_transform(&t_invalid);
        assert!(res_invalid.is_err());
        assert_eq!(res_invalid.unwrap_err(), "Invalid length");
    }

    #[test]
    fn test_unknown_transform() {
        let t = create_transform("rotate-180", vec![]);
        let res = parse_transform(&t);

        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Unknown transform: rotate-180"));
    }

    #[test]
    fn test_hasher_xxhash32() {
        let algo = HashAlgorithm {
            name: "xxhash32".to_string(),
            seed: 0,
        };
        let op = parse_hasher(&algo).unwrap();

        match op {
            HashOp::XxHash32(seed) => assert_eq!(seed, 0),
            _ => panic!("Expected XxHash32"),
        }
    }

    #[test]
    fn test_hasher_xxhash32_with_seed() {
        let algo = HashAlgorithm {
            name: "xxhash32".to_string(),
            seed: 12345,
        };
        let op = parse_hasher(&algo).unwrap();

        match op {
            HashOp::XxHash32(seed) => assert_eq!(seed, 12345),
            _ => panic!("Expected XxHash32 with seed"),
        }
    }

    #[test]
    fn test_hasher_seed_overflow() {
        #[cfg(target_pointer_width = "64")]
        {
            let big_seed = (u32::MAX as usize) + 1;
            let algo = HashAlgorithm {
                name: "xxhash32".to_string(),
                seed: big_seed,
            };

            let res = parse_hasher(&algo);
            assert!(res.is_err());
        }
    }

    #[test]
    fn test_hasher_unknown() {
        let algo = HashAlgorithm {
            name: "md5-legacy".to_string(),
            seed: 0,
        };
        let res = parse_hasher(&algo);

        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), "Unknown hash algorithm: md5-legacy");
    }
}
