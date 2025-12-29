use miette::Result;
use std::path::PathBuf;

use crate::common_types::definitions_table::DefinitionsTable;
use crate::config_source::ConfigSource;
use crate::internal::Config;
use crate::kdl::linker::ConfigLinker;
use crate::kdl::models::root::RootDef;
use crate::kdl::parser::ctx::ParseContext;
use crate::kdl::parser::parsable::KdlParsable;

#[allow(async_fn_in_trait)]
pub trait FileConfigLoaderProvider {
    async fn load_entry_point(
        self,
        path: Option<PathBuf>,
        global_definitions: &mut DefinitionsTable,
    ) -> Result<Option<Config>>;
}

#[derive(Clone)]
pub struct ConfigLoader<S: ConfigSource> {
    source: S,
}

impl<S: ConfigSource> FileConfigLoaderProvider for ConfigLoader<S> {
    async fn load_entry_point(
        self,
        path: Option<PathBuf>,
        global_definitions: &mut DefinitionsTable,
    ) -> Result<Option<Config>> {
        if let Some(path) = path {
            let documents = self.source.collect(path).await?;

            let mut roots = Vec::with_capacity(documents.len());
            for (doc, source_name) in documents {
                let ctx = ParseContext::new(doc, &source_name);
                let root = RootDef::parse_node(&ctx, &())?;
                roots.push(root);
            }

            let linker = ConfigLinker::new(global_definitions);
            let config = linker.link(roots)?;

            Ok(Some(config))
        } else {
            Ok(None)
        }
    }
}

impl<S: ConfigSource> ConfigLoader<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use kdl::KdlDocument;
    use miette::Result;

    use crate::{
        common_types::definitions_table::DefinitionsTable,
        config_source::ConfigSource,
        loader::{ConfigLoader, FileConfigLoaderProvider},
    };

    #[derive(Clone, Default)]
    struct MockConfigSource {
        files: Vec<(&'static str, &'static str)>,
    }

    impl MockConfigSource {
        fn new(files: Vec<(&'static str, &'static str)>) -> Self {
            Self { files }
        }
    }

    impl ConfigSource for MockConfigSource {
        async fn collect(&self, _path: PathBuf) -> Result<Vec<(KdlDocument, String)>> {
            let mut docs = Vec::new();
            for (name, content) in &self.files {
                let doc: KdlDocument = content.parse().expect("Invalid KDL in test setup");
                docs.push((doc, name.to_string()));
            }
            Ok(docs)
        }
    }

    #[tokio::test]
    async fn test_full_configuration_snapshot() {
        let kdl_content = r#"
            system {
                threads-per-service 8
                daemonize #false
                pid-file "/tmp/motya.pid"
                providers {
                    files
                }
            }

            definitions {
                storages {
                    memory "main_mem" {
                        max-keys 1000
                        cleanup-interval "60s"
                    }
                }

                rate-limits {
                    policy "api_limit" {
                        key "GLOBAL"
                        algorithm "token_bucket" 
                        rate "10s" 
                        burst 50
                    }
                }

                modifiers {
                    chain-filters "secure-chain" {
                        filter "gzip" name="lol" prop-some="basbdsdb"
                        rate-limit "api_limit"
                    }
                }
            }

            services {
                MyApiProxy {
                    listeners {
                        "0.0.0.0:8080"
                    }
                    connectors {
                        section "/api/v1" {
                            use-chain "secure-chain"
                            proxy "http://127.0.0.1:3000"
                        }

                        section "/health" {
                            return 200 "OK"
                        }
                    }
                }

                MyStaticServer {
                    listeners { "127.0.0.1:9090" }
                    file-server root="/var/www/html"
                }
            }
        "#;

        let source = MockConfigSource::new(vec![("main.kdl", kdl_content)]);
        let loader = ConfigLoader { source };

        let mut table = DefinitionsTable::new_with_global();

        let config = loader
            .load_entry_point(Some(PathBuf::from("dummy")), &mut table)
            .await
            .expect("Should compile without errors")
            .expect("Should return config");

        insta::assert_debug_snapshot!(config);
    }
}
