use crate::node_parser::{
    model::{BlockSpec, ChildMode, NodeModel, NodeModelKind},
    utils::is_primitive_type,
};
use proc_macro2::TokenStream;
use quote::quote;

pub struct SchemaGenerator<'a> {
    model: &'a NodeModel,
}

impl<'a> SchemaGenerator<'a> {
    pub fn new(model: &'a NodeModel) -> Self {
        Self { model }
    }

    pub fn generate(&self) -> TokenStream {
        let struct_name = &self.model.struct_name;
        let kdl_name = &self.model.kdl_name;

        let is_wrapper = self.model.props.is_empty()
            && self.model.args.is_empty()
            && matches!(self.model.block, BlockSpec::Empty)
            && self.model.node_name_field.is_some();

        let explicit_schema_name = self
            .model
            .node_name_field
            .as_ref()
            .and_then(|f| f.base.opts.schema_name.clone());

        let (keyword_opt, schema_name_logic) = if let Some(name_field) = &self.model.node_name_field
        {
            let ty = &name_field.base.inner_type;

            let s_name = if let Some(name) = explicit_schema_name {
                quote!(#name)
            } else {
                quote!(<#ty as crate::kdl::schema::utils::KdlSchemaType>::SCHEMA_NAME)
            };

            (quote!(None), s_name)
        } else {
            (quote!(Some(#kdl_name)), quote!(#kdl_name))
        };

        let allowed_names_def = self.gen_allowed_names();
        let docs = &self.model.docs;
        let props = self.gen_props_def();
        let args = self.gen_args_def();
        let block_def = self.gen_block_def();

        quote! {
            impl crate::kdl::schema::utils::KdlSchemaType for #struct_name {
                const SCHEMA_NAME: &'static str = #schema_name_logic;
            }

            #[allow(clippy::all, unused_variables)]
            impl crate::kdl::schema::definitions::NodeDefinition for #struct_name {
                const KEYWORD: Option<&'static str> = #keyword_opt;
                const SCHEMA_NAME: &'static str = <Self as crate::kdl::schema::utils::KdlSchemaType>::SCHEMA_NAME;
                const IS_VALUE_WRAPPER: bool = #is_wrapper;
                const DOCS: &'static [crate::kdl::schema::definitions::DocEntry] = #docs;
                const PROPS: &'static [crate::kdl::schema::definitions::PropDef] = &[#(#props),*];
                const ARGS: &'static [crate::kdl::schema::definitions::ArgDef] = &[#(#args),*];
                const BLOCK: crate::kdl::schema::definitions::BlockContent = #block_def;
                const ALLOWED_NAMES: &'static [&'static str] = #allowed_names_def;
            }
        }
    }

    fn gen_allowed_names(&self) -> TokenStream {
        match &self.model.kind {
            NodeModelKind::Struct => {
                quote! {
                    if let Some(kw) = <Self as crate::kdl::schema::definitions::NodeDefinition>::KEYWORD {
                        &[kw]
                    } else {
                        &[]
                    }
                }
            }
            NodeModelKind::Enum(variants) => {
                let names = variants.iter().map(|v| &v.kdl_name);
                quote! { &[ #(#names),* ] }
            }
        }
    }

    fn gen_props_def(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.model.props.iter().map(|p| {
            let k = &p.key;
            let kind = &p.primitive_kind;
            let req = p.required;
            let doc = &p.base.docs;
            quote! {
                crate::kdl::schema::definitions::PropDef {
                    key: #k,
                    kind: crate::kdl::parser::utils::PrimitiveType::#kind,
                    required: #req,
                    docs: #doc
                }
            }
        })
    }

    fn gen_args_def(&self) -> impl Iterator<Item = TokenStream> + '_ {
        self.model.args.iter().map(|a| {
            let n = &a.name;
            let kind = &a.primitive_kind;
            let req = a.required;
            let doc = &a.base.docs;
            quote! {
                crate::kdl::schema::definitions::ArgDef {
                    name: #n,
                    kind: crate::kdl::parser::utils::PrimitiveType::#kind,
                    required: #req,
                    docs: #doc
                }
            }
        })
    }

    fn gen_block_def(&self) -> TokenStream {
        match &self.model.block {
            BlockSpec::Empty => quote!(crate::kdl::schema::definitions::BlockContent::Empty),
            BlockSpec::Dynamic {
                inner_type,
                docs,
                opts,
                ..
            } => {
                let proxy = &opts.proxy;
                let ty = if let Some(p) = proxy {
                    quote!(#p)
                } else {
                    quote!(#inner_type)
                };
                quote! {
                    crate::kdl::schema::definitions::BlockContent::DynamicList {
                        child_type: <#ty as crate::kdl::schema::definitions::NodeDefinition>::SCHEMA_NAME,
                        docs: #docs,
                    }
                }
            }
            BlockSpec::Strict(children) => {
                let rules = children.iter().map(|c| {
                    let ident = &c.base.ident;
                    let inner = &c.base.inner_type;
                    let opts = &c.base.opts;
                    let mult = &c.multiplicity;
                    let doc = &c.base.docs;

                    let field_name_kdl = ident.to_string().replace('_', "-");

                    let keyword_expr = if let Some(n) = &opts.schema_name {
                        quote!(Some(#n))
                    } else if c.mode == ChildMode::Field || is_primitive_type(inner) {
                        quote!(Some(#field_name_kdl))
                    } else {
                        let proxy = &opts.proxy;
                        let ty = if let Some(p) = proxy {
                            quote!(#p)
                        } else {
                            quote!(#inner)
                        };
                        quote!(<#ty as crate::kdl::schema::definitions::NodeDefinition>::KEYWORD)
                    };

                    quote! {
                        crate::kdl::schema::definitions::ChildRule {
                            keyword: #keyword_expr,
                            rule: crate::kdl::schema::definitions::Multiplicity::#mult,
                            docs: #doc,
                        }
                    }
                });
                quote!(crate::kdl::schema::definitions::BlockContent::Strict(&[ #(#rules),* ]))
            }
        }
    }
}
