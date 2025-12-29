use crate::kdl::parser::utils::PrimitiveType;

#[derive(Debug, Clone, Copy)]
pub struct DocEntry {
    pub lang: &'static str,
    pub text: &'static str,
}

impl DocEntry {
    pub fn find_in(docs: &[DocEntry], lang: &str) -> &'static str {
        docs.iter()
            .find(|d| d.lang == lang)
            .map(|d| d.text)
            .or_else(|| docs.iter().find(|d| d.lang == "en").map(|d| d.text))
            .unwrap_or("")
    }
}

/// Defines how many times a child node is allowed to appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Multiplicity {
    Optional,   // ? (0 or 1)
    Required,   // ! (exactly 1)
    Repeated,   // * (0 or more)
    AtLeastOne, // + (1 or more)
}

/// Describes a relationship "Parent -> Child".
#[derive(Debug, Clone, Copy)]
pub struct ChildRule {
    pub keyword: Option<&'static str>,
    pub rule: Multiplicity,
    pub docs: &'static [DocEntry],
}

/// Describes a named property (key="value").
#[derive(Debug, Clone, Copy)]
pub struct PropDef {
    pub key: &'static str,
    pub kind: PrimitiveType,
    pub required: bool,
    pub docs: &'static [DocEntry],
}

/// Describes a positional argument (node arg1 arg2).
#[derive(Debug, Clone, Copy)]
pub struct ArgDef {
    pub name: &'static str, // Argument name for documentation
    pub kind: PrimitiveType,
    pub required: bool,
    pub docs: &'static [DocEntry],
}

/// Describes the content allowed inside the node's block `{ ... }`.
pub enum BlockContent {
    /// No children block allowed (leaf node).
    Empty,

    /// A strict map of allowed named children nodes.
    /// Used for structural nodes (e.g., `connectors`, `proxy`).
    Strict(&'static [ChildRule]),

    /// A dynamic list of values/nodes.
    /// Used when children names are arbitrary or values (e.g., `includes`).
    DynamicList {
        child_type: &'static str,
        docs: &'static [DocEntry],
    },
}

/// The main trait that every configuration node must implement.
/// This serves as the Source of Truth for validation and documentation.
pub trait NodeDefinition {
    const KEYWORD: Option<&'static str>;

    const SCHEMA_NAME: &'static str;

    /// Human-readable description.
    const DOCS: &'static [DocEntry];

    /// Definition of positional arguments.
    const ARGS: &'static [ArgDef] = &[];

    /// Definition of named properties (key=value).
    const PROPS: &'static [PropDef] = &[];

    /// Definition of the children block.
    const BLOCK: BlockContent = BlockContent::Empty;

    const IS_VALUE_WRAPPER: bool = false;

    /// A list of valid node names for this type.
    /// For structs, it's usually just `[KEYWORD]`.
    /// For enums, it's the list of all variant names.
    const ALLOWED_NAMES: &'static [&'static str];
}

impl<T: NodeDefinition> NodeDefinition for Box<T> {
    const KEYWORD: Option<&'static str> = T::KEYWORD;
    const SCHEMA_NAME: &'static str = T::SCHEMA_NAME;
    const DOCS: &'static [DocEntry] = T::DOCS;
    const ARGS: &'static [ArgDef] = T::ARGS;
    const PROPS: &'static [PropDef] = T::PROPS;
    const BLOCK: BlockContent = T::BLOCK;
    const IS_VALUE_WRAPPER: bool = T::IS_VALUE_WRAPPER;
    const ALLOWED_NAMES: &'static [&'static str] = T::ALLOWED_NAMES;
}
