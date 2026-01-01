#[derive(Default)]
pub struct SchemaContext {
    stack: Vec<&'static str>,
}

impl SchemaContext {

    pub fn enter(&mut self, name: &'static str) -> bool {
        if self.stack.contains(&name) {
            return false;
        }
        self.stack.push(name);
        true
    }

    pub fn exit(&mut self) {
        self.stack.pop();
    }
}