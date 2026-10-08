use base64::Engine;
use lopdf::xref::XrefType;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat, dictionary};
use serde_json::Value;
use std::collections::BTreeMap;

#[cfg(target_arch = "wasm32")]
mod wasm;

mod appearance;
mod appearance_renderer;
mod field_strategy;
mod sign;

pub use appearance::PdfAppearance;
pub use sign::{CertificateSigner, CmsSignatureMode, EcdsaSigner, PdfSigner, SignError, Signer};

#[derive(Debug, Clone)]
pub enum FieldStatus {
    Filled,
    Missing,
    Invalid,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone)]
pub struct FieldResult {
    pub field: String,
    pub status: FieldStatus,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FillReport {
    pub fields: Vec<FieldResult>,
}

impl FillReport {
    pub fn filled_count(&self) -> usize {
        self.fields
            .iter()
            .filter(|x| matches!(x.status, FieldStatus::Filled))
            .count()
    }

    pub fn count(&self, status: FieldStatus) -> usize {
        self.fields
            .iter()
            .filter(|x| std::mem::discriminant(&x.status) == std::mem::discriminant(&status))
            .count()
    }
}

fn pdf_text(value: &str) -> Object {
    let mut bytes = vec![0xfe, 0xff];
    for unit in value.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(bytes, StringFormat::Hexadecimal)
}

fn object_text(value: &Object) -> Option<String> {
    match value {
        Object::String(bytes, _) => {
            if bytes.starts_with(&[0xfe, 0xff]) {
                let units: Vec<u16> = bytes[2..]
                    .chunks_exact(2)
                    .map(|x| u16::from_be_bytes([x[0], x[1]]))
                    .collect();
                String::from_utf16(&units).ok()
            } else {
                Some(String::from_utf8_lossy(bytes).into_owned())
            }
        }
        _ => None,
    }
}

fn field_name(dict: &Dictionary) -> Option<String> {
    dict.get(b"T").ok().and_then(object_text)
}

fn field_type(dict: &Dictionary) -> Option<Vec<u8>> {
    dict.get(b"FT")
        .ok()
        .and_then(|x| x.as_name().ok())
        .map(|x| x.to_vec())
}

fn inherited_field_type(doc: &Document, id: ObjectId, dict: &Dictionary) -> Option<Vec<u8>> {
    if let Some(ft) = field_type(dict) {
        return Some(ft);
    }
    let mut current = dict.get(b"Parent").ok().and_then(|x| x.as_reference().ok());
    while let Some(parent_id) = current {
        let Ok(parent) = doc.get_object(parent_id).and_then(|x| x.as_dict()) else {
            break;
        };
        if let Some(ft) = field_type(parent) {
            return Some(ft);
        }
        current = parent
            .get(b"Parent")
            .ok()
            .and_then(|x| x.as_reference().ok());
    }
    let _ = id;
    None
}

fn field_flags(dict: &Dictionary) -> i64 {
    dict.get(b"Ff")
        .ok()
        .and_then(|x| x.as_i64().ok())
        .unwrap_or(0)
}

fn is_read_only(dict: &Dictionary) -> bool {
    field_flags(dict) & 1 != 0
}

fn rect(dict: &Dictionary) -> Result<(f64, f64), String> {
    let values = dict
        .get(b"Rect")
        .map_err(|_| "Field has no Rect".to_string())?
        .as_array()
        .map_err(|_| "Field Rect is invalid".to_string())?;
    if values.len() != 4 {
        return Err("Field Rect must have four values".into());
    }
    let n = |i: usize| -> Result<f64, String> {
        match &values[i] {
            Object::Integer(v) => Ok(*v as f64),
            Object::Real(v) => Ok(*v as f64),
            _ => Err("Field Rect contains a non-numeric value".into()),
        }
    };
    Ok(((n(2)? - n(0)?).abs(), (n(3)? - n(1)?).abs()))
}

fn jpeg_size(bytes: &[u8]) -> Result<(f64, f64), String> {
    if bytes.len() < 4 || bytes[0] != 0xff || bytes[1] != 0xd8 {
        return Err("Invalid JPEG image".into());
    }
    let mut p = 2usize;
    while p + 9 < bytes.len() {
        if bytes[p] != 0xff {
            p += 1;
            continue;
        }
        let marker = bytes[p + 1];
        p += 2;
        if marker == 0xd8 || marker == 0xd9 {
            continue;
        }
        if p + 2 > bytes.len() {
            break;
        }
        let len = u16::from_be_bytes([bytes[p], bytes[p + 1]]) as usize;
        if len < 2 || p + len > bytes.len() {
            break;
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) && len >= 7 {
            let h = u16::from_be_bytes([bytes[p + 3], bytes[p + 4]]) as f64;
            let w = u16::from_be_bytes([bytes[p + 5], bytes[p + 6]]) as f64;
            return Ok((w, h));
        }
        p += len;
    }
    Err("Unable to determine JPEG dimensions".into())
}

fn parse_image(value: &str) -> Result<Vec<u8>, String> {
    let encoded = value
        .strip_prefix("data:image/jpeg;base64,")
        .unwrap_or(value);
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| format!("Invalid image Base64: {e}"))
}

