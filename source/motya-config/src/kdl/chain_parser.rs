use fqdn::FQDN;

use crate::{
    common_types::definitions::{ChainItem, ConfiguredFilter, FilterChain},
    kdl::{
        parser::{block::BlockParser, ctx::ParseContext, ensures::Rule},
        rate_limit::RateLimitPolicyParser,
    },
};
use std::{collections::HashMap, sync::atomic::AtomicUsize};

pub struct ChainParser;

impl ChainParser {
    pub fn parse(
        &self,
        ctx: ParseContext<'_>,
        anon_counter: Option<&AtomicUsize>,
        path_slug: Option<&str>,
    ) -> miette::Result<FilterChain> {
        let mut raw_items: Vec<(usize, ChainItem)> = Vec::new();

        let mut block = BlockParser::new(ctx)?;

        block.repeated("filter", |filter_ctx| {
            let offset = filter_ctx.current_span().offset();

            let filter = self.parse_single_filter(filter_ctx)?;

            raw_items.push((offset, ChainItem::Filter(filter)));
            Ok(())
        })?;

        block.repeated("rate-limit", |ctx| {
            let offset = ctx.current_span().offset();

            let config = RateLimitPolicyParser.parse(ctx, anon_counter, path_slug)?;

            raw_items.push((offset, ChainItem::RateLimiter(config)));
            Ok(())
        })?;

        block.exhaust()?;

        raw_items.sort_by_key(|(offset, _)| *offset);

        let items = raw_items.into_iter().map(|(_, item)| item).collect();

        Ok(FilterChain { items })
    }

    fn parse_single_filter(&self, ctx: ParseContext<'_>) -> miette::Result<ConfiguredFilter> {
        ctx.validate(&[Rule::NoChildren, Rule::NoPositionalArgs])?;

        let name = ctx.prop("name")?.parse_as::<FQDN>()?;

        let args = ctx.args_named_typed()?[1..]
            .iter()
            .map(|v| {
                Ok((
                    v.name().expect("name should exist").to_string(),
                    v.as_str()?,
                ))
            })
            .collect::<miette::Result<HashMap<_, _>>>()?;

        Ok(ConfiguredFilter { name, args })
    }
}

#[cfg(test)]
mod tests {
    use crate::kdl::parser::ctx::Current;

    use super::*;
    use kdl::KdlDocument;

    #[test]
    fn test_chain_parser_success_happy_path() {
        let kdl_input = r#"
            filter name="com.example.auth"
            filter name="com.example.logger" level="debug" format="json"
        "#;
        let doc: KdlDocument = kdl_input.parse().unwrap();

        let ctx = ParseContext::new(&doc, Current::Document(&doc), "test");
        let chain = ChainParser
            .parse(ctx, None, None)
            .expect("Should parse valid chain");

        assert_eq!(chain.items.len(), 2);

        let ChainItem::Filter(f1) = &chain.items[0] else {
            unreachable!()
        };
        assert_eq!(f1.name.to_string(), "com.example.auth");
        assert!(f1.args.is_empty());

        let ChainItem::Filter(f2) = &chain.items[1] else {
            unreachable!()
        };
        assert_eq!(f2.name.to_string(), "com.example.logger");
        assert_eq!(f2.args.get("level").unwrap(), "debug");
        assert_eq!(f2.args.get("format").unwrap(), "json");
    }

    #[test]
    fn test_chain_parser_empty_block() {
        let kdl_input = "";
        let doc: KdlDocument = kdl_input.parse().unwrap();

        let ctx = ParseContext::new(&doc, Current::Document(&doc), "test");
        let chain = ChainParser
            .parse(ctx, None, None)
            .expect("Should parse valid chain");
        assert!(chain.items.is_empty());
    }

    #[test]
    fn test_chain_parser_invalid_directive_name() {
        let kdl_input = r#"
            filter name="good.filter"
            not-filter name="bad.one"
        "#;
        let doc: KdlDocument = kdl_input.parse().unwrap();

        let ctx = ParseContext::new(&doc, Current::Document(&doc), "test");
        let result = ChainParser.parse(ctx, None, None);
        let msg_err = result.unwrap_err().help().unwrap().to_string();

        crate::assert_err_contains!(msg_err, "Unknown directive: 'not-filter'");
    }

    #[test]
    fn test_chain_parser_missing_name_argument() {
        let kdl_input = r#"
            filter arg="value"
        "#;
        let doc: KdlDocument = kdl_input.parse().unwrap();

        let ctx = ParseContext::new(&doc, Current::Document(&doc), "test");
        let result = ChainParser.parse(ctx, None, None);
        let msg_err = result.unwrap_err().help().unwrap().to_string();
        crate::assert_err_contains!(msg_err, "Missing required property 'name'");
    }

    #[test]
    fn test_chain_parser_invalid_fqdn() {
        let kdl_input = r#"
            filter name="invalid name with spaces"
        "#;
        let doc: KdlDocument = kdl_input.parse().unwrap();

        let ctx = ParseContext::new(&doc, Current::Document(&doc), "test");
        let result = ChainParser.parse(ctx, None, None);
        let msg_err = result.unwrap_err().help().unwrap().to_string();

        crate::assert_err_contains!(
            msg_err,
            "Invalid FQDN 'invalid name with spaces'. Reason: invalid char found in FQDN"
        );
    }
}
