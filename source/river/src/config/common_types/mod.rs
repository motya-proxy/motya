pub mod bad;
pub mod connectors;
pub mod listeners;
pub mod path_control;
pub mod rate_limiter;
pub mod definitions;
pub mod system_data;
pub mod file_server;
pub mod service;

pub trait SectionParser<TDocument, TResult> {
    fn parse_node(&self, document: &TDocument) -> miette::Result<TResult>;
}