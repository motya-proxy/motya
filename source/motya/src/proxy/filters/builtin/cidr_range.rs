use std::collections::BTreeMap;

use async_trait::async_trait;
use cidr::IpCidr;
use motya_config::common_types::value::Value;
use pingora::{protocols::l4::socket::SocketAddr, Error, Result};
use pingora_proxy::Session;

use crate::proxy::{
    filters::{
        builtin::helpers::{ConfigMapExt, RequiredValueExt},
        types::RequestFilterMod,
    },
    MotyaContext,
};

pub struct CidrRangeFilter {
    blocks: Vec<IpCidr>,
}

impl CidrRangeFilter {
    pub fn from_settings(mut settings: BTreeMap<String, Value>) -> Result<Self> {
        let addrs_raw = settings.take_val::<String>("addrs")?.required("addrs")?;

        let blocks = addrs_raw
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.parse::<IpCidr>().map_err(|e| {
                    tracing::error!("Failed to parse '{s}' as a valid CIDR: {e:?}");
                    Error::new_str("Invalid configuration: Invalid CIDR notation")
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self { blocks })
    }
}

#[async_trait]
impl RequestFilterMod for CidrRangeFilter {
    async fn request_filter(&self, session: &mut Session, _ctx: &mut MotyaContext) -> Result<bool> {
        let Some(addr) = session.downstream_session.client_addr() else {
            // Unable to determine source address, assuming it should be blocked
            session.downstream_session.respond_error(401).await?;
            return Ok(true);
        };
        let SocketAddr::Inet(addr) = addr else {
            // CIDR filters don't apply to UDS
            return Ok(false);
        };
        let ip_addr = addr.ip();

        if self.blocks.iter().any(|b| b.contains(&ip_addr)) {
            session.downstream_session.respond_error(401).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_from_settings_valid_ipv4() {
        let mut settings = BTreeMap::new();
        settings.insert(
            "addrs".to_string(),
            Value::String("192.168.0.0/16, 10.0.0.0/8".to_string()),
        );

        let filter =
            CidrRangeFilter::from_settings(settings).expect("Should successfully create filter");

        assert_eq!(filter.blocks.len(), 2);
        assert!(filter
            .blocks
            .iter()
            .any(|b| b.to_string() == "192.168.0.0/16"));
        assert!(filter.blocks.iter().any(|b| b.to_string() == "10.0.0.0/8"));
    }

    #[test]
    fn test_from_settings_valid_mixed_ipv4_ipv6() {
        let mut settings = BTreeMap::new();
        settings.insert(
            "addrs".to_string(),
            Value::String("10.0.0.0/8, 2001:db8::/32, ::1/128, 1.1.1.1/32".to_string()),
        );

        let filter =
            CidrRangeFilter::from_settings(settings).expect("Should successfully create filter");

        assert_eq!(filter.blocks.len(), 4);
        assert!(filter
            .blocks
            .iter()
            .any(|b| b.to_string() == "2001:db8::/32"));
        assert!(filter.blocks.iter().any(|b| b.to_string() == "::1"));
        assert!(filter.blocks.iter().any(|b| b.to_string() == "1.1.1.1"));
    }

    #[test]
    fn test_from_settings_invalid_cidr() {
        let mut settings = BTreeMap::new();
        settings.insert(
            "addrs".to_string(),
            Value::String("192.168.0.0/16, not_a_cidr, 10.0.0.0/8".to_string()),
        );

        let result = CidrRangeFilter::from_settings(settings);

        assert!(result.is_err());

        let err = result.err().unwrap();
        assert!(format!("{:?}", err).contains("Invalid configuration"));
    }

    #[test]
    fn test_from_settings_missing_addrs_key() {
        let settings = BTreeMap::new();

        let result = CidrRangeFilter::from_settings(settings);

        assert!(result.is_err());

        let err = result.err().unwrap();
        assert!(format!("{:?}", err).contains("Missing configuration"));
    }
}
