use proc_macro2::TokenStream;
use quote::quote;

use crate::node_parser::model::{
    ArgSpec, BlockSpec, ChildSpec, NodeModel, NodeModelKind, PropSpec, VariantFields,
    VariantSpec,
};

pub struct SchemaGenerator<'a> {
    model: &'a NodeModel,
}

impl<'a> SchemaGenerator<'a> {
    pub fn new(model: &'a NodeModel) -> Self {
        Self { model }
    }

    pub fn generate(&self) -> TokenStream {
        let struct_name = &self.model.struct_name;

        let body = match &self.model.kind {
            NodeModelKind::Struct => self.gen_struct_schema(),
            NodeModelKind::Enum(variants) => self.gen_enum_schema(variants),
        };

        quote! {
            impl crate::kdl::schema::definitions::GetSchema for #struct_name {
                fn schemas(ctx: &mut crate::kdl::schema::schema_context::SchemaContext) -> Vec<crate::kdl::schema::definitions::NodeSchema> {
                    #body
                }
            }
        }
    }

    fn gen_struct_schema(&self) -> TokenStream {
        
        let matcher = self.gen_matcher(
            self.model.kdl_name.as_deref(),
            self.model.node_name_field.as_ref().map(|n| &n.base),
        );

        let type_id = self.model.kdl_name.as_ref()
            .cloned()
            .unwrap_or_else(|| self.model.struct_name.to_string());

        let args = self.gen_args(&self.model.args);
        let props = self.gen_props(&self.model.props);
        let children = self.gen_children_block(&self.model.block);
        let docs = &self.model.docs;

        quote! {
            {
                let id = #type_id; 
                if !ctx.enter(id) {
                    return vec![crate::kdl::schema::definitions::NodeSchema {
                        matcher: #matcher,
                        description: std::borrow::Cow::Borrowed(&[]),
                        examples: vec![],
                        args: vec![],
                        props: vec![],
                        children: crate::kdl::schema::definitions::ChildrenSchema::Recursive(id.to_string()),
                    }];
                }

                let res = vec![
                    crate::kdl::schema::definitions::NodeSchema {
                        matcher: #matcher,
                        description: std::borrow::Cow::Borrowed(#docs),
                        examples: vec![],
                        args: #args,
                        props: #props,
                        children: #children,
                    }
                ];
            
                ctx.exit();
                res
            }
        }
    }

    fn gen_enum_schema(&self, variants: &[VariantSpec]) -> TokenStream {
        let schemas = variants.iter().map(|v| {
            let docs = &v.docs;

            match &v.fields {
                VariantFields::Unit => {
                    let name = v.kdl_name.as_ref().unwrap_or(&v.ident.to_string()).clone();
                    quote! {
                        vec![crate::kdl::schema::definitions::NodeSchema {
                            matcher: crate::kdl::schema::definitions::NodeNameMatcher::Keyword(#name.to_string()),
                            description: std::borrow::Cow::Borrowed(#docs),
                            examples: vec![],
                            args: vec![],
                            props: vec![],
                            children: crate::kdl::schema::definitions::ChildrenSchema::None,
                        }]
                    }
                }
                VariantFields::Newtype(inner_ty) => {
                    let override_name = v.kdl_name.as_ref();

                    if let Some(name) = override_name {
                        // We wrap the inner schema but force the matcher to be our keyword
                        quote! {
                            <#inner_ty as crate::kdl::schema::definitions::GetSchema>::schemas(ctx)
                                .into_iter()
                                .map(|mut s| {
                                    s.matcher = crate::kdl::schema::definitions::NodeNameMatcher::Keyword(#name.to_string());
                                    if s.description.is_empty() {
                                        s.description = std::borrow::Cow::Borrowed(#docs);
                                    }
                                    s
                                })
                                .collect::<Vec<_>>()
                        }
                    } else {
                        // Inherit completely (transparent variant)
                        quote! {
                            <#inner_ty as crate::kdl::schema::definitions::GetSchema>::schemas(ctx)
                        }
                    }
                }
                VariantFields::Struct { props, args, block, node_name, .. } => {
                    let matcher = self.gen_matcher(
                        v.kdl_name.as_deref().or(Some(&v.ident.to_string())),
                        node_name.as_ref().map(|n| &n.base)
                    );
                    let args_gen = self.gen_args(args);
                    let props_gen = self.gen_props(props);
                    let children_gen = self.gen_children_block(block);

                    quote! {
                        vec![
                            crate::kdl::schema::definitions::NodeSchema {
                                matcher: #matcher,
                                description: std::borrow::Cow::Borrowed(#docs),
                                examples: vec![],
                                args: #args_gen,
                                props: #props_gen,
                                children: #children_gen,
                            }
                        ]
                    }
                }
            }
        });

        // Flatten all vectors produced by variants
        quote! {
            {
                let mut all = Vec::new();
                #(
                    let variant_schemas: Vec<crate::kdl::schema::definitions::NodeSchema> = #schemas;
                    all.extend(variant_schemas);
                )*
                all
            }
        }
    }

    fn gen_matcher(
        &self,
        kdl_name: Option<&str>,
        node_name_field: Option<&crate::node_parser::model::BaseField>,
    ) -> TokenStream {
        if let Some(field) = node_name_field {
            let label = field.ident.to_string().replace('_', "-");
            quote! {
                crate::kdl::schema::definitions::NodeNameMatcher::Variable {
                    label: #label.to_string()
                }
            }
        } else if let Some(name) = kdl_name {
            quote! {
                crate::kdl::schema::definitions::NodeNameMatcher::Keyword(#name.to_string())
            }
        } else {
            // Fallback, though usually logic shouldn't reach here for unnamed nodes
            quote! {
                crate::kdl::schema::definitions::NodeNameMatcher::Keyword("<unknown>".to_string())
            }
        }
    }

    fn gen_args(&self, args: &[ArgSpec]) -> TokenStream {
        let items = args.iter().map(|arg| {
            let name = &arg.name;
            let req = arg.required;
            let kind = self.gen_value_kind(&arg.base.inner_type, &arg.base.opts);
            let docs = &arg.base.docs;

            quote! {
                crate::kdl::schema::definitions::ArgSchema {
                    name: #name.to_string(),
                    description: std::borrow::Cow::Borrowed(#docs),
                    kind: #kind,
                    required: #req,
                    default: None,
                }
            }
        });
        quote! { vec![ #(#items),* ] }
    }

    fn gen_props(&self, props: &[PropSpec]) -> TokenStream {
        let items = props.iter().map(|prop| {
            let name = &prop.key;
            let req = prop.required;
            let kind = self.gen_value_kind(&prop.base.inner_type, &prop.base.opts);
            let docs = &prop.base.docs;

            quote! {
                crate::kdl::schema::definitions::PropSchema {
                    name: #name.to_string(),
                    description: std::borrow::Cow::Borrowed(#docs),
                    kind: #kind,
                    required: #req,
                    default: None,
                }
            }
        });
        quote! { vec![ #(#items),* ] }
    }

    fn gen_children_block(&self, block: &BlockSpec) -> TokenStream {
        match block {
            BlockSpec::Empty => quote!(crate::kdl::schema::definitions::ChildrenSchema::None),
            BlockSpec::Strict(children) => {
                let rules = children
                    .iter()
                    .map(|child| self.gen_child_schema_call(child));

                quote! {
                    crate::kdl::schema::definitions::ChildrenSchema::Fixed({
                        let mut list = Vec::new();
                        #(
                            list.extend(#rules);
                        )*
                        list
                    })
                }
            }
            BlockSpec::Dynamic { inner_type, .. } => {
                // If it's a dynamic list (Vec<T>), usually T corresponds to a list of allowed nodes.
                // In KDL schema terms, `ChildrenSchema::Fixed` allows a set of nodes to appear.
                // `ChildrenSchema::Dynamic` implies a repetition of a single schema,
                // but if T is an enum, it returns multiple schemas.
                //
                // Strategy: We delegate to T::schemas().

                quote! {
                     crate::kdl::schema::definitions::ChildrenSchema::Fixed(
                        <#inner_type as crate::kdl::schema::definitions::GetSchema>::schemas(ctx)
                     )
                }
            }
        }
    }

    fn gen_child_schema_call(&self, child: &ChildSpec) -> TokenStream {
        let ty = &child.base.inner_type;
        let ident = &child.base.ident;

        let node_name = child
            .name
            .as_ref()
            .cloned()
            .unwrap_or_else(|| ident.to_string().replace('_', "-"));

        if child.mode == crate::node_parser::model::ChildMode::Field {
            let kind = self.gen_value_kind(ty, &child.base.opts);
            let docs = &child.base.docs;

            quote! {
                vec![
                    crate::kdl::schema::definitions::NodeSchema {
                        matcher: crate::kdl::schema::definitions::NodeNameMatcher::Keyword(#node_name.to_string()),
                        description: std::borrow::Cow::Borrowed(#docs),
                        examples: vec![],
                        args: vec![
                            crate::kdl::schema::definitions::ArgSchema {
                                name: "value".to_string(),
                                description: std::borrow::Cow::Borrowed(&[]),
                                kind: #kind,
                                required: true,
                                default: None,
                            }
                        ],
                        props: vec![],
                        children: crate::kdl::schema::definitions::ChildrenSchema::None,
                    }
                ]
            }
        } else {
            let proxy = &child.base.opts.proxy;
            let target_type = if let Some(p) = proxy {
                quote!(#p)
            } else {
                quote!(#ty)
            };

            if let Some(name) = &child.name {
                quote! {
                    <#target_type as crate::kdl::schema::definitions::GetSchema>::schemas(ctx)
                        .into_iter()
                        .map(|mut s| {
                            s.matcher = crate::kdl::schema::definitions::NodeNameMatcher::Keyword(#name.to_string());
                            s
                        })
                        .collect::<Vec<_>>()
                }
            } else {
                quote! {
                    <#target_type as crate::kdl::schema::definitions::GetSchema>::schemas(ctx)
                }
            }
        }
    }

    fn gen_value_kind(
        &self,
        ty: &syn::Type,
        opts: &crate::node_parser::model::ParseOptions,
    ) -> TokenStream {
        if let Some(schema_name) = &opts.schema_name {
            return quote! {
                crate::kdl::schema::definitions::ValueKind::Catalog {
                    name: #schema_name.to_string()
                }
            };
        }

        if let Some(inner_ty) = self.extract_option(ty) {
            return self.gen_value_kind(&inner_ty, opts);
        }

        quote! {
            <#ty as crate::kdl::schema::value_info::KdlValueInfo>::value_kind()
        }
    }

    fn extract_option(&self, ty: &syn::Type) -> Option<syn::Type> {
        if let syn::Type::Path(tp) = ty
            && let Some(seg) = tp.path.segments.last()
            && seg.ident == "Option"
            && let syn::PathArguments::AngleBracketed(args) = &seg.arguments
            && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
        {
            Some(inner.clone())
        } else {
            None
        }
    }
}
