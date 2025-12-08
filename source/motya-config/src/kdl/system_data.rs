use crate::common_types::system_data::HttpProviderConfig;
use crate::{
    common_types::{
        bad::{Bad, OptExtParse},
        section_parser::SectionParser,
        system_data::{ConfigProvider, FilesProviderConfig, S3ProviderConfig, SystemData},
    },
    kdl::utils::{self, HashMapValidationExt},
};
use kdl::{KdlDocument, KdlEntry, KdlNode};
use std::path::PathBuf;
use std::{collections::HashMap, net::SocketAddr};

pub struct SystemDataSection<'a> {
    doc: &'a KdlDocument,
    name: &'a str
}

impl SectionParser<KdlDocument, Option<SystemData>> for SystemDataSection<'_> {
    fn parse_node(&self, _: &KdlDocument) -> miette::Result<Option<SystemData>> {
        self.extract_system_data()
    }
}

impl<'a> SystemDataSection<'a> {
    pub fn new(doc: &'a KdlDocument, name: &'a str) -> Self {
        Self { doc, name }
    }

    fn extract_system_data(&self) -> miette::Result<Option<SystemData>> {
        let Some(sys) = utils::optional_child_doc(self.doc, self.doc, "system") else {
            return Ok(None);
        };

        let tps = self.extract_threads_per_service(sys)?;

        let daemonize = if let Some(n) = sys.get("daemonize") {
            utils::extract_one_bool_arg(self.doc, n, "daemonize", n.entries(), self.name)?
        } else {
            false
        };

        let upgrade_socket = if let Some(n) = sys.get("upgrade-socket") {
            let x = utils::extract_one_str_arg(self.doc, n, "upgrade-socket", n.entries(), self.name, |s| {
                Some(PathBuf::from(s))
            })?;
            Some(x)
        } else {
            None
        };

        let pid_file = if let Some(n) = sys.get("pid-file") {
            let x = utils::extract_one_str_arg(self.doc, n, "pid-file", n.entries(), self.name, |s| {
                Some(PathBuf::from(s))
            })?;
            Some(x)
        } else {
            None
        };

        let provider = self.extract_provider_config(sys)?;

        Ok(Some(SystemData {
            threads_per_service: tps,
            daemonize,
            upgrade_socket,
            pid_file,
            provider,
        }))
    }

    fn extract_threads_per_service(&self, sys: &KdlDocument) -> miette::Result<usize> {
        let Some(tps) = sys.get("threads-per-service") else {
            return Ok(8);
        };

        let [tps_node] = tps.entries() else {
            return Err(Bad::docspan(
                "system > threads-per-service should have exactly one entry",
                self.doc,
                &tps.span(), self.name
            )
            .into());
        };

        let val = tps_node.value().as_integer().or_bail(
            "system > threads-per-service should be an integer",
            self.doc,
            &tps_node.span(), self.name
        )?;
        val.try_into().ok().or_bail(
            "system > threads-per-service should fit in a usize",
            self.doc,
            &tps_node.span(), self.name
        )
    }

