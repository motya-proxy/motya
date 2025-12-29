use std::collections::BTreeMap;

use async_trait::async_trait;
use motya_config::common_types::value::Value;
use pingora::Result;
use pingora_http::RequestHeader;
use pingora_proxy::Session;

use crate::proxy::{
    filters::{
        builtin::helpers::{ConfigMapExt, RequiredValueExt},
        types::RequestModifyMod,
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

#[async_trait]
impl RequestModifyMod for UpsertHeader {
    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        header: &mut RequestHeader,
        _ctx: &mut MotyaContext,
    ) -> Result<()> {
        if let Some(h) = header.remove_header(&self.key) {
            tracing::debug!("Removed header: {h:?}");
        }
        header.append_header(self.key.clone(), &self.value)?;
        tracing::debug!("Inserted header: {}: {}", self.key, self.value);
        Ok(())
    }
}
