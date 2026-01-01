use proc_macro2::TokenStream;
use quote::quote;

use super::content_gen::ContentGenerator;
use crate::node_parser::{
    codegen::{
        heuristics::ScoreGenerator,
        parser::{Namespaces, types::ParseTarget},
    },
    model::{NodeModel, VariantFields, VariantSpec},
};

pub struct EnumGenerator<'a> {
    model: &'a NodeModel,
    variants: &'a [VariantSpec],
    namespaces: Namespaces,
}

impl<'a> EnumGenerator<'a> {
    pub fn new(model: &'a NodeModel, variants: &'a [VariantSpec], namespaces: Namespaces) -> Self {
        Self {
            model,
            variants,
            namespaces,
        }
    }

    pub fn generate(&self) -> TokenStream {
        let error_mod = &self.namespaces.error_mod;
        let helpers = &self.namespaces.helpers;

        let variant_names: Vec<String> = self
            .variants
            .iter()
            .map(|v| {
                if let Some(kdl_name) = &v.kdl_name {
                    kdl_name.clone()
                } else {
                    v.ident.to_string()
                }
            })
            .collect();

        let match_arms = self.variants.iter().enumerate().map(|(idx, v)| {
            let body = self.gen_variant_body(v);
            quote! { Some(#idx) => { #body } }
        });

        quote! {
            let scores = Self::__kdl_score_variants(ctx);

            let best_match = scores
                .iter()
                .enumerate()
                .filter(|(_, (s, _))| *s >= 0)
                .max_by_key(|(_, (s, _))| *s)
                .map(|(i, _)| i);

            match best_match {
                #(#match_arms)*
                _ => {
                    let node_name = ctx.name().unwrap_or("?");
                    let v_names = [ #(#variant_names),* ];

                    let mut relevant_errors = Vec::new();

                    for (i, (score, reason)) in scores.iter().enumerate() {
                        let v_name = v_names[i];

                        if v_name == node_name || *score > -1 {
                            let reason_text = reason.as_deref().unwrap_or("Unknown validation error");
                            relevant_errors.push((v_name, reason_text));
                        }
                    }

                    let msg = if relevant_errors.is_empty() {

                        format!(
                            "Unexpected node '{}'. Expected one of the following nodes: {:?}",
                            node_name,
                            v_names
                        )
                    } else {
                        let mut s = format!("Invalid usage of node '{}'. Errors:", node_name);

                        for (v_name, reason) in relevant_errors {
                            if v_name == node_name {
                                s.push_str(&format!("\n  - {}", reason));
                            } else {
                                s.push_str(&format!("\n  - Variant '{}': {}", v_name, reason));
                            }
                        }
                        s
                    };

                    Err(#error_mod::ConfigError::from_list(vec![
                        #helpers::to_parse_error(ctx.error(msg), ctx.current_span(), ctx.source())
                    ]))
                }
            }
        }
    }

    fn gen_variant_body(&self, v: &VariantSpec) -> TokenStream {
        let ident = &v.ident;
        let content_gen = ContentGenerator::new(&self.namespaces, self.model);

        match &v.fields {
            VariantFields::Unit => quote!(Ok(Self::#ident)),
            VariantFields::Newtype(ty) => quote! {
                let data = <#ty as crate::kdl::parser::parsable::KdlParsable<S>>::parse_node(ctx, state)?;
                Ok(Self::#ident(data))
            },
            VariantFields::Struct {
                props,
                args,
                block,
                is_tuple,
                all_props,
                all_args,
                node_name,
            } => {
                let target = ParseTarget {
                    props,
                    args,
                    block,
                    all_props,
                    all_args,
                    node_name,
                    ctor_path: quote!(Self::#ident),
                    is_tuple: *is_tuple,
                };

                let body = content_gen.gen_body(&target);

                quote! {
                    let mut __errors = Vec::new();
                    #body
                }
            }
        }
    }

    pub fn gen_scoring_helper(&self) -> TokenStream {
        let struct_name = &self.model.struct_name;
        let count = self.variants.len();

        let calcs = self
            .variants
            .iter()
            .map(ScoreGenerator::gen_variant_score);

        quote! {
            impl #struct_name {
                #[doc(hidden)]
                pub fn __kdl_score_variants(ctx: &crate::kdl::parser::ctx::ParseContext) -> [(isize, Option<String>); #count] {
                    [ #(#calcs),* ]
                }
            }
        }
    }
}
