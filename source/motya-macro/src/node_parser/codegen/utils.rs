use crate::node_parser::model::ParseOptions;
use proc_macro2::TokenStream;
use quote::quote;
use syn::Type;

pub fn gen_value_parser(ty: &Type, opts: &ParseOptions) -> TokenStream {
    if let Some(func) = &opts.parse_with {
        return quote!( #func(v, state)? );
    }

    let type_str = quote!(#ty).to_string().replace(' ', "");

    if type_str.contains("NonZero") {
        match type_str.as_str() {
            "NonZeroUsize" | "std::num::NonZeroUsize" => {
                return quote! {
                    {
                        let raw = v.as_usize()?;
                        std::num::NonZeroUsize::new(raw).ok_or_else(|| ctx.error("Value cannot be zero"))?
                    }
                };
            }
            "NonZeroU32" | "std::num::NonZeroU32" => {
                return quote! {
                    {
                        let raw = v.as_i64()? as u32;
                        std::num::NonZeroU32::new(raw).ok_or_else(|| ctx.error("Value cannot be zero"))?
                    }
                };
            }
            _ => return quote!(v.parse_as()?),
        }
    }

    match type_str.as_str() {
        "String" => quote!(v.as_str()?.to_string()),
        "usize" => quote!(v.as_usize()?),
        "i64" => quote!(v.as_i64()?),
        "bool" => quote!(v.as_bool()?),
        _ => quote!(v.parse_as()?),
    }
}
