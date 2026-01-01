use crate::{
    common_types::{
        definitions_table::DefinitionsTable,
        error::ConfigError,
        file_server::FileServerConfig,
        listeners::{ListenerConfig, Listeners},
        system_data::SystemData,
    },
    internal::{Config, ProxyConfig},
    kdl::{
        connectors::ConnectorsLinker,
        definitions::DefinitionsCompiler,
        models::{
            listeners::ListenersDef,
            root::RootDef,
            services::{ServiceDef, ServiceModeData},
        },
    },
};

pub struct ConfigLinker<'a> {
    table: &'a mut DefinitionsTable,
    errors: ConfigError,
}

impl<'a> ConfigLinker<'a> {
    pub fn new(table: &'a mut DefinitionsTable) -> Self {
        Self {
            table,
            errors: ConfigError::default(),
        }
    }

    pub fn link(mut self, roots: Vec<RootDef>) -> Result<Config, ConfigError> {
        let mut final_config = Config::default();

        let mut system_defined = false;

        for root in &roots {
            let (data, ctx) = root.clone().into_parts();

            if let Some(sys_def) = data.system {
                if system_defined {
                    self.errors.push_report(
                        ctx.err_system("Multiple 'system' sections found. Only one 'system' block is allowed globally."),
                        &ctx.ctx
                    );
                } else {
                    system_defined = true;

                    match SystemData::try_from(sys_def) {
                        Ok(sys_data) => {
                            final_config.threads_per_service = sys_data.threads_per_service;
                            final_config.daemonize = sys_data.daemonize;
                            final_config.upgrade_socket = sys_data.upgrade_socket;
                            final_config.pid_file = sys_data.pid_file;
                            // final_config.provider = sys_data.provider;
                        }
                        Err(e) => {
                            self.errors.push_report(e, &ctx.ctx);
                        }
                    }
                }
            }

            if let Some(defs_def) = &root.definitions {
                DefinitionsCompiler.collect_prerequisites(
                    defs_def.clone(),
                    self.table,
                    &mut self.errors,
                );
            }
        }

        if !self.errors.is_empty() {
            return Err(self.errors);
        }

        for root in &roots {
            if let Some(defs_def) = &root.definitions {
                if let Some(modifiers) = &defs_def.modifiers {
                    let (data, _) = modifiers.clone().into_parts();
                    DefinitionsCompiler.compile_modifiers(data, self.table, &mut self.errors);
                }
            }
        }

        for root in roots {
            for services_section in root.services.clone() {
                let (section_data, _) = services_section.into_parts();

                for service_def in section_data.items {
                    self.compile_service(service_def, &mut final_config);
                }
            }
        }

        if !self.errors.is_empty() {
            Err(self.errors)
        } else {
            Ok(final_config)
        }
    }

    fn compile_listeners(&self, def: ListenersDef) -> (Listeners, ConfigError) {
        let (data, ctx) = def.into_parts();
        let mut result_listeners = Vec::new();
        let mut errors = ConfigError::default();

        for item in data.items {
            match ListenerConfig::try_from(item) {
                Ok(cfg) => result_listeners.push(cfg),
                Err(e) => errors.push_report(e, &ctx.ctx),
            }
        }

        (
            Listeners {
                list_cfgs: result_listeners,
            },
            errors,
        )
    }

    fn compile_service(&mut self, service_def: ServiceDef, config: &mut Config) {
        let (data, _) = service_def.into_parts();
        let name = data.name;

        let (listeners, l_err) = self.compile_listeners(data.listeners);

        self.errors.merge(l_err);

        let mode = data.mode.into_inner();

        match mode {
            ServiceModeData::Connectors(connectors_def) => {
                let connectors_linker = ConnectorsLinker::new(self.table);

                let (connectors, c_err) = connectors_linker.link(connectors_def);
                self.errors.merge(c_err);

                config.basic_proxies.push(ProxyConfig {
                    name,
                    listeners,
                    connectors,
                });
            }
            ServiceModeData::FileServer(fs_def) => {
                let fs_data = fs_def;

                config.file_servers.push(FileServerConfig {
                    name,
                    listeners,
                    base_path: fs_data.root,
                });
            }
        }
    }
}
