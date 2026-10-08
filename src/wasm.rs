use crate::{fill_pdf, form_fields_json, get_piece_info, report_json, validate_pdf};
use base64::Engine;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn get_form_fields_result(template: &[u8]) -> Result<String, JsValue> {
    form_fields_json(template).map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen]
pub fn fill_pdf_bytes(
    template: &[u8],
    json: &str,
    piece_info: Option<String>,
) -> Result<Vec<u8>, JsValue> {
    fill_pdf(template, json, piece_info.as_deref())
        .map(|x| x.0)
        .map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen]
pub fn fill_pdf_base64(
    template: &[u8],
    json: &str,
    piece_info: Option<String>,
) -> Result<String, JsValue> {
    let (bytes, _) =
        fill_pdf(template, json, piece_info.as_deref()).map_err(|e| JsValue::from_str(&e))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[wasm_bindgen]
pub fn fill_pdf_result(
    template: &[u8],
    json: &str,
    piece_info: Option<String>,
) -> Result<String, JsValue> {
    let (_, report) =
        fill_pdf(template, json, piece_info.as_deref()).map_err(|e| JsValue::from_str(&e))?;
    Ok(report_json(&report))
}

#[wasm_bindgen]
pub fn get_piece_info_result(template: &[u8]) -> Result<Option<String>, JsValue> {
    get_piece_info(template)
        .map(|opt| opt.map(|v| v.to_string()))
        .map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen]
pub fn validate_pdf_result(template: &[u8], json: &str) -> Result<String, JsValue> {
    validate_pdf(template, json).map_err(|e| JsValue::from_str(&e))
}
