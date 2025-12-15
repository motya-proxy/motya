use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, LitStr, parse_macro_input};

#[proc_macro_attribute]
pub fn validate(args: TokenStream, input: TokenStream) -> TokenStream {
    let func = parse_macro_input!(input as ItemFn);

    let mut expected_node_name: Option<String> = None;

    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("ensure_node_name") {
            let value: LitStr = meta.value()?.parse()?;
            expected_node_name = Some(value.value());
            Ok(())
        } else {
            Err(meta.error("Unsupported argument. Use 'ensure_node_name = \"...\"'"))
        }
    });

    parse_macro_input!(args with parser);

    let expected_name = match expected_node_name {
        Some(name) => name,
        None => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "The #[validate] attribute requires the `ensure_node_name` argument.",
            )
            .to_compile_error()
            .into();
        }
    };

    let fn_vis = &func.vis;
    let fn_sig = &func.sig;
    let fn_attrs = &func.attrs;
    let fn_block = &func.block;

    let output = quote! {
        #(#fn_attrs)*
        #fn_vis #fn_sig {
            let __actual_name = ctx.name()?;

            if __actual_name != #expected_name {
                return Err(ctx.error(format!(
                    "Invalid section node: expected '{}', found '{}'.",
                    #expected_name,
                    __actual_name
                )));
            }

            #fn_block
        }
    };

    output.into()
}
