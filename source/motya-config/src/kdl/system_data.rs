use crate::block_parser;
use crate::common_types::system_data::HttpProviderConfig;
use crate::common_types::{
    section_parser::SectionParser,
    system_data::{ConfigProvider, FilesProviderConfig, S3ProviderConfig, SystemData},
};
use crate::kdl::parser::ctx::ParseContext;
use crate::kdl::parser::ensures::Rule;
use crate::kdl::parser::utils::{OptionTypedValueExt, PrimitiveType};
use http::uri::PathAndQuery;
use motya_macro::validate;
use std::net::SocketAddr;
use std::path::PathBuf;

pub struct SystemDataSection;

impl SectionParser<ParseContext<'_>, Option<SystemData>> for SystemDataSection {
    #[validate(ensure_node_name = "system")]
    fn parse_node(&self, ctx: ParseContext) -> miette::Result<Option<SystemData>> {
        self.extract_system_data(ctx)
    }
}

impl SystemDataSection {
    fn extract_system_data(&self, ctx: ParseContext) -> miette::Result<Option<SystemData>> {
        block_parser!(
            ctx,
            tps: optional("threads-per-service") => |ctx| self.parse_threads_per_service(ctx),
            daemonize: optional("daemonize") => |ctx| self.parse_daemonize(ctx),
            upgrade: optional("upgrade-socket") => |ctx| self.parse_upgrade_socket(ctx),
            pid: optional("pid-file") => |ctx| self.parse_pid_file(ctx),
            provider: optional("providers") => |ctx| self.parse_providers(ctx)
        );

        Ok(Some(SystemData {
            threads_per_service: tps.unwrap_or(8),
            daemonize: daemonize.unwrap_or(false),
            upgrade_socket: upgrade,
            pid_file: pid,
            provider,
        }))
    }

