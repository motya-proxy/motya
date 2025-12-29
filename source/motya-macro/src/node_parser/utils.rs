use super::model::DocTokens;
use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{Attribute, Expr, ExprLit, Lit, Meta, Type};

pub struct TypeAnalyzer;
impl TypeAnalyzer {
    pub fn analyze(ty: &Type) -> (Type, bool, bool) {
        if let Type::Path(tp) = ty
            && let Some(seg) = tp.path.segments.last()
        {
            if seg.ident == "Option" {
                return (Self::extract(seg).unwrap_or(ty.clone()), false, true);
            } else if seg.ident == "Vec" {
                return (Self::extract(seg).unwrap_or(ty.clone()), true, false);
            }
        }
        (ty.clone(), false, false)
    }

    fn extract(seg: &syn::PathSegment) -> Option<Type> {
        if let syn::PathArguments::AngleBracketed(args) = &seg.arguments
            && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
        {
            Some(inner.clone())
        } else {
            None
        }
    }

    pub fn to_primitive(ty: &Type) -> TokenStream {
        let s = quote!(#ty).to_string().replace(' ', "");

        if s.starts_with("NonZero") || s.contains("::NonZero") {
            return quote!(Integer);
        }

        match s.as_str() {
            "String" | "str" => quote!(String),
            "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize" => {
                quote!(Integer)
            }
            "bool" => quote!(Bool),
            _ => quote!(String),
        }
    }
}

pub struct DocParser;
impl DocParser {
    pub fn parse(attrs: &[Attribute]) -> DocTokens {
        let raw_lines = attrs
            .iter()
            .filter(|attr| attr.path().is_ident("doc"))
            .filter_map(|attr| {
                if let Meta::NameValue(meta) = &attr.meta
                    && let Expr::Lit(ExprLit {
                        lit: Lit::Str(lit), ..
                    }) = &meta.value
                {
                    return Some(lit.value().trim().to_string());
                }
                None
            });

        let (mut entries, last_lang, last_text) = raw_lines.fold(
            (Vec::new(), "en".to_string(), String::new()),
            |(mut acc, curr_lang, mut curr_text), line| {
                if let Some(rest) = line.strip_prefix('@') {
                    if !curr_text.trim().is_empty() {
                        acc.push((curr_lang.clone(), curr_text.trim().to_string()));
                        curr_text = String::new();
                    }

                    let end_lang = rest
                        .find(|c: char| c.is_whitespace() || c == ':')
                        .unwrap_or(rest.len());
                    let new_lang = rest[..end_lang].to_string();
                    if !new_lang.is_empty() {
                        let mut start_text = end_lang;
                        if start_text < rest.len() && rest.as_bytes()[start_text] == b':' {
                            start_text += 1;
                        }
                        let text_part = rest[start_text..].trim();
                        if !text_part.is_empty() {
                            curr_text.push_str(text_part);
                            curr_text.push('\n');
                        }
                        return (acc, new_lang, curr_text);
                    }
                }

                if !(curr_text.is_empty() && line.is_empty()) {
                    curr_text.push_str(&line);
                    curr_text.push('\n');
                }
                (acc, curr_lang, curr_text)
            },
        );

        if !last_text.trim().is_empty() {
            entries.push((last_lang, last_text.trim().to_string()));
        }

        let items = entries.iter().map(|(l, t)| {
            quote! { crate::kdl::schema::definitions::DocEntry { lang: #l, text: #t } }
        });
        DocTokens(quote! { &[ #(#items),* ] })
    }
}

pub fn is_primitive_type(ty: &Type) -> bool {
    let type_str = quote!(#ty).to_string().replace(' ', "");

    matches!(
        type_str.as_str(),
        "String"
            | "str"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
            | "bool"
            | "char"
    ) || type_str.starts_with("NonZero")
}

impl ToTokens for DocTokens {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
    }
}
