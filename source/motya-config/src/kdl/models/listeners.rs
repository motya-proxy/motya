use std::net::SocketAddr;

use motya_macro::{motya_node, NodeSchema, Parser};

use crate::common_types::listeners::{ListenerConfig, ListenerKind, TlsConfig};

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
pub struct ListenerDef {
    #[node(node_name)]
    pub addr: SocketAddr,

    #[node(prop, name = "cert-path")]
    pub cert_path: Option<String>,

    #[node(prop, name = "key-path")]
    pub key_path: Option<String>,

    #[node(prop, name = "offer-h2")]
    pub offer_h2: Option<bool>,
}

#[motya_node]
#[derive(Parser, Clone, Debug, NodeSchema)]
#[node(name = "listeners")]
pub struct ListenersDef {
    #[node(dynamic_child)]
    pub items: Vec<ListenerDef>,
}

impl TryFrom<ListenerDef> for ListenerConfig {
    type Error = miette::Report;

    fn try_from(def: ListenerDef) -> Result<Self, Self::Error> {
        let (data, ctx) = def.into_parts();

        match (data.cert_path, data.key_path) {
            (Some(cpath), Some(kpath)) => Ok(ListenerConfig {
                source: ListenerKind::Tcp {
                    addr: data.addr.to_string(),
                    tls: Some(TlsConfig {
                        cert_path: cpath.into(),
                        key_path: kpath.into(),
                    }),
                    offer_h2: data.offer_h2.unwrap_or(true),
                },
            }),

            (None, None) => {
                if data.offer_h2.is_some() {
                    return Err(ctx.err_offer_h2(
                        "'offer-h2' requires TLS. Please specify 'cert-path' and 'key-path' or remove 'offer-h2'.",
                    ));
                }

                Ok(ListenerConfig {
                    source: ListenerKind::Tcp {
                        addr: data.addr.to_string(),
                        tls: None,
                        offer_h2: false,
                    },
                })
            }

            (Some(_), None) => {
                Err(ctx.err_key_path("'key-path' is missing, but 'cert-path' is provided"))
            }
            (None, Some(_)) => {
                Err(ctx.err_cert_path("'cert-path' is missing, but 'key-path' is provided"))
            }
        }
    }
}
