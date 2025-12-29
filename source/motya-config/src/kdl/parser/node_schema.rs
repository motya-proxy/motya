use crate::kdl::parser::ctx::ParseContext;

pub trait NodeSchema {
    fn applicable_node_names() -> &'static [&'static str];

    fn match_score(ctx: &ParseContext) -> (isize, Option<String>);
}
