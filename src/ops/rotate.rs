use crate::{flatten_form_fields, insert_locked_piece_info, insert_piece_info, ops::PageSelection};
use lopdf::content::Content;
use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

fn default_clockwise() -> RotationDirection {
    RotationDirection::Clockwise
}

/// Target orientation for normalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetOrientation {
    Portrait,
    Landscape,
}

impl TargetOrientation {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "portrait" | "port" | "p" => Some(Self::Portrait),
            "landscape" | "land" | "l" => Some(Self::Landscape),
            _ => None,
        }
    }
}

/// Rotation direction when rotating a page to match target orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RotationDirection {
    Clockwise,
    CounterClockwise,
}

impl RotationDirection {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "ccw" | "counter-clockwise" | "counterclockwise" | "left" | "-90" => {
                Self::CounterClockwise
            }
            _ => Self::Clockwise,
        }
    }
}

/// Operating mode for rotating PDF pages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RotationMode {
    /// Rotate by explicit angle in degrees (e.g. 90, 180, 270, -90).
    Angle {
        degrees: i32,
        #[serde(default = "default_true")]
        relative: bool,
    },
    /// Normalize pages to target orientation (Portrait or Landscape).
    ToOrientation {
        target: TargetOrientation,
        #[serde(default = "default_clockwise")]
        direction: RotationDirection,
    },
    /// Smart detection: inspect text transformation matrix (Tm, cm) in content stream
    /// to determine orientation and rotate page so text is upright (0° horizontal).
    AutoDetectText {
        #[serde(default)]
        fallback_angle: Option<i32>,
    },
}

impl Default for RotationMode {
    fn default() -> Self {
        Self::Angle {
            degrees: 90,
            relative: true,
        }
    }
}

/// Comprehensive options for rotating and normalizing PDF pages.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RotateOptions {
    /// Rotation mode (Angle, ToOrientation, or AutoDetectText).
    pub mode: RotationMode,
    /// Target pages to apply rotation to. Default: PageSelection::All.
    #[serde(default)]
    pub pages: PageSelection,
    /// Flatten form fields after rotation. Default: false.
    #[serde(default)]
    pub flatten: bool,
    /// Optional ISO 32000-1 /PieceInfo metadata JSON to embed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub piece_info: Option<serde_json::Value>,
    /// Optional cryptographic locked PieceInfo configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked_piece_info: Option<crate::LockedPieceInfoConfig>,
}

impl RotateOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rotate by relative angle (e.g. 90, 180, -90).
    pub fn angle(mut self, degrees: i32) -> Self {
        self.mode = RotationMode::Angle {
            degrees,
            relative: true,
        };
        self
    }

    /// Set absolute rotation angle (e.g. exactly 0, 90, 180, 270).
    pub fn angle_absolute(mut self, degrees: i32) -> Self {
        self.mode = RotationMode::Angle {
            degrees,
            relative: false,
        };
        self
    }

    /// Normalize target pages to Portrait orientation.
    pub fn to_portrait(mut self) -> Self {
        self.mode = RotationMode::ToOrientation {
            target: TargetOrientation::Portrait,
            direction: RotationDirection::Clockwise,
        };
        self
    }

    /// Normalize target pages to Landscape orientation.
    pub fn to_landscape(mut self) -> Self {
        self.mode = RotationMode::ToOrientation {
            target: TargetOrientation::Landscape,
            direction: RotationDirection::Clockwise,
        };
        self
    }

    /// Normalize to target orientation with explicit rotation direction.
    pub fn to_orientation(
        mut self,
        target: TargetOrientation,
        direction: RotationDirection,
    ) -> Self {
        self.mode = RotationMode::ToOrientation { target, direction };
        self
    }

    /// Automatically inspect content stream text matrix to make text upright.
    pub fn auto_detect_text(mut self) -> Self {
        self.mode = RotationMode::AutoDetectText {
            fallback_angle: None,
        };
        self
    }

    /// Automatically inspect text with fallback angle if page has no text content.
    pub fn auto_detect_text_with_fallback(mut self, fallback_angle: i32) -> Self {
        self.mode = RotationMode::AutoDetectText {
            fallback_angle: Some(fallback_angle),
        };
        self
    }

    /// Set page selection.
    pub fn pages(mut self, pages: PageSelection) -> Self {
        self.pages = pages;
        self
    }

    /// Flatten form fields.
    pub fn flatten(mut self, flatten: bool) -> Self {
        self.flatten = flatten;
        self
    }

    /// Embed custom PieceInfo metadata JSON.
    pub fn piece_info(mut self, piece_info: serde_json::Value) -> Self {
        self.piece_info = Some(piece_info);
        self
    }

    /// Embed stealth encrypted PieceInfo metadata.
    pub fn locked_piece_info(
        mut self,
        app_name: impl Into<String>,
        data: serde_json::Value,
        secret_key: impl Into<String>,
    ) -> Self {
        self.locked_piece_info = Some(crate::LockedPieceInfoConfig {
            app_name: app_name.into(),
            data,
            secret_key: secret_key.into(),
        });
        self
    }
}

