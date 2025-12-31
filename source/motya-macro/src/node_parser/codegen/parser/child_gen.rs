use super::Namespaces;
use super::validation::ValidationGenerator;
use crate::node_parser::codegen::utils::gen_value_parser;
use crate::node_parser::model::{BlockSpec, ChildMode, ChildSpec, ParseOptions};
use proc_macro2::TokenStream;
use quote::quote;

pub struct ChildGenerator<'a> {
    namespaces: &'a Namespaces,
    validator: &'a ValidationGenerator<'a>,
}

impl<'a> ChildGenerator<'a> {
    pub fn new(namespaces: &'a Namespaces, validator: &'a ValidationGenerator<'a>) -> Self {
        Self {
            namespaces,
            validator,
        }
    }

    pub fn generate(&self, block: &BlockSpec, ignore_unknown: bool) -> TokenStream {
        match block {
            BlockSpec::Empty => quote!(),

            BlockSpec::Dynamic {
                field_ident,
                inner_type,
                opts,
                ..
            } => self.gen_dynamic_block(field_ident, inner_type, opts),

            BlockSpec::Strict(children) => self.gen_strict_block(children, ignore_unknown),
        }
    }

    fn gen_dynamic_block(
        &self,
        ident: &syn::Ident,
        inner_type: &syn::Type,
        opts: &ParseOptions,
    ) -> TokenStream {
        let helpers = &self.namespaces.helpers;

        let parse_call = if let Some(func) = &opts.parse_with {
            quote!(#func(&child_ctx, state))
        } else {
            quote!(<#inner_type as crate::kdl::parser::parsable::KdlParsable<S>>::parse_node(&child_ctx, state))
        };

        let vec_checks = self.validator.gen_vec_bounds(ident, opts, "Children block");

        quote! {
            let mut #ident = Vec::new();
            if let Ok(iter) = ctx.nodes() {
                for child_ctx in iter {
                    match #parse_call {
                        Ok(v) => #ident.push(v),
                        Err(e) => #helpers::merge_child_errors(&mut __errors, Err(e)),
                    }
                }
            }
            #vec_checks
        }
    }

    fn gen_strict_block(&self, children: &[ChildSpec], ignore_unknown: bool) -> TokenStream {
        let helpers = &self.namespaces.helpers;
        let error_mod = &self.namespaces.error_mod;

        let mut decls = Vec::new();
        let mut processing = Vec::new();
        let mut finals = Vec::new();

        for child in children {
            let ident = &child.base.ident;
            if child.is_vec {
                decls.push(quote!(let mut #ident = Vec::new();));
            } else {
                decls.push(quote!(let mut #ident = None;));
            }
        }

        decls.push(quote! {
            let mut __children_map: std::collections::HashMap<String, Vec<crate::kdl::parser::ctx::ParseContext>> =
                std::collections::HashMap::new();

            if let Ok(nodes) = ctx.nodes() {
                for child in nodes {
                    if let Ok(name) = child.name() {
                        __children_map.entry(name.to_string()).or_default().push(child);
                    }
                }
            }
        });

        for child in children {
            let ident = &child.base.ident;
            let inner = &child.base.inner_type;
            let opts = &child.base.opts;

            if child.name.is_some() && opts.flatten {
                return syn::Error::new_spanned(
                    ident,
                    "Attributes `name` and `flatten` are mutually exclusive.",
                )
                .to_compile_error();
            }

            let names_expr = if let Some(n) = &child.name {
                quote! { &[#n] }
            } else if opts.flatten {
                quote! { <#inner as crate::kdl::parser::node_schema::NodeSchema>::applicable_node_names() }
            } else {
                match child.mode {
                    ChildMode::Field => {
                        let n = ident.to_string().replace('_', "-");
                        quote! { &[#n] }
                    }
                    ChildMode::Node => {
                        let default_name = ident.to_string().replace('_', "-");
                        quote! {
                            {
                                let schema_names = <#inner as crate::kdl::parser::node_schema::NodeSchema>::applicable_node_names();
                                if schema_names.is_empty() {
                                    &[#default_name]
                                } else {
                                    schema_names
                                }
                            }
                        }
                    }
                }
            };

            let parse_call = if let Some(func) = &opts.parse_with {
                quote!(#func(&child_ctx, state))
            } else {
                match child.mode {
                    crate::node_parser::model::ChildMode::Field => {
                        let val_parser = gen_value_parser(inner, opts);
                        let val_validator = self.validator.gen_value_check(opts, child.is_vec);
                        quote! {
                            (|| -> std::result::Result<_, miette::Report> {
                                let v = child_ctx.arg(0)?;
                                let ctx = &child_ctx;
                                let val = #val_parser;
                                #val_validator
                                Ok(val)
                            })().map_err(|e| #error_mod::ConfigError::from_list(vec![
                                #helpers::to_parse_error(e, child_ctx.current_span(), child_ctx.source())
                            ]))
                        }
                    }
                    crate::node_parser::model::ChildMode::Node => {
                        quote!(<#inner as crate::kdl::parser::parsable::KdlParsable<S>>::parse_node(&child_ctx, state))
                    }
                }
            };

            let error_node_name_literal = if let Some(n) = &child.name {
                n.clone()
            } else {
                ident.to_string().replace('_', "-")
            };

            let is_mandatory =
                !child.base.is_option && !child.is_vec && child.base.opts.default.is_none();

            let cardinality_checks = if child.is_vec {
                let mut checks = Vec::new();

                if let Some(min) = opts.min {
                    checks.push(quote! {
                        if #ident.len() < #min {
                            let msg = format!("Expected at least {} nodes of '{}', found {}", #min, #error_node_name_literal, #ident.len());
                            #helpers::push_custom(&mut __errors, msg, None, ctx.current_span(), ctx.source().clone());
                        }
                    });
                }
                if let Some(max) = opts.max {
                    checks.push(quote! {
                        if #ident.len() > #max as usize {
                            let msg = format!("Expected at most {} nodes of '{}', found {}", #max, #error_node_name_literal, #ident.len());
                            #helpers::push_custom(&mut __errors, msg, None, ctx.current_span(), ctx.source().clone());
                        }
                    });
                }

                quote!( #(#checks)* )
            } else {
                quote!()
            };

            let missing_error_logic = if is_mandatory {
                quote! {
                    let available_nodes: Vec<_> = __children_map.keys().collect();
                    let msg = if available_nodes.is_empty() {
                        format!("Missing required node(s). Expected one of: {:?}", __lookup_names)
                    } else {
                        format!("Missing required node(s). Expected one of: {:?}. Found other nodes: {:?}", __lookup_names, available_nodes)
                    };
                    #helpers::push_custom(&mut __errors, msg, None, ctx.current_span(), ctx.source().clone());
                }
            } else {
                quote!()
            };

            let parse_logic = if child.is_vec {
                quote! {
                    for child_ctx in __extracted_nodes {
                        match #parse_call {
                            Ok(v) => #ident.push(v),
                            Err(e) => #helpers::merge_child_errors(&mut __errors, Err(e)),
                        }
                    }
                    #cardinality_checks
                }
            } else {
                let dup_check = self.validator.gen_duplicate_node_check(
                    &quote!(#error_node_name_literal),
                    &quote!(__extracted_nodes),
                );
                quote! {
                    #dup_check
                    let child_ctx = __extracted_nodes.pop().unwrap();
                    match #parse_call {
                        Ok(v) => #ident = Some(v),
                        Err(e) => #helpers::merge_child_errors(&mut __errors, Err(e)),
                    }
                }
            };

            processing.push(quote! {
                {
                    let __lookup_names: &[&str] = #names_expr;
                    let mut __extracted_nodes = Vec::new();
                    for name in __lookup_names {
                        if let Some(mut nodes) = __children_map.remove(*name) {
                            __extracted_nodes.append(&mut nodes);
                        }
                    }

                    if __extracted_nodes.is_empty() {
                        #missing_error_logic
                    } else {
                        #parse_logic
                    }
                }
            });
        }

        if !ignore_unknown {
            finals.push(quote! {
                for (name, nodes) in __children_map {
                    if !nodes.is_empty() {
                        let first = &nodes[0];
                        let msg = format!("Unknown child node '{}'", name);
                        #helpers::push_custom(&mut __errors, msg, None, first.current_span(), first.source());
                    }
                }
            });
        }

        quote! {
            #(#decls)*
            #(#processing)*
            #(#finals)*
        }
    }
}
