use base64::Engine;
use lopdf::xref::XrefType;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat, dictionary};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[cfg(target_arch = "wasm32")]
mod wasm;

pub mod ops;
pub use ops::*;

#[cfg(not(target_arch = "wasm32"))]
pub mod batch;
#[cfg(not(target_arch = "wasm32"))]
pub use batch::*;

pub mod appearance;
mod appearance_parser;
mod appearance_renderer;
mod field_strategy;
mod piece_info_security;
mod sign;
pub mod unicode_font;

pub use appearance::{
    GraphicPosition, PdfAppearance, SignatureAppearanceOptions, SignatureDesign, SignatureFont,
    SignatureLabels, SignatureTextLine, TextAlign,
};
pub use piece_info_security::{
    PieceInfoUnlockResult, PieceInfoUnlockStatus, compute_doc_fingerprint,
    insert_locked_piece_info, list_piece_info_applications, verify_and_unlock_piece_info,
};
pub use sign::{
    CertificateSigner, CmsSignatureMode, EcdsaSigner, PdfSigner, SignError, Signer,
    fill_and_sign_pdf,
};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LockedPieceInfoConfig {
    pub app_name: String,
    pub data: Value,
    pub secret_key: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FillOptions {
    #[serde(default)]
    pub flatten: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub piece_info: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked_piece_info: Option<LockedPieceInfoConfig>,
}

impl FillOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn flatten(mut self, flatten: bool) -> Self {
        self.flatten = flatten;
        self
    }

    pub fn piece_info(mut self, piece_info: Value) -> Self {
        self.piece_info = Some(piece_info);
        self
    }

    pub fn locked_piece_info(
        mut self,
        app_name: impl Into<String>,
        data: Value,
        secret_key: impl Into<String>,
    ) -> Self {
        self.locked_piece_info = Some(LockedPieceInfoConfig {
            app_name: app_name.into(),
            data,
            secret_key: secret_key.into(),
        });
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldStatus {
    Filled,
    Missing,
    Invalid,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

pub(crate) fn inherited_field_flags(doc: &Document, dict: &Dictionary) -> i64 {
    if let Ok(ff) = dict.get(b"Ff").and_then(|x| x.as_i64()) {
        return ff;
    }
    let mut current = dict.get(b"Parent").ok().and_then(|x| x.as_reference().ok());
    while let Some(parent_id) = current {
        let Ok(parent) = doc.get_object(parent_id).and_then(|x| x.as_dict()) else {
            break;
        };
        if let Ok(ff) = parent.get(b"Ff").and_then(|x| x.as_i64()) {
            return ff;
        }
        current = parent
            .get(b"Parent")
            .ok()
            .and_then(|x| x.as_reference().ok());
    }
    0
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
    let flags = inherited_field_flags(doc, field) as u32;
    let is_editable = flags & (1 << 18) != 0;
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
        if let Some((index, pair)) = options
            .iter()
            .enumerate()
            .find(|(_, pair)| pair.iter().any(|x| x == value))
        {
            indexes.push(index);
            exports.push(pair[0].clone());
        } else if is_editable {
            exports.push(value.clone());
        } else {
            return Err(format!(
                "Choice value '{value}' is not present in field options"
            ));
        }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design: Option<SignatureDesign>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_select: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_option: Option<bool>,
}

fn field_type_name(
    doc: &Document,
    field_id: ObjectId,
    field: &Dictionary,
    ft: &[u8],
) -> FormFieldType {
    match ft {
        b"Sig" => FormFieldType::Signature,
        b"Btn" => {
            let flags = inherited_field_flags(doc, field) as u32;
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
            let flags = inherited_field_flags(doc, field) as u32;
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

fn find_image_stream_in_object<'a>(
    doc: &'a Document,
    obj: &'a Object,
    depth: usize,
) -> Option<&'a Stream> {
    if depth > 5 {
        return None;
    }
    match obj {
        Object::Reference(id) => {
            let target = doc.get_object(*id).ok()?;
            find_image_stream_in_object(doc, target, depth + 1)
        }
        Object::Stream(stream) => {
            let subtype = stream
                .dict
                .get(b"Subtype")
                .ok()
                .and_then(|x| x.as_name().ok());
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
                                    if let Some(found) =
                                        find_image_stream_in_object(doc, xobj_val, depth + 1)
                                    {
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

fn find_field_image_stream<'a>(
    doc: &'a Document,
    field_id: ObjectId,
    field: &'a Dictionary,
) -> Option<&'a Stream> {
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
                } else if s.len() > 20
                    && base64::engine::general_purpose::STANDARD.decode(s).is_ok()
                {
                    return Some(Value::String(format!("data:image/jpeg;base64,{}", s)));
                }
            }
        }
    }

    let stream = find_field_image_stream(doc, field_id, field)?;
    let filter = stream
        .dict
        .get(b"Filter")
        .ok()
        .and_then(|x| x.as_name().ok());
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
            let bytes = stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone());
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
    for key in [
        b"F".as_slice(),
        b"K".as_slice(),
        b"V".as_slice(),
        b"C".as_slice(),
    ] {
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
                for key in [
                    b"K".as_slice(),
                    b"F".as_slice(),
                    b"V".as_slice(),
                    b"C".as_slice(),
                ] {
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
                return (
                    signer_name,
                    signer_org,
                    issuer,
                    not_before,
                    not_after,
                    serial,
                );
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

    let name = sig_dict
        .and_then(|d| d.get(b"Name").ok())
        .and_then(object_text);
    let reason = sig_dict
        .and_then(|d| d.get(b"Reason").ok())
        .and_then(object_text);
    let location = sig_dict
        .and_then(|d| d.get(b"Location").ok())
        .and_then(object_text);
    let contact_info = sig_dict
        .and_then(|d| d.get(b"ContactInfo").ok())
        .and_then(object_text);
    let signing_time = sig_dict
        .and_then(|d| d.get(b"M").ok())
        .and_then(object_text);
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

    let design = appearance_parser::parse_signature_appearance(doc, field_id, field);

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
        design,
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
            let flags = inherited_field_flags(doc, &definition.field) as u32;
            let read_only = flags & 1 != 0;
            let first = locations.first();
            let field_type = field_type_name(
                doc,
                definition.id,
                &definition.field,
                &definition.field_type,
            );
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
                signature.as_ref().and_then(|s| {
                    s.image
                        .clone()
                        .map(Value::String)
                        .or_else(|| s.name.clone().map(Value::String))
                })
            } else {
                extract_resolved_field_value(doc, definition.id, &definition.field)
            };
            let date_format = if field_type == FormFieldType::Date {
                field_date_format(doc, definition.id, &definition.field)
            } else {
                None
            };
            let is_choice = definition.field_type == b"Ch";
            let multi_select = if is_choice {
                Some(flags & (1 << 21) != 0)
            } else {
                None
            };
            let editable = if is_choice {
                Some(flags & (1 << 18) != 0)
            } else {
                None
            };
            let custom_option = editable;
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
                multi_select,
                editable,
                custom_option,
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

fn add_xobject_to_page(
    doc: &mut Document,
    page_id: ObjectId,
    xobj_name: &str,
    xobj_id: ObjectId,
) -> Result<(), String> {
    let res_obj_id = {
        let page = doc
            .get_object(page_id)
            .map_err(|e| e.to_string())?
            .as_dict()
            .map_err(|e| e.to_string())?;
        match page.get(b"Resources") {
            Ok(Object::Reference(res_id)) => Some(*res_id),
            _ => None,
        }
    };

    if let Some(res_id) = res_obj_id {
        let xobj_ref = {
            let res_dict = doc
                .get_object(res_id)
                .map_err(|e| e.to_string())?
                .as_dict()
                .map_err(|e| e.to_string())?;
            match res_dict.get(b"XObject") {
                Ok(Object::Reference(xid)) => Some(*xid),
                _ => None,
            }
        };

        if let Some(xid) = xobj_ref {
            let x_dict = doc
                .get_object_mut(xid)
                .map_err(|e| e.to_string())?
                .as_dict_mut()
                .map_err(|e| e.to_string())?;
            x_dict.set(xobj_name.to_string(), Object::Reference(xobj_id));
        } else {
            let res_dict = doc
                .get_object_mut(res_id)
                .map_err(|e| e.to_string())?
                .as_dict_mut()
                .map_err(|e| e.to_string())?;
            if res_dict.get(b"XObject").is_err() {
                res_dict.set("XObject", Dictionary::new());
            }
            let x_dict = res_dict
                .get_mut(b"XObject")
                .map_err(|e| e.to_string())?
                .as_dict_mut()
                .map_err(|e| e.to_string())?;
            x_dict.set(xobj_name.to_string(), Object::Reference(xobj_id));
        }
    } else {
        let xobj_ref = {
            let page = doc
                .get_object(page_id)
                .map_err(|e| e.to_string())?
                .as_dict()
                .map_err(|e| e.to_string())?;
            match page
                .get(b"Resources")
                .ok()
                .and_then(|r| r.as_dict().ok())
                .and_then(|rd| rd.get(b"XObject").ok())
            {
                Some(Object::Reference(xid)) => Some(*xid),
                _ => None,
            }
        };

        if let Some(xid) = xobj_ref {
            let x_dict = doc
                .get_object_mut(xid)
                .map_err(|e| e.to_string())?
                .as_dict_mut()
                .map_err(|e| e.to_string())?;
            x_dict.set(xobj_name.to_string(), Object::Reference(xobj_id));
        } else {
            let page = doc
                .get_object_mut(page_id)
                .map_err(|e| e.to_string())?
                .as_dict_mut()
                .map_err(|e| e.to_string())?;
            if page.get(b"Resources").is_err() {
                page.set("Resources", Dictionary::new());
            }
            let res_dict = page
                .get_mut(b"Resources")
                .map_err(|e| e.to_string())?
                .as_dict_mut()
                .map_err(|e| e.to_string())?;
            if res_dict.get(b"XObject").is_err() {
                res_dict.set("XObject", Dictionary::new());
            }
            let x_dict = res_dict
                .get_mut(b"XObject")
                .map_err(|e| e.to_string())?
                .as_dict_mut()
                .map_err(|e| e.to_string())?;
            x_dict.set(xobj_name.to_string(), Object::Reference(xobj_id));
        }
    }
    Ok(())
}

fn append_page_content(
    doc: &mut Document,
    page_id: ObjectId,
    content_bytes: Vec<u8>,
) -> Result<(), String> {
    let new_stream_id = doc.add_object(Stream::new(Dictionary::new(), content_bytes));
    let page = doc
        .get_object_mut(page_id)
        .map_err(|e| e.to_string())?
        .as_dict_mut()
        .map_err(|e| e.to_string())?;

    match page.get_mut(b"Contents") {
        Ok(Object::Array(arr)) => {
            arr.push(Object::Reference(new_stream_id));
        }
        Ok(Object::Reference(existing_id)) => {
            let existing = *existing_id;
            page.set(
                "Contents",
                Object::Array(vec![
                    Object::Reference(existing),
                    Object::Reference(new_stream_id),
                ]),
            );
        }
        _ => {
            page.set("Contents", Object::Reference(new_stream_id));
        }
    }
    Ok(())
}

fn resolve_or_create_widget_appearance(
    doc: &mut Document,
    widget_id: ObjectId,
    def: &FieldDefinition,
    w: f64,
    h: f64,
    unicode_ctx: Option<&unicode_font::UnicodeFontContext>,
) -> Option<ObjectId> {
    let widget_dict = doc.get_object(widget_id).ok()?.as_dict().ok()?.clone();

    let ap_obj = widget_dict
        .get(b"AP")
        .ok()
        .or_else(|| def.field.get(b"AP").ok());
    if let Some(ap) = ap_obj.and_then(|x| get_dict_from_object(doc, x)) {
        if let Ok(n_obj) = ap.get(b"N") {
            if let Ok(stream_id) = n_obj.as_reference() {
                if let Ok(obj) = doc.get_object(stream_id) {
                    if obj.as_stream().is_ok() {
                        return Some(stream_id);
                    }
                    if let Ok(n_dict) = obj.as_dict() {
                        let state = widget_dict
                            .get(b"AS")
                            .ok()
                            .and_then(|x| x.as_name().ok())
                            .or_else(|| def.field.get(b"V").ok().and_then(|x| x.as_name().ok()));
                        if let Some(state_name) = state {
                            if state_name != b"Off" {
                                if let Ok(state_stream_id) =
                                    n_dict.get(state_name).and_then(|x| x.as_reference())
                                {
                                    return Some(state_stream_id);
                                }
                            }
                        }
                    }
                }
            } else if let Ok(n_dict) = n_obj.as_dict() {
                let state = widget_dict
                    .get(b"AS")
                    .ok()
                    .and_then(|x| x.as_name().ok())
                    .or_else(|| def.field.get(b"V").ok().and_then(|x| x.as_name().ok()));
                if let Some(state_name) = state {
                    if state_name != b"Off" {
                        if let Ok(state_stream_id) =
                            n_dict.get(state_name).and_then(|x| x.as_reference())
                        {
                            return Some(state_stream_id);
                        }
                    }
                }
            } else if let Ok(stream) = n_obj.as_stream() {
                let id = doc.add_object(stream.clone());
                return Some(id);
            }
        }
    }

    if def.field_type == b"Tx" || def.field_type == b"Ch" {
        let val_opt = extract_resolved_field_value(doc, def.id, &def.field);
        if let Some(Value::String(val_str)) = val_opt {
            if !val_str.is_empty() {
                return Some(unicode_font::create_text_appearance_stream(
                    doc,
                    w,
                    h,
                    &val_str,
                    unicode_ctx,
                ));
            }
        }
    }

    None
}

pub fn flatten_form_fields(
    doc: &mut Document,
    keep_unsigned_signatures: bool,
) -> Result<(), String> {
    let definitions = collect_field_definitions(doc);
    let mut fields_to_keep = std::collections::HashSet::new();
    let mut fields_to_flatten = Vec::new();

    for def in definitions {
        if def.field_type == b"Sig" {
            let is_signed = get_signature_dict(doc, def.id, &def.field).is_some()
                || def.field.get(b"V").is_ok()
                || widget_ids(doc, def.id, &def.field).iter().any(|&w| {
                    doc.get_object(w)
                        .and_then(|o| o.as_dict())
                        .map(|d| d.get(b"V").is_ok())
                        .unwrap_or(false)
                });
            if is_signed {
                fields_to_keep.insert(def.id);
                for w in widget_ids(doc, def.id, &def.field) {
                    fields_to_keep.insert(w);
                    if let Ok(w_dict) = doc.get_object_mut(w).and_then(|o| o.as_dict_mut()) {
                        let f = w_dict
                            .get(b"F")
                            .ok()
                            .and_then(|x| x.as_i64().ok())
                            .unwrap_or(0);
                        w_dict.set("F", Object::Integer(f | 1 | 64));
                        let ff = w_dict
                            .get(b"Ff")
                            .ok()
                            .and_then(|x| x.as_i64().ok())
                            .unwrap_or(0);
                        w_dict.set("Ff", Object::Integer(ff | 1));
                    }
                }
                if let Ok(f_dict) = doc.get_object_mut(def.id).and_then(|o| o.as_dict_mut()) {
                    let ff = f_dict
                        .get(b"Ff")
                        .ok()
                        .and_then(|x| x.as_i64().ok())
                        .unwrap_or(0);
                    f_dict.set("Ff", Object::Integer(ff | 1));
                }
            } else if keep_unsigned_signatures {
                fields_to_keep.insert(def.id);
                for w in widget_ids(doc, def.id, &def.field) {
                    fields_to_keep.insert(w);
                }
            } else {
                fields_to_flatten.push(def);
            }
        } else {
            fields_to_flatten.push(def);
        }
    }

    // Retain hierarchy of kept fields (signature fields, their widgets, and ancestors)
    let mut all_kept_ids = fields_to_keep.clone();
    for kept_id in &fields_to_keep {
        let mut current = *kept_id;
        while let Ok(dict) = doc.get_object(current).and_then(|x| x.as_dict()) {
            if let Ok(parent_id) = dict.get(b"Parent").and_then(|x| x.as_reference()) {
                all_kept_ids.insert(parent_id);
                current = parent_id;
            } else {
                break;
            }
        }
    }

    let mut widget_page_obj_map = HashMap::new();
    for (_page_number, page_id) in doc.get_pages() {
        if let Ok(page) = doc.get_object(page_id).and_then(|x| x.as_dict()) {
            if let Ok(annots) = page.get(b"Annots").and_then(|x| x.as_array()) {
                for annot in annots {
                    if let Ok(id) = annot.as_reference() {
                        widget_page_obj_map.insert(id, page_id);
                    }
                }
            }
        }
    }

    let mut widgets_to_remove = std::collections::HashSet::new();

    for def in &fields_to_flatten {
        let widgets = widget_ids(doc, def.id, &def.field);
        for widget_id in widgets {
            if !all_kept_ids.contains(&widget_id) {
                widgets_to_remove.insert(widget_id);
            }
        }
    }

    // Scan text/combo fields to flatten for non-ASCII characters
    let mut non_ascii_chars = Vec::new();
    for def in &fields_to_flatten {
        if def.field_type == b"Tx" || def.field_type == b"Ch" {
            if let Some(Value::String(val_str)) = extract_resolved_field_value(doc, def.id, &def.field) {
                for c in val_str.chars() {
                    if (c as u32) > 127 && !non_ascii_chars.contains(&c) {
                        non_ascii_chars.push(c);
                    }
                }
            }
        }
    }

    let unicode_ctx = if !non_ascii_chars.is_empty() {
        Some(unicode_font::UnicodeFontContext::new(doc, &non_ascii_chars))
    } else {
        None
    };

    for def in &fields_to_flatten {
        let widgets = widget_ids(doc, def.id, &def.field);
        for widget_id in widgets {
            if all_kept_ids.contains(&widget_id) {
                continue;
            }

            let widget_dict = match doc.get_object(widget_id).and_then(|o| o.as_dict()) {
                Ok(d) => d.clone(),
                Err(_) => continue,
            };

            let page_id_opt = widget_dict
                .get(b"P")
                .ok()
                .and_then(|p| p.as_reference().ok())
                .filter(|id| doc.objects.contains_key(id))
                .or_else(|| widget_page_obj_map.get(&widget_id).copied());

            let Some(page_id) = page_id_opt else {
                continue;
            };

            let Some(rect_vals) = widget_rect_values(&widget_dict) else {
                continue;
            };

            let min_x = rect_vals[0].min(rect_vals[2]);
            let min_y = rect_vals[1].min(rect_vals[3]);
            let w = (rect_vals[2] - rect_vals[0]).abs();
            let h = (rect_vals[3] - rect_vals[1]).abs();

            if w <= 0.0 || h <= 0.0 {
                continue;
            }

            if let Some(app_id) = resolve_or_create_widget_appearance(doc, widget_id, def, w, h, unicode_ctx.as_ref()) {
                let (sx, sy, tx, ty) = if let Ok(stream_obj) = doc.get_object(app_id) {
                    if let Ok(stream) = stream_obj.as_stream() {
                        if let Ok(bbox) = stream.dict.get(b"BBox").and_then(|x| x.as_array()) {
                            if bbox.len() == 4 {
                                let n = |i: usize| -> f64 {
                                    match &bbox[i] {
                                        Object::Integer(v) => *v as f64,
                                        Object::Real(v) => *v as f64,
                                        _ => 0.0,
                                    }
                                };
                                let bx1 = n(0);
                                let by1 = n(1);
                                let bx2 = n(2);
                                let by3 = n(3);
                                let bw = (bx2 - bx1).abs();
                                let bh = (by3 - by1).abs();
                                if bw > 0.0 && bh > 0.0 {
                                    let sx = w / bw;
                                    let sy = h / bh;
                                    let tx = min_x - bx1 * sx;
                                    let ty = min_y - by1 * sy;
                                    (sx, sy, tx, ty)
                                } else {
                                    (1.0, 1.0, min_x, min_y)
                                }
                            } else {
                                (1.0, 1.0, min_x, min_y)
                            }
                        } else {
                            (1.0, 1.0, min_x, min_y)
                        }
                    } else {
                        (1.0, 1.0, min_x, min_y)
                    }
                } else {
                    (1.0, 1.0, min_x, min_y)
                };

                let xobj_name = format!("FlatX{}_{}", widget_id.0, widget_id.1);
                add_xobject_to_page(doc, page_id, &xobj_name, app_id)?;
                let draw_cmd =
                    format!("q {sx:.6} 0 0 {sy:.6} {tx:.6} {ty:.6} cm /{xobj_name} Do Q\n");
                append_page_content(doc, page_id, draw_cmd.into_bytes())?;
            }
        }
    }

    // Remove widgets from page /Annots
    for (_page_num, page_id) in doc.get_pages() {
        if let Ok(page) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
            if let Ok(annots) = page.get_mut(b"Annots").and_then(|x| x.as_array_mut()) {
                annots.retain(|item| match item.as_reference() {
                    Ok(id) => !widgets_to_remove.contains(&id),
                    _ => true,
                });
            }
        }
    }

    for kept_id in &all_kept_ids {
        if let Ok(dict) = doc.get_object_mut(*kept_id).and_then(|x| x.as_dict_mut()) {
            if let Ok(kids) = dict.get_mut(b"Kids").and_then(|x| x.as_array_mut()) {
                kids.retain(|k| match k.as_reference() {
                    Ok(id) => all_kept_ids.contains(&id),
                    _ => true,
                });
            }
        }
    }

    // Clean up AcroForm /Fields
    for (_, object) in doc.objects.iter_mut() {
        if let Ok(dict) = object.as_dict_mut() {
            if let Ok(fields) = dict.get_mut(b"Fields").and_then(|x| x.as_array_mut()) {
                fields.retain(|item| match item.as_reference() {
                    Ok(id) => all_kept_ids.contains(&id),
                    _ => true,
                });
            }
            if all_kept_ids.is_empty() {
                dict.remove(b"NeedAppearances");
            }
        }
    }

    // Remove flattened field and widget objects (and their non-kept parents) from doc.objects
    let mut flattened_ids = std::collections::HashSet::new();
    for def in &fields_to_flatten {
        flattened_ids.insert(def.id);
        let mut current = def.id;
        while let Ok(dict) = doc.get_object(current).and_then(|x| x.as_dict()) {
            if let Ok(parent_id) = dict.get(b"Parent").and_then(|x| x.as_reference()) {
                if !all_kept_ids.contains(&parent_id) {
                    flattened_ids.insert(parent_id);
                }
                current = parent_id;
            } else {
                break;
            }
        }
    }
    for id in &flattened_ids {
        if !all_kept_ids.contains(id) {
            doc.objects.remove(id);
        }
    }
    for wid in &widgets_to_remove {
        if !all_kept_ids.contains(wid) {
            doc.objects.remove(wid);
        }
    }

    Ok(())
}

pub fn json_to_pdf_object(val: &Value) -> Object {
    match val {
        Value::Null => Object::Null,
        Value::Bool(b) => Object::Boolean(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Object::Integer(i)
            } else if let Some(f) = n.as_f64() {
                Object::Real(f as f32)
            } else {
                Object::Null
            }
        }
        Value::String(s) => Object::string_literal(s.as_str()),
        Value::Array(arr) => {
            let pdf_arr = arr.iter().map(json_to_pdf_object).collect();
            Object::Array(pdf_arr)
        }
        Value::Object(map) => {
            let mut dict = Dictionary::new();
            for (k, v) in map {
                dict.set(k.as_bytes().to_vec(), json_to_pdf_object(v));
            }
            Object::Dictionary(dict)
        }
    }
}

pub fn pdf_object_to_json(obj: &Object, doc: &Document) -> Value {
    match obj {
        Object::Null => Value::Null,
        Object::Boolean(b) => Value::Bool(*b),
        Object::Integer(i) => Value::Number((*i).into()),
        Object::Real(f) => serde_json::Number::from_f64((*f).into())
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Object::Name(bytes) => Value::String(String::from_utf8_lossy(bytes).into_owned()),
        Object::String(bytes, _) => {
            if let Some(txt) = object_text(obj) {
                Value::String(txt)
            } else {
                Value::String(String::from_utf8_lossy(bytes).into_owned())
            }
        }
        Object::Array(arr) => {
            let items: Vec<Value> = arr
                .iter()
                .map(|item| pdf_object_to_json(item, doc))
                .collect();
            Value::Array(items)
        }
        Object::Dictionary(dict) => {
            let mut map = serde_json::Map::new();
            for (k, v) in dict.iter() {
                let key = String::from_utf8_lossy(k).into_owned();
                map.insert(key, pdf_object_to_json(v, doc));
            }
            Value::Object(map)
        }
        Object::Reference(id) => {
            if let Ok(resolved) = doc.get_object(*id) {
                pdf_object_to_json(resolved, doc)
            } else {
                Value::Null
            }
        }
        Object::Stream(stream) => {
            let mut map = serde_json::Map::new();
            for (k, v) in stream.dict.iter() {
                let key = String::from_utf8_lossy(k).into_owned();
                map.insert(key, pdf_object_to_json(v, doc));
            }
            Value::Object(map)
        }
    }
}

pub fn insert_piece_info(doc: &mut Document, piece_info: &Value) -> Result<(), String> {
    if piece_info.is_null() {
        return Ok(());
    }
    let catalog_id = match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => *id,
        _ => return Err("PDF Catalog not found in trailer".to_string()),
    };

    let new_obj = json_to_pdf_object(piece_info);

    let existing_ref = {
        let catalog = doc
            .get_object(catalog_id)
            .map_err(|e| format!("Failed to get Catalog: {e}"))?
            .as_dict()
            .map_err(|e| format!("Catalog is not a dictionary: {e}"))?;
        match catalog.get(b"PieceInfo") {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        }
    };

    if let Some(r_id) = existing_ref {
        if let Ok(piece_dict) = doc.get_object_mut(r_id).and_then(|o| o.as_dict_mut()) {
            if let Object::Dictionary(new_dict) = new_obj {
                for (k, v) in new_dict.iter() {
                    piece_dict.set(k.clone(), v.clone());
                }
            }
            return Ok(());
        }
    }

    let catalog = doc
        .get_object_mut(catalog_id)
        .map_err(|e| format!("Failed to get Catalog: {e}"))?
        .as_dict_mut()
        .map_err(|e| format!("Catalog is not a dictionary: {e}"))?;

    match catalog.get_mut(b"PieceInfo") {
        Ok(Object::Dictionary(existing_dict)) => {
            if let Object::Dictionary(new_dict) = new_obj {
                for (k, v) in new_dict.iter() {
                    existing_dict.set(k.clone(), v.clone());
                }
            }
        }
        _ => {
            catalog.set(b"PieceInfo".as_slice(), new_obj);
        }
    }

    Ok(())
}

