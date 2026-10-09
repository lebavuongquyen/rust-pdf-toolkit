use crate::{
    LockedPieceInfoConfig, flatten_form_fields, insert_locked_piece_info, insert_piece_info,
    ops::PageSelection,
};
use lopdf::content::Content;
use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

fn default_crop_padding() -> f64 {
    18.0 // 0.25 inch default padding in points
}

/// Target box dictionary key to apply crop boundaries to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetBox {
    #[default]
    CropBox,
    MediaBox,
    TrimBox,
    BleedBox,
    AllBoxes,
}

impl TargetBox {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "media" | "mediabox" => Self::MediaBox,
            "trim" | "trimbox" => Self::TrimBox,
            "bleed" | "bleedbox" => Self::BleedBox,
            "all" | "allboxes" => Self::AllBoxes,
            _ => Self::CropBox,
        }
    }
}

/// Operating mode for cropping PDF pages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CropMode {
    /// Crop margins inward from existing page boundary.
    Margins {
        #[serde(default)]
        top: f64,
        #[serde(default)]
        bottom: f64,
        #[serde(default)]
        left: f64,
        #[serde(default)]
        right: f64,
        /// If true, margins are relative fractions (0.0 to 1.0) of page width / height.
        /// If false, margins are in PDF points (1/72 inch). Default: false.
        #[serde(default)]
        relative: bool,
    },
    /// Set explicit bounding box [llx, lly, urx, ury] in PDF points.
    Box { rect: [f64; 4] },
    /// Automatically inspect content stream (text, drawings, images)
    /// to determine tight content bounding box and crop with padding.
    AutoDetectContent {
        #[serde(default = "default_crop_padding")]
        padding: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fallback_rect: Option<[f64; 4]>,
    },
}

impl Default for CropMode {
    fn default() -> Self {
        Self::Margins {
            top: 0.0,
            bottom: 0.0,
            left: 0.0,
            right: 0.0,
            relative: false,
        }
    }
}

/// Comprehensive options for cropping PDF pages.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CropOptions {
    /// Cropping mode (Margins, Box, or AutoDetectContent).
    pub mode: CropMode,
    /// Target box to modify. Default: CropBox.
    #[serde(default)]
    pub target_box: TargetBox,
    /// Page selection to apply crop to. Default: PageSelection::All.
    #[serde(default)]
    pub pages: PageSelection,
    /// Whether to clamp the resulting crop box inside the page's MediaBox. Default: true.
    #[serde(default = "default_true")]
    pub clamp_to_media_box: bool,
    /// Flatten form fields after cropping. Default: false.
    #[serde(default)]
    pub flatten: bool,
    /// Optional ISO 32000-1 /PieceInfo metadata JSON to embed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub piece_info: Option<serde_json::Value>,
    /// Optional cryptographic locked PieceInfo configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked_piece_info: Option<LockedPieceInfoConfig>,
}

