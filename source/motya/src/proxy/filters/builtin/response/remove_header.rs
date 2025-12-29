use std::collections::BTreeMap;

use motya_config::common_types::value::Value;
use pingora::{Error, Result};
use pingora_http::ResponseHeader;
use pingora_proxy::Session;
use regex::Regex;

use crate::proxy::{
    filters::{
        builtin::helpers::{ConfigMapExt, RequiredValueExt},
        types::ResponseModifyMod,
    },
    MotyaContext,
};

pub struct RemoveHeaderKeyRegex {
    regex: Regex,
}

impl RemoveHeaderKeyRegex {
    pub fn from_settings(mut settings: BTreeMap<String, Value>) -> Result<Self> {
        let pattern = settings
            .take_val::<String>("pattern")?
            .required("pattern")?;

        let regex = Regex::new(&pattern).map_err(|e| {
            tracing::error!("Bad pattern: '{pattern}': {e:?}");
            Error::new_str("Error building regex")
        })?;

        Ok(Self { regex })
    }
}

impl ResponseModifyMod for RemoveHeaderKeyRegex {
    fn upstream_response_filter(
        &self,
        _session: &mut Session,
        header: &mut ResponseHeader,
        _ctx: &mut MotyaContext,
    ) {
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
    }
}
