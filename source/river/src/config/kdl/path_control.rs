use std::collections::BTreeMap;

use kdl::KdlDocument;

use crate::config::{
    common_types::{
        SectionParser, bad::Bad, path_control::PathControl
    },
    kdl::utils,
};


pub struct PathControlSection<'a> {
    doc: &'a KdlDocument,
}

impl SectionParser<KdlDocument, PathControl> for PathControlSection<'_> {
    fn parse_node(&self, parent_node: &KdlDocument) -> miette::Result<PathControl> {
        let mut pc = PathControl::default();

        if let Some(pc_node) = utils::optional_child_doc(self.doc, parent_node, "path-control") {
            
            // request-filters (optional)
            if let Some(node) = utils::optional_child_doc(self.doc, pc_node, "request-filters") {
                pc.request_filters = self.collect_filters(node)?;
            }

            // upstream-request (optional)
            if let Some(node) = utils::optional_child_doc(self.doc, pc_node, "upstream-request") {
                pc.upstream_request_filters = self.collect_filters(node)?;
            }

            // upstream-response (optional)
            if let Some(node) = utils::optional_child_doc(self.doc, pc_node, "upstream-response") {
                pc.upstream_response_filters = self.collect_filters(node)?;
            }
        }
        
        Ok(pc)
    }
}
impl<'a> PathControlSection<'a> {
    
    pub fn new(doc: &'a KdlDocument) -> Self {
        Self { doc }
    }

    /// Collects all the filters, where the node name must be "filter", and the rest of the args
    /// are collected as a BTreeMap of String:String values
    ///
    /// ```kdl
    /// upstream-request {
    ///     filter kind="remove-header-key-regex" pattern=".*SECRET.*"
    ///     filter kind="remove-header-key-regex" pattern=".*secret.*"
    ///     filter kind="upsert-header" key="x-proxy-friend" value="river"
    /// }
    /// ```
    ///
    /// creates something like:
    ///
    /// ```json
    /// [
    ///     { kind: "remove-header-key-regex", pattern: ".*SECRET.*" },
    ///     { kind: "remove-header-key-regex", pattern: ".*secret.*" },
    ///     { kind: "upsert-header", key: "x-proxy-friend", value: "river" }
    /// ]
    /// ```
    fn collect_filters(
        &self,
        node: &KdlDocument,
    ) -> miette::Result<Vec<BTreeMap<String, String>>> {
        let filters = utils::data_nodes(self.doc, node)?;
        let mut fout = vec![];

        for (filter_node, name, args) in filters {
            if name != "filter" {
                return Err(Bad::docspan(
                    format!("Invalid directive '{name}'. Expected 'filter'."),
                    self.doc,
                    &filter_node.span()
                ).into());
            }

            let args = utils::str_str_args(self.doc, args)?;
            fout.push(
                args.iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            );
        }
        Ok(fout)
    }
}