use super::Namespaces;
use super::validation::ValidationGenerator;
use crate::node_parser::codegen::utils::gen_value_parser;
use crate::node_parser::model::{ArgSpec, BaseField, PropSpec};
use proc_macro2::TokenStream;
use quote::quote;

pub struct FieldGenerator<'a> {
    namespaces: &'a Namespaces,
    validator: &'a ValidationGenerator<'a>,
}

impl<'a> FieldGenerator<'a> {
    pub fn new(namespaces: &'a Namespaces, validator: &'a ValidationGenerator<'a>) -> Self {
        Self {
            namespaces,
            validator,
        }
    }

    pub fn gen_args(&self, args: &[ArgSpec]) -> TokenStream {
        let mut streams = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let fetch = quote!(ctx.arg(#i).map(Some));
            let desc = format!("argument #{}", i);
            streams.push(self.gen_binding(&arg.base, fetch, desc, arg.required));
        }
        quote!( #(#streams)* )
    }

    pub fn gen_props(&self, props: &[PropSpec]) -> TokenStream {
        let mut streams = Vec::new();
        for prop in props {
            let key = &prop.key;
            let fetch = quote!(ctx.opt_prop(#key));
            let desc = format!("property '{}'", key);
            streams.push(self.gen_binding(&prop.base, fetch, desc, prop.required));
        }
        quote!( #(#streams)* )
    }

    pub fn gen_all_args(&self, field: &Option<BaseField>, skip: usize) -> TokenStream {
        let Some(f) = field else { return quote!() };
        let ident = &f.ident;
        let helpers = &self.namespaces.helpers;

        quote! {
            let #ident = match ctx.args_typed() {
                Ok(entries) => entries.into_iter()
                    .filter(|e| e.name().is_none())
                    .skip(#skip)
                    .collect(),
                Err(e) => {
                     #helpers::push_report(&mut __errors, e, ctx.current_span(), ctx.source().clone());
                     Vec::new()
                }
            };
        }
    }

    pub fn gen_all_props(&self, field: &Option<BaseField>, props: &[PropSpec]) -> TokenStream {
        let Some(f) = field else { return quote!() };
        let ident = &f.ident;

        let known_keys: Vec<&String> = props.iter().map(|p| &p.key).collect();

        let loop_body = if known_keys.is_empty() {
            quote! {
                if let Some(k_str) = val.name() {
                    #ident.insert(k_str.to_string(), val);
                }
            }
        } else {
            quote! {
                if let Some(k_str) = val.name() {
                    let known = &[ #(#known_keys),* ];
                    if !known.contains(&k_str.as_str()) {
                        #ident.insert(k_str.to_string(), val);
                    }
                }
            }
        };

        quote! {
            let mut #ident = std::collections::BTreeMap::new();
            if let Ok(entries) = ctx.props_typed() {
                for val in entries {
                    #loop_body
                }
            }
        }
    }

    fn gen_binding(
        &self,
        base: &BaseField,
        fetch_expr: TokenStream,
        desc: String,
        is_required: bool,
    ) -> TokenStream {
        let helpers = &self.namespaces.helpers;

        let ident = &base.ident;
        let ty = &base.inner_type;
        let opts = &base.opts;

        let parser = gen_value_parser(ty, opts);

        let validator = self.validator.gen_value_check(opts, false);

        let has_default = opts.default.is_some();
        let def_expr = if let Some(d) = &opts.default {
            quote!(Some(#d))
        } else {
            quote!(None)
        };

        let (ty_annot, unwrap) = if has_default {
            (quote!(#ty), quote!(.expect("Guaranteed by default value")))
        } else {
            (quote!(Option<#ty>), quote!())
        };

        quote! {
            let #ident: #ty_annot = #helpers::parse_input_value(
                #fetch_expr,
                |v| {
                    let val = #parser;
                    #validator
                    Ok(val)
                },
                #def_expr,
                #is_required,
                #desc,
                ctx,
                &mut __errors
            )#unwrap;
        }
    }
}
