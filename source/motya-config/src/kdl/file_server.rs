use std::path::PathBuf;

use kdl::KdlDocument;
use motya_macro::validate;

use crate::{
    common_types::{file_server::FileServerPartialConfig, section_parser::SectionParser},
    kdl::parser::{
        ctx::ParseContext,
        ensures::Rule,
        utils::{OptionTypedValueExt, PrimitiveType},
    },
};

pub struct FileServerSection<'a> {
    name: &'a str,
}

impl SectionParser<ParseContext<'_>, FileServerPartialConfig> for FileServerSection<'_> {
    #[validate(ensure_node_name = "file-server")]
    fn parse_node(&self, ctx: ParseContext) -> miette::Result<FileServerPartialConfig> {
        ctx.validate(&[
            Rule::NoChildren,
            Rule::NoPositionalArgs,
            Rule::OnlyKeysTyped(&[("base-path", PrimitiveType::String)]),
        ])?;

        let base_path = ctx.opt_prop("base-path")?.as_str()?.map(PathBuf::from);

        Ok(FileServerPartialConfig {
            name: self.name.to_string(),
            base_path,
        })
    }
}

impl<'a> FileServerSection<'a> {
    pub fn new(name: &'a str) -> Self {
        Self { name }
    }
}
