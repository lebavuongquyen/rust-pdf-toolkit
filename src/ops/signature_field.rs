use lopdf::{Dictionary, Document, Object, ObjectId, dictionary};
use serde::{Deserialize, Serialize};

/// Preset visual placement positions for signature fields on a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FieldPresetPosition {
    #[default]
    BottomRight,
    BottomLeft,
    BottomCenter,
    TopRight,
    TopLeft,
    TopCenter,
    Center,
}

impl FieldPresetPosition {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "bottom-left" | "bottomleft" => Self::BottomLeft,
            "bottom-center" | "bottomcenter" => Self::BottomCenter,
            "top-right" | "topright" => Self::TopRight,
            "top-left" | "topleft" => Self::TopLeft,
            "top-center" | "topcenter" => Self::TopCenter,
            "center" | "middle" => Self::Center,
            _ => Self::BottomRight,
        }
    }
}

/// Placement configuration of a signature field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum SignaturePlacement {
    /// Invisible cryptographic signature (Rect [0, 0, 0, 0]).
    /// Widely used for automated billing, receipts, background verification.
    #[default]
    Invisible,
    /// Explicit rectangle coordinates in points: [llx, lly, urx, ury].
    Rect([f64; 4]),
    /// Automatic placement using a preset position with width, height, and margins.
    Preset {
        position: FieldPresetPosition,
        width: f64,
        height: f64,
        margin_x: f64,
        margin_y: f64,
    },
}

impl SignaturePlacement {
    pub fn visible_preset(position: FieldPresetPosition, width: f64, height: f64) -> Self {
        Self::Preset {
            position,
            width,
            height,
            margin_x: 36.0,
            margin_y: 36.0,
        }
    }

    /// Computes [llx, lly, urx, ury] given the page bounding box (page_llx, page_lly, width, height).
    pub fn compute_rect(&self, page_llx: f64, page_lly: f64, page_w: f64, page_h: f64) -> [f64; 4] {
        match self {
            Self::Invisible => [0.0, 0.0, 0.0, 0.0],
            Self::Rect(r) => *r,
            Self::Preset {
                position,
                width,
                height,
                margin_x,
                margin_y,
            } => {
                let w = width.clamp(10.0, page_w);
                let h = height.clamp(10.0, page_h);

                let (x, y) = match position {
                    FieldPresetPosition::BottomRight => {
                        (page_llx + page_w - margin_x - w, page_lly + margin_y)
                    }
                    FieldPresetPosition::BottomLeft => (page_llx + margin_x, page_lly + margin_y),
                    FieldPresetPosition::BottomCenter => {
                        (page_llx + (page_w - w) / 2.0, page_lly + margin_y)
                    }
                    FieldPresetPosition::TopRight => (
                        page_llx + page_w - margin_x - w,
                        page_lly + page_h - margin_y - h,
                    ),
                    FieldPresetPosition::TopLeft => {
                        (page_llx + margin_x, page_lly + page_h - margin_y - h)
                    }
                    FieldPresetPosition::TopCenter => (
                        page_llx + (page_w - w) / 2.0,
                        page_lly + page_h - margin_y - h,
                    ),
                    FieldPresetPosition::Center => {
                        (page_llx + (page_w - w) / 2.0, page_lly + (page_h - h) / 2.0)
                    }
                };

                [x, y, x + w, y + h]
            }
        }
    }
}

/// Options for adding a new signature field to a PDF.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddSignatureFieldOptions {
    /// Field name (e.g. "Signature1", "ManagerSignature").
    pub field_name: String,
    /// 1-based page index. 0 or None defaults to the last page.
    #[serde(default)]
    pub page: Option<u32>,
    /// Field placement: Invisible or Visible Rect / Preset.
    #[serde(default)]
    pub placement: SignaturePlacement,
    /// Whether signing this field should lock the document against further form changes.
    #[serde(default = "default_true")]
    pub lock_all_fields: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AddSignatureFieldOptions {
    fn default() -> Self {
        Self {
            field_name: "Signature1".to_string(),
            page: None,
            placement: SignaturePlacement::Invisible,
            lock_all_fields: true,
        }
    }
}

impl AddSignatureFieldOptions {
    pub fn new(field_name: impl Into<String>) -> Self {
        Self {
            field_name: field_name.into(),
            ..Default::default()
        }
    }

