use proc_macro2::TokenStream;
use quote::quote;

use super::types::ParseTarget;
use crate::node_parser::model::{BaseField, BlockSpec};

pub struct ConstructorGenerator;

impl ConstructorGenerator {
    pub fn new() -> Self {
        Self
    }

    pub fn generate(&self, target: &ParseTarget) -> TokenStream {
        struct FieldRef {
            ident: syn::Ident,
            needs_unwrap: bool,
            sort_key: String,
        }

        let mut all_fields = Vec::new();

        if let Some(nn) = target.node_name {
            all_fields.push(FieldRef {
                ident: nn.base.ident.clone(),
                needs_unwrap: false,
                sort_key: nn.base.ident.to_string(),
            });
        }

        let mut add = |base: &BaseField, req: bool| {
            let ident = base.ident.clone();

            let needs_unwrap = req && base.opts.default.is_none();
            let sort_key = ident.to_string();
            all_fields.push(FieldRef {
                ident,
                needs_unwrap,
                sort_key,
            });
        };

        for a in target.args {
            add(&a.base, a.required);
        }
        for p in target.props {
            add(&p.base, p.required);
        }
        if let Some(ap) = target.all_props {
            add(ap, false);
        }
        if let Some(aa) = target.all_args {
            add(aa, false);
        }

        match target.block {
            BlockSpec::Dynamic { field_ident, .. } => {
                all_fields.push(FieldRef {
                    ident: field_ident.clone(),
                    needs_unwrap: false,
                    sort_key: field_ident.to_string(),
                });
            }
            BlockSpec::Strict(children) => {
                for c in children {
                    let needs_unwrap =
                        !c.is_vec && !c.base.is_option && c.base.opts.default.is_none();
                    add(&c.base, needs_unwrap);
                }
            }
            _ => {}
        }

        let path = &target.ctor_path;

        if target.is_tuple {
            all_fields.sort_by(|a, b| {
                let get_idx = |s: &str| -> u32 {
                    if let Some(rest) = s.strip_prefix("_tup_") {
                        rest.parse().unwrap_or(u32::MAX)
                    } else {
                        u32::MAX
                    }
                };
                get_idx(&a.sort_key).cmp(&get_idx(&b.sort_key))
            });

            let exprs = all_fields.iter().map(|f| {
                let id = &f.ident;
                if f.needs_unwrap {
                    quote!(#id.expect("Validation passed"))
                } else {
                    quote!(#id)
                }
            });
            quote! { #path( #(#exprs),* ) }
        } else {
            let fields = all_fields.iter().map(|f| {
                let id = &f.ident;
                let expr = if f.needs_unwrap {
                    quote!(#id.expect("Validation passed"))
                } else {
                    quote!(#id)
                };
                quote!(#id: #expr)
            });
            quote! { #path { #(#fields),* } }
        }
    }
}
