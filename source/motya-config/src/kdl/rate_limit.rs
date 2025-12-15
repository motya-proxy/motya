use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    block_parser,
    common_types::{key_template::TransformOp, rate_limiter::RateLimitPolicy},
    kdl::{
        key_template::KeyTemplateParser,
        parser::{block::BlockParser, ctx::ParseContext, ensures::Rule, utils::PrimitiveType},
        transforms_order::TransformsOrderParser,
    },
};
pub struct RateLimitPolicyParser;

impl RateLimitPolicyParser {
    pub fn parse(
        &self,
        ctx: ParseContext<'_>,
        anon_counter: Option<&AtomicUsize>,
        path_slug: Option<&str>,
    ) -> miette::Result<RateLimitPolicy> {
        let name = if ctx.args()?.is_empty() {
            ctx.validate(&[Rule::ReqChildren, Rule::NoPositionalArgs])?;

            let counter = anon_counter.ok_or_else(|| {
                ctx.error(
                    "Context error: Inline rate-limit is not allowed here (missing anon_counter)",
                )
            })?;

            let slug = path_slug.unwrap_or("global");
            let id = counter.fetch_add(1, Ordering::Relaxed);

            format!("__anon_rl_{id}_{slug}")
        } else {
            ctx.validate(&[Rule::ReqChildren, Rule::ExactArgs(1)])?;
            ctx.first()?.as_str()?.to_string()
        };

        block_parser!(ctx.enter_block()?,
            algorithm: required("algorithm") => |ctx| {
                ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1)])?;
                Ok(ctx.first()?.as_str()?.to_string())
            },
            storage_key: required("storage") => |ctx| {
                ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1)])?;
                Ok(ctx.first()?.as_str()?.to_string())
            },
            key_template: required("key") => |ctx| Ok(KeyTemplateParser.parse(ctx)?.0),
            transforms: optional("transforms-order") => |ctx| TransformsOrderParser.parse(ctx),
            burst: required("burst") => |ctx| {
                ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1)])?;

                ctx.first()?.as_usize()
            },
            rate_req_per_sec: required("rate") => |ctx| {
                ctx.validate(&[Rule::NoChildren, Rule::ExactArgs(1)])?;
                let rate_str = ctx.first()?.as_str()?;
                self.parse_rate_string(ctx, &rate_str)
            }
        );

        Ok(RateLimitPolicy {
            name,
            algorithm,
            storage_key,
            key_template,
            transforms: transforms.unwrap_or_default(),
            burst,
            rate_req_per_sec,
        })
    }

    fn parse_rate_string(&self, ctx: ParseContext<'_>, input: &str) -> miette::Result<f64> {
        let (num_str, suffix) = input
            .split_once('/')
            .ok_or_else(|| ctx.error("Rate format must be 'NUMBER/PERIOD' (e.g. '100/s')"))?;

        let num: f64 = num_str.parse().map_err(|_| ctx.error("Invalid number"))?;

        let period = match suffix {
            "s" | "sec" => 1.0,
            "m" | "min" => 60.0,
            "h" | "hour" => 3600.0,
            _ => return Err(ctx.error(format!("Unknown time unit '{}'", suffix))),
        };

        if period == 0.0 {
            return Err(ctx.error("Period cannot be zero"));
        }
        Ok(num / period)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kdl::parser::ctx::Current;
    use kdl::KdlDocument;
    use miette::Result;

    fn parse_policy_from_kdl(input: &str) -> Result<RateLimitPolicy> {
        let doc: KdlDocument = input.parse().unwrap();
        let ctx = ParseContext::new(&doc, Current::Document(&doc), "test");
        let mut block = BlockParser::new(ctx).unwrap();

        block.required("policy", |ctx| RateLimitPolicyParser.parse(ctx, None, None))
    }

    #[test]
    fn test_parse_full_policy() {
        let input = r#"
        policy "api_protection" {
            algorithm "token-bucket"
            storage "redis-cluster"
            key "bla-bla"
            transforms-order {
                lowercase
            }
            burst 20
            rate "100/m"
        }
        "#;

        let policy = parse_policy_from_kdl(input).expect("Parsing failed for full policy");

        assert_eq!(policy.algorithm, "token-bucket");
        assert_eq!(policy.storage_key, "redis-cluster");
        assert_eq!(policy.burst, 20);

        // 100 / 60 = 1.666...
        assert!((policy.rate_req_per_sec - 1.6666).abs() < 0.001);

        assert_eq!(policy.transforms.len(), 1);
    }
}