/// Detailed per-page rotation execution report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRotationDetail {
    pub page: u32,
    pub original_rotation: i32,
    pub new_rotation: i32,
    pub detected_text_angle: Option<i32>,
    pub character_count: usize,
    pub modified: bool,
}

/// Overall report returned by page rotation execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRotationReport {
    pub total_pages: u32,
    pub rotated_pages: u32,
    pub pages_details: Vec<PageRotationDetail>,
}

// ---------------------------------------------------------------------------
// High-Level Functions
// ---------------------------------------------------------------------------

/// Rotates pages in a PDF byte buffer according to the given options.
pub fn rotate_pdf_pages(
    pdf_bytes: &[u8],
    options: &RotateOptions,
) -> Result<(Vec<u8>, PageRotationReport), String> {
    let mut doc = Document::load_mem(pdf_bytes).map_err(|e| format!("Failed to parse PDF: {e}"))?;
    let report = rotate_pdf_pages_in_doc(&mut doc, options)?;
    let mut out_bytes = Vec::new();
    doc.save_to(&mut out_bytes)
        .map_err(|e| format!("Failed to serialize rotated PDF: {e}"))?;
    Ok((out_bytes, report))
}

/// Rotates pages directly in an existing lopdf Document.
pub fn rotate_pdf_pages_in_doc(
    doc: &mut Document,
    options: &RotateOptions,
) -> Result<PageRotationReport, String> {
    let page_map = doc.get_pages();
    let total_pages = page_map.len() as u32;

    let mut pages_details = Vec::with_capacity(page_map.len());
    let mut rotated_pages = 0u32;

    for (&page_num, &page_id) in &page_map {
        let is_selected = options.pages.matches(page_num, total_pages);
        let orig_rot = get_page_rotation(doc, page_id);

        if !is_selected {
            pages_details.push(PageRotationDetail {
                page: page_num,
                original_rotation: orig_rot,
                new_rotation: orig_rot,
                detected_text_angle: None,
                character_count: 0,
                modified: false,
            });
            continue;
        }

        let mut detected_text_angle = None;
        let mut char_count = 0usize;

        let new_rot = match &options.mode {
            RotationMode::Angle { degrees, relative } => {
                let norm = normalize_degrees(*degrees);
                if *relative {
                    (orig_rot + norm) % 360
                } else {
                    norm
                }
            }

            RotationMode::ToOrientation { target, direction } => {
                let bbox = get_page_box(doc, page_id);
                let raw_w = (bbox[2] - bbox[0]).abs();
                let raw_h = (bbox[3] - bbox[1]).abs();

                // Compute effective visual dimensions considering current rotation
                let (visual_w, visual_h) = if orig_rot == 90 || orig_rot == 270 {
                    (raw_h, raw_w)
                } else {
                    (raw_w, raw_h)
                };

                let is_matching = match target {
                    TargetOrientation::Portrait => visual_h >= visual_w,
                    TargetOrientation::Landscape => visual_w > visual_h,
                };

                if is_matching {
                    orig_rot
                } else {
                    match direction {
                        RotationDirection::Clockwise => (orig_rot + 90) % 360,
                        RotationDirection::CounterClockwise => (orig_rot + 270) % 360,
                    }
                }
            }

            RotationMode::AutoDetectText { fallback_angle } => {
                let (detected_angle, count) = detect_page_text_orientation(doc, page_id);
                detected_text_angle = detected_angle;
                char_count = count;

                if let Some(angle) = detected_angle {
                    // In PDF viewer, rotating clockwise by `target_R` maps text at angle `angle` to horizontal 0°.
                    // Thus target /Rotate = angle.
                    normalize_degrees(angle)
                } else if let Some(fb) = fallback_angle {
                    (orig_rot + normalize_degrees(*fb)) % 360
                } else {
                    orig_rot
                }
            }
        };

        let modified = new_rot != orig_rot;
        if modified {
            if let Ok(page_dict) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
                page_dict.set("Rotate", Object::Integer(new_rot as i64));
            }
            rotated_pages += 1;
        }

        pages_details.push(PageRotationDetail {
            page: page_num,
            original_rotation: orig_rot,
            new_rotation: new_rot,
            detected_text_angle,
            character_count: char_count,
            modified,
        });
    }

    if options.flatten {
        flatten_form_fields(doc, true)?;
    }

    if let Some(pi) = &options.piece_info {
        insert_piece_info(doc, pi)?;
    }

    if let Some(lpi) = &options.locked_piece_info {
        insert_locked_piece_info(doc, &lpi.app_name, &lpi.data, &lpi.secret_key)?;
    }

    Ok(PageRotationReport {
        total_pages,
        rotated_pages,
        pages_details,
    })
}

