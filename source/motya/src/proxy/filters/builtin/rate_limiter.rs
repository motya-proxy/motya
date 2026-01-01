use std::sync::Arc;

use async_trait::async_trait;
use pingora::Result;
use pingora_proxy::Session;

use crate::proxy::{
    context::SessionInfo, filters::types::RequestFilterMod,
    rate_limiter::instance::RateLimiterInstance, MotyaContext,
};

pub struct RateLimitFilter {
    limiter: RateLimiterInstance,
}

impl RateLimitFilter {
    pub fn new(limiter: RateLimiterInstance) -> Self {
        Self { limiter }
    }
}

#[async_trait]
impl RequestFilterMod for RateLimitFilter {
    async fn request_filter(&self, session: &mut Session, _ctx: &mut MotyaContext) -> Result<bool> {
        let result = self
            .limiter
            .check(&SessionInfo {
                client_addr: session.downstream_session.client_addr(),
                headers: &session.downstream_session.req_header(),
                path: session
                    .downstream_session
                    .req_header()
                    .uri
                    .path_and_query()
                    .unwrap(),
            })
            .await
            .map_err(|e| pingora::Error::new(pingora::ErrorType::InternalError))?;

        let headers = session.downstream_session.req_header_mut();

        headers.insert_header("X-RateLimit-Remaining", result.remaining.to_string())?;

        if result.allowed {
            Ok(false)
        } else {
            let retry_secs = result.reset_after.as_secs().max(1).to_string();
            session
                .downstream_session
                .req_header_mut()
                .insert_header("Retry-After", retry_secs)?;

            session.downstream_session.respond_error(429).await?;
            Ok(true)
        }
    }
}
