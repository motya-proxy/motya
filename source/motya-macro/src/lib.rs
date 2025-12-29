use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, ItemFn, LitStr, parse_macro_input, parse_quote};

use crate::node_parser::codegen::{parser::ParserGenerator, schema::SchemaGenerator};

mod node_parser;

/// Derive macro for the `KdlParsable` trait.
///
/// It performs strict structural validation, including checking argument counts, property keys,
/// and child node multiplicity, while accumulating all errors into a `ConfigError`.
///
/// # Supported Attributes (`#[node(...)]`)
///
/// ### Struct Level:
/// - `name = "..."`: Overrides the expected KDL node name (defaults to `kebab-case` of the struct name).
/// - `allow_empty`: Allows the node's children block to be empty even if children are defined.
///
/// ### Enum Support (Polymorphic Nodes):
/// Enums allow parsing a child node that can be one of several types. Two modes are supported:
///
/// **1. Dispatch Polymorphism (Heterogeneous List)**
/// Used when the Enum represents a collection of *different* nodes (e.g., `filter` vs `rate-limit`).
/// - The **Enum** itself should **NOT** have a `name`.
/// - Each **Variant** **MUST** have a `#[node(name = "...")]` (or default to kebab-case).
/// - The parser selects the variant based on the KDL node name.
///
/// **2. Shape Polymorphism (Structural Overloading)**
/// Used when a single KDL node name (e.g., `use-chain`) can have different internal structures
/// (e.g., `use-chain "name"` vs `use-chain { ... }`).
/// - The **Enum** **MUST** have a `#[node(name = "...")]`.
/// - Variants define different valid shapes (args, props, children) for that node.
/// - **Heuristic Scoring**: The parser does NOT match by variant name. Instead, it inspects the input
///   (args count, specific props, children presence) and calculates a score for each variant.
///   The variant with the highest score is selected for parsing.
///
/// ### Field Level:
/// - `#[node(arg)]`: Maps a field to a positional argument. Required unless the field is an `Option`.
/// - `#[node(prop)]`: Maps a field to a property (`key=value`). Supports `name = "..."` override.
/// - `#[node(child)]`: Maps to a specific sub-node. Validates keyword and multiplicity.
///   - Supports primitives (e.g., `algorithm: String` parses `algorithm "sha256"`).
/// - `#[node(dynamic_child)]`: Maps a block of varied children into a collection (e.g., `Vec<T>`).
///   - Supports `min = N` and `max = N` to restrict the number of children.
/// - `#[node(node_name)]`: Captures the actual KDL tag/identifier as the field's value.
/// - `#[node(default)]`: Uses `Default::default()` (or specific value) if the field is missing.
/// - `#[node(proxy = "Type")]`: Specifies that this field should be parsed using `Type`'s schema.
///   Useful when `Type` is a "Schema Definition" (struct with `#[motya_node]`) and the field
///   is a "Domain Model". The system will parse `Type` and then call `Convert::convert`.
///
/// ### Validation & Constraints:
/// - `min = N`: Enforces minimum value for numbers or minimum count for `dynamic_child`.
/// - `max = N`: Enforces maximum value for numbers or maximum count for `dynamic_child`.
/// - `validate_with = "path"`: calls a custom validation function `fn(&T, &State) -> miette::Result<()>`.
/// - **NonZero Types**: Fields like `NonZeroUsize` are automatically validated to be non-zero during parsing.
///
/// ### Common Options:
/// - `parse_with = "path"`: Specifies a custom parsing function.
///
/// # The Proxy/Convert Pattern
/// To separate the KDL Schema from your Domain Business Logic, use the `#[motya_node]` attribute
/// on your definition struct.
///
/// 1. Define a **Schema Struct** (`ServerDef`) mirroring the KDL structure.
/// 2. Implement `crate::kdl::traits::Convert<S>` for `ServerDef`.
///    - `type Output = Server;` (Your Domain Model).
///    - `fn convert(self, state: &S) -> miette::Result<Server>`.
/// 3. The macro generates a `parse_as<S, T>` method that parses the KDL and automatically
///    runs your conversion logic, binding errors to the source code spans.
///
/// # Validation Logic
/// 1. Node name matching (or Structural Heuristic for Enums).
/// 2. Unknown property detection (strict schema).
/// 3. Argument/Property type checking (including `NonZero` checks).
/// 4. Value constraints (`min`, `max`, custom validators).
/// 5. Required field/child presence.
#[proc_macro_derive(NodeDefinition, attributes(node))]
pub fn derive_node_definition(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    match node_parser::parse::parse(input) {
        Ok(model) => {
            let schema_gen = SchemaGenerator::new(&model);
            schema_gen.generate().into()
        }
        Err(e) => e.to_compile_error().into(),
    }
}