fn create_image(doc: &mut Document, jpeg: Vec<u8>, width: f64, height: f64) -> ObjectId {
    doc.add_object(Stream::new(dictionary! {
        "Type" => "XObject", "Subtype" => "Image", "Width" => width as i64, "Height" => height as i64,
        "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8, "Filter" => "DCTDecode",
    }, jpeg))
}

fn widget_ids(doc: &Document, field_id: ObjectId, field: &Dictionary) -> Vec<ObjectId> {
    if let Ok(kids) = field.get(b"Kids").and_then(|x| x.as_array()) {
        let result: Vec<ObjectId> = kids
            .iter()
            .filter_map(|x| x.as_reference().ok())
            .filter(|id| doc.objects.contains_key(id))
            .collect();
        if !result.is_empty() {
            return result;
        }
    }
    if doc.objects.contains_key(&field_id) {
        vec![field_id]
    } else {
        Vec::new()
    }
}

fn button_state_names(doc: &Document, widget_id: ObjectId) -> Vec<Vec<u8>> {
    let Ok(dict) = doc.get_object(widget_id).and_then(|x| x.as_dict()) else {
        return Vec::new();
    };
    let Ok(ap) = dict.get(b"AP").and_then(|x| x.as_dict()) else {
        return Vec::new();
    };
    let Ok(n) = ap.get(b"N").and_then(|x| x.as_dict()) else {
        return Vec::new();
    };
    n.iter()
        .filter_map(|(k, _)| {
            if k.as_slice() != b"Off" {
                Some(k.clone())
            } else {
                None
            }
        })
        .collect()
}

fn has_button_states(doc: &Document, field_id: ObjectId, field: &Dictionary) -> bool {
    widget_ids(doc, field_id, field)
        .iter()
        .any(|id| !button_state_names(doc, *id).is_empty())
}

fn all_button_states(doc: &Document, field_id: ObjectId, field: &Dictionary) -> Vec<Vec<u8>> {
    widget_ids(doc, field_id, field)
        .into_iter()
        .flat_map(|id| button_state_names(doc, id))
        .collect()
}