    pub fn page(mut self, page_num: u32) -> Self {
        self.page = Some(page_num);
        self
    }

    pub fn placement(mut self, placement: SignaturePlacement) -> Self {
        self.placement = placement;
        self
    }

    pub fn invisible(mut self) -> Self {
        self.placement = SignaturePlacement::Invisible;
        self
    }

    pub fn rect(mut self, llx: f64, lly: f64, urx: f64, ury: f64) -> Self {
        self.placement = SignaturePlacement::Rect([llx, lly, urx, ury]);
        self
    }

    pub fn preset(mut self, position: FieldPresetPosition, width: f64, height: f64) -> Self {
        self.placement = SignaturePlacement::visible_preset(position, width, height);
        self
    }

    pub fn preset_with_margins(
        mut self,
        position: FieldPresetPosition,
        width: f64,
        height: f64,
        margin_x: f64,
        margin_y: f64,
    ) -> Self {
        self.placement = SignaturePlacement::Preset {
            position,
            width,
            height,
            margin_x,
            margin_y,
        };
        self
    }

    pub fn lock_all_fields(mut self, lock: bool) -> Self {
        self.lock_all_fields = lock;
        self
    }
}

/// Options for removing signature fields from a PDF.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RemoveSignatureFieldOptions {
    /// Remove specific signature field by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_name: Option<String>,
    /// Only remove unsigned signature fields (keep already-signed certificates). Default: true.
    #[serde(default = "default_true")]
    pub only_unsigned: bool,
    /// Remove all signature fields regardless of signature status.
    #[serde(default)]
    pub all: bool,
}

impl RemoveSignatureFieldOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn field_name(mut self, name: impl Into<String>) -> Self {
        self.field_name = Some(name.into());
        self
    }

    pub fn only_unsigned(mut self, only: bool) -> Self {
        self.only_unsigned = only;
        self
    }

    pub fn by_name(name: impl Into<String>) -> Self {
        Self {
            field_name: Some(name.into()),
            only_unsigned: false,
            all: false,
        }
    }

    pub fn all_unsigned() -> Self {
        Self {
            field_name: None,
            only_unsigned: true,
            all: false,
        }
    }

    pub fn all() -> Self {
        Self {
            field_name: None,
            only_unsigned: false,
            all: true,
        }
    }
}

// ---------------------------------------------------------------------------
// High-Level Functions
// ---------------------------------------------------------------------------

/// Adds a new signature field to PDF bytes and returns modified bytes.
pub fn add_signature_field(
    pdf_bytes: &[u8],
    options: &AddSignatureFieldOptions,
) -> Result<Vec<u8>, String> {
    let mut doc = Document::load_mem(pdf_bytes).map_err(|e| format!("Failed to parse PDF: {e}"))?;
    add_signature_field_to_doc(&mut doc, options)?;
    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| format!("Failed to save PDF: {e}"))?;
    Ok(out)
}

/// Removes signature fields from PDF bytes and returns modified bytes.
pub fn remove_signature_field(
    pdf_bytes: &[u8],
    options: &RemoveSignatureFieldOptions,
) -> Result<(Vec<u8>, usize), String> {
    let mut doc = Document::load_mem(pdf_bytes).map_err(|e| format!("Failed to parse PDF: {e}"))?;
    let count = remove_signature_field_from_doc(&mut doc, options)?;
    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| format!("Failed to save PDF: {e}"))?;
    Ok((out, count))
}

// ---------------------------------------------------------------------------
// Document In-Place AST Manipulation
// ---------------------------------------------------------------------------

