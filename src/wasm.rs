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

#[wasm_bindgen]
pub fn list_piece_info_applications_result(template: &[u8]) -> Result<String, JsValue> {
    let apps = crate::list_piece_info_applications(template).map_err(|e| JsValue::from_str(&e))?;
    serde_json::to_string(&apps).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen]
pub fn merge_pdfs(pdf_list: js_sys::Array) -> Result<Vec<u8>, JsValue> {
    let mut docs_bytes = Vec::new();
    for i in 0..pdf_list.length() {
        let val = pdf_list.get(i);
        let uint8 = js_sys::Uint8Array::new(&val);
        docs_bytes.push(uint8.to_vec());
    }
    let slices: Vec<&[u8]> = docs_bytes.iter().map(|b| b.as_slice()).collect();
    crate::ops::merge_pdf_bytes(&slices).map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen]
pub fn merge_pdfs_with_options(
    pdf_list: js_sys::Array,
    options_json: &str,
) -> Result<Vec<u8>, JsValue> {
    let opts: crate::ops::MergeOptions = serde_json::from_str(options_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid merge options JSON: {e}")))?;
    let mut docs_bytes = Vec::new();
    for i in 0..pdf_list.length() {
        let val = pdf_list.get(i);
        let uint8 = js_sys::Uint8Array::new(&val);
        docs_bytes.push(uint8.to_vec());
    }
    let slices: Vec<&[u8]> = docs_bytes.iter().map(|b| b.as_slice()).collect();
    crate::ops::merge_pdf_bytes_with_options(&slices, &opts).map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen]
pub fn split_pdf_with_options(
    template: &[u8],
    options_json: &str,
) -> Result<js_sys::Array, JsValue> {
    let opts: crate::ops::SplitOptions = serde_json::from_str(options_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid split options JSON: {e}")))?;
    let parts = crate::ops::split_pdf_bytes_with_options(template, &opts)
        .map_err(|e| JsValue::from_str(&e))?;
    let out_arr = js_sys::Array::new();
    for (label, part_bytes) in parts {
        let obj = js_sys::Object::new();
        js_sys::Reflect::set(&obj, &"label".into(), &JsValue::from_str(&label))?;
        let uint8 = js_sys::Uint8Array::from(part_bytes.as_slice());
        js_sys::Reflect::set(&obj, &"bytes".into(), &uint8)?;
        out_arr.push(&obj);
    }
    Ok(out_arr)
}

#[wasm_bindgen]
pub fn get_image_metadata_json(image_bytes: &[u8]) -> Result<Option<String>, JsValue> {
    let meta_opt =
        crate::ops::get_image_metadata(image_bytes).map_err(|e| JsValue::from_str(&e))?;
    match meta_opt {
        Some(val) => serde_json::to_string(&val)
            .map(Some)
            .map_err(|e| JsValue::from_str(&e.to_string())),
        None => Ok(None),
    }
}

#[wasm_bindgen]
pub fn inject_image_metadata_bytes(
    image_bytes: &[u8],
    metadata_json: &str,
) -> Result<Vec<u8>, JsValue> {
    let val: serde_json::Value = serde_json::from_str(metadata_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid metadata JSON: {e}")))?;
    let format = crate::ops::ImageFormatType::detect(image_bytes)
        .ok_or_else(|| JsValue::from_str("Unsupported image format (must be PNG or JPEG)"))?;
    crate::ops::inject_image_metadata(image_bytes, &val, format).map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen]
pub fn extract_images_from_pdf_wasm(
    template: &[u8],
    options_json: &str,
) -> Result<js_sys::Array, JsValue> {
    let opts: crate::ops::ExtractImageOptions = if options_json.trim().is_empty() {
        crate::ops::ExtractImageOptions::default()
    } else {
        serde_json::from_str(options_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid extract image options JSON: {e}")))?
    };
    let images = crate::ops::extract_images_from_bytes(template, &opts)
        .map_err(|e| JsValue::from_str(&e))?;
    let out_arr = js_sys::Array::new();
    for img in images {
        let obj = js_sys::Object::new();
        js_sys::Reflect::set(&obj, &"page".into(), &JsValue::from_f64(img.page as f64))?;
        js_sys::Reflect::set(&obj, &"width".into(), &JsValue::from_f64(img.width as f64))?;
        js_sys::Reflect::set(
            &obj,
            &"height".into(),
            &JsValue::from_f64(img.height as f64),
        )?;
        js_sys::Reflect::set(&obj, &"format".into(), &JsValue::from_str(&img.format))?;
        js_sys::Reflect::set(&obj, &"fileName".into(), &JsValue::from_str(&img.file_name))?;
        let uint8 = js_sys::Uint8Array::from(img.data.as_slice());
        js_sys::Reflect::set(&obj, &"bytes".into(), &uint8)?;
        out_arr.push(&obj);
    }
    Ok(out_arr)
}

#[wasm_bindgen]
pub fn apply_watermark_wasm(template: &[u8], options_json: &str) -> Result<Vec<u8>, JsValue> {
    let opts: crate::ops::WatermarkOptions = if options_json.trim().is_empty() {
        crate::ops::WatermarkOptions::default()
    } else {
        serde_json::from_str(options_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid watermark options JSON: {e}")))?
    };
    crate::ops::apply_watermark(template, &opts).map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen]
pub fn apply_page_numbering_wasm(template: &[u8], options_json: &str) -> Result<Vec<u8>, JsValue> {
    let opts: crate::ops::NumberingOptions = if options_json.trim().is_empty() {
        crate::ops::NumberingOptions::default()
    } else {
        serde_json::from_str(options_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid numbering options JSON: {e}")))?
    };
    crate::ops::apply_page_numbering(template, &opts).map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen]
pub fn add_signature_field_wasm(template: &[u8], options_json: &str) -> Result<Vec<u8>, JsValue> {
    let opts: crate::ops::AddSignatureFieldOptions = if options_json.trim().is_empty() {
        crate::ops::AddSignatureFieldOptions::default()
    } else {
        serde_json::from_str(options_json).map_err(|e| {
            JsValue::from_str(&format!("Invalid add signature field options JSON: {e}"))
        })?
    };
    crate::ops::add_signature_field(template, &opts).map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen]
pub fn remove_signature_field_wasm(
    template: &[u8],
    options_json: &str,
) -> Result<js_sys::Object, JsValue> {
    let opts: crate::ops::RemoveSignatureFieldOptions = if options_json.trim().is_empty() {
        crate::ops::RemoveSignatureFieldOptions::default()
    } else {
        serde_json::from_str(options_json).map_err(|e| {
            JsValue::from_str(&format!("Invalid remove signature field options JSON: {e}"))
        })?
    };
    let (bytes, count) =
        crate::ops::remove_signature_field(template, &opts).map_err(|e| JsValue::from_str(&e))?;
    let obj = js_sys::Object::new();
    let uint8 = js_sys::Uint8Array::from(bytes.as_slice());
    js_sys::Reflect::set(&obj, &"bytes".into(), &uint8)?;
    js_sys::Reflect::set(
        &obj,
        &"removedCount".into(),
        &JsValue::from_f64(count as f64),
    )?;
    Ok(obj)
}

#[wasm_bindgen]
pub fn rotate_pdf_pages_wasm(template: &[u8], options_json: &str) -> Result<Vec<u8>, JsValue> {
    let opts: crate::ops::RotateOptions = if options_json.trim().is_empty() {
        crate::ops::RotateOptions::default()
    } else {
        serde_json::from_str(options_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid rotate options JSON: {e}")))?
    };
    let (bytes, _report) =
        crate::ops::rotate_pdf_pages(template, &opts).map_err(|e| JsValue::from_str(&e))?;
    Ok(bytes)
}

#[wasm_bindgen]
pub fn rotate_pdf_pages_report_wasm(
    template: &[u8],
    options_json: &str,
) -> Result<String, JsValue> {
    let opts: crate::ops::RotateOptions = if options_json.trim().is_empty() {
        crate::ops::RotateOptions::default()
    } else {
        serde_json::from_str(options_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid rotate options JSON: {e}")))?
    };
    let (_bytes, report) =
        crate::ops::rotate_pdf_pages(template, &opts).map_err(|e| JsValue::from_str(&e))?;
    serde_json::to_string(&report)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {e}")))
}

#[wasm_bindgen]
pub fn remove_pdf_pages_wasm(template: &[u8], options_json: &str) -> Result<Vec<u8>, JsValue> {
    let opts: crate::ops::RemovePagesOptions = if options_json.trim().is_empty() {
        crate::ops::RemovePagesOptions::default()
    } else {
        serde_json::from_str(options_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid remove pages options JSON: {e}")))?
    };
    let (bytes, _report) =
        crate::ops::remove_pdf_pages(template, &opts).map_err(|e| JsValue::from_str(&e))?;
    Ok(bytes)
}

#[wasm_bindgen]
pub fn remove_pdf_pages_report_wasm(
    template: &[u8],
    options_json: &str,
) -> Result<String, JsValue> {
    let opts: crate::ops::RemovePagesOptions = if options_json.trim().is_empty() {
        crate::ops::RemovePagesOptions::default()
    } else {
        serde_json::from_str(options_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid remove pages options JSON: {e}")))?
    };
    let (_bytes, report) =
        crate::ops::remove_pdf_pages(template, &opts).map_err(|e| JsValue::from_str(&e))?;
    serde_json::to_string(&report)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {e}")))
}

#[wasm_bindgen]
pub fn crop_pdf_pages_wasm(template: &[u8], options_json: &str) -> Result<Vec<u8>, JsValue> {
    let opts: crate::ops::CropOptions = if options_json.trim().is_empty() {
        crate::ops::CropOptions::default()
    } else {
        serde_json::from_str(options_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid crop options JSON: {e}")))?
    };
    let (bytes, _report) =
        crate::ops::crop_pdf_pages(template, &opts).map_err(|e| JsValue::from_str(&e))?;
    Ok(bytes)
}

#[wasm_bindgen]
pub fn crop_pdf_pages_report_wasm(template: &[u8], options_json: &str) -> Result<String, JsValue> {
    let opts: crate::ops::CropOptions = if options_json.trim().is_empty() {
        crate::ops::CropOptions::default()
    } else {
        serde_json::from_str(options_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid crop options JSON: {e}")))?
    };
    let (_bytes, report) =
        crate::ops::crop_pdf_pages(template, &opts).map_err(|e| JsValue::from_str(&e))?;
    serde_json::to_string(&report)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {e}")))
}

#[wasm_bindgen]
pub fn extract_text_wasm(template: &[u8], options_json: &str) -> Result<String, JsValue> {
    let opts: crate::ops::TextExtractionOptions = if options_json.trim().is_empty() {
        crate::ops::TextExtractionOptions::default()
    } else {
        serde_json::from_str(options_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid text extraction options JSON: {e}")))?
    };
    crate::ops::extract_text(template, &opts).map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen]
pub fn extract_text_structured_wasm(
    template: &[u8],
    options_json: &str,
) -> Result<String, JsValue> {
    let opts: crate::ops::TextExtractionOptions = if options_json.trim().is_empty() {
        crate::ops::TextExtractionOptions::default()
    } else {
        serde_json::from_str(options_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid text extraction options JSON: {e}")))?
    };
    let report =
        crate::ops::extract_text_structured(template, &opts).map_err(|e| JsValue::from_str(&e))?;
    serde_json::to_string(&report)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {e}")))
}

#[wasm_bindgen]
pub fn flatten_pdf_wasm(template: &[u8]) -> Result<Vec<u8>, JsValue> {
    let mut doc = lopdf::Document::load_mem(template)
        .map_err(|e| JsValue::from_str(&format!("PDF load failed: {e}")))?;
    crate::flatten_form_fields(&mut doc, true)
        .map_err(|e| JsValue::from_str(&e))?;
    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| JsValue::from_str(&format!("PDF save failed: {e}")))?;
    Ok(out)
}
