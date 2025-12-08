use crate::{
    common_types::{bad::Bad, section_parser::SectionParser},
    kdl::utils,
};
use kdl::KdlDocument;

pub struct IncludesSection<'a> {
    doc: &'a KdlDocument,
    name: &'a str
}

impl SectionParser<KdlDocument, Vec<String>> for IncludesSection<'_> {
    fn parse_node(&self, _node: &KdlDocument) -> miette::Result<Vec<String>> {
        self.extract_includes()
    }
}

impl<'a> IncludesSection<'a> {
    pub fn new(doc: &'a KdlDocument, name: &'a str) -> Self {
        Self { doc, name }
    }

    fn extract_includes(&self) -> miette::Result<Vec<String>> {
        let mut paths = Vec::new();

        if let Some(inc_block) = utils::optional_child_doc(self.doc, self.doc, "includes") {
            
            let nodes = utils::data_nodes(self.doc, inc_block)?;

            for (node, name, args) in nodes {
                
                if node.children().is_some() {
                    return Err(Bad::docspan(
                        format!("Includes directive must be a simple node with a single string argument (e.g., '{} \"path/to/file.kdl\"'), but it has a block.", name),
                        self.doc,
                        &node.span(),
                        self.name
                    ).into());
                }

                if !args.is_empty() {
                    return Err(Bad::docspan(
                        format!("Include path '{}' should not have additional arguments. Expected just a string path.", name),
                        self.doc,
                        &node.span(),
                        self.name
                    ).into());
                }

                paths.push(name.to_string());
            }

            if paths.is_empty() {
                return Err(Bad::docspan(
                    "The 'includes' block is present but contains no include paths. Expected nodes like 'path/to/file.kdl'.",
                    self.doc,
                    &inc_block.span(),
                    self.name
                ).into());
            }
        }

        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kdl::KdlDocument;

    const VALID_INCLUDES: &str = r#"
    includes {
        "path/to/first.kdl"
        "second.kdl"
        "../parent/config.kdl"
    }
    "#;

    #[test]
    fn test_valid_includes() {
        let doc: KdlDocument = VALID_INCLUDES.parse().unwrap();
        let parser = IncludesSection::new(&doc, "test.kdl");
        let paths = parser.extract_includes().expect("Should parse valid includes");

        assert_eq!(paths.len(), 3);
        assert_eq!(paths[0], "path/to/first.kdl");
        assert_eq!(paths[1], "second.kdl");
        assert_eq!(paths[2], "../parent/config.kdl");
    }

    const NO_INCLUDES_SECTION: &str = r#"
    system {
        threads-per-service 2
    }
    
    services {
        TestService {
            listeners { "127.0.0.1:8080" }
            connectors {
                return code="200" response="OK"
            }
        }
    }
    "#;

    #[test]
    fn test_no_includes_section() {
        let doc: KdlDocument = NO_INCLUDES_SECTION.parse().unwrap();
        let parser = IncludesSection::new(&doc, "test.kdl");
        let paths = parser.extract_includes().expect("Should return empty vec");

        assert!(paths.is_empty());
    }

    const EMPTY_INCLUDES_BLOCK: &str = r#"
    includes {}
    "#;

    #[test]
    fn test_error_empty_includes_block() {
        let doc: KdlDocument = EMPTY_INCLUDES_BLOCK.parse().unwrap();
        let parser = IncludesSection::new(&doc, "test.kdl");
        let result = parser.extract_includes();

        assert!(result.is_err());
        let err_msg = result.unwrap_err().help().unwrap().to_string();
        crate::assert_err_contains!(
            err_msg,
            "The 'includes' block is present but contains no include paths"
        );
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
        let doc: KdlDocument = INCLUDE_WITH_BLOCK_CHILDREN.parse().unwrap();
        let parser = IncludesSection::new(&doc, "test.kdl");
        let result = parser.extract_includes();

        assert!(result.is_err());
        let err_msg = result.unwrap_err().help().unwrap().to_string();
        crate::assert_err_contains!(
            err_msg,
            "Includes directive must be a simple node with a single string argument"
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
        let doc: KdlDocument = INCLUDE_WITH_MULTIPLE_ARGS.parse().unwrap();
        let parser = IncludesSection::new(&doc, "test.kdl");
        let result = parser.extract_includes();

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
        let doc: KdlDocument = INCLUDE_WITH_NAMED_ARG.parse().unwrap();
        let parser = IncludesSection::new(&doc, "test.kdl");
        let result = parser.extract_includes();

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
                return code="200" response="Service One"
            }
        }
    }
    "#;

    #[test]
    fn test_includes_in_complex_document() {
        let doc: KdlDocument = COMPLEX_DOCUMENT_WITH_INCLUDES.parse().unwrap();
        let parser = IncludesSection::new(&doc, "main.kdl");
        let paths = parser.extract_includes().expect("Should parse includes");

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
        let doc: KdlDocument = INCLUDE_WITH_COMMENTS.parse().unwrap();
        let parser = IncludesSection::new(&doc, "test.kdl");
        let paths = parser.extract_includes().expect("Should ignore comments");

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
        let doc: KdlDocument = INCLUDE_WITH_SPECIAL_CHARACTERS.parse().unwrap();
        let parser = IncludesSection::new(&doc, "test.kdl");
        let paths = parser.extract_includes().expect("Should parse paths with special chars");

        assert_eq!(paths.len(), 4);
        assert_eq!(paths[0], "path with spaces.kdl");
        assert_eq!(paths[1], "C:\\Windows\\Path\\config.kdl");
        assert_eq!(paths[2], "/unix/path/with-dashes.kdl");
        assert_eq!(paths[3], "relative/../parent/./current/config.kdl");
    }
}
