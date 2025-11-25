use std::collections::BTreeMap;


#[derive(Debug, Clone, Default, PartialEq)]
pub struct PathControl {
    pub(crate) request_filters: Vec<BTreeMap<String, String>>,
    pub(crate) upstream_request_filters: Vec<BTreeMap<String, String>>,
    pub(crate) upstream_response_filters: Vec<BTreeMap<String, String>>,
}

pub trait PathControlSectionParser<T> {
    fn parse_node(&self, node: &T) -> miette::Result<PathControl>;
}