/// Derive macro for the `NodeDefinition` trait.
///
/// This macro generates static metadata about a KDL node. It distinguishes between
/// "Commands" (nodes with a fixed keyword) and "Value Wrappers".
///
/// # Metadata Generated
/// - `KEYWORD`: The identifier used to trigger this node.
/// - `SCHEMA_NAME`: The human-readable label (e.g., "SocketAddr").
/// - `ALLOWED_NAMES`: List of all valid node names (useful for Enum hints).
/// - `DOCS`, `PROPS`, `ARGS`, `BLOCK`: Detailed structural schema.
///
/// # Auto-derived Traits
/// - `KdlSchemaType`: Allows other nodes to reference this schema.
///
/// # Supported Attributes (`#[node(...)]`)
/// - `name = "..."`: Sets the fixed KDL keyword.
/// - `node_name`: Captures the node's tag (sets `KEYWORD` to `None`).
/// - `schema_name = "..."`: Overrides the name used in documentation.
/// - `proxy = "Type"`: Uses the metadata of `Type` instead of the field's actual type (useful for Proxy Pattern).
/// - `#[node(arg)]`, `#[node(prop)]`, `#[node(child)]`: Standard schema definitions.
///
/// # Documentation Localization
/// Parses `///` comments. Use `@lang:` to categorize text.
#[proc_macro_derive(Parser, attributes(node))]
pub fn derive_parser(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    match node_parser::parse::parse(input) {
        Ok(model) => {
            let parser_gen = ParserGenerator::new(&model);
            parser_gen.generate().into()
        }
        Err(e) => e.to_compile_error().into(),
    }
}

