use motya_macro::validate;

use crate::{
    common_types::key_template::KeyTemplate,
    kdl::parser::{ctx::ParseContext, ensures::Rule, utils::PrimitiveType},
};

//`key "${ip}" fallback="${header-x}"`
pub struct KeyTemplateParser;

impl KeyTemplateParser {
    #[validate(ensure_node_name = "key")]
    pub fn parse(
        &self,
        ctx: ParseContext<'_>,
    ) -> miette::Result<(KeyTemplate, Option<KeyTemplate>)> {
        ctx.validate(&[
            Rule::NoChildren,
            Rule::AtLeastArgs(1),
            Rule::OnlyKeysTyped(&[("fallback", PrimitiveType::String)]),
        ])?;

        let source = ctx.first()?.parse_as::<KeyTemplate>()?;

        let fallback = ctx
            .opt_prop("fallback")?
            .map(|v| v.parse_as::<KeyTemplate>())
            .transpose()?;

        Ok((source, fallback))
    }
}