fn set_button(
    doc: &mut Document,
    field_id: ObjectId,
    field: &Dictionary,
    value: &Value,
) -> Result<(), String> {
    let widgets = widget_ids(doc, field_id, field);
    if value.is_boolean() {
        let checked = value.as_bool().unwrap_or(false);
        let state = if checked {
            button_state_names(doc, *widgets.first().ok_or("Button has no widget")?)
                .into_iter()
                .next()
                .ok_or("Button has no on-state")?
        } else {
            b"Off".to_vec()
        };
        let field = doc
            .get_object_mut(field_id)
            .map_err(|e| e.to_string())?
            .as_dict_mut()
            .map_err(|e| e.to_string())?;
        field.set("V", Object::Name(state.clone()));
        for id in widgets {
            if let Ok(widget) = doc.get_object_mut(id).and_then(|x| x.as_dict_mut()) {
                widget.set("AS", Object::Name(state.clone()));
            }
        }
        return Ok(());
    }
    let selected = value
        .as_str()
        .ok_or("Button value must be boolean or option name")?;
    for id in &widgets {
        let states = button_state_names(doc, *id);
        if let Some(state) = states
            .into_iter()
            .find(|x| String::from_utf8_lossy(x) == selected)
        {
            let field = doc
                .get_object_mut(field_id)
                .map_err(|e| e.to_string())?
                .as_dict_mut()
                .map_err(|e| e.to_string())?;
            field.set("V", Object::Name(state.clone()));
            for wid in &widgets {
                if let Ok(widget) = doc.get_object_mut(*wid).and_then(|x| x.as_dict_mut()) {
                    widget.set("AS", Object::Name(state.clone()));
                }
            }
            return Ok(());
        }
    }
    Err(format!("Button option '{selected}' not found"))
}

fn choice_options(dict: &Dictionary) -> Vec<Vec<String>> {
    let Ok(opt) = dict.get(b"Opt").and_then(|x| x.as_array()) else {
        return Vec::new();
    };
    opt.iter()
        .filter_map(|x| match x {
            Object::String(_, _) => object_text(x).map(|v| vec![v]),
            Object::Array(pair) if pair.len() >= 2 => {
                let export = object_text(&pair[0])?;
                let display = object_text(&pair[1])?;
                Some(vec![export, display])
            }
            _ => None,
        })
        .collect()
}

fn set_choice(
    doc: &mut Document,
    field_id: ObjectId,
    field: &Dictionary,
    value: &Value,
) -> Result<(), String> {
    let options = choice_options(field);
    let values: Vec<String> = if let Some(s) = value.as_str() {
        vec![s.to_string()]
    } else if let Some(a) = value.as_array() {
        a.iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect()
    } else {
        return Err("Choice value must be a string or array of strings".into());
    };
    if values.is_empty() {
        return Err("Choice value cannot be empty".into());
    }
    let mut exports = Vec::new();
    let mut indexes = Vec::new();
    for value in &values {
        if options.is_empty() {
            exports.push(value.clone());
            continue;
        }
        let Some((index, pair)) = options
            .iter()
            .enumerate()
            .find(|(_, pair)| pair.iter().any(|x| x == value))
        else {
            return Err(format!(
                "Choice value '{value}' is not present in field options"
            ));
        };
        indexes.push(index);
        exports.push(pair[0].clone());
    }
    let field = doc
        .get_object_mut(field_id)
        .map_err(|e| e.to_string())?
        .as_dict_mut()
        .map_err(|e| e.to_string())?;
    if exports.len() == 1 {
        field.set("V", pdf_text(&exports[0]));
    } else {
        field.set(
            "V",
            Object::Array(exports.iter().map(|v| pdf_text(v)).collect()),
        );
    }
    if !indexes.is_empty() {
        field.set(
            "I",
            Object::Array(
                indexes
                    .into_iter()
                    .map(|i| Object::Integer(i as i64))
                    .collect(),
            ),
        );
    }
    Ok(())
}

fn set_text(doc: &mut Document, field_id: ObjectId, value: &Value) -> Result<(), String> {
    let text = value.as_str().ok_or("Text field value must be a string")?;
    let field = doc
        .get_object_mut(field_id)
        .map_err(|e| e.to_string())?
        .as_dict_mut()
        .map_err(|e| e.to_string())?;
    field.set("V", pdf_text(text));
    Ok(())
}

