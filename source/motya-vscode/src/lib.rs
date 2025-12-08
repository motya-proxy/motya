mod dto;
mod utils;
mod adapter;

use motya_config::common_types::bad::Bad;
use motya_config::common_types::definitions_table::DefinitionsTable;
use motya_config::loader::{ConfigLoader, FileConfigLoaderProvider};
use wasm_bindgen::prelude::*;
use std::path::PathBuf;
use crate::dto::{DiagnosticError, Snapshot};
use crate::adapter::map_collector::MapCollector;
use miette::Report;
use miette::Diagnostic;

#[wasm_bindgen]
pub async fn validate_workspace(entry_point: String, snapshot_js: JsValue) -> JsValue {
    
    let snapshot: Snapshot = match serde_wasm_bindgen::from_value(snapshot_js) {
        Ok(s) => s,
        Err(e) => return return_single_error(&entry_point, &format!("Internal: Serialization error: {}", e)),
    };

    let collector = MapCollector::new(snapshot);
    let loader = ConfigLoader::new(collector);
    let mut defs = DefinitionsTable::new_with_global();

    let entry_path = PathBuf::from(&entry_point);

    match loader.load_entry_point(Some(entry_path), &mut defs).await {
        Ok(_) => JsValue::NULL, 
        Err(report) => convert_report_to_js(report, &entry_point),
    }
}

fn return_single_error(file: &str, msg: &str) -> JsValue {
    let err = DiagnosticError {
        file_path: file.to_string(),
        message: msg.to_string(),
        severity: "error".to_string(),
        start_offset: 0,
        end_offset: 0,
    };
    serde_wasm_bindgen::to_value(&vec![err]).unwrap()
}

fn convert_report_to_js(report: Report, default_entry_point: &str) -> JsValue {
    
    let error = if let Some(bad) = report.downcast_ref::<Bad>() {

        let message = bad.help().map(|h| h.to_string()).unwrap_or("No help message here.".to_string());
        
        let file_path = bad.src.name().to_string();

        let start = bad.err_span.offset();
        let end = start + bad.err_span.len();

        DiagnosticError {
            file_path,
            message,
            severity: "error".to_string(),
            start_offset: start,
            end_offset: end,
        }
    } else {
        DiagnosticError {
            file_path: default_entry_point.to_string(),
            message: report.to_string(),
            severity: "error".to_string(),
            start_offset: 0,
            end_offset: 0,
        }
    };

    serde_wasm_bindgen::to_value(&error).unwrap()
}