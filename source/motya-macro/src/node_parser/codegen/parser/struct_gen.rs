use proc_macro2::TokenStream;
use quote::quote;

use super::content_gen::ContentGenerator;
use crate::node_parser::{
    codegen::parser::{Namespaces, types::ParseTarget},
    model::NodeModel,
};

pub struct StructGenerator<'a> {
    model: &'a NodeModel,
    namespaces: Namespaces,
}

impl<'a> StructGenerator<'a> {
    pub fn new(model: &'a NodeModel, namespaces: Namespaces) -> Self {
        Self { model, namespaces }
    }

    pub fn generate(&self) -> TokenStream {
        let name_extraction = if self.model.is_root {
            quote! { let __actual_name = "<document_root>"; }
        } else {
            self.gen_name_extraction()
        };

        let name_check = if self.model.is_root {
            quote! {}
        } else {
            self.gen_node_name_check()
        };

        let content_gen = ContentGenerator::new(&self.namespaces, self.model);

        let target = ParseTarget {
            props: &self.model.props,
            args: &self.model.args,
            block: &self.model.block,
            all_props: &self.model.all_props_field,
            all_args: &self.model.all_args_field,
            node_name: &self.model.node_name_field,
            ctor_path: quote!(Self),
            is_tuple: false,
        };

        let body = content_gen.gen_body(&target);

        let error_mod = &self.namespaces.error_mod;

        quote! {
            let mut __errors: Vec<#error_mod::ParseError> = Vec::new();
            #name_extraction
            #name_check
            #body
        }
    }

    fn gen_node_name_check(&self) -> TokenStream {
        let helpers = &self.namespaces.helpers;
        let error_mod = &self.namespaces.error_mod;

        if let Some(spec) = &self.model.node_name_field {
            let ident = &spec.base.ident;
            let ty = &spec.base.inner_type;
            let opts = &spec.base.opts;

            let parse_expr = if let Some(func) = &opts.parse_with {
                quote! { #func(__actual_name, state) }
            } else if quote!(#ty).to_string() == "String" {
                quote! { Ok::<#ty, ::std::convert::Infallible>(__actual_name.to_string()) }
            } else {
                quote! { __actual_name.parse::<#ty>() }
            };

            quote! {
                let #ident: #ty = match #parse_expr {
                    Ok(v) => v,
                    Err(e) => {
                        let msg = format!("Invalid node name '{}': {}", __actual_name, e);
                        #helpers::push_custom(
                            &mut __errors,
                            msg,
                            None,
                            ctx.current_span(),
                            ctx.source().clone()
                        );
                        return Err(#error_mod::ConfigError::from_list(__errors));
                    }
                };
            }
        } else {
            quote! {}
        }
    }

    fn gen_name_extraction(&self) -> TokenStream {
        let helpers = &self.namespaces.helpers;
        let error_mod = &self.namespaces.error_mod;

        quote! {
            let __actual_name = match ctx.name() {
                Ok(n) => n,
                Err(e) => {
                    return Err(#error_mod::ConfigError::from_list(vec![
                        #helpers::to_parse_error(e, ctx.current_span(), ctx.source().clone())
                    ]));
                }
            };
        }
    }
}
