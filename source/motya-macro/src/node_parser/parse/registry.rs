use super::analyzer::AnalyzedField;
use crate::node_parser::{model::*, utils::is_primitive_type};
use darling::Error as DarlingError;

#[derive(Default)]
pub struct FieldRegistry {
    props: Vec<PropSpec>,
    args: Vec<ArgSpec>,
    strict_children: Vec<ChildSpec>,
    dynamic_child: Option<(syn::Ident, syn::Type, ParseOptions, DocTokens)>,

    node_name: Option<NameSpec>,
    all_props: Option<BaseField>,
    all_args: Option<BaseField>,

    errors: Vec<DarlingError>,
}

impl FieldRegistry {
    pub fn register(&mut self, field: AnalyzedField) {
        let roles = [
            ("prop", field.attrs.prop),
            ("arg", field.attrs.arg),
            ("child", field.attrs.child),
            ("dynamic_child", field.attrs.dynamic_child),
            ("node_name", field.attrs.node_name),
            ("all_props", field.attrs.all_props),
            ("all_args", field.attrs.all_args),
        ];

        let active_roles: Vec<&str> = roles
            .iter()
            .filter(|(_, active)| *active)
            .map(|(name, _)| *name)
            .collect();

        if active_roles.len() > 1 {
            self.errors.push(
                DarlingError::custom(format!(
                    "Field cannot be both {}",
                    active_roles.join(" and ")
                ))
                .with_span(field.ident()),
            );
            return;
        }

        if active_roles.is_empty() {
            return;
        }

        if field.attrs.prop {
            self.add_prop(field);
        } else if field.attrs.arg {
            self.add_arg(field);
        } else if field.attrs.child {
            self.add_child(field);
        } else if field.attrs.dynamic_child {
            self.add_dynamic_child(field);
        } else if field.attrs.node_name {
            self.add_node_name(field);
        } else if field.attrs.all_props {
            self.add_all_props(field);
        } else if field.attrs.all_args {
            self.add_all_args(field);
        }
    }

    fn base_field(f: &AnalyzedField) -> BaseField {
        BaseField {
            ident: f.ident().clone(),
            inner_type: f.type_info.inner.clone(),
            is_option: f.type_info.is_option,
            opts: f.parse_opts.clone(),
            docs: f.docs.clone(),
        }
    }

    fn add_prop(&mut self, f: AnalyzedField) {
        let key = f
            .attrs
            .name
            .clone()
            .unwrap_or_else(|| f.ident().to_string().replace('_', "-"));

        let required = !f.type_info.is_option && f.parse_opts.default.is_none();

        self.props.push(PropSpec {
            base: Self::base_field(&f),
            key,
            primitive_kind: f.type_info.primitive_kind,
            required,
        });
    }

    fn add_arg(&mut self, f: AnalyzedField) {
        let required = !f.type_info.is_option && f.parse_opts.default.is_none();
        self.args.push(ArgSpec {
            base: Self::base_field(&f),
            name: f.ident().to_string(),
            primitive_kind: f.type_info.primitive_kind,
            required,
        });
    }

    fn add_child(&mut self, f: AnalyzedField) {
        let is_vec = f.type_info.is_vec;
        let is_opt = f.type_info.is_option;

        let multiplicity = if is_vec {
            quote::quote!(Repeated)
        } else if is_opt {
            quote::quote!(Optional)
        } else {
            quote::quote!(Required)
        };

        let mode = if f.attrs.flat || is_primitive_type(&f.type_info.inner) {
            ChildMode::Field
        } else {
            ChildMode::Node
        };

        self.strict_children.push(ChildSpec {
            base: Self::base_field(&f),
            multiplicity,
            is_vec,
            group: f.attrs.group.clone(),
            mode,
            name: f.attrs.name,
        });
    }

    fn add_dynamic_child(&mut self, f: AnalyzedField) {
        if self.dynamic_child.is_some() {
            self.errors.push(
                DarlingError::custom("Duplicate #[node(dynamic_child)]").with_span(f.ident()),
            );
            return;
        }
        self.dynamic_child = Some((
            f.ident().clone(),
            f.type_info.inner.clone(),
            f.parse_opts,
            f.docs,
        ));
    }

    fn add_node_name(&mut self, f: AnalyzedField) {
        if self.node_name.is_some() {
            self.errors
                .push(DarlingError::custom("Duplicate #[node(node_name)]").with_span(f.ident()));
            return;
        }
        self.node_name = Some(NameSpec {
            base: Self::base_field(&f),
        });
    }

    fn add_all_props(&mut self, f: AnalyzedField) {
        if self.all_props.is_some() {
            self.errors
                .push(DarlingError::custom("Duplicate #[node(all_props)]").with_span(f.ident()));
            return;
        }

        self.all_props = Some(Self::base_field(&f));
    }

    fn add_all_args(&mut self, f: AnalyzedField) {
        if self.all_args.is_some() {
            self.errors
                .push(DarlingError::custom("Duplicate #[node(all_args)]").with_span(f.ident()));
            return;
        }

        self.all_args = Some(Self::base_field(&f));
    }

    pub fn finalize(
        self,
        struct_span: &syn::Ident,
    ) -> Result<
        (
            Vec<PropSpec>,
            Vec<ArgSpec>,
            BlockSpec,
            Option<NameSpec>,
            Option<BaseField>,
            Option<BaseField>,
        ),
        DarlingError,
    > {
        if !self.errors.is_empty() {
            return Err(DarlingError::multiple(self.errors));
        }

        let block = match (self.dynamic_child, self.strict_children.is_empty()) {
            (Some((ident, ty, opts, docs)), true) => BlockSpec::Dynamic {
                field_ident: ident,
                inner_type: ty,
                opts,
                docs,
            },
            (Some(_), false) => {
                return Err(DarlingError::custom(
                    "Cannot mix #[node(child)] and #[node(dynamic_child)]",
                )
                .with_span(struct_span));
            }
            (None, false) => BlockSpec::Strict(self.strict_children),
            (None, true) => BlockSpec::Empty,
        };

        Ok((
            self.props,
            self.args,
            block,
            self.node_name,
            self.all_props,
            self.all_args,
        ))
    }
}