// ---------------------------------------------------------------------------
// Helpers: Page Attributes and Geometry
// ---------------------------------------------------------------------------

/// Normalizes an angle in degrees to a valid PDF rotation multiple of 90: 0, 90, 180, 270.
pub fn normalize_degrees(deg: i32) -> i32 {
    let rem = deg.rem_euclid(360);
    match rem {
        0..=44 | 316..=360 => 0,
        45..=134 => 90,
        135..=224 => 180,
        225..=315 => 270,
        _ => 0,
    }
}

/// Retrieves the effective rotation of a page, traversing parent dictionaries if inherited.
pub fn get_page_rotation(doc: &Document, page_id: ObjectId) -> i32 {
    let mut current_id = page_id;
    for _ in 0..20 {
        if let Ok(obj) = doc.get_object(current_id)
            && let Ok(dict) = obj.as_dict()
        {
            if let Ok(rot_obj) = dict.get(b"Rotate")
                && let Ok(deg) = rot_obj.as_i64()
            {
                return normalize_degrees(deg as i32);
            }
            if let Ok(parent_ref) = dict.get(b"Parent").and_then(|p| p.as_reference()) {
                current_id = parent_ref;
                continue;
            }
        }
        break;
    }
    0
}

/// Retrieves the effective CropBox or MediaBox of a page [llx, lly, urx, ury].
pub fn get_page_box(doc: &Document, page_id: ObjectId) -> [f64; 4] {
    let mut current_id = page_id;
    for _ in 0..20 {
        if let Ok(obj) = doc.get_object(current_id)
            && let Ok(dict) = obj.as_dict()
        {
            let box_obj = dict.get(b"CropBox").or_else(|_| dict.get(b"MediaBox"));
            if let Ok(Object::Array(arr)) = box_obj
                && arr.len() == 4
            {
                let nums: Vec<f64> = arr
                    .iter()
                    .filter_map(|v| v.as_float().ok().map(|x| x as f64))
                    .collect();
                if nums.len() == 4 {
                    return [nums[0], nums[1], nums[2], nums[3]];
                }
            }
            if let Ok(parent_ref) = dict.get(b"Parent").and_then(|p| p.as_reference()) {
                current_id = parent_ref;
                continue;
            }
        }
        break;
    }
    [0.0, 0.0, 612.0, 792.0]
}

