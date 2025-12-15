use crate::{
    block_parser,
    common_types::{
        definitions::BalancerConfig,
        key_template::{
            parse_hasher, parse_transform, HashAlgorithm, HashOp, KeyTemplate, Transform,
        },
    },
    kdl::{
        key_template::KeyTemplateParser,
        parser::{
            block::BlockParser,
            ctx::ParseContext,
            ensures::Rule,
            utils::{OptionTypedValueExt, PrimitiveType},
        },
        transforms_order::TransformsOrderParser,
    },
};

pub struct KeyProfileParser;

impl KeyProfileParser {
    pub fn parse(&self, ctx: ParseContext<'_>) -> miette::Result<BalancerConfig> {
        block_parser!(ctx,
            key: required("key") => |ctx| KeyTemplateParser.parse(ctx),
            transforms: optional("transforms-order") => |ctx| TransformsOrderParser.parse(ctx),
            algorithm: optional("algorithm") => |ctx| self.parse_hash_alg(ctx)
        );

        let (source, fallback) = key;

        let transforms = transforms.unwrap_or_default();

        let algorithm = algorithm.unwrap_or(HashOp::XxHash64(0));

        Ok(BalancerConfig {
            source,
            fallback,
            algorithm,
            transforms,
        })
    }

    fn parse_hash_alg(&self, ctx: ParseContext<'_>) -> Result<HashOp, miette::Error> {
        ctx.validate(&[
            Rule::NoChildren,
            Rule::OnlyKeysTyped(&[
                ("name", PrimitiveType::String),
                ("seed", PrimitiveType::Integer),
            ]),
        ])?;

        let [name, seed] = ctx.props(["name", "seed"])?;
        let name = name.as_str()?.unwrap_or("xxhash64".to_string());
        let seed = seed.as_usize()?.unwrap_or(0);

        let alg = HashAlgorithm { name, seed };

        parse_hasher(&alg).map_err(|err| ctx.error(format!("Failed to parse algorithm: '{err}'")))
    }
}

#[cfg(test)]
mod tests {
    use crate::{common_types::key_template::TransformOp, kdl::parser::ctx::Current};

    use super::*;
    use kdl::KdlDocument;

    #[test]
    fn test_parse_key_profile() {
        let kdl_input = r#"
            key "${uri-path}" fallback="${client-ip}:${user-agent}"
            algorithm name="xxhash32" seed=0
            transforms-order {
                remove-query-params
                lowercase
                truncate length="256"
            }
        "#;

        let doc: KdlDocument = kdl_input.parse().unwrap();

        let ctx = ParseContext::new(&doc, Current::Document(&doc), "test");
        let template = KeyProfileParser.parse(ctx).expect("Should parse");

        assert_eq!(
            template.source,
            "${uri-path}".parse::<KeyTemplate>().unwrap()
        );
        assert_eq!(
            template.fallback,
            Some("${client-ip}:${user-agent}".parse::<KeyTemplate>().unwrap())
        );
        assert_eq!(template.algorithm, HashOp::XxHash32(0));

        assert_eq!(template.transforms.len(), 3);
        assert_eq!(template.transforms[0], TransformOp::RemoveQueryParams);
        assert_eq!(template.transforms[1], TransformOp::Lowercase);
        assert_eq!(
            template.transforms[2],
            TransformOp::Truncate { length: 256 }
        );
    }

    #[test]
    fn test_parse_minimal_profile() {
        let kdl_input = r#"key "${uri-path}""#;
        let doc: KdlDocument = kdl_input.parse().unwrap();

        let ctx = ParseContext::new(&doc, Current::Document(&doc), "test");
        let template = KeyProfileParser.parse(ctx).unwrap();

        assert_eq!(
            template.source,
            "${uri-path}".parse::<KeyTemplate>().unwrap()
        );
        assert!(template.fallback.is_none());
        assert_eq!(template.algorithm, HashOp::XxHash64(0));
        assert!(template.transforms.is_empty());
    }

    #[test]
    fn test_missing_key_error() {
        let kdl_input = r#"algorithm name="xxhash32""#;
        let doc: KdlDocument = kdl_input.parse().unwrap();

        let ctx = ParseContext::new(&doc, Current::Document(&doc), "test");
        let result = KeyProfileParser.parse(ctx);

        let msg_err = result.unwrap_err().help().unwrap().to_string();
        crate::assert_err_contains!(msg_err, "Missing required directive 'key'");
    }
}
