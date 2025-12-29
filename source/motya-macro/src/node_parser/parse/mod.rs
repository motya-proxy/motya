use darling::FromField;
use darling::{FromDeriveInput, FromVariant};
use syn::spanned::Spanned;
use syn::{DeriveInput, Result};

mod analyzer;
mod attrs;
mod registry;

use crate::node_parser::{
    model::{
        ArgSpec, BaseField, BlockSpec, NameSpec, NodeModel, NodeModelKind, PropSpec, VariantFields,
        VariantSpec,
    },
    parse::attrs::NodeVariantAttrs,
    utils::DocParser,
};
use analyzer::AnalyzedField;
use attrs::{NodeFieldAttrs, NodeStructAttrs};
use registry::FieldRegistry;

pub fn parse(input: DeriveInput) -> Result<NodeModel> {
    let struct_attrs = NodeStructAttrs::from_derive_input(&input)?;

    let struct_name = struct_attrs.ident;
    let explicit_name = struct_attrs.name.is_some();
    let kdl_name = struct_attrs.name;

    let allow_empty_block = struct_attrs.allow_empty;
    let docs = DocParser::parse(&input.attrs);

    match input.data {
        syn::Data::Struct(data) => {
            let (props, args, block, node_name, all_props, all_args) =
                parse_fields_batch(data.fields, &struct_name)?;

            Ok(NodeModel {
                struct_name,
                kdl_name,
                docs,
                props,
                args,
                block,
                node_name_field: node_name,
                explicit_name,
                allow_empty_block,
                all_props_field: all_props,
                all_args_field: all_args,
                kind: NodeModelKind::Struct,
                is_root: struct_attrs.root.unwrap_or(false),
            })
        }
        syn::Data::Enum(data) => {
            let mut variants = Vec::new();
            let mut errors = Vec::new();

            for v in data.variants {
                match parse_variant(v) {
                    Ok(spec) => variants.push(spec),
                    Err(e) => errors.push(e),
                }
            }

            if !errors.is_empty() {
                return Err(darling::Error::multiple(errors).into());
            }

            Ok(NodeModel {
                struct_name,
                kdl_name,
                docs,
                props: vec![],
                args: vec![],
                block: crate::node_parser::model::BlockSpec::Empty,
                explicit_name,
                allow_empty_block,
                node_name_field: None,
                all_props_field: None,
                all_args_field: None,
                is_root: false,
                kind: NodeModelKind::Enum(variants),
            })
        }
        syn::Data::Union(_) => Err(syn::Error::new(
            struct_name.span(),
            "Unions are not supported",
        )),
    }
}

fn parse_fields_batch(
    fields: impl IntoIterator<Item = syn::Field>,
    span_source: &syn::Ident,
) -> Result<(
    Vec<PropSpec>,
    Vec<ArgSpec>,
    BlockSpec,
    Option<NameSpec>,
    Option<BaseField>,
    Option<BaseField>,
)> {
    let mut registry = FieldRegistry::default();
    let mut errors = Vec::new();

    for field in fields {
        let attrs = match NodeFieldAttrs::from_field(&field) {
            Ok(a) => a,
            Err(e) => {
                errors.push(e);
                continue;
            }
        };

        let analyzed = match AnalyzedField::new(field, attrs) {
            Ok(a) => a,
            Err(e) => {
                errors.push(darling::Error::from(e));
                continue;
            }
        };

        registry.register(analyzed);
    }

    if !errors.is_empty() {
        return Err(darling::Error::multiple(errors).into());
    }

    Ok(registry.finalize(span_source)?)
}

fn parse_variant(variant: syn::Variant) -> std::result::Result<VariantSpec, darling::Error> {
    let attrs = NodeVariantAttrs::from_variant(&variant)?;

    let ident = attrs.ident;
    let kdl_name = attrs.name;

    let docs = DocParser::parse(&attrs.attrs);

    let fields_spec = match variant.fields {
        syn::Fields::Unit => VariantFields::Unit,

        syn::Fields::Unnamed(fields) => {
            let is_candidate = fields.unnamed.len() == 1;
            let has_kdl_attrs = if is_candidate {
                let f = &fields.unnamed[0];
                f.attrs.iter().any(|a| a.path().is_ident("node"))
            } else {
                false
            };

            if is_candidate && !has_kdl_attrs {
                VariantFields::Newtype(fields.unnamed[0].ty.clone())
            } else {
                let synthetic_fields = fields.unnamed.into_iter().enumerate().map(|(i, mut f)| {
                    f.ident = Some(syn::Ident::new(&format!("_tup_{}", i), f.span()));
                    f
                });

                let (props, args, block, node_name, all_props, all_args) =
                    parse_fields_batch(synthetic_fields, &ident).map_err(darling::Error::from)?;

                VariantFields::Struct {
                    props,
                    args,
                    block,
                    node_name,
                    all_args,
                    all_props,
                    is_tuple: true,
                }
            }
        }

        syn::Fields::Named(fields) => {
            let (props, args, block, node_name, all_props, all_args) =
                parse_fields_batch(fields.named, &ident).map_err(darling::Error::from)?;

            VariantFields::Struct {
                props,
                args,
                block,
                node_name,
                all_args,
                all_props,
                is_tuple: false,
            }
        }
    };

    Ok(VariantSpec {
        ident,
        kdl_name,
        fields: fields_spec,
        docs,
    })
}
