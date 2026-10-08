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

#[wasm_bindgen]
pub fn verify_and_unlock_piece_info_result(
    template: &[u8],
    app_name: &str,
    secret_key: &str,
) -> Result<String, JsValue> {
    let res = crate::verify_and_unlock_piece_info(template, app_name, secret_key)
        .map_err(|e| JsValue::from_str(&e))?;
    let status_str = match res.status {
        crate::PieceInfoUnlockStatus::Valid => "valid",
        crate::PieceInfoUnlockStatus::Tampered(ref msg) => {
            return Ok(serde_json::json!({
                "status": "tampered",
                "reason": msg,
                "app_name": res.app_name
            })
            .to_string());
        }
        crate::PieceInfoUnlockStatus::DocumentMismatch => "document_mismatch",
        crate::PieceInfoUnlockStatus::WrongKey => "wrong_key",
        crate::PieceInfoUnlockStatus::NotFound => "not_found",
    };
    Ok(serde_json::json!({
        "status": status_str,
        "data": res.data,
        "app_name": res.app_name
    })
    .to_string())
}

#[wasm_bindgen]
pub fn lock_pdf_piece_info(
    template: &[u8],
    app_name: &str,
    data_json: &str,
    secret_key: &str,
) -> Result<Vec<u8>, JsValue> {
    let mut doc = lopdf::Document::load_mem(template)
        .map_err(|e| JsValue::from_str(&format!("PDF load failed: {e}")))?;
    let val: serde_json::Value = serde_json::from_str(data_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid data JSON: {e}")))?;
    crate::insert_locked_piece_info(&mut doc, app_name, &val, secret_key)
        .map_err(|e| JsValue::from_str(&e))?;
    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| JsValue::from_str(&format!("PDF save failed: {e}")))?;
    Ok(out)
}

