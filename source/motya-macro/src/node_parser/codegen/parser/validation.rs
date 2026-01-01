use proc_macro2::TokenStream;
use quote::quote;

use super::Namespaces;
use crate::node_parser::model::{ArgSpec, BlockSpec, ParseOptions, PropSpec};

pub struct ValidationGenerator<'a> {
    namespaces: &'a Namespaces,
}

impl<'a> ValidationGenerator<'a> {
    pub fn new(namespaces: &'a Namespaces) -> Self {
        Self { namespaces }
    }

    pub fn gen_global_rules(
        &self,
        props: &[PropSpec],
        args: &[ArgSpec],
        block: &BlockSpec,
        has_all_props: bool,
        has_all_args: bool,
        allow_empty_block: bool,
        is_root: bool,
    ) -> TokenStream {
        let helpers = &self.namespaces.helpers;
        let mut rules = Vec::new();

        match block {
            BlockSpec::Empty => rules.push(quote!(crate::kdl::parser::ensures::Rule::NoChildren)),
            _ if !allow_empty_block => {
                rules.push(quote!(crate::kdl::parser::ensures::Rule::ReqChildren))
            }
            _ => {}
        }

        if !is_root {
            if has_all_args {
                let min = args
                    .iter()
                    .filter(|a| !a.base.is_option && a.base.opts.default.is_none())
                    .count();
                if min > 0 {
                    rules.push(quote!(crate::kdl::parser::ensures::Rule::AtLeastArgs(#min)));
                }
            } else {
                let arg_count = args.len();
                if arg_count == 0 {
                    rules.push(quote!(crate::kdl::parser::ensures::Rule::NoPositionalArgs));
                } else {
                    let has_optional = args.iter().any(|a| a.base.is_option);
                    if !has_optional {
                        rules
                            .push(quote!(crate::kdl::parser::ensures::Rule::ExactArgs(#arg_count)));
                    } else {
                        let min = args.iter().filter(|a| !a.base.is_option).count();
                        rules.push(quote!(crate::kdl::parser::ensures::Rule::AtLeastArgs(#min)));
                    }
                }
            }

            if !has_all_props {
                let prop_schema = props.iter().map(|p| {
                    let k = &p.key;
                    let pt = &p.primitive_kind;
                    quote!((#k, crate::kdl::parser::utils::PrimitiveType::#pt))
                });
                rules.push(
                    quote!(crate::kdl::parser::ensures::Rule::OnlyKeysTyped(&[ #(#prop_schema),* ])),
                );
            }
        }

        if rules.is_empty() {
            return quote!();
        }

        quote! {
            if let Err(e) = ctx.validate(&[ #(#rules),* ]) {
                #helpers::push_report(&mut __errors, e, ctx.current_span(), ctx.source().clone());
            }
        }
    }

    pub fn gen_value_check(&self, opts: &ParseOptions, is_vec: bool) -> TokenStream {
        let mut checks = Vec::new();

        if !is_vec {
            if let Some(min) = opts.min {
                checks.push(quote! {
                    if val < #min as _ {
                        return Err(ctx.error(format!("Value must be at least {}", #min)));
                    }
                });
            }
            if let Some(max) = opts.max {
                checks.push(quote! {
                    if val > #max as _ {
                        return Err(ctx.error(format!("Value must be at most {}", #max)));
                    }
                });
            }
            if let Some(validator) = &opts.validate_with {
                checks.push(quote! {
                    if let Err(e) = #validator(&val, state) {
                        return Err(ctx.error(e.to_string()));
                    }
                });
            }
        }

        quote!( #(#checks)* )
    }

    pub fn gen_vec_bounds(
        &self,
        ident: &syn::Ident,
        opts: &ParseOptions,
        context_name: &str,
    ) -> TokenStream {
        let helpers = &self.namespaces.helpers;
        let mut checks = Vec::new();

        if let Some(min) = opts.min {
            checks.push(quote! {
                if #ident.len() < #min {
                    let msg = format!("{} must contain at least {} item(s), found {}", #context_name, #min, #ident.len());
                    #helpers::push_custom(&mut __errors, msg, None, ctx.current_span(), ctx.source().clone());
                }
            });
        }
        if let Some(max) = opts.max {
            checks.push(quote! {
                if #ident.len() > #max {
                    let msg = format!("{} cannot contain more than {} item(s), found {}", #context_name, #max, #ident.len());
                    #helpers::push_custom(&mut __errors, msg, None, ctx.current_span(), ctx.source().clone());
                }
            });
        }

        quote!( #(#checks)* )
    }

    pub fn gen_duplicate_node_check(
        &self,
        node_name_expr: &TokenStream,
        nodes_var: &TokenStream,
    ) -> TokenStream {
        let helpers = &self.namespaces.helpers;
        quote! {
            if #nodes_var.len() > 1 {
                 let msg = format!("Node '{}' cannot be repeated", #node_name_expr);

                 #helpers::push_custom(&mut __errors, msg, None, #nodes_var[1].current_span(), #nodes_var[1].source());
            }
        }
    }
}