impl CropOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Crop margins inward by points (top, bottom, left, right).
    pub fn margins(mut self, top: f64, bottom: f64, left: f64, right: f64) -> Self {
        self.mode = CropMode::Margins {
            top,
            bottom,
            left,
            right,
            relative: false,
        };
        self
    }

    /// Crop uniform margin inward on all four sides in points.
    pub fn margins_uniform(mut self, margin: f64) -> Self {
        self.mode = CropMode::Margins {
            top: margin,
            bottom: margin,
            left: margin,
            right: margin,
            relative: false,
        };
        self
    }

    /// Crop margins inward by relative fraction (0.0 to 1.0) of page dimensions.
    pub fn margins_relative(mut self, top: f64, bottom: f64, left: f64, right: f64) -> Self {
        self.mode = CropMode::Margins {
            top,
            bottom,
            left,
            right,
            relative: true,
        };
        self
    }

    /// Set an explicit crop box rectangle [llx, lly, urx, ury] in PDF points.
    pub fn crop_box(mut self, rect: [f64; 4]) -> Self {
        self.mode = CropMode::Box { rect };
        self
    }

    /// Automatically crop around content bounding box with padding in points.
    pub fn auto_detect_content(mut self, padding: f64) -> Self {
        self.mode = CropMode::AutoDetectContent {
            padding,
            fallback_rect: None,
        };
        self
    }

    /// Automatically crop around content bounding box with fallback rect if content is empty.
    pub fn auto_detect_content_with_fallback(
        mut self,
        padding: f64,
        fallback_rect: [f64; 4],
    ) -> Self {
        self.mode = CropMode::AutoDetectContent {
            padding,
            fallback_rect: Some(fallback_rect),
        };
        self
    }

    /// Set which PDF box dictionary entry is modified (CropBox, MediaBox, TrimBox, BleedBox, AllBoxes).
    pub fn target_box(mut self, target: TargetBox) -> Self {
        self.target_box = target;
        self
    }

    /// Apply crop to specific pages using `PageSelection`.
    pub fn pages(mut self, pages: PageSelection) -> Self {
        self.pages = pages;
        self
    }

    /// Apply crop to specific pages via string (e.g. "1-3", "odd", "even", "all").
    pub fn pages_str(mut self, s: &str) -> Self {
        self.pages = PageSelection::parse(s);
        self
    }

    /// Whether to clamp resulting crop box within MediaBox boundaries.
    pub fn clamp_to_media_box(mut self, clamp: bool) -> Self {
        self.clamp_to_media_box = clamp;
        self
    }

    /// Flatten form fields after cropping.
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
        self.locked_piece_info = Some(LockedPieceInfoConfig {
            app_name: app_name.into(),
            data,
            secret_key: secret_key.into(),
        });
        self
    }
}

/// Execution detail for an individual page crop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageCropDetail {
    pub page: u32,
    pub original_box: [f64; 4],
    pub new_box: [f64; 4],
    pub detected_content_bbox: Option<[f64; 4]>,
    pub modified: bool,
}

/// Summary report returned after cropping PDF pages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageCropReport {
    pub total_pages: u32,
    pub cropped_pages: u32,
    pub pages_details: Vec<PageCropDetail>,
}

