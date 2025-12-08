use std::collections::HashSet;

use kdl::KdlDocument;
use miette::{miette, Result};
use crate::common_types::bad::Bad;
use crate::common_types::definitions_table::DefinitionsTable;
use crate::common_types::section_parser::SectionParser;
use crate::internal::Config;
use crate::kdl::{
    definitions::DefinitionsSection, 
    services::ServicesSection, 
    system_data::SystemDataSection
};

/// Orchestrates the loading and composition of the configuration from multiple KDL files.
///
/// This loader implements a **Two-Pass Parsing** strategy to allow for cross-file references
/// and modular configuration (e.g., defining filters in one file and using them in another).
///
/// # Loading Process
///
/// 1. **File Discovery (Recursive)**:
///    Starts from the `entry_point` path and recursively resolves `include` directives
///    to build a flat list of unique KDL documents. Cycles and duplicate imports are handled.
///
/// 2. **Phase 1: Definitions & Plugins**:
///    Iterates through *all* loaded documents to collect and merge `definitions` blocks.
///    - Parses named filter chains, plugin definitions and key-profiles for load-balancer.
///
/// 3. **Phase 2: System & Services**:
///    Iterates through the documents again to build the concrete configuration:
///    - **System Data**: Extracted *only* from the entry point document.
///    - **Services**: Aggregated from *all* documents.
///      - During service parsing, anonymous chains and key templates are detected
///        and registered into the global definitions table with generated names.
pub struct ConfigCompiler {
    documents: Vec<(KdlDocument, String)>,
}

impl ConfigCompiler {
    pub fn new(documents: Vec<(KdlDocument, String)>) -> Self {
        Self { documents }
    }

    pub fn compile(self, global_definitions: &mut DefinitionsTable) -> Result<Config> {
        if self.documents.is_empty() {
            return Err(miette!("No configuration documents provided"));
        }

        if !self.documents.iter().any(|(doc, _)| {
            doc.get("definitions").is_some() || doc.get("services").is_some()
        }) {
            return Err(miette!("Configuration must contain at least one 'definitions' or 'services' section."));
        }

        let allowed_names: HashSet<&str> = ["services", "definitions", "includes", "system"].iter().cloned().collect();

        for (doc, source_name) in &self.documents {
            
            let present_names: HashSet<&str> = doc.nodes().iter().map(|n| n.name().value()).collect();

            let unknown_names: Vec<&str> = present_names.difference(&allowed_names).cloned().collect();

            if !unknown_names.is_empty() {

                if let Some(node) = doc.nodes().iter().find(|n| unknown_names.contains(&n.name().value())) {
                    let unknown = node.name().value();
                    return Err(Bad::docspan(
                        format!("Unknown top-level section '{}' in '{}'. Allowed: services, definitions, includes, system.", unknown, source_name),
                        doc,
                        &node.span(),
                        source_name
                    ).into());
                }
            }
        }


        let mut final_config = Config::default();

        let sys_data = self.documents.iter()
            .try_fold(None, |acc, (doc, name)| {
                let parsed = SystemDataSection::new(doc, name).parse_node(doc)?;
                match (acc, parsed) {
                    (prev, None) => Ok(prev),
                    (None, Some(curr)) => Ok(Some(curr)),
                    (Some(_), Some(_)) => Err(miette!("Multiple 'system' sections found.")),
                }
            })?
            .ok_or_else(|| miette!("Missing 'system' section in configuration"))?;


        final_config.threads_per_service = sys_data.threads_per_service;
        final_config.daemonize = sys_data.daemonize;
        final_config.upgrade_socket = sys_data.upgrade_socket;
        final_config.pid_file = sys_data.pid_file;

        
        for (doc, name) in &self.documents {
            
            if let Ok(defs) = DefinitionsSection::new(doc, name).parse_node(doc) {
                global_definitions.merge(defs)?;
            }
        }

        for (doc, name) in &self.documents {
            if doc.get("services").is_none() {
                continue;
            }
            
            let services_config = ServicesSection::new(global_definitions, name).parse_node(doc)?;

            final_config.basic_proxies.extend(services_config.proxies);
            final_config.file_servers.extend(services_config.file_servers);
        }

        Ok(final_config)
    }
}


#[cfg(test)]
mod tests {