    fn extract_provider_config(&self, sys: &KdlDocument) -> miette::Result<Option<ConfigProvider>> {
        let Some(providers_doc) = utils::optional_child_doc(self.doc, sys, "providers") else {
            return Ok(None);
        };

        let nodes = utils::data_nodes(self.doc, providers_doc)?;

        if nodes.is_empty() {
            return Ok(None);
        }

        if nodes.len() > 1 {
            let (node, _, _) = nodes[1];
            return Err(Bad::docspan(
                "Multiple providers defined. Only one configuration provider is allowed at a time.",
                self.doc,
                &node.span(), self.name
            )
            .into());
        }

        let (node, name, args) = nodes[0];

        let mut args_map = utils::str_value_args(self.doc, args, self.name)?
            .into_iter()
            .collect::<HashMap<&str, &KdlEntry>>();

        match name {
            "files" => {
                args_map = args_map.ensure_only_keys(&["watch"], self.doc, node, self.name)?;

                let watch = self.opt_bool("watch", &args_map, false)?;

                Ok(Some(ConfigProvider::Files(FilesProviderConfig { watch })))
            }
            "s3" => {
                args_map = args_map.ensure_only_keys(
                    &["bucket", "key", "region", "interval", "endpoint"],
                    self.doc,
                    node,
                    self.name
                )?;

                let bucket = self.req_str("bucket", &args_map, node)?;
                let key = self.req_str("key", &args_map, node)?;
                let region = self.req_str("region", &args_map, node)?;

                let interval = self.opt_str("interval", &args_map)?
                    .unwrap_or_else(|| "60s".to_string());
                let endpoint = self.opt_str("endpoint", &args_map)?;

                Ok(Some(ConfigProvider::S3(S3ProviderConfig {
                    bucket,
                    key,
                    region,
                    interval,
                    endpoint,
                })))
            }
            "http" => {
                args_map =
                    args_map.ensure_only_keys(&["address", "path", "persist"], self.doc, node, self.name)?;

                let addr_str = self.req_str("address", &args_map, node)?;
                let address: SocketAddr = addr_str.parse().map_err(|e| {
                    Bad::docspan(
                        format!("Invalid address format: {e}"),
                        self.doc,
                        &node.span(), self.name
                    )
                })?;

                let path = self.req_str("path", &args_map, node)?;
                if !path.starts_with('/') {
                    return Err(
                        Bad::docspan("Path must start with '/'", self.doc, &node.span(), self.name).into(),
                    );
                }

                let persist = self.opt_bool("persist", &args_map, false)?;

                Ok(Some(ConfigProvider::Http(HttpProviderConfig {
                    address,
                    path,
                    persist,
                })))
            }
            unknown => Err(Bad::docspan(
                format!("Unknown provider type: '{unknown}'. Supported: 'files', 's3', 'http'"),
                self.doc,
                &node.span(), self.name
            )
            .into()),
        }
    }

    fn req_str(
        &self,
        key: &str,
        map: &HashMap<&str, &KdlEntry>,
        parent: &KdlNode,
    ) -> miette::Result<String> {
        let entry = map.get(key).or_bail(
            format!("Missing required argument: '{key}'"),
            self.doc,
            &parent.span(), self.name
        )?;
        entry.value().as_string().map(|s| s.to_string()).or_bail(
            format!("'{key}' must be a string"),
            self.doc,
            &entry.span(), self.name
        )
    }

    fn opt_str(
        &self,
        key: &str,
        map: &HashMap<&str, &KdlEntry>,
    ) -> miette::Result<Option<String>> {
        if let Some(entry) = map.get(key) {
            let s = entry.value().as_string().map(|s| s.to_string()).or_bail(
                format!("'{key}' must be a string"),
                self.doc,
                &entry.span(), self.name
            )?;
            Ok(Some(s))
        } else {
            Ok(None)
        }
    }

