use serde::Serialize;


#[derive(Serialize, Debug)]
pub struct DiagnosticError {
    pub file_path: String,
    pub message: String,
    pub severity: String,
    pub start_offset: usize,
    pub end_offset: usize,
}

pub type Snapshot = std::collections::HashMap<String, String>;