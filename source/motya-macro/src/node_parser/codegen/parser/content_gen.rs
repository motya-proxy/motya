use crate::node_parser::codegen::parser::Namespaces;
use crate::node_parser::codegen::parser::child_gen::ChildGenerator;
use crate::node_parser::codegen::parser::constructor_gen::ConstructorGenerator;
use crate::node_parser::codegen::parser::field_gen::FieldGenerator;
use crate::node_parser::codegen::parser::types::ParseTarget;
use crate::node_parser::codegen::parser::validation::ValidationGenerator;
use crate::node_parser::model::NodeModel;
use proc_macro2::TokenStream;
use quote::quote;

pub struct ContentGenerator<'a> {
    namespaces: &'a Namespaces,
    validator: ValidationGenerator<'a>,
    model: &'a NodeModel,
}

impl<'a> ContentGenerator<'a> {
    pub fn new(namespaces: &'a Namespaces, model: &'a NodeModel) -> Self {
        Self {
            namespaces,
            validator: ValidationGenerator::new(namespaces),
            model,
        }
    }

    pub fn gen_body(&self, target: &ParseTarget) -> TokenStream {
        let error_mod = &self.namespaces.error_mod;

        let field_gen = FieldGenerator::new(self.namespaces, &self.validator);
        let child_gen = ChildGenerator::new(self.namespaces, &self.validator);
        let ctor_gen = ConstructorGenerator::new();

        let static_checks = self.validator.gen_global_rules(
            target.props,
            target.args,
            target.block,
            target.all_props.is_some(),
            target.all_args.is_some(),
            self.model.allow_empty_block,
            self.model.is_root,
        );

        let parse_args = field_gen.gen_args(target.args);
        let parse_props = field_gen.gen_props(target.props);
        let parse_all_args = field_gen.gen_all_args(target.all_args, target.args.len());
        let parse_all_props = field_gen.gen_all_props(target.all_props, target.props);

        let parse_children = child_gen.generate(target.block, self.model.ignore_unknown);

        let constructor = ctor_gen.generate(target);

        quote! {

            #static_checks


            #parse_args
            #parse_props
            #parse_all_args
            #parse_all_props


            #parse_children


            if !__errors.is_empty() {
                return Err(#error_mod::ConfigError::from_list(__errors));
            }


            Ok(#constructor)
        }
    }
}
