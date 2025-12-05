use async_trait::async_trait;
use bytes::Bytes;
use pingora_core::Result;
use pingora_http::ResponseHeader;
use pingora_proxy::Session;

use crate::{
    config::internal::SimpleResponse, 
    proxy::{MotyaContext, filters::types::RequestFilterMod}
};


#[async_trait]
impl RequestFilterMod for SimpleResponse {
    async fn request_filter(&self, session: &mut Session, _: &mut MotyaContext) -> Result<bool> {

        let body = self.response_body.clone();
        let mut response = ResponseHeader::build(self.http_code, None)?;
        response.insert_header("Content-Type", "text/plain; charset=utf-8")?;
        
        session.downstream_session.write_response_header(Box::new(response)).await?;
        session.downstream_session.write_response_body(Bytes::from(body), true).await?;
        session.downstream_session.set_keepalive(None);
        return Ok(true);
    }
}
