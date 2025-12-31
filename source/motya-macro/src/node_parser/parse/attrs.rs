use darling::{FromDeriveInput, FromField, FromVariant};
use syn::{Expr, Ident, Path, Type};

#[derive(FromDeriveInput)]
#[darling(attributes(node), forward_attrs(doc, allow, warn))]
pub struct NodeStructAttrs {
    pub ident: Ident,
    pub attrs: Vec<syn::Attribute>,

    #[darling(default)]
    pub name: Option<String>,

    #[darling(default)]
    pub root: Option<bool>,

    #[darling(default)]
    pub allow_empty: bool,

    #[darling(default)]
    pub ignore_unknown: bool,
}

#[derive(FromField)]
#[darling(attributes(node), forward_attrs(doc))]
pub struct NodeFieldAttrs {
    pub ident: Option<Ident>,
    pub ty: Type,
    pub attrs: Vec<syn::Attribute>,

    #[darling(default)]
    pub prop: bool,
    #[darling(default)]
    pub arg: bool,
    #[darling(default)]
    pub child: bool,
    #[darling(default)]
    pub dynamic_child: bool,
    #[darling(default)]
    pub node_name: bool,
    #[darling(default)]
    pub all_props: bool,
    #[darling(default)]
    pub all_args: bool,
    #[darling(default)]
    pub flat: bool,

    #[darling(default)]
    pub flatten: bool,

    #[darling(default)]
    pub name: Option<String>,

    #[darling(default)]
    pub default: Option<Expr>,

    #[darling(default)]
    pub parse_with: Option<Path>,

    #[darling(default)]
    pub validate_with: Option<Path>,

    #[darling(default)]
    pub schema_name: Option<String>,

    #[darling(default)]
    pub proxy: Option<Path>,

    #[darling(default)]
    pub group: Option<String>,

    #[darling(default)]
    pub min: Option<usize>,

    #[darling(default)]
    pub max: Option<usize>,
}

#[derive(FromVariant)]
#[darling(attributes(node), forward_attrs(doc, allow, warn))]
pub struct NodeVariantAttrs {
    pub ident: Ident,
    pub attrs: Vec<syn::Attribute>,

    #[darling(default)]
    pub name: Option<String>,
}
