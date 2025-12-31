use std::sync::Arc;
use dashmap::DashMap;
use ropey::Rope;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range, Url};
use motya_config::common_types::error::{ConfigError, ParseError};

pub struct DiagnosticConverter {
    documents: Arc<DashMap<Url, Rope>>,
}

impl DiagnosticConverter {
    pub fn new(documents: Arc<DashMap<Url, Rope>>) -> Self {
        Self { documents }
    }

    pub fn errors_to_diagnostics(&self, config_error: ConfigError, uri: &Url) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for err in config_error.errors {
            if let Some(diag) = self.parse_error_to_diagnostic(&err, uri) {
                diagnostics.push(diag);
            }
        }

        diagnostics
    }

    fn parse_error_to_diagnostic(&self, err: &ParseError, current_uri: &Url) -> Option<Diagnostic> {
        
        let source_name = err.src.name();
        
        let current_path_lossy = current_uri.to_file_path().ok()?.to_string_lossy().to_string();
        
        if !source_name.is_empty() && source_name != current_path_lossy {
            return None;
        }
        let msg = if let Some(help) = &err.help {
            help
        }
        else {
            &err.message
        };
        
        let Some(span) = err.label else {
             return Some(Diagnostic {
                message: msg.clone(),
                range: Range::default(),
                severity: Some(DiagnosticSeverity::ERROR),
                ..Default::default()
             });
        };

        let rope = self.documents.get(current_uri)?;
        
        let start_byte = span.offset();
        let end_byte = start_byte + span.len();

        if end_byte > rope.len_bytes() {
            return None;
        }

        let start_char = rope.try_byte_to_char(start_byte).ok()?;
        let end_char = rope.try_byte_to_char(end_byte).ok()?;

        let start_line = rope.try_char_to_line(start_char).ok()?;
        let start_col = start_char - rope.try_line_to_char(start_line).ok()?;

        let end_line = rope.try_char_to_line(end_char).ok()?;
        let end_col = end_char - rope.try_line_to_char(end_line).ok()?;

        Some(Diagnostic {
            range: Range {
                start: Position::new(start_line as u32, start_col as u32),
                end: Position::new(end_line as u32, end_col as u32),
            },
            severity: Some(DiagnosticSeverity::ERROR),
            message: msg.clone(),
            source: Some("motya-lsp".to_string()),
            ..Default::default()
        })
    }
}