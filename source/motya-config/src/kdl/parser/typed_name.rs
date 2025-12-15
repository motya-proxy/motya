use std::net::SocketAddr;
use std::str::FromStr;

use fqdn::FQDN;
use miette::Result;

use crate::kdl::parser::ctx::ParseContext;

#[derive(Clone, Copy)]
pub struct TypedName<'a> {
    ctx: &'a ParseContext<'a>,
    raw: &'a str,
}

impl<'a> TypedName<'a> {
    pub fn new(ctx: &'a ParseContext<'a>, raw: &'a str) -> Self {
        Self { ctx, raw }
    }

    pub fn as_str(self) -> &'a str {
        self.raw
    }

    pub fn as_socket_addr(self) -> Result<SocketAddr> {
        self.raw.parse::<SocketAddr>().map_err(|_| {
            self.ctx.error(format!(
                "Invalid node name '{}'. Expected a valid socket address 'IP:PORT' (e.g., '127.0.0.1:8080' or '[::1]:443')",
                self.raw
            ))
        })
    }

    pub fn as_fqdn(self) -> Result<FQDN> {
        FQDN::from_str(self.raw).map_err(|e| {
            self.ctx.error(format!(
                "Invalid node name '{}'. Expected a valid FQDN: {}",
                self.raw, e
            ))
        })
    }
}

impl<'a> ParseContext<'a> {
    pub fn validated_name<'b>(&'a self) -> Result<TypedName<'b>>
    where
        'a: 'b,
    {
        let name = self.name()?;
        Ok(TypedName::new(self, name))
    }
}