    //Procedural macros are crying because of you...
    fn parse_threads_per_service(&self, ctx: ParseContext<'_>) -> miette::Result<usize> {
        ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1)])?;

        ctx.first()?.as_usize()
    }

    fn parse_daemonize(&self, ctx: ParseContext<'_>) -> miette::Result<bool> {
        ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1)])?;
        ctx.first()?.as_bool()
    }

    fn parse_upgrade_socket(&self, ctx: ParseContext<'_>) -> miette::Result<PathBuf> {
        ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1)])?;

        ctx.first()?.parse_as::<PathBuf>()
    }

    fn parse_pid_file(&self, ctx: ParseContext<'_>) -> miette::Result<PathBuf> {
        ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1)])?;

        ctx.first()?.parse_as::<PathBuf>()
    }

    fn parse_providers(&self, providers_ctx: ParseContext<'_>) -> miette::Result<ConfigProvider> {
        providers_ctx.validate(&[Rule::ReqChildren, Rule::NoArgs])?;

        block_parser!(
            providers_ctx.enter_block()?,
            provider: required_any(&["files", "s3", "http"]) => |ctx, name| {
                match name {
                    "files" => self.parse_files_provider(ctx),
                    "s3" => self.parse_s3_provider(ctx),
                    "http" => self.parse_http_provider(ctx),
                    _ => unreachable!("Guaranteed by BlockParser"),
                }
            }
        );

        Ok(provider)
    }

    fn parse_files_provider(&self, ctx: ParseContext<'_>) -> miette::Result<ConfigProvider> {
        ctx.validate(&[
            Rule::NoChildren,
            Rule::NoPositionalArgs,
            Rule::OnlyKeysTyped(&[("watch", PrimitiveType::Bool)]),
        ])?;

        let watch = ctx.opt_prop("watch")?.as_bool()?.unwrap_or(false);
        Ok(ConfigProvider::Files(FilesProviderConfig { watch }))
    }

    fn parse_s3_provider(&self, ctx: ParseContext<'_>) -> miette::Result<ConfigProvider> {
        ctx.validate(&[
            Rule::NoChildren,
            Rule::NoPositionalArgs,
            Rule::OnlyKeysTyped(&[
                ("bucket", PrimitiveType::String),
                ("key", PrimitiveType::String),
                ("region", PrimitiveType::String),
                ("interval", PrimitiveType::String),
                ("endpoint", PrimitiveType::String),
            ]),
        ])?;

        let bucket = ctx.prop("bucket")?.as_str()?;
        let key = ctx.prop("key")?.as_str()?;
        let region = ctx.prop("region")?.as_str()?;

        let interval = ctx
            .opt_prop("interval")?
            .as_str()?
            .unwrap_or_else(|| "60s".to_string());

        let endpoint = ctx.opt_prop("endpoint")?.as_str()?;

        Ok(ConfigProvider::S3(S3ProviderConfig {
            bucket,
            key,
            region,
            interval,
            endpoint,
        }))
    }

    fn parse_http_provider(&self, ctx: ParseContext<'_>) -> miette::Result<ConfigProvider> {
        ctx.validate(&[
            Rule::NoChildren,
            Rule::NoPositionalArgs,
            Rule::OnlyKeysTyped(&[
                ("address", PrimitiveType::String),
                ("path", PrimitiveType::String),
                ("persist", PrimitiveType::Bool),
            ]),
        ])?;

        let addr_str = ctx.prop("address")?.as_str()?;
        let address: SocketAddr = addr_str
            .parse()
            .map_err(|e| ctx.error(format!("Invalid address format: {e}")))?;

        let path = ctx.prop("path")?.parse_as::<PathAndQuery>()?;

        let persist = ctx.opt_prop("persist")?.as_bool()?.unwrap_or(false);

        Ok(ConfigProvider::Http(HttpProviderConfig {
            address,
            path,
            persist,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_err_contains;
    use crate::kdl::parser::block::BlockParser;
    use crate::kdl::parser::ctx::Current;
    use crate::var_registry::VarRegistry;
    use kdl::KdlDocument;

    fn parse_system_with_registry(
        input: &str,
        registry: Option<&VarRegistry>,
    ) -> miette::Result<SystemData> {
        let doc: KdlDocument = input.parse().unwrap();

        let mut ctx = ParseContext::new(&doc, Current::Document(&doc), "test");
        ctx.registry = registry;

        let mut block = BlockParser::new(ctx)?;

        let data = block.required("system", |ctx| SystemDataSection.extract_system_data(ctx))?;

        data.ok_or_else(|| miette::miette!("System section parsed but returned None"))
    }

    fn parse_system(input: &str) -> miette::Result<SystemData> {
        parse_system_with_registry(input, None)
    }

    #[test]
    fn test_threads_from_sys_var() {
        let registry = VarRegistry::new();
        let real_cpu_count = num_cpus::get();

        let input = r#"
        system {
            threads-per-service (var)"num_cpus"
        }
        "#;

        let data = parse_system_with_registry(input, Some(&registry))
            .expect("Should parse with sys variable");

        assert_eq!(
            data.threads_per_service, real_cpu_count,
            "threads-per-service should match actual system CPU count"
        );
    }

    #[test]
    fn test_threads_from_custom_registry_val() {
        let mut registry = VarRegistry::new();

        registry
            .vars
            .insert("num_cpus".to_string(), "42".to_string());

        let input = r#"
        system {
            threads-per-service (var)"num_cpus"
        }
        "#;

        let data = parse_system_with_registry(input, Some(&registry)).expect("Should parse");

        assert_eq!(data.threads_per_service, 42);
    }

    #[test]
    fn test_env_var_injection() {
        let registry = VarRegistry::new();
        unsafe {
            std::env::set_var("TEST_DAEMONIZE", "true");
        }

        let input = r#"
        system {
            daemonize (env)"TEST_DAEMONIZE"
        }
        "#;

        let data =
            parse_system_with_registry(input, Some(&registry)).expect("Should parse env var");

        assert!(data.daemonize);

        unsafe {
            std::env::remove_var("TEST_DAEMONIZE");
        }
    }

    #[test]
    fn test_files_provider() {
        let input = r#"
        system {
            providers {
                files watch=#true
            }
        }
        "#;

        let data = parse_system(input).expect("Should parse files");

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

        let data = parse_system(input).expect("Should parse s3");

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

        let data = parse_system(input).expect("Should parse minimal s3");

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

        let data = parse_system(input).expect("Should parse http");

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

        let data = parse_system(input).expect("Should parse http defaults");

        if let Some(ConfigProvider::Http(cfg)) = data.provider {
            assert!(!cfg.persist);
            assert_eq!(cfg.path, "/update");
        } else {
            panic!("Wrong provider type");
        }
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

        let result = parse_system(input);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().help().unwrap().to_string();

        assert_err_contains!(
            err_msg,
            "Directive 'http' conflicts with 's3' (mutually exclusive)"
        );
    }
}
