use motya_macro::validate;

use crate::common_types::{
    definitions_table::DefinitionsTable, file_server::FileServerConfig, listeners::Listeners,
    section_parser::SectionParser, services::ServicesConfig,
};
use crate::{
    internal::ProxyConfig,
    kdl::{
        connectors::ConnectorsSection,
        file_server::FileServerSection,
        listeners::ListenersSection,
        parser::{block::BlockParser, ctx::ParseContext},
    },
};

#[derive(Debug)]
pub enum ServiceConfig {
    Proxy(ProxyConfig),
    FileServer(FileServerConfig),
}

pub struct ServicesSection<'a> {
    global_definitions: &'a DefinitionsTable,
}

impl SectionParser<ParseContext<'_>, ServicesConfig> for ServicesSection<'_> {
    #[validate(ensure_node_name = "services")]
    fn parse_node(&self, ctx: ParseContext) -> miette::Result<ServicesConfig> {
        self.parse(ctx)
    }
}

impl<'a> ServicesSection<'a> {
    pub fn new(global_definitions: &'a DefinitionsTable) -> Self {
        Self { global_definitions }
    }

    pub fn parse(&self, ctx: ParseContext) -> miette::Result<ServicesConfig> {
        let mut proxies: Vec<ProxyConfig> = vec![];
        let mut file_servers: Vec<FileServerConfig> = vec![];

        for node in ctx.nodes()? {
            match self.parse_service(node)? {
                ServiceConfig::FileServer(fs) => file_servers.push(fs),
                ServiceConfig::Proxy(proxy) => proxies.push(proxy),
            }
        }

        Ok(ServicesConfig {
            proxies,
            file_servers,
        })
    }

    fn parse_service(&self, service_ctx: ParseContext<'_>) -> miette::Result<ServiceConfig> {
        let service_name = service_ctx.name()?.to_string();
        let mut block = BlockParser::new(service_ctx.clone())?;

        let listeners = block.required("listeners", |ctx| ListenersSection.parse_node(ctx))?;

        let service_type =
            block.required_any(&["connectors", "file-server"], |ctx, name| match name {
                "connectors" => self.parse_proxy(ctx, listeners, &service_name),
                "file-server" => self.parse_file_server(ctx, listeners, &service_name),
                _ => unreachable!("Guaranteed by BlockParser"),
            })?;

        block.exhaust()?;

        Ok(service_type)
    }

    fn parse_proxy(
        &self,
        ctx: ParseContext<'_>,
        listeners: Listeners,
        service_name: &str,
    ) -> miette::Result<ServiceConfig> {
        let connectors = ConnectorsSection::new(self.global_definitions).parse_node(ctx)?;

        Ok(ServiceConfig::Proxy(ProxyConfig {
            name: service_name.to_string(),
            listeners,
            connectors,
        }))
    }

    fn parse_file_server(
        &self,
        ctx: ParseContext<'_>,
        listeners: Listeners,
        service_name: &str,
    ) -> miette::Result<ServiceConfig> {
        let file_server = FileServerSection::new(service_name).parse_node(ctx)?;

        Ok(ServiceConfig::FileServer(FileServerConfig {
            name: service_name.to_string(),
            listeners,
            base_path: file_server.base_path,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kdl::parser::block::BlockParser;
    use crate::{assert_err_contains, kdl::parser::ctx::Current};
    use kdl::KdlDocument;

    fn parse_services(input: &str) -> miette::Result<ServicesConfig> {
        let doc: KdlDocument = input.parse().unwrap();

        let table = DefinitionsTable::default();

        let ctx = ParseContext::new(&doc, Current::Document(&doc), "test");
        let mut block = BlockParser::new(ctx)?;

        block.required("services", |ctx| {
            ServicesSection::new(&table).parse_node(ctx)
        })
    }

    const PROXY_SERVICE: &str = r#"
        services {
            MyProxy {
                listeners { "127.0.0.1:8080" }
                connectors {
                    return code=200 response="OK"
                }
            }
        }
    "#;

    #[test]
    fn test_parse_proxy_service() {
        let config = parse_services(PROXY_SERVICE).expect("Should parse proxy service");

        assert_eq!(config.proxies.len(), 1);
        assert_eq!(config.file_servers.len(), 0);

        let proxy = &config.proxies[0];
        assert_eq!(proxy.name, "MyProxy");
    }

    const FILE_SERVER_SERVICE: &str = r#"
        services {
            StaticFiles {
                listeners { "127.0.0.1:8080" }
                file-server base-path="/var/www"
            }
        }
    "#;

    #[test]
    fn test_parse_file_server_service() {
        let config = parse_services(FILE_SERVER_SERVICE).expect("Should parse file server");

        assert_eq!(config.file_servers.len(), 1);
        assert_eq!(config.proxies.len(), 0);

        let fs = &config.file_servers[0];
        assert_eq!(fs.name, "StaticFiles");
        assert_eq!(fs.base_path, Some("/var/www".into()));
    }

    const MIXED_SERVICES: &str = r#"
        services {
            ApiProxy {
                listeners { "127.0.0.1:8081" }
                connectors {
                    proxy "http://127.0.0.1:3000"
                }
            }
            StaticContent {
                listeners { "127.0.0.1:8082" }
                file-server base-path="/var/www/html"
            }
        }
    "#;

    #[test]
    fn test_parse_mixed_services() {
        let config = parse_services(MIXED_SERVICES).expect("Should parse mixed services");

        assert_eq!(config.proxies.len(), 1);
        assert_eq!(config.file_servers.len(), 1);

        assert_eq!(config.proxies[0].name, "ApiProxy");
        assert_eq!(config.file_servers[0].name, "StaticContent");
    }

    const INVALID_BOTH_SECTIONS: &str = r#"
        services {
            InvalidService {
                listeners { "127.0.0.1:8080" }
                connectors {
                    return code=200 response="OK"
                }
                file-server base-path="/var/www"
            }
        }
    "#;

    #[test]
    fn test_error_both_sections() {
        let result = parse_services(INVALID_BOTH_SECTIONS);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().help().unwrap().to_string();

        assert!(err_msg.contains("conflict") || err_msg.contains("mutually exclusive"));
    }

    const INVALID_NO_SECTION: &str = r#"
        services {
            InvalidService {
                listeners { "127.0.0.1:8080" }
            }
        }
    "#;

    #[test]
    fn test_error_no_section() {
        let result = parse_services(INVALID_NO_SECTION);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().help().unwrap().to_string();
        assert_err_contains!(
            err_msg,
            "Block must contain exactly one of: [\"connectors\", \"file-server\"]"
        );
    }
}