pub fn get_piece_info(pdf: &[u8]) -> Result<Option<Value>, String> {
    let doc = Document::load_mem(pdf).map_err(|e| format!("PDF load failed: {e}"))?;
    let catalog_id = match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => *id,
        _ => return Ok(None),
    };
    let catalog = match doc.get_object(catalog_id).and_then(|o| o.as_dict()) {
        Ok(d) => d,
        Err(_) => return Ok(None),
    };
    match catalog.get(b"PieceInfo") {
        Ok(obj) => {
            let val = pdf_object_to_json(obj, &doc);
            if val.is_null() {
                Ok(None)
            } else {
                Ok(Some(val))
            }
        }
        Err(_) => Ok(None),
    }
}

pub fn fill_pdf(
    template: &[u8],
    json: &str,
    piece_info: Option<&str>,
) -> Result<(Vec<u8>, FillReport), String> {
    let mut piece_info_val = None;
    let mut locked_piece_info_val = None;
    let mut flatten_val = false;
    if let Some(s) = piece_info {
        if !s.trim().is_empty() {
            let val: serde_json::Value =
                serde_json::from_str(s).map_err(|e| format!("Invalid piece_info JSON: {e}"))?;

            if let Some(f) = val
                .get("flatten")
                .and_then(|v| v.as_bool())
                .or_else(|| val.get("__flatten").and_then(|v| v.as_bool()))
                .or_else(|| {
                    val.get("WasmPlayground")
                        .and_then(|w| w.get("flatten"))
                        .and_then(|v| v.as_bool())
                })
            {
                flatten_val = f;
            }

            if let Some(locked_obj) = val.get("__locked") {
                if let (Some(app), Some(data), Some(sec)) = (
                    locked_obj.get("app_name").and_then(|v| v.as_str()),
                    locked_obj.get("data"),
                    locked_obj.get("secret_key").and_then(|v| v.as_str()),
                ) {
                    locked_piece_info_val = Some(LockedPieceInfoConfig {
                        app_name: app.to_string(),
                        data: data.clone(),
                        secret_key: sec.to_string(),
                    });
                }
            } else {
                piece_info_val = Some(val);
            }
        }
    }
    fill_pdf_with_options(
        template,
        json,
        &FillOptions {
            flatten: flatten_val,
            piece_info: piece_info_val,
            locked_piece_info: locked_piece_info_val,
        },
    )
}

pub fn fill_pdf_with_options(
    template: &[u8],
    json: &str,
    options: &FillOptions,
) -> Result<(Vec<u8>, FillReport), String> {
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

    if options.flatten {
        flatten_form_fields(&mut doc, true)?;
    } else {
        for id in acroforms {
            if let Ok(d) = doc.get_object_mut(id).and_then(|x| x.as_dict_mut()) {
                d.remove(b"NeedAppearances");
            }
        }
    }

    if let Some(ref p_info) = options.piece_info {
        insert_piece_info(&mut doc, p_info)?;
    }

    if let Some(ref locked) = options.locked_piece_info {
        piece_info_security::insert_locked_piece_info(
            &mut doc,
            &locked.app_name,
            &locked.data,
            &locked.secret_key,
        )?;
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
