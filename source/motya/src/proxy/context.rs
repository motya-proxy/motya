use std::net::IpAddr;

use cookie::Cookie;
use http::uri::PathAndQuery;
use pingora::protocols::l4::socket::SocketAddr;
use pingora_http::RequestHeader;

use crate::proxy::key_selector::KeySourceContext;

pub struct SessionInfo<'a> {
    pub headers: &'a RequestHeader,
    pub client_addr: Option<&'a SocketAddr>,
    pub path: &'a PathAndQuery,
}

pub struct ContextInfo {}

impl<'a> KeySourceContext for SessionInfo<'a> {
    fn get_path(&self) -> &PathAndQuery {
        self.path
    }

    fn get_cookie(&self, name: &str) -> Option<Cookie<'_>> {
        let header_value = self.headers.headers.get("cookie")?;

        let cookie_str = header_value.to_str().ok()?;

        for cookie_pair in cookie_str.split(';') {
            let trimmed_pair = cookie_pair.trim();
            if let Ok(cookie) = Cookie::parse(trimmed_pair) {
                if cookie.name() == name {
                    return Some(cookie);
                }
            }
        }

        None
    }

    fn get_header(&self, name: &str) -> Option<&str> {
        self.headers.headers.get(name).and_then(|v| v.to_str().ok())
    }

    fn get_ip(&self) -> Option<IpAddr> {
        self.client_addr
            .and_then(|addr| addr.as_inet())
            .map(|addr| addr.ip())
    }
}