    fn opt_bool(
        &self,
        key: &str,
        map: &HashMap<&str, &KdlEntry>,
        default: bool,
    ) -> miette::Result<bool> {
        if let Some(entry) = map.get(key) {
            entry.value().as_bool().or_bail(
                format!("'{key}' must be a boolean"),
                self.doc,
                &entry.span(), self.name
            )
        } else {
            Ok(default)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_files_provider() {
        let input = r#"
        system {
            providers {
                files watch=#true
            }
        }
        "#;
        let doc: KdlDocument = input.parse().unwrap();
        let parser = SystemDataSection::new(&doc, "test");
        let data = parser.extract_system_data().expect("Should parse files").unwrap();

        if let Some(ConfigProvider::Files(cfg)) = data.provider {
            assert!(cfg.watch);
        } else {
            panic!("Wrong provider type");
        }
    }

    #[test]
    fn test_s3_provider_full() {
        let input = r#"
        system {
            providers {
                s3 bucket="configs" key="prod.kdl" region="us-east-1" interval="10s" endpoint="http://minio:9000"
            }
        }
        "#;
        let doc: KdlDocument = input.parse().unwrap();
        let parser = SystemDataSection::new(&doc, "test");
        let data = parser.extract_system_data().expect("Should parse s3").unwrap();

        if let Some(ConfigProvider::S3(cfg)) = data.provider {
            assert_eq!(cfg.bucket, "configs");
            assert_eq!(cfg.key, "prod.kdl");
            assert_eq!(cfg.region, "us-east-1");
            assert_eq!(cfg.interval, "10s");
            assert_eq!(cfg.endpoint, Some("http://minio:9000".to_string()));
        } else {
            panic!("Wrong provider type");
        }
    }

    #[test]
    fn test_s3_provider_minimal() {
        let input = r#"
        system {
            providers {
                s3 bucket="configs" key="prod.kdl" region="eu-central-1"
            }
        }
        "#;
        let doc: KdlDocument = input.parse().unwrap();
        let parser = SystemDataSection::new(&doc, "test");
        let data = parser
            .extract_system_data()
            .expect("Should parse minimal s3").unwrap();

        if let Some(ConfigProvider::S3(cfg)) = data.provider {
            assert_eq!(cfg.region, "eu-central-1");
            assert_eq!(cfg.interval, "60s"); // Default
            assert_eq!(cfg.endpoint, None);
        } else {
            panic!("Wrong provider type");
        }
    }

    #[test]
    fn test_http_provider_persist() {
        let input = r#"
        system {
            providers {
                http address="127.0.0.1:9090" path="/admin/config" persist=#true
            }
        }
        "#;
        let doc: KdlDocument = input.parse().unwrap();
        let parser = SystemDataSection::new(&doc, "test");
        let data = parser.extract_system_data().expect("Should parse http").unwrap();

        if let Some(ConfigProvider::Http(cfg)) = data.provider {
            assert_eq!(cfg.address.port(), 9090);
            assert_eq!(cfg.path, "/admin/config");
            assert!(cfg.persist);
        } else {
            panic!("Wrong provider type");
        }
    }

    #[test]
    fn test_http_provider_in_memory_default() {
        let input = r#"
        system {
            providers {
                http address="0.0.0.0:8000" path="/update"
            }
        }
        "#;
        let doc: KdlDocument = input.parse().unwrap();
        let parser = SystemDataSection::new(&doc, "test");
        let data = parser
            .extract_system_data()
            .expect("Should parse http defaults").unwrap();

        if let Some(ConfigProvider::Http(cfg)) = data.provider {
            assert!(!cfg.persist);
            assert_eq!(cfg.path, "/update");
        } else {
            panic!("Wrong provider type");
        }
    }

    #[test]
    fn test_http_bad_path() {
        let input = r#"
        system {
            providers {
                http address="0.0.0.0:80" path="no-slash"
            }
        }
        "#;
        let doc: KdlDocument = input.parse().unwrap();
        let parser = SystemDataSection::new(&doc, "test");
        let result = parser.extract_system_data();

        let err_msg = result.unwrap_err().help().unwrap().to_string();
        crate::assert_err_contains!(err_msg, "Path must start with '/'");
    }

    #[test]
    fn test_conflict_providers() {
        let input = r#"
        system {
            providers {
                s3 bucket="b" key="k" region="r"
                http address="127.0.0.1:80" path="/"
            }
        }
        "#;
        let doc: KdlDocument = input.parse().unwrap();
        let parser = SystemDataSection::new(&doc, "test");
        let result = parser.extract_system_data();

        let err_msg = result.unwrap_err().help().unwrap().to_string();
        crate::assert_err_contains!(err_msg, "Only one configuration provider is allowed");
    }
}
