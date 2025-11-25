use crate::config::{common_types::{
    connectors::ConnectorsSectionParser, 
    listeners::ListenersSectionParser, 
    path_control::PathControlSectionParser, 
    rate_limiter::RateLimitSectionParser
}, internal::ProxyConfig
};

pub struct ServiceSection<'a, T> {
    listeners: &'a dyn ListenersSectionParser<T>,
    connectors: &'a dyn ConnectorsSectionParser<T>,
    pc: &'a dyn PathControlSectionParser<T>,
    rl: &'a dyn RateLimitSectionParser<T>,
    name: &'a str
}

pub trait ServiceSectionParser<T> {
    fn parse_node(&self, node: &T) -> miette::Result<ProxyConfig>;
}

impl<'a, T> ServiceSection<'a, T> {
    pub fn new(
        listeners: &'a dyn ListenersSectionParser<T>,
        connectors: &'a dyn ConnectorsSectionParser<T>,
        pc: &'a dyn PathControlSectionParser<T>,
        rl: &'a dyn RateLimitSectionParser<T>,
        name: &'a str
    ) -> Self {
        Self { listeners, connectors, pc, rl, name }
    }
}

impl<T> ServiceSectionParser<T> for ServiceSection<'_, T> {

    fn parse_node(&self, node: &T) -> miette::Result<ProxyConfig> {
        
        let listeners = self.listeners.parse_node(node)?;
        let connectors = self.connectors.parse_node(node)?;
        let pc = self.pc.parse_node(node)?;
        let rl = self.rl.parse_node(node)?;

        Ok(ProxyConfig {
            name: self.name.to_string(),
            listeners,
            connectors,
            path_control: pc,
            rate_limiting: rl,
        })
    }
}