/// Transforms the struct or enum into a **Spanned Wrapper** and generates a **Proxy Parser** via the `Convert` trait.
///
/// This macro separates the parsing model (KDL schema) from the business logic model (Domain type),
/// while preserving source location information for precise error reporting.
///
/// # Core Features
///
/// ## 1. Spanned Wrapper & Error Reporting
/// The macro renames the original type `T` to `TData` and re-declares `T` as a wrapper struct holding `Spanned<TData>`.
/// This architecture allows access to the source span of every field *after* parsing is complete.
///
/// **Generated API:**
/// - **`Deref` to `TData`**: Transparent access to the parsed fields.
/// - **Specific Error Helpers**: Generates methods like `err_{field_name}(msg)` for every field.
///   - For a field `port`: `def.err_port("Invalid port")` creates an error pointing exactly to that property in the KDL file.
///   - Handles both named properties (via `err_prop`) and positional arguments (via `err_arg`).
///
/// ## 2. Proxy Pattern (Schema $\to$ Domain)
/// Instead of validating inside the parser, you implement the `Convert` trait to transform the
/// "Schema Struct" into a "Domain Struct". The macro generates the glue code to run this conversion automatically.
///
/// **Requirement:**
/// You must implement `crate::kdl::parser::convert::Convert<S>` for the struct/enum.
///
/// **Generated Method:**
/// - `parse_as<S, T>(&ctx, &state) -> Result<T, ConfigError>`:
///   1. Parses the raw KDL into `Self` (the Schema Type).
///   2. Calls `Convert::convert(self, state)`.
///   3. Automatically maps any `miette::Report` returned by `convert` into the parser's `ConfigError`.
#[proc_macro_attribute]
pub fn motya_node(_: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);

    let original_ident = input.ident.clone();
    let vis = input.vis.clone();

    let data_ident = format_ident!("{}Data", original_ident);
    let context_ident = format_ident!("{}ErrCtx", original_ident);

    let mut data_item = input.clone();
    data_item.ident = data_ident.clone();
    clean_err_attributes(&mut data_item);

    data_item
        .attrs
        .push(parse_quote!(#[allow(non_camel_case_types, non_snake_case)]));
    data_item
        .attrs
        .push(parse_quote!(#[doc = "Inner data container holding the parsed values."]));

    let wrapper_struct = quote! {
        #[derive(Clone)]
        #vis struct #original_ident(pub crate::kdl::parser::spanned::Spanned<#data_ident>);

        impl std::fmt::Debug for #original_ident {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_tuple(stringify!(#original_ident))
                    .field(&self.0)
                    .finish()
            }
        }
    };

    let context_struct = quote! {
        #[derive(Clone)]
        #vis struct #context_ident {
            pub ctx: crate::kdl::parser::ctx::ParseContext,
        }
    };

    let error_helpers = gen_context_error_helpers(&input, &context_ident);

    let common_impls = quote! {
        impl std::ops::Deref for #original_ident {
            type Target = #data_ident;

            fn deref(&self) -> &Self::Target {
                &self.0.data
            }
        }

        impl #original_ident {
            pub fn span(&self) -> &crate::kdl::parser::ctx::ParseContext {
                &self.0.ctx
            }

            pub fn into_inner(self) -> #data_ident {
                self.0.data
            }


            pub fn into_parts(self) -> (#data_ident, #context_ident) {
                (
                    self.0.data,
                    #context_ident { ctx: self.0.ctx }
                )
            }
        }

        impl From<crate::kdl::parser::spanned::Spanned<#data_ident>> for #original_ident {
            fn from(spanned: crate::kdl::parser::spanned::Spanned<#data_ident>) -> Self {
                Self(spanned)
            }
        }
    };

    let kdl_impls = quote! {
        impl<S> crate::kdl::parser::parsable::KdlParsable<S> for #original_ident {
            fn parse_node(
                ctx: &crate::kdl::parser::ctx::ParseContext,
                state: &S
            ) -> Result<Self, crate::common_types::error::ConfigError> {
                let data = <#data_ident as crate::kdl::parser::parsable::KdlParsable<S>>::parse_node(ctx, state)?;
                Ok(#original_ident(crate::kdl::parser::spanned::Spanned::new(data, ctx.clone())))
            }
        }

        impl crate::kdl::parser::node_schema::NodeSchema for #original_ident {
            fn applicable_node_names() -> &'static [&'static str] {
                #data_ident::applicable_node_names()
            }

            fn match_score(ctx: &crate::kdl::parser::ctx::ParseContext) -> (isize, Option<String>) {
                #data_ident::match_score(ctx)
            }
        }
        // impl crate::kdl::schema::definitions::NodeDefinition for #original_ident {
        //     const KEYWORD: Option<&'static str> = #data_ident::KEYWORD;
        //     const SCHEMA_NAME: &'static str = #data_ident::SCHEMA_NAME;
        //     const IS_VALUE_WRAPPER: bool = #data_ident::IS_VALUE_WRAPPER;
        //     const DOCS: &'static [crate::kdl::schema::definitions::DocEntry] = #data_ident::DOCS;
        //     const PROPS: &'static [crate::kdl::schema::definitions::PropDef] = #data_ident::PROPS;
        //     const ARGS: &'static [crate::kdl::schema::definitions::ArgDef] = #data_ident::ARGS;
        //     const BLOCK: crate::kdl::schema::definitions::BlockContent = #data_ident::BLOCK;
        //     const ALLOWED_NAMES: &'static [&'static str] = #data_ident::ALLOWED_NAMES;
        // }
    };

    let proxy_method = quote! {
       impl #original_ident {
        pub fn parse_as<S, T>(
            ctx: &crate::kdl::parser::ctx::ParseContext,
            state: &S
        ) -> Result<T, crate::common_types::error::ConfigError>
        where
            Self: crate::kdl::parser::parsable::KdlParsable<S> + crate::kdl::parser::convert::Convert<S, Output = T>,
        {
            let schema = <Self as crate::kdl::parser::parsable::KdlParsable<S>>::parse_node(ctx, state)?;

            match crate::kdl::parser::convert::Convert::convert(schema, state) {
                Ok(val) => Ok(val),

                Err(e) => {
                    let report: miette::Report = e.into();

                    let parse_err = crate::kdl::parser::utils::macros_helpers::to_parse_error(
                        report,
                        ctx.current_span(),
                        ctx.source().clone()
                    );
                    Err(crate::common_types::error::ConfigError::from_list(vec![parse_err]))
                }
            }
        }
       }
    };

    let expanded = quote! {
        #data_item
        #wrapper_struct
        #context_struct
        #common_impls
        #kdl_impls
        #error_helpers
        #proxy_method
    };

    expanded.into()
}