/// Adds a signature field directly into a lopdf Document AST in-place according to ISO 32000.
pub fn add_signature_field_to_doc(
    doc: &mut Document,
    options: &AddSignatureFieldOptions,
) -> Result<ObjectId, String> {
    if options.field_name.trim().is_empty() {
        return Err("Signature field name cannot be empty".to_string());
    }

    // 1. Check for existing field name collision
    let fields = crate::collect_fields(doc);
    if fields.contains_key(&options.field_name) {
        return Err(format!(
            "A form field with name '{}' already exists in the document",
            options.field_name
        ));
    }

    let pages_map = doc.get_pages();
    let total_pages = pages_map.len() as u32;
    if total_pages == 0 {
        return Err("Document contains 0 pages".to_string());
    }

    // Determine target page
    let target_page_num = match options.page {
        Some(p) if p >= 1 && p <= total_pages => p,
        Some(0) => total_pages,
        None => total_pages,
        Some(p) => {
            return Err(format!(
                "Invalid page number {p}: document has {total_pages} pages"
            ));
        }
    };

    let target_page_id = *pages_map
        .get(&target_page_num)
        .ok_or_else(|| format!("Target page {target_page_num} not found"))?;

    // 2. Compute bounding rectangle
    let (llx, lly, width, height) = get_page_dimensions(doc, target_page_id)?;
    let rect = options.placement.compute_rect(llx, lly, width, height);

    // 3. Ensure Catalog has AcroForm with SigFlags = 3
    let acroform_id = ensure_acroform(doc)?;

    // 4. Create the Signature Field dictionary
    let mut field_dict = Dictionary::new();
    field_dict.set("Type", "Annot");
    field_dict.set("Subtype", "Widget");
    field_dict.set("FT", "Sig");
    field_dict.set("T", Object::string_literal(options.field_name.clone()));
    field_dict.set(
        "Rect",
        vec![
            Object::Real(rect[0] as f32),
            Object::Real(rect[1] as f32),
            Object::Real(rect[2] as f32),
            Object::Real(rect[3] as f32),
        ],
    );
    // Annotation Flags: 4 (Print) + 128 (Locked) = 132
    field_dict.set("F", Object::Integer(132));
    field_dict.set("P", Object::Reference(target_page_id));

    if options.lock_all_fields {
        let mut lock_dict = Dictionary::new();
        lock_dict.set("Type", "SigFieldLock");
        lock_dict.set("Action", "All");
        field_dict.set("Lock", Object::Dictionary(lock_dict));
    }

    let field_id = doc.add_object(field_dict);

    // 5. Link field into AcroForm /Fields
    let acroform = doc
        .get_object_mut(acroform_id)
        .map_err(|e| format!("Failed to get AcroForm object: {e}"))?
        .as_dict_mut()
        .map_err(|e| format!("AcroForm is not a dictionary: {e}"))?;

    match acroform.get_mut(b"Fields") {
        Ok(Object::Array(arr)) => {
            arr.push(Object::Reference(field_id));
        }
        _ => {
            acroform.set("Fields", Object::Array(vec![Object::Reference(field_id)]));
        }
    }

    // 6. Link field into target page /Annots
    let page = doc
        .get_object_mut(target_page_id)
        .map_err(|e| format!("Failed to get page object: {e}"))?
        .as_dict_mut()
        .map_err(|e| format!("Page object is not a dictionary: {e}"))?;

    match page.get_mut(b"Annots") {
        Ok(Object::Array(arr)) => {
            arr.push(Object::Reference(field_id));
        }
        Ok(Object::Reference(annots_id)) => {
            let annots_id_val = *annots_id;
            let annots_obj = doc
                .get_object_mut(annots_id_val)
                .map_err(|e| format!("Failed to get Annots reference: {e}"))?;
            if let Ok(arr) = annots_obj.as_array_mut() {
                arr.push(Object::Reference(field_id));
            }
        }
        _ => {
            page.set("Annots", Object::Array(vec![Object::Reference(field_id)]));
        }
    }

    Ok(field_id)
}

