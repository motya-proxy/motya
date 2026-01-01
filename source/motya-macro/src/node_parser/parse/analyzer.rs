use proc_macro2::TokenStream;
use syn::{Field, Type};

use super::attrs::NodeFieldAttrs;
use crate::node_parser::{
    model::{DocTokens, ParseOptions},
    utils::{DocParser, TypeAnalyzer},
};

pub struct AnalyzedField {
    pub original: Field,
    pub attrs: NodeFieldAttrs,
    pub type_info: TypeInfo,
    pub parse_opts: ParseOptions,
    pub docs: DocTokens,
}

pub struct TypeInfo {
    pub inner: Type,
    pub is_vec: bool,
    pub is_option: bool,
    pub primitive_kind: TokenStream,
}

impl AnalyzedField {
    pub fn new(original: Field, attrs: NodeFieldAttrs) -> syn::Result<Self> {
        let (inner, is_vec, is_option) = TypeAnalyzer::analyze(&original.ty);
        let primitive_kind = TypeAnalyzer::to_primitive(&inner);

        let default_expr = attrs.default.clone();

        let mut parse_with = attrs.parse_with.clone();
        if attrs.proxy.is_some() && parse_with.is_none() {
            let p = attrs.proxy.as_ref().unwrap();

            let method: syn::Path = syn::parse_quote!(#p::parse_as);
            parse_with = Some(method);
        }

        let parse_opts = ParseOptions {
            parse_with,
            default: default_expr,
            schema_name: attrs.schema_name.clone(),
            min: attrs.min,
            max: attrs.max,
            proxy: attrs.proxy.clone(),
            validate_with: attrs.validate_with.clone(),
            flatten: attrs.flatten,
        };

        let docs = DocParser::parse(&attrs.attrs);

        Ok(Self {
            original,
            attrs,
            type_info: TypeInfo {
                inner,
                is_vec,
                is_option,
                primitive_kind,
            },
            parse_opts,
            docs,
        })
    }

    pub fn ident(&self) -> &syn::Ident {
        self.attrs.ident.as_ref().unwrap()
    }
}