// ---------------------------------------------------------------------------
// 2D Transformation Matrix & Content Bounding Box Detection
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

    fn transform_point(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
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

/// Automatically scans content stream operators (paths, text, XObjects)
/// to detect the tightest bounding box enclosing all visual content.
pub fn detect_page_content_bbox(doc: &Document, page_id: ObjectId) -> Option<[f64; 4]> {
    let content_bytes = doc.get_page_content(page_id);
    if content_bytes.is_empty() {
        return None;
    }

    let content = Content::decode(&content_bytes).ok()?;

    let mut ctm = Matrix::identity();
    let mut graphics_stack: Vec<Matrix> = Vec::new();
    let mut tm = Matrix::identity();
    let mut tlm = Matrix::identity();
    let mut current_font_size = 12.0f64;

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut found_content = false;

    let mut update_box = |px: f64, py: f64| {
        if px.is_finite() && py.is_finite() {
            min_x = min_x.min(px);
            min_y = min_y.min(py);
            max_x = max_x.max(px);
            max_y = max_y.max(py);
            found_content = true;
        }
    };

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
            // Rectangle operator: x y w h re
            "re" => {
                if op.operands.len() >= 4
                    && let (Ok(x), Ok(y), Ok(w), Ok(h)) = (
                        op.operands[0].as_float(),
                        op.operands[1].as_float(),
                        op.operands[2].as_float(),
                        op.operands[3].as_float(),
                    )
                {
                    let (x, y, w, h) = (x as f64, y as f64, w as f64, h as f64);
                    for &(px, py) in &[(x, y), (x + w, y), (x + w, y + h), (x, y + h)] {
                        let (tx, ty) = ctm.transform_point(px, py);
                        update_box(tx, ty);
                    }
                }
            }
            // Path move-to / line-to operators: m, l
            "m" | "l" => {
                if op.operands.len() >= 2
                    && let (Ok(x), Ok(y)) = (op.operands[0].as_float(), op.operands[1].as_float())
                {
                    let (tx, ty) = ctm.transform_point(x as f64, y as f64);
                    update_box(tx, ty);
                }
            }
            // Cubic bezier operators: c, v, y
            "c" => {
                if op.operands.len() >= 6 {
                    for i in 0..3 {
                        if let (Ok(x), Ok(y)) = (
                            op.operands[i * 2].as_float(),
                            op.operands[i * 2 + 1].as_float(),
                        ) {
                            let (tx, ty) = ctm.transform_point(x as f64, y as f64);
                            update_box(tx, ty);
                        }
                    }
                }
            }
            "v" | "y" => {
                if op.operands.len() >= 4 {
                    for i in 0..2 {
                        if let (Ok(x), Ok(y)) = (
                            op.operands[i * 2].as_float(),
                            op.operands[i * 2 + 1].as_float(),
                        ) {
                            let (tx, ty) = ctm.transform_point(x as f64, y as f64);
                            update_box(tx, ty);
                        }
                    }
                }
            }
            // Font setting: /FontName size Tf
            "Tf" => {
                if op.operands.len() >= 2
                    && let Ok(sz) = op.operands[1].as_float()
                {
                    current_font_size = (sz as f64).abs().max(1.0);
                }
            }
            // Text matrix operators
            "BT" => {
                tm = Matrix::identity();
                tlm = Matrix::identity();
            }
            "Tm" => {
                if let Some(m) = parse_matrix_operands(&op.operands) {
                    tm = m.clone();
                    tlm = m;
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
                    tlm = offset.multiply(&tlm);
                    tm = tlm.clone();
                }
            }
            "T*" => {
                let offset = Matrix {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: 1.0,
                    e: 0.0,
                    f: -current_font_size,
                };
                tlm = offset.multiply(&tlm);
                tm = tlm.clone();
            }
            // Text rendering operators: Tj, TJ, ', "
            "Tj" | "'" | "\"" => {
                let text_bytes_len = match op.operator.as_str() {
                    "\"" => op
                        .operands
                        .get(2)
                        .and_then(|o| o.as_str().ok().map(|s| s.len()))
                        .unwrap_or(0),
                    _ => op
                        .operands
                        .first()
                        .and_then(|o| o.as_str().ok().map(|s| s.len()))
                        .unwrap_or(0),
                };

                let m = tm.multiply(&ctm);
                let (x1, y1) = (m.e, m.f);
                update_box(x1, y1);

                // Approximate advance box based on font size and character count
                let approx_width = (text_bytes_len as f64 * current_font_size * 0.55).max(1.0);
                let (x2, y2) = m.transform_point(approx_width, current_font_size);
                update_box(x2, y2);
            }
            "TJ" => {
                let mut text_len = 0usize;
                if let Some(Object::Array(arr)) = op.operands.first() {
                    for item in arr {
                        if let Ok(s) = item.as_str() {
                            text_len += s.len();
                        }
                    }
                }
                let m = tm.multiply(&ctm);
                let (x1, y1) = (m.e, m.f);
                update_box(x1, y1);

                let approx_width = (text_len as f64 * current_font_size * 0.55).max(1.0);
                let (x2, y2) = m.transform_point(approx_width, current_font_size);
                update_box(x2, y2);
            }
            // Form XObject or Image: Do /Name
            "Do" => {
                // In PDF, an image/XObject fills unit square [0,0] to [1,1] under CTM
                for &(px, py) in &[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
                    let (tx, ty) = ctm.transform_point(px, py);
                    update_box(tx, ty);
                }
            }
            _ => {}
        }
    }

    if found_content && min_x < max_x && min_y < max_y {
        Some([min_x, min_y, max_x, max_y])
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Helpers: Box Extraction & Clamping
// ---------------------------------------------------------------------------

/// Retrieves the MediaBox of a page, traversing parent dictionaries if necessary.
pub fn get_page_mediabox(doc: &Document, page_id: ObjectId) -> [f64; 4] {
    let mut current_id = page_id;
    for _ in 0..20 {
        if let Ok(obj) = doc.get_object(current_id)
            && let Ok(dict) = obj.as_dict()
        {
            if let Ok(Object::Array(arr)) = dict.get(b"MediaBox")
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

/// Retrieves the current TargetBox of a page, falling back to MediaBox if not explicitly set.
pub fn get_page_target_box(doc: &Document, page_id: ObjectId, target: TargetBox) -> [f64; 4] {
    let key = match target {
        TargetBox::CropBox | TargetBox::AllBoxes => b"CropBox".as_slice(),
        TargetBox::MediaBox => b"MediaBox".as_slice(),
        TargetBox::TrimBox => b"TrimBox".as_slice(),
        TargetBox::BleedBox => b"BleedBox".as_slice(),
    };

    let mut current_id = page_id;
    for _ in 0..20 {
        if let Ok(obj) = doc.get_object(current_id)
            && let Ok(dict) = obj.as_dict()
        {
            if let Ok(Object::Array(arr)) = dict.get(key)
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

    get_page_mediabox(doc, page_id)
}

fn make_box_array(rect: [f64; 4]) -> Object {
    Object::Array(vec![
        Object::Real(rect[0] as f32),
        Object::Real(rect[1] as f32),
        Object::Real(rect[2] as f32),
        Object::Real(rect[3] as f32),
    ])
}

// ---------------------------------------------------------------------------
// High-Level Functions
// ---------------------------------------------------------------------------

/// Crops pages in a PDF byte buffer according to the given options.
pub fn crop_pdf_pages(
    pdf_bytes: &[u8],
    options: &CropOptions,
) -> Result<(Vec<u8>, PageCropReport), String> {
    let mut doc = Document::load_mem(pdf_bytes).map_err(|e| format!("Failed to parse PDF: {e}"))?;
    let report = crop_pdf_pages_in_doc(&mut doc, options)?;
    let mut out_bytes = Vec::new();
    doc.save_to(&mut out_bytes)
        .map_err(|e| format!("Failed to serialize cropped PDF: {e}"))?;
    Ok((out_bytes, report))
}

/// Crops pages directly in an existing lopdf Document.
pub fn crop_pdf_pages_in_doc(
    doc: &mut Document,
    options: &CropOptions,
) -> Result<PageCropReport, String> {
    let page_map = doc.get_pages();
    let total_pages = page_map.len() as u32;

    let mut pages_details = Vec::with_capacity(page_map.len());
    let mut cropped_pages = 0u32;

    for (&page_num, &page_id) in &page_map {
        let is_selected = options.pages.matches(page_num, total_pages);
        let media_box = get_page_mediabox(doc, page_id);
        let orig_box = get_page_target_box(doc, page_id, options.target_box);

        if !is_selected {
            pages_details.push(PageCropDetail {
                page: page_num,
                original_box: orig_box,
                new_box: orig_box,
                detected_content_bbox: None,
                modified: false,
            });
            continue;
        }

        let mut detected_bbox = None;

        let raw_new_box = match &options.mode {
            CropMode::Margins {
                top,
                bottom,
                left,
                right,
                relative,
            } => {
                let orig_w = (orig_box[2] - orig_box[0]).abs();
                let orig_h = (orig_box[3] - orig_box[1]).abs();

                let (dl, dr, db, dt) = if *relative {
                    (orig_w * left, orig_w * right, orig_h * bottom, orig_h * top)
                } else {
                    (*left, *right, *bottom, *top)
                };

                let new_llx = orig_box[0].min(orig_box[2]) + dl;
                let new_lly = orig_box[1].min(orig_box[3]) + db;
                let new_urx = orig_box[0].max(orig_box[2]) - dr;
                let new_ury = orig_box[1].max(orig_box[3]) - dt;

                [new_llx, new_lly, new_urx, new_ury]
            }

            CropMode::Box { rect } => *rect,

            CropMode::AutoDetectContent {
                padding,
                fallback_rect,
            } => {
                let detected = detect_page_content_bbox(doc, page_id);
                detected_bbox = detected;

                if let Some(c_box) = detected {
                    [
                        c_box[0] - padding,
                        c_box[1] - padding,
                        c_box[2] + padding,
                        c_box[3] + padding,
                    ]
                } else if let Some(fb) = fallback_rect {
                    *fb
                } else {
                    orig_box
                }
            }
        };

        // Normalize coordinates so llx < urx and lly < ury
        let mut final_llx = raw_new_box[0].min(raw_new_box[2]);
        let mut final_lly = raw_new_box[1].min(raw_new_box[3]);
        let mut final_urx = raw_new_box[0].max(raw_new_box[2]);
        let mut final_ury = raw_new_box[1].max(raw_new_box[3]);

        // Clamp to MediaBox if requested
        if options.clamp_to_media_box {
            let m_llx = media_box[0].min(media_box[2]);
            let m_lly = media_box[1].min(media_box[3]);
            let m_urx = media_box[0].max(media_box[2]);
            let m_ury = media_box[1].max(media_box[3]);

            final_llx = final_llx.clamp(m_llx, m_urx);
            final_urx = final_urx.clamp(m_llx, m_urx);
            final_lly = final_lly.clamp(m_lly, m_ury);
            final_ury = final_ury.clamp(m_lly, m_ury);
        }

        // Ensure minimum 1.0 point dimension to prevent inverted / collapsed box
        if final_urx <= final_llx {
            final_urx = (final_llx + 1.0).min(media_box[2]);
        }
        if final_ury <= final_lly {
            final_ury = (final_lly + 1.0).min(media_box[3]);
        }

        let new_box = [final_llx, final_lly, final_urx, final_ury];
        let modified = (new_box[0] - orig_box[0]).abs() > 0.001
            || (new_box[1] - orig_box[1]).abs() > 0.001
            || (new_box[2] - orig_box[2]).abs() > 0.001
            || (new_box[3] - orig_box[3]).abs() > 0.001;

        if modified {
            if let Ok(page_dict) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
                let box_obj = make_box_array(new_box);
                match options.target_box {
                    TargetBox::CropBox => {
                        page_dict.set("CropBox", box_obj);
                    }
                    TargetBox::MediaBox => {
                        page_dict.set("MediaBox", box_obj);
                    }
                    TargetBox::TrimBox => {
                        page_dict.set("TrimBox", box_obj);
                    }
                    TargetBox::BleedBox => {
                        page_dict.set("BleedBox", box_obj);
                    }
                    TargetBox::AllBoxes => {
                        page_dict.set("CropBox", box_obj.clone());
                        page_dict.set("MediaBox", box_obj.clone());
                        page_dict.set("TrimBox", box_obj.clone());
                        page_dict.set("BleedBox", box_obj);
                    }
                }
            }
            cropped_pages += 1;
        }

        pages_details.push(PageCropDetail {
            page: page_num,
            original_box: orig_box,
            new_box,
            detected_content_bbox: detected_bbox,
            modified,
        });
    }

    if options.flatten {
        flatten_form_fields(doc, true)?;
    }

    if let Some(ref pi) = options.piece_info {
        insert_piece_info(doc, pi)?;
    }

    if let Some(ref lpi) = options.locked_piece_info {
        insert_locked_piece_info(doc, &lpi.app_name, &lpi.data, &lpi.secret_key)?;
    }

    Ok(PageCropReport {
        total_pages,
        cropped_pages,
        pages_details,
    })
}