// ---------------------------------------------------------------------------
// Text Orientation Detection via Content Stream Transformation Matrices
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Matrix {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Matrix {
    fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    fn multiply(&self, o: &Matrix) -> Matrix {
        Matrix {
            a: self.a * o.a + self.b * o.c,
            b: self.a * o.b + self.b * o.d,
            c: self.c * o.a + self.d * o.c,
            d: self.c * o.b + self.d * o.d,
            e: self.e * o.a + self.f * o.c + o.e,
            f: self.e * o.b + self.f * o.d + o.f,
        }
    }
}

fn parse_matrix_operands(ops: &[Object]) -> Option<Matrix> {
    if ops.len() < 6 {
        return None;
    }
    let a = ops[0].as_float().ok()? as f64;
    let b = ops[1].as_float().ok()? as f64;
    let c = ops[2].as_float().ok()? as f64;
    let d = ops[3].as_float().ok()? as f64;
    let e = ops[4].as_float().ok()? as f64;
    let f = ops[5].as_float().ok()? as f64;
    Some(Matrix { a, b, c, d, e, f })
}

fn get_string_len(obj: &Object) -> usize {
    match obj {
        Object::String(bytes, _) => bytes.len(),
        _ => 0,
    }
}

fn record_text_angle(
    ctm: &Matrix,
    tm: &Matrix,
    len: usize,
    counts: &mut [usize; 4],
    total_chars: &mut usize,
) {
    let m = tm.multiply(ctm);
    let rad = m.b.atan2(m.a);
    let mut deg = rad.to_degrees();
    deg = ((deg % 360.0) + 360.0) % 360.0;

    let bucket = if deg >= 315.0 || deg < 45.0 {
        0 // 0 deg
    } else if (45.0..135.0).contains(&deg) {
        1 // 90 deg
    } else if (135.0..225.0).contains(&deg) {
        2 // 180 deg
    } else {
        3 // 270 deg
    };

    counts[bucket] += len;
    *total_chars += len;
}

/// Analyzes the content stream of a page to determine the primary orientation of rendered text.
pub fn detect_page_text_orientation(doc: &Document, page_id: ObjectId) -> (Option<i32>, usize) {
    let content_bytes = doc.get_page_content(page_id);
    if content_bytes.is_empty() {
        return (None, 0);
    }

    let content = match Content::decode(&content_bytes) {
        Ok(c) => c,
        Err(_) => return (None, 0),
    };

    let mut ctm = Matrix::identity();
    let mut graphics_stack: Vec<Matrix> = Vec::new();
    let mut tm = Matrix::identity();

    let mut counts = [0usize; 4]; // [0 deg, 90 deg, 180 deg, 270 deg]
    let mut total_chars = 0usize;

    for op in &content.operations {
        match op.operator.as_str() {
            "q" => {
                graphics_stack.push(ctm.clone());
            }
            "Q" => {
                ctm = graphics_stack.pop().unwrap_or_else(Matrix::identity);
            }
            "cm" => {
                if let Some(m) = parse_matrix_operands(&op.operands) {
                    ctm = m.multiply(&ctm);
                }
            }
            "BT" => {
                tm = Matrix::identity();
            }
            "Tm" => {
                if let Some(m) = parse_matrix_operands(&op.operands) {
                    tm = m;
                }
            }
            "Td" | "TD" => {
                if op.operands.len() >= 2
                    && let (Ok(tx), Ok(ty)) = (op.operands[0].as_float(), op.operands[1].as_float())
                {
                    let offset = Matrix {
                        a: 1.0,
                        b: 0.0,
                        c: 0.0,
                        d: 1.0,
                        e: tx as f64,
                        f: ty as f64,
                    };
                    tm = offset.multiply(&tm);
                }
            }
            "Tj" => {
                if let Some(s) = op.operands.first() {
                    let len = get_string_len(s);
                    if len > 0 {
                        record_text_angle(&ctm, &tm, len, &mut counts, &mut total_chars);
                    }
                }
            }
            "TJ" => {
                if let Some(Object::Array(arr)) = op.operands.first() {
                    let mut len = 0;
                    for item in arr {
                        len += get_string_len(item);
                    }
                    if len > 0 {
                        record_text_angle(&ctm, &tm, len, &mut counts, &mut total_chars);
                    }
                }
            }
            "'" => {
                if let Some(s) = op.operands.first() {
                    let len = get_string_len(s);
                    if len > 0 {
                        record_text_angle(&ctm, &tm, len, &mut counts, &mut total_chars);
                    }
                }
            }
            "\"" => {
                if let Some(s) = op.operands.get(2) {
                    let len = get_string_len(s);
                    if len > 0 {
                        record_text_angle(&ctm, &tm, len, &mut counts, &mut total_chars);
                    }
                }
            }
            _ => {}
        }
    }

    if total_chars == 0 {
        return (None, 0);
    }

    let mut best_idx = 0;
    let mut best_count = counts[0];
    for (i, &count) in counts.iter().enumerate().skip(1) {
        if count > best_count {
            best_count = count;
            best_idx = i;
        }
    }

    let detected_deg = match best_idx {
        0 => 0,
        1 => 90,
        2 => 180,
        3 => 270,
        _ => 0,
    };

    (Some(detected_deg), total_chars)
}
