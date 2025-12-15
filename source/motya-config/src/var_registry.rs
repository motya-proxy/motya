use std::collections::HashMap;
use std::env;

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
