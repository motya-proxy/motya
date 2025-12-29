use crate::node_parser::{
    codegen::heuristics::ScoreGenerator,
    model::{NodeModel, NodeModelKind},
};
use proc_macro2::TokenStream;
use quote::quote;

mod child_gen;
mod constructor_gen;
mod content_gen;
mod enum_gen;
mod field_gen;
mod struct_gen;
mod types;
mod validation;

use enum_gen::EnumGenerator;
use struct_gen::StructGenerator;

#[derive(Clone)]
pub struct Namespaces {
    pub helpers: TokenStream,
    pub error_mod: TokenStream,
}

impl Namespaces {
    pub fn new() -> Self {
        Self {
            helpers: quote!(crate::kdl::parser::utils::macros_helpers),
            error_mod: quote!(crate::common_types::error),
        }
    }
}

pub struct ParserGenerator<'a> {
    model: &'a NodeModel,
}

impl<'a> ParserGenerator<'a> {
    pub fn new(model: &'a NodeModel) -> Self {
        Self { model }
    }

    pub fn generate(&self) -> TokenStream {
        let struct_name = &self.model.struct_name;
        let ns = Namespaces::new();

        let mut helper_impl = quote!();

        let body = match &self.model.kind {
            NodeModelKind::Struct => StructGenerator::new(self.model, ns.clone()).generate(),
            NodeModelKind::Enum(variants) => {
                let gen_ = EnumGenerator::new(self.model, variants, ns.clone());

                helper_impl = gen_.gen_scoring_helper();
                gen_.generate()
            }
        };

        let parsable_impl = self.wrap_trait_impl(struct_name, body, &ns);
        let schema_impl = self.gen_node_schema_impl(struct_name);

        quote! {
            #helper_impl
            #parsable_impl
            #schema_impl
        }
    }

    fn wrap_trait_impl(
        &self,
        struct_name: &syn::Ident,
        body: TokenStream,
        namespaces: &Namespaces,
    ) -> TokenStream {
        let error_mod = &namespaces.error_mod;

        quote! {
            #[allow(clippy::all, unused_mut, unused_variables)]
            impl<S> crate::kdl::parser::parsable::KdlParsable<S> for #struct_name {
                fn parse_node(
                    ctx: &crate::kdl::parser::ctx::ParseContext,
                    state: &S
                ) -> Result<Self, #error_mod::ConfigError> {
                    #body
                }
            }
        }
    }

    fn gen_node_schema_impl(&self, struct_name: &syn::Ident) -> TokenStream {
        let names_body = match &self.model.kind {
            NodeModelKind::Struct => {
                if let Some(name) = &self.model.kdl_name {
                    quote! { &[#name] }
                } else {
                    quote! { &[] }
                }
            }
            NodeModelKind::Enum(variants) => {
                if let Some(enum_name) = &self.model.kdl_name {
                    quote! { &[#enum_name] }
                } else {
                    let mut collected_names = Vec::new();
                    for v in variants {
                        if let Some(name) = &v.kdl_name {
                            collected_names.push(name.clone());
                        } else {
                            let name = v.ident.to_string().replace('_', "-").to_lowercase();
                            collected_names.push(name);
                        }
                    }
                    if collected_names.is_empty() {
                        quote! { &[] }
                    } else {
                        quote! { &[ #(#collected_names),* ] }
                    }
                }
            }
        };

        let match_score_body = ScoreGenerator::gen_match_score_impl(self.model);

        quote! {
            impl crate::kdl::parser::node_schema::NodeSchema for #struct_name {
                fn applicable_node_names() -> &'static [&'static str] {
                    #names_body
                }

                fn match_score(ctx: &crate::kdl::parser::ctx::ParseContext) -> (isize, Option<String>) {
                    #match_score_body
                }
            }
        }
    }
}