fn set_image(
    doc: &mut Document,
    field_id: ObjectId,
    field: &Dictionary,
    value: &Value,
) -> Result<(), String> {
    let encoded = value
        .as_str()
        .ok_or("Image field value must be a Base64 string")?;
    let image = parse_image(encoded)?;
    let (iw, ih) = jpeg_size(&image)?;
    let widgets = widget_ids(doc, field_id, field);
    if widgets.is_empty() {
        return Err("Image field has no widget".into());
    }

    let mut rendered = 0usize;
    for widget_id in widgets {
        let (bw, bh) = {
            let widget = doc
                .get_object(widget_id)
                .map_err(|e| format!("Widget access failed: {e}"))?
                .as_dict()
                .map_err(|e| format!("Widget is not a dictionary: {e}"))?;
            rect(widget)?
        };

        let image_id = create_image(doc, image.clone(), iw, ih);
        let renderer = appearance_renderer::ImageAppearanceRenderer;
        let appearance_id = appearance_renderer::AppearanceRenderer::render_image(
            &renderer, doc, image_id, bw, bh, iw, ih,
        );

        let widget = doc
            .get_object_mut(widget_id)
            .map_err(|e| format!("Widget access failed: {e}"))?
            .as_dict_mut()
            .map_err(|e| format!("Widget is not a dictionary: {e}"))?;
        appearance_renderer::replace_appearance(widget, appearance_id);
        rendered += 1;
    }

    if rendered == 0 {
        return Err("Image field has no renderable widget".into());
    }
    Ok(())
}

fn save_document(doc: &mut Document) -> Result<Vec<u8>, String> {
    for key in [
        b"Type".as_slice(),
        b"W",
        b"Index",
        b"Length",
        b"Filter",
        b"DecodeParms",
    ] {
        doc.trailer.remove(key);
    }
    doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
    let mut output = Vec::new();
    doc.save_to(&mut output)
        .map_err(|e| format!("PDF save failed: {e}"))?;
    Ok(output)
}

enum FillError {
    Invalid(String),
    Unsupported(String),
    Failed(String),
}

fn fill_one(
    doc: &mut Document,
    field_id: ObjectId,
    field: &Dictionary,
    ft: &[u8],
    value: &Value,
    registry: &field_strategy::FieldStrategyRegistry,
) -> Result<(), FillError> {
    if ft == b"Sig" {
        return Err(FillError::Unsupported("Digital signature fields must be handled by PdfSigner; use PdfAppearance for a visual signature image".into()));
    }

    let strategy = registry
        .find(ft, value, doc, field_id, field)
        .ok_or_else(|| {
            FillError::Unsupported(format!(
                "No strategy supports field type /{} and supplied value",
                String::from_utf8_lossy(ft)
            ))
        })?;

    strategy
        .validate(doc, field_id, field, value)
        .map_err(FillError::Invalid)?;

    strategy
        .fill(doc, field_id, field, value)
        .map_err(FillError::Failed)
}

fn collect_fields(doc: &Document) -> BTreeMap<String, (ObjectId, Dictionary, Vec<u8>)> {
    let mut fields = BTreeMap::new();
    for (id, object) in &doc.objects {
        let Ok(dict) = object.as_dict() else { continue };
        let Some(name) = field_name(dict) else {
            continue;
        };
        if let Some(ft) = inherited_field_type(doc, *id, dict) {
            fields.insert(name, (*id, dict.clone(), ft));
        }
    }
    fields
}

