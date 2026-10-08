use crate::{fill_pdf, report_json, validate_pdf};
use base64::Engine;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn fill_pdf_bytes(template: &[u8], json: &str) -> Result<Vec<u8>, JsValue> {
    fill_pdf(template, json)
        .map(|x| x.0)
        .map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen]
pub fn fill_pdf_base64(template: &[u8], json: &str) -> Result<String, JsValue> {
    let (bytes, _) = fill_pdf(template, json).map_err(|e| JsValue::from_str(&e))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[wasm_bindgen]
pub fn fill_pdf_result(template: &[u8], json: &str) -> Result<String, JsValue> {
    let (_, report) = fill_pdf(template, json).map_err(|e| JsValue::from_str(&e))?;
    Ok(report_json(&report))
}

#[wasm_bindgen]
pub fn validate_pdf_result(template: &[u8], json: &str) -> Result<String, JsValue> {
    validate_pdf(template, json).map_err(|e| JsValue::from_str(&e))
}
