
pub struct RulesTable;

#[derive(Debug, Clone)]
pub enum Modificator {
    Rule(Rule)
}

#[derive(Debug, Clone)]
pub struct Rule;

impl RulesTable {
    pub fn get_rule_by_name(&self, name: &str) -> Option<Rule> { todo!() }
}