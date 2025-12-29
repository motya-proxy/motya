use std::collections::BTreeMap;

use motya_config::common_types::value::Value;
use pingora::Result;
use pingora_http::ResponseHeader;
use pingora_proxy::Session;

use crate::proxy::{
    filters::{
        builtin::helpers::{ConfigMapExt, RequiredValueExt},
        types::ResponseModifyMod,
    },
    MotyaContext,
};

pub struct UpsertHeader {
    key: String,
    value: String,
}

impl UpsertHeader {
    pub fn from_settings(mut settings: BTreeMap<String, Value>) -> Result<Self> {
        let key = settings.take_val::<String>("key")?.required("key")?;
        let value = settings.take_val::<String>("value")?.required("value")?;

        Ok(Self { key, value })
    }
}

impl ResponseModifyMod for UpsertHeader {
    fn upstream_response_filter(
        &self,
        _session: &mut Session,
        header: &mut ResponseHeader,
        _ctx: &mut MotyaContext,
    ) {
        if let Some(h) = header.remove_header(&self.key) {
            tracing::debug!("Removed header: {h:?}");
        }
        let _ = header.append_header(self.key.clone(), &self.value);
        tracing::debug!("Inserted header: {}: {}", self.key, self.value);
    }
}
