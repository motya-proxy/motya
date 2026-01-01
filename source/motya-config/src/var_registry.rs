use std::{collections::HashMap, env};

#[derive(Default, Clone, Debug)]
pub struct VarRegistry {
    pub(crate) vars: HashMap<String, String>,
}

impl VarRegistry {
    pub fn new() -> Self {
        let mut vars = HashMap::new();
        vars.insert("num_cpus".to_string(), num_cpus::get().to_string());
        Self { vars }
    }

    pub fn resolve(&self, key: &str, source_type: &str) -> Option<String> {
        match source_type {
            "env" => env::var(key).ok(),
            "var" => self.vars.get(key).cloned(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use kdl::KdlDocument;
    use motya_macro::Parser;

    use crate::{
        kdl::parser::{ctx::ParseContext, parsable::KdlParsable},
        var_registry::VarRegistry,
    };

    #[test]
    fn test_threads_from_sys_var() {
        let real_cpu_count = num_cpus::get();

        #[derive(Parser)]
        #[node(root)]
        struct Test {
            #[node(child)]
            pub system: SystemTest,
        }

        #[derive(Parser)]
        struct SystemTest {
            #[node(child)]
            pub tps: usize,
        }

        let input = r#"
            system {
                tps (var)"num_cpus" 
            }
        "#;

        let ctx = ParseContext::new_with_registry(
            input.parse::<KdlDocument>().unwrap().into(),
            "<test>".into(),
            VarRegistry::new().into(),
        );

        let test = Test::parse_node(&ctx, &()).unwrap();

        assert_eq!(test.system.tps, real_cpu_count);
    }
}
