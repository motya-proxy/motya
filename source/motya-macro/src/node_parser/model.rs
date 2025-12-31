use proc_macro2::TokenStream;
use syn::{Expr, Ident, Type};

pub struct NodeModel {
    pub struct_name: Ident,
    pub kdl_name: Option<String>,
    pub docs: DocTokens,
    pub props: Vec<PropSpec>,
    pub node_name_field: Option<NameSpec>,
    pub args: Vec<ArgSpec>,
    pub block: BlockSpec,
    pub explicit_name: bool,
    pub allow_empty_block: bool,
    pub kind: NodeModelKind,
    pub all_props_field: Option<BaseField>,
    pub all_args_field: Option<BaseField>,
    pub is_root: bool,
    pub ignore_unknown: bool
}

pub struct BaseField {
    pub ident: Ident,
    pub inner_type: Type,
    pub is_option: bool,
    pub opts: ParseOptions,
    pub docs: DocTokens,
}
pub struct NameSpec {
    pub base: BaseField,
}

pub struct PropSpec {
    pub base: BaseField,
    pub key: String,
    pub primitive_kind: TokenStream,
    pub required: bool,
}

pub struct ArgSpec {
    pub base: BaseField,
    pub name: String,
    pub primitive_kind: TokenStream,
    pub required: bool,
}

pub enum NodeModelKind {
    Struct,
    Enum(Vec<VariantSpec>),
}

pub struct VariantSpec {
    pub ident: syn::Ident,
    pub kdl_name: Option<String>,
    pub fields: VariantFields,
    pub docs: DocTokens,
}

pub enum VariantFields {
    Unit,
    Newtype(syn::Type),
    Struct {
        props: Vec<PropSpec>,
        args: Vec<ArgSpec>,
        block: BlockSpec,
        node_name: Option<NameSpec>,
        is_tuple: bool,
        all_props: Option<BaseField>,
        all_args: Option<BaseField>,
    },
}

pub enum ParsedField {
    Prop(PropSpec),
    Arg(ArgSpec),
    Child(ChildSpec),
    Dynamic((syn::Ident, syn::Type, ParseOptions, DocTokens)),
    Ignored,
    Name(NameSpec),
    AllProps(BaseField),
    AllArgs(BaseField),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChildMode {
    Node,
    Field,
}

pub struct ChildSpec {
    pub base: BaseField,
    pub multiplicity: TokenStream,
    pub is_vec: bool,
    pub group: Option<String>,
    pub mode: ChildMode,
    pub name: Option<String>,
}

#[allow(clippy::large_enum_variant)]
pub enum BlockSpec {
    Empty,
    Strict(Vec<ChildSpec>),
    Dynamic {
        field_ident: Ident,
        inner_type: Type,
        opts: ParseOptions,
        docs: DocTokens,
    },
}

#[derive(Clone)]
pub struct ParseOptions {
    pub parse_with: Option<syn::Path>,
    pub default: Option<Expr>,
    pub schema_name: Option<String>,
    pub min: Option<usize>,
    pub max: Option<usize>,
    pub proxy: Option<syn::Path>,
    pub validate_with: Option<syn::Path>,
    pub flatten: bool,
}

#[derive(Clone)]
pub struct DocTokens(pub TokenStream);