fn gen_context_error_helpers(
    input: &DeriveInput,
    context_ident: &syn::Ident,
) -> proc_macro2::TokenStream {
    let mut methods = Vec::new();

    let mut create_method = |variant_prefix: Option<String>, field: &syn::Field, index: usize| {
        let meta = parse_err_meta(field);

        let method_suffix = if let Some(n) = &meta.method_suffix {
            n.clone()
        } else if let Some(ident) = &field.ident {
            ident.to_string()
        } else {
            format!("arg_{}", index)
        };

        let method_suffix_clean = method_suffix.replace('-', "_").to_lowercase();
        let method_name = if let Some(prefix) = &variant_prefix {
            format_ident!("err_{}_{}", prefix, method_suffix_clean)
        } else {
            format_ident!("err_{}", method_suffix_clean)
        };

        let body = if let Some(arg_idx) = meta.target_arg {
            quote! {
                let span = self.ctx.arg_span(#arg_idx).unwrap_or_else(|| self.ctx.current_span());
                self.ctx.error_with_span(msg, span)
            }
        } else if let Some(prop_key) = meta.target_prop {
            quote! {
                let span = self.ctx.prop_span(#prop_key).unwrap_or_else(|| self.ctx.current_span());
                self.ctx.error_with_span(msg, span)
            }
        } else if field.ident.is_none() {
            quote! {
                let span = self.ctx.arg_span(#index).unwrap_or_else(|| self.ctx.current_span());
                self.ctx.error_with_span(msg, span)
            }
        } else {
            let key = field.ident.as_ref().unwrap().to_string();
            let kdl_key = key.replace('_', "-");
            quote! {
                let span = self.ctx.prop_span(#kdl_key).unwrap_or_else(|| self.ctx.current_span());
                self.ctx.error_with_span(msg, span)
            }
        };

        methods.push(quote! {
            #[doc = concat!("Returns an error associated with `", #method_suffix, "`.")]
            pub fn #method_name(&self, msg: impl Into<String>) -> miette::Error {
                #body
            }
        });
    };

    match &input.data {
        syn::Data::Struct(s) => {
            for (i, field) in s.fields.iter().enumerate() {
                create_method(None, field, i);
            }
        }
        syn::Data::Enum(e) => {
            for variant in &e.variants {
                let variant_prefix = variant.ident.to_string().to_lowercase();
                for (i, field) in variant.fields.iter().enumerate() {
                    create_method(Some(variant_prefix.clone()), field, i);
                }
            }
        }
        _ => {}
    }

    quote! {
        impl #context_ident {

            pub fn err_self(&self, msg: impl Into<String>) -> miette::Error {
                self.ctx.error(msg)
            }

            #(#methods)*
        }
    }
}

#[derive(Default)]
struct ErrMeta {
    method_suffix: Option<String>,
    target_prop: Option<String>,
    target_arg: Option<usize>,
}

fn parse_err_meta(field: &syn::Field) -> ErrMeta {
    let mut meta = ErrMeta::default();
    for attr in &field.attrs {
        if attr.path().is_ident("err") {
            let _ = attr.parse_nested_meta(|m| {
                if m.path.is_ident("name") {
                    let s: syn::LitStr = m.value()?.parse()?;
                    meta.method_suffix = Some(s.value());
                } else if m.path.is_ident("prop") {
                    let s: syn::LitStr = m.value()?.parse()?;
                    meta.target_prop = Some(s.value());
                } else if m.path.is_ident("arg") {
                    let i: syn::LitInt = m.value()?.parse()?;
                    meta.target_arg = Some(i.base10_parse()?);
                }
                Ok(())
            });
        }
    }
    meta
}

fn clean_err_attributes(input: &mut DeriveInput) {
    let clean_fields = |fields: &mut syn::Fields| {
        for field in fields {
            field.attrs.retain(|attr| !attr.path().is_ident("err"));
        }
    };
    match &mut input.data {
        syn::Data::Struct(s) => clean_fields(&mut s.fields),
        syn::Data::Enum(e) => {
            for v in &mut e.variants {
                clean_fields(&mut v.fields);
            }
        }
        _ => {}
    }
}

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
