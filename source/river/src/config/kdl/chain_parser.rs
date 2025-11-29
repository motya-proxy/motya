use std::collections::HashMap;
use kdl::KdlDocument;
use crate::config::{
    common_types::{
        bad::Bad, definitions::{ConfiguredFilter, FilterChain},
    },
    kdl::utils,
};

pub struct ChainParser;

impl ChainParser {
    pub fn parse(doc: &KdlDocument, block: &KdlDocument) -> miette::Result<FilterChain> {
        let nodes = utils::data_nodes(doc, block)?;
        let mut filters = Vec::new();

        for (node, name, args) in nodes {
            if name != "filter" {
                return Err(Bad::docspan(
                    format!("Expected 'filter' directive, found '{name}'"),
                    doc,
                    &node.span(),
                ).into());
            }

            let mut args_map: HashMap<String, String> = utils::str_str_args(doc, args)?
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();

            let filter_name = args_map.remove("name").ok_or_else(|| {
                Bad::docspan(
                    "filter requires a 'name' argument",
                    doc,
                    &node.span(),
                )
            })?;

            filters.push(ConfiguredFilter {
                name: filter_name,
                args: args_map,
            });
        }

        Ok(FilterChain { filters })
    }
}