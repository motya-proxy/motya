use std::collections::BTreeMap;

use async_trait::async_trait;
use motya_config::common_types::value::Value;
use pingora::{Error, Result};
use pingora_http::RequestHeader;
use pingora_proxy::Session;
use regex::Regex;

use crate::proxy::{
    filters::{
        builtin::helpers::{ConfigMapExt, RequiredValueExt},
        types::RequestModifyMod,
    },
    MotyaContext,
};

pub struct RemoveHeaderKeyRegex {
    regex: Regex,
}

impl RemoveHeaderKeyRegex {
    pub fn from_settings(mut settings: BTreeMap<String, Value>) -> Result<Self> {
        let pat = settings
            .take_val::<String>("pattern")?
            .required("pattern")?;

        let reg = Regex::new(&pat).map_err(|e| {
            tracing::error!("Bad pattern: '{pat}': {e:?}");
            Error::new_str("Error building regex")
        })?;

        Ok(Self { regex: reg })
    }
}

#[async_trait]
impl RequestModifyMod for RemoveHeaderKeyRegex {
    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        header: &mut RequestHeader,
        _ctx: &mut MotyaContext,
    ) -> Result<()> {
        let headers = header
            .headers
            .keys()
            .filter_map(|k| {
                if self.regex.is_match(k.as_str()) {
                    tracing::debug!("Removing header: {k:?}");
                    Some(k.to_owned())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for h in headers {
            assert!(header.remove_header(&h).is_some());
        }

        Ok(())
    }
}