/// Removes signature fields from a Document AST according to options.
pub fn remove_signature_field_from_doc(
    doc: &mut Document,
    options: &RemoveSignatureFieldOptions,
) -> Result<usize, String> {
    let mut to_remove_ids = Vec::new();
    let mut sig_obj_ids = Vec::new();

    let fields = crate::collect_fields(doc);

    for (name, (id, field, ft)) in fields {
        if ft.as_slice() != b"Sig" {
            continue;
        }

        let is_signed = field.get(b"V").is_ok();

        let should_remove = if options.all {
            true
        } else if let Some(ref target_name) = options.field_name {
            name == *target_name && (!options.only_unsigned || !is_signed)
        } else if options.only_unsigned {
            !is_signed
        } else {
            false
        };

        if should_remove {
            to_remove_ids.push(id);
            if let Ok(v_ref) = field.get(b"V").and_then(|v| v.as_reference()) {
                sig_obj_ids.push(v_ref);
            }
        }
    }

    if to_remove_ids.is_empty() {
        return Ok(0);
    }

    let remove_set: std::collections::HashSet<ObjectId> = to_remove_ids.iter().copied().collect();

    // 1. Remove from AcroForm /Fields
    for obj in doc.objects.values_mut() {
        if let Ok(dict) = obj.as_dict_mut()
            && let Ok(fields_arr) = dict.get_mut(b"Fields").and_then(|f| f.as_array_mut())
        {
            fields_arr.retain(|item| match item.as_reference() {
                Ok(id) => !remove_set.contains(&id),
                _ => true,
            });
        }
    }

    // 2. Remove from Page /Annots
    for (_num, page_id) in doc.get_pages() {
        if let Ok(page) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut())
            && let Ok(annots) = page.get_mut(b"Annots").and_then(|a| a.as_array_mut())
        {
            annots.retain(|item| match item.as_reference() {
                Ok(id) => !remove_set.contains(&id),
                _ => true,
            });
        }
    }

    // 3. Remove field objects and associated signature value objects from Document
    for id in &to_remove_ids {
        doc.objects.remove(id);
    }
    for sig_id in &sig_obj_ids {
        doc.objects.remove(sig_id);
    }

    Ok(to_remove_ids.len())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Ensures Catalog has an AcroForm dictionary with SigFlags = 3 (SignaturesExist + AppendOnly).
fn ensure_acroform(doc: &mut Document) -> Result<ObjectId, String> {
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(|r| r.as_reference())
        .map_err(|_| "PDF trailer missing /Root Catalog reference".to_string())?;

    let catalog_acroform_ref = {
        let catalog = doc
            .get_object(catalog_id)
            .map_err(|e| format!("Failed to read Catalog: {e}"))?
            .as_dict()
            .map_err(|e| format!("Catalog is not a dictionary: {e}"))?;

        match catalog.get(b"AcroForm") {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        }
    };

    if let Some(acroform_id) = catalog_acroform_ref {
        // AcroForm already exists; update SigFlags
        let acroform = doc
            .get_object_mut(acroform_id)
            .map_err(|e| format!("Failed to get AcroForm object: {e}"))?
            .as_dict_mut()
            .map_err(|e| format!("AcroForm is not a dictionary: {e}"))?;

        let current_sig_flags = acroform
            .get(b"SigFlags")
            .ok()
            .and_then(|f| f.as_i64().ok())
            .unwrap_or(0);
        acroform.set("SigFlags", Object::Integer(current_sig_flags | 3));
        return Ok(acroform_id);
    }

    // Create new AcroForm object
    let new_acroform_id = doc.add_object(dictionary! {
        "Fields" => Object::Array(Vec::new()),
        "SigFlags" => Object::Integer(3),
    });

    let catalog = doc
        .get_object_mut(catalog_id)
        .map_err(|e| format!("Failed to get Catalog: {e}"))?
        .as_dict_mut()
        .map_err(|e| format!("Catalog is not a dictionary: {e}"))?;
    catalog.set("AcroForm", Object::Reference(new_acroform_id));

    Ok(new_acroform_id)
}

/// Reads page dimensions (llx, lly, width, height) from MediaBox or CropBox.
fn get_page_dimensions(doc: &Document, page_id: ObjectId) -> Result<(f64, f64, f64, f64), String> {
    let page_obj = doc
        .get_object(page_id)
        .map_err(|e| format!("Failed to get page {page_id:?}: {e}"))?;
    let page_dict = page_obj
        .as_dict()
        .map_err(|e| format!("Page is not a dictionary: {e}"))?;

    let box_obj = page_dict
        .get(b"CropBox")
        .ok()
        .or_else(|| page_dict.get(b"MediaBox").ok());

    if let Some(Object::Array(arr)) = box_obj
        && arr.len() >= 4
    {
        let parse_num = |obj: &Object| -> f64 {
            match obj {
                Object::Integer(i) => *i as f64,
                Object::Real(r) => *r as f64,
                _ => 0.0,
            }
        };
        let x1 = parse_num(&arr[0]);
        let y1 = parse_num(&arr[1]);
        let x2 = parse_num(&arr[2]);
        let y2 = parse_num(&arr[3]);
        let llx = x1.min(x2);
        let lly = y1.min(y2);
        let width = (x2 - x1).abs();
        let height = (y2 - y1).abs();
        return Ok((llx, lly, width, height));
    }

    // Default standard A4
    Ok((0.0, 0.0, 595.28, 841.89))
}
