use miette::miette;

use crate::{
    common_types::key_template::{parse_transform, Transform, TransformOp},
    kdl::parser::{ctx::ParseContext, ensures::Rule},
};

// transforms-order {
//     lowercase
//     truncate length=10
// }
pub struct TransformsOrderParser;

impl TransformsOrderParser {
    pub fn parse(&self, ctx: ParseContext<'_>) -> miette::Result<Vec<TransformOp>> {
        ctx.validate(&[Rule::NoPositionalArgs, Rule::NoArgs])?;

        let mut steps = Vec::new();

        for step_ctx in ctx.nodes()? {
            let name = step_ctx.name().unwrap_or("").to_string();

            let params = step_ctx
                .args_named_typed()?
                .iter()
                .map(|v| {
                    Ok((
                        v.name().expect("name should be exist").to_string(),
                        v.as_str()?,
                    ))
                })
                .collect::<miette::Result<_>>()?;

            //TODO: parse with correct types. Currently only string params are supported.
            let op = parse_transform(&Transform { name, params })
                .map_err(|err| step_ctx.error(format!("Failed to parse transform: '{err}'")))?;

            steps.push(op);
        }

        Ok(steps)
    }
}
