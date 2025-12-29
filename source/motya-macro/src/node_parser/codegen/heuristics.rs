use crate::node_parser::model::{
    ArgSpec, BlockSpec, NodeModel, NodeModelKind, PropSpec, VariantFields, VariantSpec,
};
use proc_macro2::TokenStream;
use quote::quote;

pub struct ScoreGenerator;

impl ScoreGenerator {
    const MATCH_REQUIRED: isize = 10;
    const MATCH_OPTIONAL: isize = 5;
    const MATCH_CHILDREN: isize = 5;
    const DISQUALIFY: isize = -1;

    pub fn gen_match_score_impl(model: &NodeModel) -> TokenStream {
        match &model.kind {
            NodeModelKind::Struct => {
                let logic = Self::gen_struct_score(
                    model.kdl_name.as_deref(),
                    &model.props,
                    &model.args,
                    &model.block,
                    model.all_args_field.is_some(),
                    model.all_props_field.is_some(),
                );
                quote! {
                    #logic
                }
            }
            NodeModelKind::Enum(variants) => {
                let variant_scores = variants.iter().map(|v| {
                    let logic = Self::gen_variant_score(v);
                    quote! { #logic }
                });

                quote! {
                    let scores = [ #(#variant_scores),* ];
                    scores.into_iter().max().unwrap_or((-1, None))
                }
            }
        }
    }

    pub fn gen_variant_score(v: &VariantSpec) -> TokenStream {
        let disqualify = Self::DISQUALIFY;

        let name_check = if let Some(name) = &v.kdl_name {
            quote! {
                if ctx.name()? != #name {
                    return Ok((#disqualify, Some(format!("Name mismatch: expected '{}', got '{}'", #name, ctx.name()?))));
                }
            }
        } else {
            quote!()
        };

        let calculation_block = match &v.fields {
            VariantFields::Newtype(ty) => {
                quote! {
                    #name_check
                    let (inner_score, reason) = <#ty as crate::kdl::parser::node_schema::NodeSchema>::match_score(ctx);
                    if inner_score < 0 {
                        Ok((#disqualify, Some(format!("Inner type '{}' did not match", reason.unwrap_or(stringify!(#ty).to_string())))))
                    } else {
                        Ok((inner_score, reason))
                    }
                }
            }

            _ => {
                let logic = match &v.fields {
                    VariantFields::Unit => {
                        quote! {
                            let args_len = ctx.args()?.len();

                            if args_len > 0 {
                                return Ok((#disqualify, Some(format!("Arity mismatch: expected 0 args, got {}", args_len))));
                            }
                            if ctx.has_children_block() {
                                return Ok((#disqualify, Some("Structure mismatch: expected no children".to_string())));
                            }
                            score += 1;
                        }
                    }
                    VariantFields::Struct {
                        props,
                        args,
                        block,
                        all_args,
                        all_props,
                        ..
                    } => {
                        let args_check = Self::gen_args_check(args, all_args.is_some());
                        let props_check = Self::gen_props_check(props, all_props.is_some());
                        let block_check = Self::gen_block_check(block);
                        quote! {
                            #args_check
                            #props_check
                            #block_check
                        }
                    }
                    _ => unreachable!(),
                };

                quote! {
                    let mut score: isize = 0;
                    #name_check
                    #logic
                    Ok((score, None))
                }
            }
        };

        quote! {
            {
                let calc = || -> std::result::Result<(isize, Option<String>), miette::Report> {
                    #calculation_block
                };

                match calc() {
                    Ok(s) => s,
                    Err(e) => {
                        let reason = if let Some(help) = e.help() {
                            help.to_string()
                        } else {
                            e.to_string()
                        };
                        (#disqualify, Some(reason))
                    }
                }
            }
        }
    }

    fn gen_struct_score(
        kdl_name: Option<&str>,
        props: &[PropSpec],
        args: &[ArgSpec],
        block: &BlockSpec,
        has_all_args: bool,
        has_all_props: bool,
    ) -> TokenStream {
        let disqualify = Self::DISQUALIFY;

        let name_check = if let Some(name) = kdl_name {
            quote! {
                if ctx.name()? != #name {
                    return Ok((#disqualify, Some(format!("Name mismatch: expected '{}', got '{}'", #name, ctx.name()?))));
                }
                score += 10;
            }
        } else {
            quote!()
        };

        let args_check = Self::gen_args_check(args, has_all_args);
        let props_check = Self::gen_props_check(props, has_all_props);
        let block_check = Self::gen_block_check(block);

        quote! {
            {
                let calc = || -> std::result::Result<(isize, Option<String>), miette::Report> {
                    let mut score: isize = 0;
                    #name_check
                    #args_check
                    #props_check
                    #block_check
                    Ok((score, None))
                };
                calc().unwrap_or((#disqualify, Some("Unknown error during score calc".to_string())))
            }
        }
    }

    fn gen_args_check(args: &[ArgSpec], has_all_args: bool) -> TokenStream {
        let disqualify = Self::DISQUALIFY;
        let match_req = Self::MATCH_REQUIRED;
        let match_opt = Self::MATCH_OPTIONAL;

        let min = args
            .iter()
            .filter(|a| !a.base.is_option && a.base.opts.default.is_none())
            .count();
        let max = args.len();

        let req_checks = args
            .iter()
            .enumerate()
            .filter(|(_, a)| a.required)
            .map(|(i, _)| {
                quote! {
                    if ctx.args_typed()?.get(#i).is_some() {
                        score += #match_req;
                    }
                }
            });

        let max_check = if has_all_args {
            quote! {
                if arg_len >= #max { score += #match_opt; }
            }
        } else {
            quote! {
                if arg_len > #max {
                    return Ok((#disqualify, Some(format!("Arity mismatch: expected at most {} args, got {}", #max, arg_len))));
                }
                if arg_len == #max { score += #match_opt; }
            }
        };

        quote! {
            let arg_len = ctx.args_typed()?.len();

            if arg_len < #min {
                return Ok((#disqualify, Some(format!("Arity mismatch: expected at least {} args, got {}", #min, arg_len))));
            }

            #max_check
            #(#req_checks)*
        }
    }

    fn gen_props_check(props: &[PropSpec], has_all_props: bool) -> TokenStream {
        let disqualify = Self::DISQUALIFY;
        let match_req = Self::MATCH_REQUIRED;
        let match_opt = Self::MATCH_OPTIONAL;

        let allowed_keys: Vec<_> = props.iter().map(|p| &p.key).collect();

        let unknown_keys_check = if has_all_props {
            quote! {
                if !props_list.is_empty() { score += 1; }
            }
        } else {
            quote! {
                let allowed = [ #(#allowed_keys),* ];
                for p in &props_list {
                    if let Some(n) = p.name() {
                        if !allowed.contains(&n) {
                            return Ok((#disqualify, Some(format!("Unknown property '{}'", n))));
                        }
                    }
                }
            }
        };

        let checks = props.iter().map(|p| {
            let key = &p.key;
            let find_expr = quote! {
                props_list.iter().any(|p| p.name() == Some(#key))
            };

            if p.required {
                quote! {
                    if #find_expr {
                        score += #match_req;
                    } else {
                        return Ok((#disqualify, Some(format!("Missing required property '{}'", #key)))); 
                    }
                }
            } else {
                quote! {
                    if #find_expr {
                        score += #match_opt;
                    }
                }
            }
        });

        quote! {
            let props_list = ctx.props_typed()?;

            #unknown_keys_check
            #(#checks)*
        }
    }

    fn gen_block_check(block: &BlockSpec) -> TokenStream {
        let disqualify = Self::DISQUALIFY;
        let match_children = Self::MATCH_CHILDREN;

        match block {
            BlockSpec::Empty => quote! {
                if ctx.has_children_block() {
                    return Ok((#disqualify, Some("Children mismatch: expected no children".to_string())));
                }
            },
            BlockSpec::Dynamic { opts, .. } => {
                let min = opts.min.unwrap_or(0);
                quote! {
                    let count = if ctx.has_children_block() {
                        ctx.nodes()?.len()
                    } else {
                        0
                    };

                    if count > 0 {
                        score += #match_children;
                        if count < #min {
                            return Ok((#disqualify, Some(format!("Children mismatch: expected at least {} children, got {}", #min, count))));
                        }
                    } else if #min > 0 {
                        return Ok((#disqualify, Some(format!("Children mismatch: expected at least {} children, got 0", #min))));
                    }
                }
            }
            BlockSpec::Strict(_) => quote! {
                if ctx.has_children_block() {
                    let count = ctx.nodes()?.len();
                    if count > 0 {
                        score += #match_children;
                    }
                }
                else {
                    return Ok((#disqualify, Some("Expected children block".to_string())));
                }
            },
        }
    }
}
