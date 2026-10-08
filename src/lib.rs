use base64::Engine;
use lopdf::xref::XrefType;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat, dictionary};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

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

fn inherited_field_name(doc: &Document, dict: &Dictionary) -> Option<String> {
    let mut names = Vec::new();
    let mut current = Some(dict);
    while let Some(current_dict) = current {
        if let Some(name) = field_name(current_dict) {
            names.push(name);
        }
        current = current_dict
            .get(b"Parent")
            .ok()
            .and_then(|x| x.as_reference().ok())
            .and_then(|id| doc.get_object(id).ok())
            .and_then(|x| x.as_dict().ok());
    }
    if names.is_empty() {
        None
    } else {
        names.reverse();
        Some(names.join("."))
    }
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

    if let Ok(field_mut) = doc.get_object_mut(field_id).and_then(|x| x.as_dict_mut()) {
        field_mut.set("V", pdf_text(encoded));
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FormFieldType {
    Text,
    Date,
    Image,
    Checkbox,
    Radio,
    ListBox,
    ComboBox,
    Signature,
    Button,
    Barcode,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldLocation {
    pub page: usize,
    pub rect: [f64; 4],
    pub visible: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormFieldOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureInfo {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub contact_info: Option<String>,
    #[serde(default)]
    pub signing_time: Option<String>,
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub sub_filter: Option<String>,
    #[serde(default)]
    pub byte_range: Option<Vec<i64>>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub signer_name: Option<String>,
    #[serde(default)]
    pub signer_organization: Option<String>,
    #[serde(default)]
    pub issuer: Option<String>,
    #[serde(default)]
    pub not_before: Option<String>,
    #[serde(default)]
    pub not_after: Option<String>,
    #[serde(default)]
    pub serial_number: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    pub id: String,
    pub name: String,
    pub field_type: FormFieldType,
    pub page: Option<usize>,
    pub rect: Option<[f64; 4]>,
    pub value: Option<Value>,
    pub default_value: Option<Value>,
    pub required: bool,
    pub read_only: bool,
    pub visible: bool,
    pub enabled: bool,
    pub tooltip: Option<String>,
    pub options: Vec<FormFieldOption>,
    pub flags: u32,
    pub locations: Vec<FieldLocation>,
    pub signed: Option<bool>,
    #[serde(default, alias = "format")]
    pub date_format: Option<String>,
    #[serde(default)]
    pub signature: Option<SignatureInfo>,
}

fn field_type_name(doc: &Document, field_id: ObjectId, field: &Dictionary, ft: &[u8]) -> FormFieldType {
    match ft {
        b"Sig" => FormFieldType::Signature,
        b"Btn" => {
            let flags = field_flags(field) as u32;
            if field_has_image_appearance(doc, field_id, field) {
                FormFieldType::Image
            } else if flags & (1 << 16) != 0 {
                FormFieldType::Button
            } else if flags & (1 << 15) != 0 {
                FormFieldType::Radio
            } else {
                FormFieldType::Checkbox
            }
        }
        b"Ch" => {
            let flags = field_flags(field) as u32;
            if flags & (1 << 17) != 0 {
                FormFieldType::ComboBox
            } else {
                FormFieldType::ListBox
            }
        }
        b"Tx" => {
            if field.get(b"DataPrep").is_ok() {
                FormFieldType::Barcode
            } else if field_has_date_javascript(doc, field_id, field) {
                FormFieldType::Date
            } else {
                FormFieldType::Text
            }
        }
        _ => FormFieldType::Unknown,
    }
}

fn get_dict_from_object<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a Dictionary> {
    match obj {
        Object::Dictionary(d) => Some(d),
        Object::Reference(id) => doc.get_object(*id).ok().and_then(|x| x.as_dict().ok()),
        _ => None,
    }
}

fn field_has_image_appearance(doc: &Document, field_id: ObjectId, field: &Dictionary) -> bool {
    let check_dict = |d: &Dictionary| -> bool {
        if let Ok(Object::Dictionary(mk)) = d.get(b"MK") {
            if mk.get(b"I").is_ok() || mk.get(b"IF").is_ok() {
                return true;
            }
        }
        false
    };

    if check_dict(field) {
        return true;
    }

    for widget_id in widget_ids(doc, field_id, field) {
        if let Ok(widget) = doc.get_object(widget_id).and_then(|x| x.as_dict()) {
            if check_dict(widget) {
                return true;
            }
        }
    }

    find_field_image_stream(doc, field_id, field).is_some()
}

fn find_image_stream_in_object<'a>(doc: &'a Document, obj: &'a Object, depth: usize) -> Option<&'a Stream> {
    if depth > 5 {
        return None;
    }
    match obj {
        Object::Reference(id) => {
            let target = doc.get_object(*id).ok()?;
            find_image_stream_in_object(doc, target, depth + 1)
        }
        Object::Stream(stream) => {
            let subtype = stream.dict.get(b"Subtype").ok().and_then(|x| x.as_name().ok());
            if subtype == Some(b"Image") {
                if !stream.content.is_empty() {
                    return Some(stream);
                }
            } else if subtype == Some(b"Form") {
                if let Ok(res_obj) = stream.dict.get(b"Resources") {
                    if let Some(res) = get_dict_from_object(doc, res_obj) {
                        if let Ok(xobjs_obj) = res.get(b"XObject") {
                            if let Some(xobjs) = get_dict_from_object(doc, xobjs_obj) {
                                for (_name, xobj_val) in xobjs.iter() {
                                    if let Some(found) = find_image_stream_in_object(doc, xobj_val, depth + 1) {
                                        return Some(found);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            None
        }
        Object::Dictionary(dict) => {
            for (key, val) in dict.iter() {
                if key != b"Off" {
                    if let Some(found) = find_image_stream_in_object(doc, val, depth + 1) {
                        return Some(found);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn find_image_stream_in_ap<'a>(doc: &'a Document, ap_obj: &'a Object) -> Option<&'a Stream> {
    let ap_dict = match ap_obj {
        Object::Dictionary(d) => d,
        Object::Reference(id) => doc.get_object(*id).ok()?.as_dict().ok()?,
        _ => return None,
    };
    if let Ok(n_obj) = ap_dict.get(b"N") {
        find_image_stream_in_object(doc, n_obj, 0)
    } else {
        None
    }
}

fn find_image_stream_in_mk<'a>(doc: &'a Document, mk_obj: &'a Object) -> Option<&'a Stream> {
    let mk_dict = match mk_obj {
        Object::Dictionary(d) => d,
        Object::Reference(id) => doc.get_object(*id).ok()?.as_dict().ok()?,
        _ => return None,
    };
    if let Ok(i_obj) = mk_dict.get(b"I") {
        find_image_stream_in_object(doc, i_obj, 0)
    } else {
        None
    }
}

fn find_field_image_stream<'a>(doc: &'a Document, field_id: ObjectId, field: &'a Dictionary) -> Option<&'a Stream> {
    if let Ok(ap) = field.get(b"AP") {
        if let Some(found) = find_image_stream_in_ap(doc, ap) {
            return Some(found);
        }
    }
    if let Ok(mk) = field.get(b"MK") {
        if let Some(found) = find_image_stream_in_mk(doc, mk) {
            return Some(found);
        }
    }
    for widget_id in widget_ids(doc, field_id, field) {
        if let Ok(widget) = doc.get_object(widget_id).and_then(|x| x.as_dict()) {
            if let Ok(ap) = widget.get(b"AP") {
                if let Some(found) = find_image_stream_in_ap(doc, ap) {
                    return Some(found);
                }
            }
            if let Ok(mk) = widget.get(b"MK") {
                if let Some(found) = find_image_stream_in_mk(doc, mk) {
                    return Some(found);
                }
            }
        }
    }
    None
}

fn extract_image_value(doc: &Document, field_id: ObjectId, field: &Dictionary) -> Option<Value> {
    if let Some(val) = field_value(field, b"V") {
        if let Some(s) = val.as_str() {
            if !s.is_empty() {
                if s.starts_with("data:image/") {
                    return Some(Value::String(s.to_string()));
                } else if s.len() > 20 && base64::engine::general_purpose::STANDARD.decode(s).is_ok() {
                    return Some(Value::String(format!("data:image/jpeg;base64,{}", s)));
                }
            }
        }
    }

    let stream = find_field_image_stream(doc, field_id, field)?;
    let filter = stream.dict.get(b"Filter").ok().and_then(|x| x.as_name().ok());
    let mime = if stream.content.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if stream.content.starts_with(&[0xff, 0xd8]) || filter == Some(b"DCTDecode") {
        "image/jpeg"
    } else if filter == Some(b"JPXDecode") {
        "image/jp2"
    } else {
        "image/jpeg"
    };

    let b64 = base64::engine::general_purpose::STANDARD.encode(&stream.content);
    Some(Value::String(format!("data:{};base64,{}", mime, b64)))
}

fn javascript_text(doc: &Document, object: &Object) -> Option<String> {
    match object {
        Object::String(_, _) => object_text(object),
        Object::Stream(stream) => {
            let bytes = stream.decompressed_content().unwrap_or_else(|_| stream.content.clone());
            String::from_utf8(bytes).ok()
        }
        Object::Reference(id) => {
            let object = doc.get_object(*id).ok()?;
            javascript_text(doc, object)
        }
        Object::Dictionary(dict) => {
            if let Ok(js) = dict.get(b"JS") {
                return javascript_text(doc, js);
            }
            None
        }
        _ => None,
    }
}

fn acrobat_standard_date_format(index: usize) -> Option<&'static str> {
    match index {
        0 => Some("m/d"),
        1 => Some("m/d/yy"),
        2 => Some("mm/dd/yy"),
        3 => Some("mm/yy"),
        4 => Some("d-mmm"),
        5 => Some("d-mmm-yy"),
        6 => Some("dd-mmm-yy"),
        7 => Some("yy-mm-dd"),
        8 => Some("mmm-yy"),
        9 => Some("mmmm-yy"),
        10 => Some("mmm d, yyyy"),
        11 => Some("mmmm d, yyyy"),
        12 => Some("m/d/yy h:MM tt"),
        13 => Some("m/d/yy HH:MM"),
        _ => None,
    }
}

fn extract_quoted_arg(text: &str) -> Option<String> {
    let open = text.find('(')?;
    let inside = text[open + 1..].trim_start();
    let quote = inside.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let content = &inside[quote.len_utf8()..];
    let mut escaped = false;
    let mut end_idx = None;
    for (idx, ch) in content.char_indices() {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            end_idx = Some(idx);
            break;
        }
    }
    let end = end_idx?;
    let res = &content[..end];
    if res.is_empty() {
        None
    } else {
        Some(res.replace("\\\"", "\"").replace("\\'", "'"))
    }
}

fn parse_date_format_from_js(js: &str) -> Option<String> {
    for prefix in ["AFDate_FormatEx", "AFDate_KeystrokeEx", "util.printd"] {
        if let Some(pos) = js.find(prefix) {
            let rest = &js[pos + prefix.len()..];
            if let Some(fmt) = extract_quoted_arg(rest) {
                return Some(fmt);
            }
        }
    }

    for prefix in ["AFDate_Format", "AFDate_Keystroke"] {
        if let Some(pos) = js.find(prefix) {
            let rest = &js[pos + prefix.len()..];
            if let Some(open) = rest.find('(') {
                let inside = rest[open + 1..].trim_start();
                let num_str: String = inside.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(idx) = num_str.parse::<usize>() {
                    if let Some(fmt) = acrobat_standard_date_format(idx) {
                        return Some(fmt.to_string());
                    }
                }
            }
        }
    }

    None
}

fn date_format_from_aa(doc: &Document, aa: &Dictionary) -> Option<String> {
    for key in [b"F".as_slice(), b"K".as_slice(), b"V".as_slice(), b"C".as_slice()] {
        if let Ok(action) = aa.get(key) {
            if let Some(js) = javascript_text(doc, action) {
                if let Some(fmt) = parse_date_format_from_js(&js) {
                    return Some(fmt);
                }
            }
        }
    }
    None
}

fn field_date_format(doc: &Document, field_id: ObjectId, field: &Dictionary) -> Option<String> {
    if let Ok(aa_obj) = field.get(b"AA") {
        if let Some(aa) = get_dict_from_object(doc, aa_obj) {
            if let Some(fmt) = date_format_from_aa(doc, aa) {
                return Some(fmt);
            }
        }
    }

    for widget_id in widget_ids(doc, field_id, field) {
        if let Ok(widget) = doc.get_object(widget_id).and_then(|x| x.as_dict()) {
            if let Ok(aa_obj) = widget.get(b"AA") {
                if let Some(aa) = get_dict_from_object(doc, aa_obj) {
                    if let Some(fmt) = date_format_from_aa(doc, aa) {
                        return Some(fmt);
                    }
                }
            }
        }
    }

    None
}

fn field_has_date_javascript(doc: &Document, field_id: ObjectId, field: &Dictionary) -> bool {
    if field_date_format(doc, field_id, field).is_some() {
        return true;
    }
    let check_dict = |d: &Dictionary| -> bool {
        if let Ok(aa_obj) = d.get(b"AA") {
            if let Some(aa) = get_dict_from_object(doc, aa_obj) {
                for key in [b"K".as_slice(), b"F".as_slice(), b"V".as_slice(), b"C".as_slice()] {
                    if let Ok(action) = aa.get(key) {
                        if let Some(js) = javascript_text(doc, action) {
                            if js.contains("AFDate_") || js.contains("util.printd") {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    };
    if check_dict(field) {
        return true;
    }
    for widget_id in widget_ids(doc, field_id, field) {
        if let Ok(widget) = doc.get_object(widget_id).and_then(|x| x.as_dict()) {
            if check_dict(widget) {
                return true;
            }
        }
    }
    false
}

fn object_id_string(id: ObjectId) -> String {
    format!("{} {} R", id.0, id.1)
}

fn annotation_flags(dict: &Dictionary) -> u32 {
    dict.get(b"F")
        .ok()
        .and_then(|x| x.as_i64().ok())
        .unwrap_or(0)
        .max(0) as u32
}

fn widget_rect_values(dict: &Dictionary) -> Option<[f64; 4]> {
    let values = dict.get(b"Rect").ok()?.as_array().ok()?;
    if values.len() != 4 {
        return None;
    }
    let mut result = [0.0; 4];
    for (index, value) in values.iter().enumerate() {
        result[index] = match value {
            Object::Integer(v) => *v as f64,
            Object::Real(v) => *v as f64,
            _ => return None,
        };
    }
    Some(result)
}

fn widget_page_map(doc: &Document) -> HashMap<ObjectId, usize> {
    let mut result = HashMap::new();
    for (page_number, page_id) in doc.get_pages() {
        let Ok(page) = doc.get_object(page_id).and_then(|x| x.as_dict()) else {
            continue;
        };
        let Ok(annots) = page.get(b"Annots").and_then(|x| x.as_array()) else {
            continue;
        };
        for annotation in annots {
            if let Ok(id) = annotation.as_reference() {
                result.insert(id, page_number as usize);
            }
        }
    }
    result
}

fn field_value_object(object: &Object) -> Option<Value> {
    match object {
        Object::String(_, _) => object_text(object).map(Value::String),
        Object::Name(name) => Some(Value::String(String::from_utf8_lossy(name).into_owned())),
        Object::Integer(value) => Some(Value::Number((*value).into())),
        Object::Real(value) => serde_json::Number::from_f64(*value as f64).map(Value::Number),
        _ => None,
    }
}

fn field_value(dict: &Dictionary, key: &[u8]) -> Option<Value> {
    match dict.get(key).ok()? {
        Object::Array(values) => Some(Value::Array(
            values.iter().filter_map(field_value_object).collect(),
        )),
        object => field_value_object(object),
    }
}

fn extract_resolved_field_value(
    doc: &Document,
    field_id: ObjectId,
    field: &Dictionary,
) -> Option<Value> {
    if let Some(val) = field_value(field, b"V") {
        return Some(val);
    }
    let mut current = field
        .get(b"Parent")
        .ok()
        .and_then(|x| x.as_reference().ok())
        .and_then(|id| doc.get_object(id).ok())
        .and_then(|x| x.as_dict().ok());
    while let Some(parent_dict) = current {
        if let Some(val) = field_value(parent_dict, b"V") {
            return Some(val);
        }
        current = parent_dict
            .get(b"Parent")
            .ok()
            .and_then(|x| x.as_reference().ok())
            .and_then(|id| doc.get_object(id).ok())
            .and_then(|x| x.as_dict().ok());
    }
    for widget_id in widget_ids(doc, field_id, field) {
        if let Ok(widget) = doc.get_object(widget_id).and_then(|x| x.as_dict()) {
            if let Some(val) = field_value(widget, b"V") {
                return Some(val);
            }
            if let Ok(as_name) = widget.get(b"AS").and_then(|x| x.as_name()) {
                if as_name != b"Off" {
                    return Some(Value::String(String::from_utf8_lossy(as_name).into_owned()));
                }
            }
        }
    }
    None
}

fn field_locations(doc: &Document, field_id: ObjectId, field: &Dictionary) -> Vec<FieldLocation> {
    let pages = widget_page_map(doc);
    widget_ids(doc, field_id, field)
        .into_iter()
        .filter_map(|widget_id| {
            let widget = doc.get_object(widget_id).ok()?.as_dict().ok()?;
            let page = pages.get(&widget_id).copied()?;
            let rect = widget_rect_values(widget)?;
            let flags = annotation_flags(widget);
            Some(FieldLocation {
                page,
                rect,
                visible: flags & 2 == 0 && flags & 32 == 0,
                enabled: !is_read_only(field),
            })
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn extract_certificate_info(
    contents: &[u8],
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    for i in 0..contents.len().saturating_sub(4) {
        if contents[i] == 0x30 && (contents[i + 1] == 0x82 || contents[i + 1] == 0x81) {
            if let Ok((_, cert)) = x509_parser::parse_x509_certificate(&contents[i..]) {
                let signer_name = cert
                    .subject()
                    .iter_common_name()
                    .next()
                    .and_then(|cn| cn.as_str().ok())
                    .map(|s| s.to_string())
                    .or_else(|| Some(cert.subject().to_string()));
                let signer_org = cert
                    .subject()
                    .iter_organization()
                    .next()
                    .and_then(|o| o.as_str().ok())
                    .map(|s| s.to_string());
                let issuer = cert
                    .issuer()
                    .iter_common_name()
                    .next()
                    .and_then(|cn| cn.as_str().ok())
                    .map(|s| s.to_string())
                    .or_else(|| Some(cert.issuer().to_string()));
                let not_before = Some(cert.validity().not_before.to_string());
                let not_after = Some(cert.validity().not_after.to_string());
                let serial = Some(cert.raw_serial_as_string());
                return (signer_name, signer_org, issuer, not_before, not_after, serial);
            }
        }
    }
    (None, None, None, None, None, None)
}

#[cfg(target_arch = "wasm32")]
fn extract_certificate_info(
    _contents: &[u8],
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    (None, None, None, None, None, None)
}

fn get_signature_dict<'a>(
    doc: &'a Document,
    field_id: ObjectId,
    field: &'a Dictionary,
) -> Option<&'a Dictionary> {
    if let Ok(v_obj) = field.get(b"V") {
        if let Some(d) = get_dict_from_object(doc, v_obj) {
            return Some(d);
        }
    }
    for widget_id in widget_ids(doc, field_id, field) {
        if let Ok(widget) = doc.get_object(widget_id).and_then(|x| x.as_dict()) {
            if let Ok(v_obj) = widget.get(b"V") {
                if let Some(d) = get_dict_from_object(doc, v_obj) {
                    return Some(d);
                }
            }
        }
    }
    None
}

fn extract_signature_info(
    doc: &Document,
    field_id: ObjectId,
    field: &Dictionary,
) -> Option<SignatureInfo> {
    let sig_dict = get_signature_dict(doc, field_id, field);
    let image = extract_image_value(doc, field_id, field).and_then(|v| match v {
        Value::String(s) => Some(s),
        _ => None,
    });

    if sig_dict.is_none() && image.is_none() {
        return None;
    }

    let name = sig_dict.and_then(|d| d.get(b"Name").ok()).and_then(object_text);
    let reason = sig_dict.and_then(|d| d.get(b"Reason").ok()).and_then(object_text);
    let location = sig_dict.and_then(|d| d.get(b"Location").ok()).and_then(object_text);
    let contact_info = sig_dict
        .and_then(|d| d.get(b"ContactInfo").ok())
        .and_then(object_text);
    let signing_time = sig_dict.and_then(|d| d.get(b"M").ok()).and_then(object_text);
    let filter = sig_dict
        .and_then(|d| d.get(b"Filter").ok())
        .and_then(|x| x.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).into_owned());
    let sub_filter = sig_dict
        .and_then(|d| d.get(b"SubFilter").ok())
        .and_then(|x| x.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).into_owned());
    let byte_range = sig_dict
        .and_then(|d| d.get(b"ByteRange").ok())
        .and_then(|x| x.as_array().ok())
        .map(|arr| arr.iter().filter_map(|item| item.as_i64().ok()).collect());

    let (signer_name, signer_organization, issuer, not_before, not_after, serial_number) =
        if let Some(contents) = sig_dict
            .and_then(|d| d.get(b"Contents").ok())
            .and_then(|x| x.as_str().ok())
        {
            extract_certificate_info(contents)
        } else {
            (None, None, None, None, None, None)
        };

    Some(SignatureInfo {
        name,
        reason,
        location,
        contact_info,
        signing_time,
        filter,
        sub_filter,
        byte_range,
        image,
        signer_name,
        signer_organization,
        issuer,
        not_before,
        not_after,
        serial_number,
    })
}

struct FieldDefinition {
    id: ObjectId,
    name: String,
    field: Dictionary,
    field_type: Vec<u8>,
}

fn collect_field_definitions(doc: &Document) -> Vec<FieldDefinition> {
    let mut fields = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (id, object) in &doc.objects {
        let Ok(dict) = object.as_dict() else { continue };
        let Some(name) = inherited_field_name(doc, dict) else {
            continue;
        };
        let Some(field_type) = inherited_field_type(doc, *id, dict) else {
            continue;
        };
        let locations = field_locations(doc, *id, dict);
        if locations.is_empty() && dict.get(b"Kids").is_err() {
            continue;
        };
        let key = format!("{}:{}", name, String::from_utf8_lossy(&field_type));
        if !seen.insert(key) {
            continue;
        };
        fields.push(FieldDefinition {
            id: *id,
            name,
            field: dict.clone(),
            field_type,
        });
    }
    fields.sort_by(|a, b| a.name.cmp(&b.name));
    fields
}

fn collect_form_fields(doc: &Document) -> Vec<FormField> {
    let mut fields: Vec<FormField> = collect_field_definitions(doc)
        .iter()
        .map(|definition| {
            let locations = field_locations(doc, definition.id, &definition.field);
            let flags = field_flags(&definition.field) as u32;
            let read_only = flags & 1 != 0;
            let first = locations.first();
            let field_type = field_type_name(doc, definition.id, &definition.field, &definition.field_type);
            let is_image_field = field_type == FormFieldType::Image;
            let is_signature_field = field_type == FormFieldType::Signature;
            let signature = if is_signature_field {
                extract_signature_info(doc, definition.id, &definition.field)
            } else {
                None
            };
            let value = if is_image_field {
                extract_image_value(doc, definition.id, &definition.field)
                    .or_else(|| extract_resolved_field_value(doc, definition.id, &definition.field))
            } else if is_signature_field {
                signature
                    .as_ref()
                    .and_then(|s| s.image.clone().map(Value::String).or_else(|| s.name.clone().map(Value::String)))
            } else {
                extract_resolved_field_value(doc, definition.id, &definition.field)
            };
            let date_format = if field_type == FormFieldType::Date {
                field_date_format(doc, definition.id, &definition.field)
            } else {
                None
            };
            FormField {
                id: object_id_string(definition.id),
                name: definition.name.clone(),
                field_type,
                page: first.map(|x| x.page),
                rect: first.map(|x| x.rect),
                value,
                default_value: field_value(&definition.field, b"DV"),
                required: flags & (1 << 1) != 0,
                read_only,
                visible: locations.iter().any(|x| x.visible),
                enabled: !read_only && locations.iter().any(|x| x.enabled),
                tooltip: definition.field.get(b"TU").ok().and_then(object_text),
                options: if definition.field_type == b"Ch" {
                    choice_options(&definition.field)
                        .into_iter()
                        .map(|pair| FormFieldOption {
                            value: pair.first().cloned().unwrap_or_default(),
                            label: pair
                                .get(1)
                                .cloned()
                                .unwrap_or_else(|| pair.first().cloned().unwrap_or_default()),
                        })
                        .collect()
                } else if definition.field_type == b"Btn" {
                    all_button_states(doc, definition.id, &definition.field)
                        .into_iter()
                        .map(|value| {
                            let text = String::from_utf8_lossy(&value).into_owned();
                            FormFieldOption {
                                value: text.clone(),
                                label: text,
                            }
                        })
                        .collect()
                } else {
                    Vec::new()
                },
                flags,
                locations,
                signed: if definition.field_type == b"Sig" {
                    Some(signature.is_some() || definition.field.get(b"V").is_ok())
                } else {
                    None
                },
                date_format,
                signature,
            }
        })
        .collect();
    fields.sort_by(|a, b| {
        a.page
            .unwrap_or(usize::MAX)
            .cmp(&b.page.unwrap_or(usize::MAX))
            .then_with(|| a.name.cmp(&b.name))
    });
    fields
}

pub fn get_form_fields(template: &[u8]) -> Result<Vec<FormField>, String> {
    let doc = Document::load_mem(template).map_err(|e| format!("PDF load failed: {e}"))?;
    Ok(collect_form_fields(&doc))
}

pub fn form_fields_json(template: &[u8]) -> Result<String, String> {
    serde_json::to_string(&get_form_fields(template)?)
        .map_err(|e| format!("Form field serialization failed: {e}"))
}

fn collect_fields(doc: &Document) -> HashMap<String, (ObjectId, Dictionary, Vec<u8>)> {
    collect_field_definitions(doc)
        .into_iter()
        .map(|definition| {
            (
                definition.name,
                (definition.id, definition.field, definition.field_type),
            )
        })
        .collect()
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
