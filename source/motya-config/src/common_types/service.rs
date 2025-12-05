use crate::{common_types::{connectors::Connectors, listeners::Listeners, rate_limiter::RateLimitingConfig, section_parser::SectionParser}, internal::ProxyConfig};

pub struct ServiceSection<'a, T> {
    listeners: &'a dyn SectionParser<T, Listeners>,
    connectors: &'a dyn SectionParser<T, Connectors>,
    rl: &'a dyn SectionParser<T, RateLimitingConfig>,
    name: &'a str
}

pub trait ServiceSectionParser<T> {
    fn parse_node(&self, node: &T) -> miette::Result<ProxyConfig>;
}

impl<'a, T> ServiceSection<'a, T> {
    pub fn new(
        listeners: &'a dyn SectionParser<T, Listeners>,
        connectors: &'a dyn SectionParser<T, Connectors>,
        rl: &'a dyn SectionParser<T, RateLimitingConfig>,
        name: &'a str
    ) -> Self {
        Self { listeners, connectors, rl, name }
    }
}

impl<T> ServiceSectionParser<T> for ServiceSection<'_, T> {

    fn parse_node(&self, node: &T) -> miette::Result<ProxyConfig> {
        
        let listeners = self.listeners.parse_node(node)?;
        let connectors = self.connectors.parse_node(node)?;
        let rl = self.rl.parse_node(node)?;

        Ok(ProxyConfig {
            name: self.name.to_string(),
            listeners,
            connectors,
            rate_limiting: rl,
        })
    }
}