    use super::*;
    use fqdn::fqdn;
    #[tokio::test]
    async fn test_namespace_merge_across_files() {

        const DEF_ONE: &str = r#"
        definitions {
            modifiers {
                namespace "motya" {
                    namespace "inner" {
                        def name="one"
                    }
                }
            }
        }
        "#;

        const DEF_TWO: &str = r#"
        definitions {
            modifiers {
                namespace "motya" {
                    namespace "inner" {
                        def name="two"
                    }
                }
            }
        }
        "#;

        const MAIN_CONFIG: &str = r#"
        system {
            threads-per-service 1
        }

        services {
            TestService {
                listeners { "127.0.0.1:8080" }
                connectors {
                    return code="200" response="OK"
                }
            }
        }
        "#;

        let def1: KdlDocument = DEF_ONE.parse().unwrap();
        let def2: KdlDocument = DEF_TWO.parse().unwrap();
        let main: KdlDocument = MAIN_CONFIG.parse().unwrap();
        
        let files = vec![(def1, "def1.kdl".to_string()), (def2, "def2.kdl".to_string()), (main, "main.kdl".to_string())];

        let compiler = ConfigCompiler::new(files);

        let mut def_table = DefinitionsTable::new_with_global();

        compiler
            .compile(&mut def_table)
            .expect("Config should load successfully");

        assert!(
            def_table
                .get_available_filters()
                .contains(&fqdn!("motya.inner.one")),
            "FQDN 'motya.inner.one' is missing in global definitions"
        );
        assert!(
            def_table
                .get_available_filters()
                .contains(&fqdn!("motya.inner.two")),
            "FQDN 'motya.inner.two' is missing in global definitions"
        );
    }

    #[tokio::test]
    async fn test_duplicate_plugin_definition_across_files() {
        

        const SHARED_PLUGIN: &str = r#"
        definitions {
            plugins {
                plugin {
                    name "duplicate-plugin"
                    load path="./assets/filter.wasm"
                }
            }
        }
        "#;

        const MAIN_CONFIG: &str = r#"
        includes {
            include "./def1.kdl"
            include "./def2.kdl"
        }
        
        system {
            threads-per-service 1
        }

        services {
            TestService {
                listeners { "127.0.0.1:8080" }
                connectors {
                    use-chain "test-chain"
                    return code="200" response="OK"
                }
            }
        }
        "#;


        let shared1: KdlDocument = SHARED_PLUGIN.parse().unwrap();
        let shared2: KdlDocument = SHARED_PLUGIN.parse().unwrap();
        let main: KdlDocument = MAIN_CONFIG.parse().unwrap();
        
        let files = vec![(shared1, "shared1.kdl".to_string()), (shared2, "shared2.kdl".to_string()), (main, "main.kdl".to_string())];

        let compiler = ConfigCompiler::new(files);

        let mut def_table = DefinitionsTable::new_with_global();

        let result = compiler
            .compile(&mut def_table);

        assert!(result.is_err());

        let err_msg = result.unwrap_err().to_string();

        crate::assert_err_contains!(
            err_msg,
            "Duplicate plugin definition across files: 'duplicate-plugin'"
        );
    }

    #[tokio::test]
    async fn test_duplicate_chain_definition_across_files() {
        
        const SHARED_DEF: &str = r#"
            definitions {
                modifiers {
                    chain-filters "conflict-chain" { }
                }
            }
        "#;

        const MAIN_CONFIG: &str = r#"
            includes {
                include "./def1.kdl"
                include "./def2.kdl"
            }
            
            system {
                threads-per-service 1
            }

            services { 
                TestService {
                    listeners { "127.0.0.1:8080" }
                    connectors {
                        use-chain "test-chain"
                        return code="200" response="OK"
                    }
                }
            }
        "#;


        let def1: KdlDocument = SHARED_DEF.parse().unwrap();
        let def2: KdlDocument = SHARED_DEF.parse().unwrap();
        let main: KdlDocument = MAIN_CONFIG.parse().unwrap();
        
        let files = vec![(def1, "def1.kdl".to_string()), (def2, "def2.kdl".to_string()), (main, "main.kdl".to_string())];

        let compiler = ConfigCompiler::new(files);

        let mut def_table = DefinitionsTable::new_with_global();

        let result = compiler
            .compile(&mut def_table);

        let err_msg = result.unwrap_err().to_string();
        crate::assert_err_contains!(
            err_msg,
            "Duplicate chain definition across files: 'conflict-chain'"
        );
    }

    #[tokio::test]
    async fn test_include_logic() {

        const DEFINITIONS_FILE: &str = r#"
            definitions {
                modifiers {
                    chain-filters "test-chain" { }
                }
            }
        "#;

        const MAIN_FILE: &str = r#"
            includes {
                include "./definitions.kdl"
            }
            
            system {
                threads-per-service 2
            }

            services {
                TestService {
                    listeners { "127.0.0.1:8080" }
                    connectors {
                        use-chain "test-chain"
                        return code="200" response="OK"
                    }
                }
            }
        "#;

        
        let def: KdlDocument = DEFINITIONS_FILE.parse().unwrap();
        let main: KdlDocument = MAIN_FILE.parse().unwrap();
        
        let files = vec![(def, "def.kdl".to_string()), (main, "main.kdl".to_string())];

        let compiler = ConfigCompiler::new(files);

        let mut def_table = DefinitionsTable::new_with_global();

        let config = compiler
            .compile(&mut def_table)
            .expect("Config should load successfully");

        assert_eq!(config.threads_per_service, 2);
        assert_eq!(config.basic_proxies.len(), 1);
    }
}

