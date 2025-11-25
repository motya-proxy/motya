use std::path::PathBuf;

pub struct SystemData {
    pub threads_per_service: usize,
    pub daemonize: bool,
    pub upgrade_socket: Option<PathBuf>,
    pub pid_file: Option<PathBuf>,
}

impl Default for SystemData {
    fn default() -> Self {
        Self {
            threads_per_service: 8,
            daemonize: false,
            upgrade_socket: None,
            pid_file: None,
        }
    }
}

pub trait SystemDataSectionParser<T> {
    fn parse_node(&self, node: &T) -> miette::Result<SystemData>;
}