pub fn fill_pdf(template: &[u8], json: &str) -> Result<(Vec<u8>, FillReport), String> {
    let data: Value = serde_json::from_str(json).map_err(|e| format!("Invalid JSON: {e}"))?;
    let values = data.as_object().ok_or("Fill data must be a JSON object")?;
    let mut doc = Document::load_mem(template).map_err(|e| format!("PDF load failed: {e}"))?;
    let fields = collect_fields(&doc);
    let registry = field_strategy::FieldStrategyRegistry::new();
    let mut report = FillReport::default();

    let acroforms: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, o)| {
            let d = o.as_dict().ok()?;
            if d.get(b"Fields").is_ok() {
                Some(*id)
            } else {
                None
            }
        })
        .collect();
    for value in values.values() {
        let _ = value;
    }

    for (name, value) in values {
        let Some((field_id, field, ft)) = fields.get(name) else {
            report.fields.push(FieldResult {
                field: name.clone(),
                status: FieldStatus::Missing,
                reason: Some("Field not found".into()),
            });
            continue;
        };
        if is_read_only(field) {
            report.fields.push(FieldResult {
                field: name.clone(),
                status: FieldStatus::Invalid,
                reason: Some("Field is read-only".into()),
            });
            continue;
        }
        match fill_one(&mut doc, *field_id, field, ft, value, &registry) {
            Ok(()) => report.fields.push(FieldResult {
                field: name.clone(),
                status: FieldStatus::Filled,
                reason: None,
            }),
            Err(FillError::Invalid(reason)) => report.fields.push(FieldResult {
                field: name.clone(),
                status: FieldStatus::Invalid,
                reason: Some(reason),
            }),
            Err(FillError::Unsupported(reason)) => report.fields.push(FieldResult {
                field: name.clone(),
                status: FieldStatus::Unsupported,
                reason: Some(reason),
            }),
            Err(FillError::Failed(reason)) => report.fields.push(FieldResult {
                field: name.clone(),
                status: FieldStatus::Failed,
                reason: Some(reason),
            }),
        }
    }

    for id in acroforms {
        if let Ok(d) = doc.get_object_mut(id).and_then(|x| x.as_dict_mut()) {
            d.remove(b"NeedAppearances");
        }
    }

    let output = save_document(&mut doc)?;
    Ok((output, report))
}

pub fn report_json(report: &FillReport) -> String {
    let fields: Vec<Value> = report
        .fields
        .iter()
        .map(|r| {
            let mut o = serde_json::Map::new();
            o.insert("field".into(), Value::String(r.field.clone()));
            o.insert(
                "status".into(),
                Value::String(
                    match r.status {
                        FieldStatus::Filled => "filled",
                        FieldStatus::Missing => "missing",
                        FieldStatus::Invalid => "invalid",
                        FieldStatus::Unsupported => "unsupported",
                        FieldStatus::Failed => "failed",
                    }
                    .into(),
                ),
            );
            if let Some(reason) = &r.reason {
                o.insert("reason".into(), Value::String(reason.clone()));
            }
            Value::Object(o)
        })
        .collect();
    serde_json::json!({
        "success": true,
        "filled": report.count(FieldStatus::Filled),
        "missing": report.count(FieldStatus::Missing),
        "invalid": report.count(FieldStatus::Invalid),
        "unsupported": report.count(FieldStatus::Unsupported),
        "failed": report.count(FieldStatus::Failed),
        "fields": fields
    })
    .to_string()
}

pub fn validate_pdf(template: &[u8], json: &str) -> Result<String, String> {
    let data: Value = serde_json::from_str(json).map_err(|e| format!("Invalid JSON: {e}"))?;
    let values = data.as_object().ok_or("Fill data must be a JSON object")?;
    let doc = Document::load_mem(template).map_err(|e| format!("PDF load failed: {e}"))?;
    let fields = collect_fields(&doc);
    let registry = field_strategy::FieldStrategyRegistry::new();
    let mut results = Vec::new();

    for (name, value) in values {
        let Some((field_id, field, ft)) = fields.get(name) else {
            results.push(
                serde_json::json!({"field":name,"status":"invalid","reason":"Field not found"}),
            );
            continue;
        };
        if is_read_only(field) {
            results.push(
                serde_json::json!({"field":name,"status":"invalid","reason":"Field is read-only"}),
            );
            continue;
        }

        let reason = if ft == b"Sig" {
            Some("Digital signature fields must be handled by PdfSigner; use PdfAppearance for a visual signature image".to_string())
        } else {
            match registry.find(ft, value, &doc, *field_id, field) {
                Some(strategy) => strategy.validate(&doc, *field_id, field, value).err(),
                None => Some("Invalid value type".to_string()),
            }
        };

        if let Some(reason) = reason {
            results.push(serde_json::json!({"field":name,"status":"invalid","reason":reason}));
        } else {
            results.push(serde_json::json!({"field":name,"status":"valid"}));
        }
    }

    Ok(
        serde_json::json!({"valid":results.iter().all(|x| x["status"]=="valid"),"fields":results})
            .to_string(),
    )
}
