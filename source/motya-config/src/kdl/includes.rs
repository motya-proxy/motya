use crate::{
    common_types::section_parser::SectionParser,
    kdl::parser::{ctx::ParseContext, ensures::Rule},
};
use miette::Result;
use motya_macro::validate;

pub struct IncludesSection;

impl SectionParser<ParseContext<'_>, Vec<String>> for IncludesSection {
    #[validate(ensure_node_name = "includes")]
    fn parse_node(&self, ctx: ParseContext) -> miette::Result<Vec<String>> {
        self.extract_includes(ctx)
    }
}

impl IncludesSection {
    fn extract_includes(&self, ctx: ParseContext) -> Result<Vec<String>> {
        let result = ctx
            .req_nodes()?
            .iter()
            .map(|node| {
                let path = node.name()?;

                node.validate(&[Rule::NoArgs, Rule::NoChildren])?;

                Ok(path.to_string())
            })
            .collect::<Result<Vec<String>>>()?;

        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_err_contains;
    use crate::kdl::parser::block::BlockParser;
    use crate::kdl::parser::ctx::Current;
    use kdl::KdlDocument;

    fn parse_includes(input: &str) -> miette::Result<Vec<String>> {
        let doc: KdlDocument = input.parse().unwrap();

        let ctx = ParseContext::new(&doc, Current::Document(&doc), "test.kdl");
        let mut block = BlockParser::new(ctx)?;

        let includes = block.required("includes", |ctx| IncludesSection.parse_node(ctx))?;

        Ok(includes)
    }

    const VALID_INCLUDES: &str = r#"
    includes {
        "path/to/first.kdl"
        "second.kdl"
        "../parent/config.kdl"
    }
    "#;

    #[test]
    fn test_valid_includes() {
        let paths = parse_includes(VALID_INCLUDES).expect("Should parse valid includes");

        assert_eq!(paths.len(), 3);
        assert_eq!(paths[0], "path/to/first.kdl");
        assert_eq!(paths[1], "second.kdl");
        assert_eq!(paths[2], "../parent/config.kdl");
    }

    const EMPTY_INCLUDES_BLOCK: &str = r#"
    includes {}
    "#;

    #[test]
    fn test_error_empty_includes_block() {
        let result = parse_includes(EMPTY_INCLUDES_BLOCK);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().help().unwrap().to_string();
        assert_err_contains!(err_msg, "Block 'includes' cannot be empty");
    }

    const INCLUDE_WITH_BLOCK_CHILDREN: &str = r#"
    includes {
        "simple.kdl"
        "with-block.kdl" {
            some "nested" "content"
        }
    }
    "#;

    #[test]
    fn test_error_include_with_block_children() {
        let result = parse_includes(INCLUDE_WITH_BLOCK_CHILDREN);

        let err_msg = result.unwrap_err().help().unwrap().to_string();
        assert_err_contains!(
            err_msg,
            "Directive 'with-block.kdl' must be a leaf node (no children block allowed)"
        );
    }

    const INCLUDE_WITH_MULTIPLE_ARGS: &str = r#"
    includes {
        "single.kdl"
        "multiple.kdl" "extra" "args"
    }
    "#;

    #[test]
    fn test_error_include_with_multiple_args() {
        let result = parse_includes(INCLUDE_WITH_MULTIPLE_ARGS);
        assert!(result.is_err());
    }

    const INCLUDE_WITH_NAMED_ARG: &str = r#"
    includes {
        "good.kdl"
        "bad.kdl" arg="value"
    }
    "#;

    #[test]
    fn test_error_include_with_named_arg() {
        let result = parse_includes(INCLUDE_WITH_NAMED_ARG);
        assert!(result.is_err());
    }

    const COMPLEX_DOCUMENT_WITH_INCLUDES: &str = r#"
    includes {
        "definitions.kdl"
        "plugins/rate-limiter.kdl"
    }
    
    system {
        threads-per-service 4
    }
    
    services {
        ServiceOne {
            listeners { "0.0.0.0:8080" }
            connectors {
                return code=200 response="Service One"
            }
        }
    }
    "#;

    #[test]
    fn test_includes_in_complex_document() {
        let paths = parse_includes(COMPLEX_DOCUMENT_WITH_INCLUDES).expect("Should parse includes");

        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], "definitions.kdl");
        assert_eq!(paths[1], "plugins/rate-limiter.kdl");
    }

    const INCLUDE_WITH_COMMENTS: &str = r#"
    includes {
        // Development config
        "dev/overrides.kdl"
        // Production config
        "prod/settings.kdl"
        // Common config
        "common.kdl"
    }
    "#;

    #[test]
    fn test_includes_with_comments() {
        let paths = parse_includes(INCLUDE_WITH_COMMENTS).expect("Should ignore comments");

        assert_eq!(paths.len(), 3);
        assert_eq!(paths[0], "dev/overrides.kdl");
        assert_eq!(paths[1], "prod/settings.kdl");
        assert_eq!(paths[2], "common.kdl");
    }

    const INCLUDE_WITH_SPECIAL_CHARACTERS: &str = r#"
    includes {
        "path with spaces.kdl"
        "C:\\Windows\\Path\\config.kdl"
        "/unix/path/with-dashes.kdl"
        "relative/../parent/./current/config.kdl"
    }
    "#;

    #[test]
    fn test_includes_with_special_characters() {
        let paths = parse_includes(INCLUDE_WITH_SPECIAL_CHARACTERS)
            .expect("Should parse paths with special chars");

        assert_eq!(paths.len(), 4);
        assert_eq!(paths[0], "path with spaces.kdl");
        assert_eq!(paths[1], "C:\\Windows\\Path\\config.kdl");
        assert_eq!(paths[2], "/unix/path/with-dashes.kdl");
        assert_eq!(paths[3], "relative/../parent/./current/config.kdl");
    }
}
