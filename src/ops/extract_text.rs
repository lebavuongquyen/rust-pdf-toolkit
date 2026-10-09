use crate::ops::{ColorRgb, PageSelection, get_page_mediabox};
use lopdf::content::Content;
use lopdf::{Dictionary, Document, Encoding, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn default_true() -> bool {
    true
}

fn default_line_tolerance() -> f64 {
    3.5
}

fn default_word_gap_factor() -> f64 {
    0.25
}

/// Granularity level for text extraction output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextGranularity {
    /// Full structured hierarchy (Document -> Page -> Blocks -> Lines -> Spans -> Words)
    #[default]
    Full,
    /// Blocks (paragraphs) only
    Blocks,
    /// Lines only
    Lines,
    /// Spans (contiguous text with same font & style) only
    Spans,
    /// Words only
    Words,
}

impl TextGranularity {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "block" | "blocks" | "paragraph" | "paragraphs" => Self::Blocks,
            "line" | "lines" => Self::Lines,
            "span" | "spans" => Self::Spans,
            "word" | "words" => Self::Words,
            _ => Self::Full,
        }
    }
}

/// Comprehensive options for precise text extraction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextExtractionOptions {
    /// Target pages to extract text from. Default: PageSelection::All.
    #[serde(default)]
    pub pages: PageSelection,

    /// Granularity level for output hierarchy. Default: TextGranularity::Full.
    #[serde(default)]
    pub granularity: TextGranularity,

    /// Sort text in natural reading order (top-to-bottom, left-to-right). Default: true.
    #[serde(default = "default_true")]
    pub sort_reading_order: bool,

    /// Vertical tolerance in points for grouping text spans onto the same line. Default: 3.5 pt.
    #[serde(default = "default_line_tolerance")]
    pub line_tolerance: f64,

    /// Horizontal spacing factor relative to font size for detecting word breaks. Default: 0.25.
    #[serde(default = "default_word_gap_factor")]
    pub word_gap_factor: f64,

    /// Include invisible text (e.g. render mode 3 or OCR hidden layer). Default: false.
    #[serde(default)]
    pub include_invisible: bool,

    /// Normalize whitespace (collapse multiple spaces, trim empty lines). Default: true.
    #[serde(default = "default_true")]
    pub normalize_whitespace: bool,
}

impl Default for TextExtractionOptions {
    fn default() -> Self {
        Self {
            pages: PageSelection::All,
            granularity: TextGranularity::Full,
            sort_reading_order: true,
            line_tolerance: default_line_tolerance(),
            word_gap_factor: default_word_gap_factor(),
            include_invisible: false,
            normalize_whitespace: true,
        }
    }
}

impl TextExtractionOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pages(mut self, pages: PageSelection) -> Self {
        self.pages = pages;
        self
    }

    pub fn pages_str(mut self, s: &str) -> Self {
        self.pages = PageSelection::parse(s);
        self
    }

    pub fn granularity(mut self, gran: TextGranularity) -> Self {
        self.granularity = gran;
        self
    }

    pub fn sort_reading_order(mut self, sort: bool) -> Self {
        self.sort_reading_order = sort;
        self
    }

    pub fn line_tolerance(mut self, tol: f64) -> Self {
        self.line_tolerance = tol;
        self
    }

    pub fn word_gap_factor(mut self, factor: f64) -> Self {
        self.word_gap_factor = factor;
        self
    }

    pub fn include_invisible(mut self, inc: bool) -> Self {
        self.include_invisible = inc;
        self
    }

    pub fn normalize_whitespace(mut self, norm: bool) -> Self {
        self.normalize_whitespace = norm;
        self
    }
}

/// Font typography, styling, and color information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    /// PostScript or font family name (e.g. "Helvetica-Bold", "ArialMT")
    pub font_name: String,
    /// Font size in PDF points (e.g. 12.0)
    pub font_size: f64,
    /// Whether the font is bold
    pub is_bold: bool,
    /// Whether the font is italic or oblique
    pub is_italic: bool,
    /// Whether the font is fixed-pitch / monospaced
    pub is_monospace: bool,
    /// Primary color (fill or stroke color)
    pub color: ColorRgb,
    /// Hex color representation (e.g. "#000000")
    pub color_hex: String,
    /// Text rendering mode (0 = Fill, 1 = Stroke, 2 = Fill+Stroke, 3 = Invisible)
    pub render_mode: u8,
    /// Text rotation angle in degrees (0.0 to 360.0)
    pub rotation: f64,
}

/// An individual word with its exact bounding box and typography style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextWord {
    pub text: String,
    /// Bounding box [llx, lly, urx, ury] in PDF points
    pub bbox: [f64; 4],
    pub style: TextStyle,
}

/// A contiguous text span sharing identical styling and font attributes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextSpan {
    pub text: String,
    /// Bounding box [llx, lly, urx, ury] in PDF points
    pub bbox: [f64; 4],
    pub style: TextStyle,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<TextWord>,
}

/// A line of text on a page composed of one or more text spans.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextLine {
    pub text: String,
    /// Union bounding box of all spans in this line [llx, lly, urx, ury]
    pub bbox: [f64; 4],
    pub spans: Vec<TextSpan>,
}

/// A paragraph or block of text lines grouped by reading proximity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextBlock {
    pub text: String,
    /// Union bounding box of all lines in this block [llx, lly, urx, ury]
    pub bbox: [f64; 4],
    pub lines: Vec<TextLine>,
}

/// All extracted text and geometry for a single PDF page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageText {
    /// 1-based page number
    pub page: u32,
    /// Page dimensions [width, height] in PDF points
    pub dimensions: [f64; 2],
    /// Plain text content of the entire page
    pub text: String,
    /// Structured text blocks (paragraphs)
    pub blocks: Vec<TextBlock>,
}

/// Complete document text extraction report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentTextReport {
    pub total_pages: u32,
    pub extracted_pages: u32,
    pub total_characters: usize,
    pub total_words: usize,
    pub pages: Vec<PageText>,
}

// ---------------------------------------------------------------------------
// 2D Affine Transformation Matrix
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

    fn translation(tx: f64, ty: f64) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: tx,
            f: ty,
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

    fn rotation_degrees(&self) -> f64 {
        let rad = self.b.atan2(self.a);
        let mut deg = rad.to_degrees();
        deg = ((deg % 360.0) + 360.0) % 360.0;
        deg
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

fn color_to_hex(c: ColorRgb) -> String {
    let r = (c.r.clamp(0.0, 1.0) * 255.0).round() as u8;
    let g = (c.g.clamp(0.0, 1.0) * 255.0).round() as u8;
    let b = (c.b.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{r:02X}{g:02X}{b:02X}")
}

// ---------------------------------------------------------------------------
// Font Metadata & Width Resolution
// ---------------------------------------------------------------------------

struct FontInfo {
    clean_name: String,
    is_bold: bool,
    is_italic: bool,
    is_monospace: bool,
    first_char: i64,
    widths: Vec<f64>,
}

fn extract_font_info(doc: &Document, font_dict: &Dictionary) -> FontInfo {
    let raw_name = font_dict
        .get(b"BaseFont")
        .and_then(Object::as_name)
        .ok()
        .map(|b| String::from_utf8_lossy(b).to_string())
        .unwrap_or_else(|| "Helvetica".to_string());

    // Clean subset prefix (e.g. "BCDFEE+TimesNewRomanPSMT" -> "TimesNewRomanPSMT")
    let clean_name = if raw_name.len() > 7 && raw_name.as_bytes().get(6) == Some(&b'+') {
        raw_name[7..].to_string()
    } else {
        raw_name
    };

    let name_lower = clean_name.to_lowercase();
    let mut is_bold = name_lower.contains("bold")
        || name_lower.contains("black")
        || name_lower.contains("heavy")
        || name_lower.contains("semibold")
        || name_lower.contains("bld");
    let mut is_italic = name_lower.contains("italic")
        || name_lower.contains("oblique")
        || name_lower.contains("ita")
        || name_lower.contains("slanted");
    let mut is_monospace = name_lower.contains("courier")
        || name_lower.contains("mono")
        || name_lower.contains("console")
        || name_lower.contains("typewriter")
        || name_lower.contains("fixed");

    // Check FontDescriptor if available
    let descriptor_obj = font_dict.get(b"FontDescriptor").ok().and_then(|o| match o {
        Object::Dictionary(d) => Some(d),
        Object::Reference(id) => doc.get_object(*id).ok().and_then(|obj| obj.as_dict().ok()),
        _ => None,
    });

    if let Some(desc) = descriptor_obj {
        if let Ok(weight) = desc.get(b"FontWeight").and_then(Object::as_i64) {
            if weight >= 700 {
                is_bold = true;
            }
        }
        if let Ok(flags) = desc.get(b"Flags").and_then(Object::as_i64) {
            if (flags & 1) != 0 {
                is_monospace = true;
            }
            if (flags & 64) != 0 {
                is_italic = true;
            }
        }
        if let Ok(angle) = desc.get(b"ItalicAngle").and_then(Object::as_float) {
            if (angle as f64).abs() > 0.1 {
                is_italic = true;
            }
        }
    }

    let first_char = font_dict
        .get(b"FirstChar")
        .and_then(Object::as_i64)
        .unwrap_or(0);
    let mut widths = Vec::new();

    let widths_obj = font_dict.get(b"Widths").ok().and_then(|o| match o {
        Object::Array(arr) => Some(arr),
        Object::Reference(id) => doc.get_object(*id).ok().and_then(|obj| obj.as_array().ok()),
        _ => None,
    });

    if let Some(arr) = widths_obj {
        widths = arr
            .iter()
            .filter_map(|v| v.as_float().ok().map(|x| x as f64))
            .collect();
    }

    FontInfo {
        clean_name,
        is_bold,
        is_italic,
        is_monospace,
        first_char,
        widths,
    }
}

fn get_glyph_width_1000(font_info: Option<&FontInfo>, char_code: u32) -> f64 {
    if let Some(info) = font_info {
        let idx = (char_code as i64) - info.first_char;
        if idx >= 0 && (idx as usize) < info.widths.len() {
            return info.widths[idx as usize];
        }
        if info.is_monospace {
            return 600.0;
        }
    }
    if char_code == 32 { 278.0 } else { 550.0 }
}

// ---------------------------------------------------------------------------
// Graphics & Text State Tracker
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct GraphicsState {
    ctm: Matrix,
    fill_color: ColorRgb,
    stroke_color: ColorRgb,
}

// ---------------------------------------------------------------------------
// Page Text Extraction Core
// ---------------------------------------------------------------------------

fn extract_page_raw_spans(
    doc: &Document,
    page_id: ObjectId,
    options: &TextExtractionOptions,
) -> Vec<TextSpan> {
    let font_dicts = doc.get_page_fonts(page_id).unwrap_or_default();
    let mut encodings: BTreeMap<Vec<u8>, Encoding> = BTreeMap::new();
    let mut font_infos: BTreeMap<Vec<u8>, FontInfo> = BTreeMap::new();

    for (name, font) in &font_dicts {
        if let Ok(enc) = font.get_font_encoding(doc) {
            encodings.insert(name.clone(), enc);
        }
        let info = extract_font_info(doc, font);
        font_infos.insert(name.clone(), info);
    }

    let content_bytes = doc.get_page_content(page_id);
    if content_bytes.is_empty() {
        return Vec::new();
    }

    let content = match Content::decode(&content_bytes) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut state = GraphicsState {
        ctm: Matrix::identity(),
        fill_color: ColorRgb::BLACK,
        stroke_color: ColorRgb::BLACK,
    };
    let mut g_stack: Vec<GraphicsState> = Vec::new();

    let mut tm = Matrix::identity();
    let mut tlm = Matrix::identity();
    let mut current_font_tag = Vec::new();
    let mut current_font_size = 12.0f64;
    let mut char_spacing = 0.0f64;
    let mut word_spacing = 0.0f64;
    let mut h_scale = 100.0f64;
    let mut text_rise = 0.0f64;
    let mut leading = 0.0f64;
    let mut render_mode = 0u8;

    let mut raw_spans = Vec::new();

    for op in &content.operations {
        match op.operator.as_str() {
            "q" => {
                g_stack.push(state.clone());
            }
            "Q" => {
                if let Some(saved) = g_stack.pop() {
                    state = saved;
                }
            }
            "cm" => {
                if let Some(m) = parse_matrix_operands(&op.operands) {
                    state.ctm = m.multiply(&state.ctm);
                }
            }
            "g" => {
                if let Some(g_val) = op.operands.first().and_then(|o| o.as_float().ok()) {
                    let g = g_val as f64;
                    state.fill_color = ColorRgb::new(g, g, g);
                }
            }
            "G" => {
                if let Some(g_val) = op.operands.first().and_then(|o| o.as_float().ok()) {
                    let g = g_val as f64;
                    state.stroke_color = ColorRgb::new(g, g, g);
                }
            }
            "rg" => {
                if op.operands.len() >= 3 {
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        op.operands[0].as_float(),
                        op.operands[1].as_float(),
                        op.operands[2].as_float(),
                    ) {
                        state.fill_color = ColorRgb::new(r as f64, g as f64, b as f64);
                    }
                }
            }
            "RG" => {
                if op.operands.len() >= 3 {
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        op.operands[0].as_float(),
                        op.operands[1].as_float(),
                        op.operands[2].as_float(),
                    ) {
                        state.stroke_color = ColorRgb::new(r as f64, g as f64, b as f64);
                    }
                }
            }
            "k" => {
                if op.operands.len() >= 4 {
                    if let (Ok(c), Ok(m), Ok(y), Ok(k)) = (
                        op.operands[0].as_float(),
                        op.operands[1].as_float(),
                        op.operands[2].as_float(),
                        op.operands[3].as_float(),
                    ) {
                        let (c, m, y, k) = (c as f64, m as f64, y as f64, k as f64);
                        let r = (1.0 - c) * (1.0 - k);
                        let g = (1.0 - m) * (1.0 - k);
                        let b = (1.0 - y) * (1.0 - k);
                        state.fill_color = ColorRgb::new(r, g, b);
                    }
                }
            }
            "K" => {
                if op.operands.len() >= 4 {
                    if let (Ok(c), Ok(m), Ok(y), Ok(k)) = (
                        op.operands[0].as_float(),
                        op.operands[1].as_float(),
                        op.operands[2].as_float(),
                        op.operands[3].as_float(),
                    ) {
                        let (c, m, y, k) = (c as f64, m as f64, y as f64, k as f64);
                        let r = (1.0 - c) * (1.0 - k);
                        let g = (1.0 - m) * (1.0 - k);
                        let b = (1.0 - y) * (1.0 - k);
                        state.stroke_color = ColorRgb::new(r, g, b);
                    }
                }
            }

            // Text State
            "BT" => {
                tm = Matrix::identity();
                tlm = Matrix::identity();
            }
            "ET" => {}
            "Tf" => {
                if let Some(font_name_obj) = op.operands.first() {
                    if let Ok(name) = font_name_obj.as_name() {
                        current_font_tag = name.to_vec();
                    }
                }
                if let Some(sz_obj) = op.operands.get(1) {
                    if let Ok(sz) = sz_obj.as_float() {
                        current_font_size = (sz as f64).abs().max(0.1);
                    }
                }
            }
            "Tm" => {
                if let Some(m) = parse_matrix_operands(&op.operands) {
                    tm = m.clone();
                    tlm = m;
                }
            }
            "Td" => {
                if op.operands.len() >= 2 {
                    if let (Ok(tx), Ok(ty)) = (op.operands[0].as_float(), op.operands[1].as_float())
                    {
                        let offset = Matrix::translation(tx as f64, ty as f64);
                        tlm = offset.multiply(&tlm);
                        tm = tlm.clone();
                    }
                }
            }
            "TD" => {
                if op.operands.len() >= 2 {
                    if let (Ok(tx), Ok(ty)) = (op.operands[0].as_float(), op.operands[1].as_float())
                    {
                        leading = -(ty as f64);
                        let offset = Matrix::translation(tx as f64, ty as f64);
                        tlm = offset.multiply(&tlm);
                        tm = tlm.clone();
                    }
                }
            }
            "T*" => {
                let offset = Matrix::translation(0.0, -leading);
                tlm = offset.multiply(&tlm);
                tm = tlm.clone();
            }
            "TL" => {
                if let Some(lead) = op.operands.first().and_then(|o| o.as_float().ok()) {
                    leading = lead as f64;
                }
            }
            "Tc" => {
                if let Some(cs) = op.operands.first().and_then(|o| o.as_float().ok()) {
                    char_spacing = cs as f64;
                }
            }
            "Tw" => {
                if let Some(ws) = op.operands.first().and_then(|o| o.as_float().ok()) {
                    word_spacing = ws as f64;
                }
            }
            "Tz" => {
                if let Some(scale) = op.operands.first().and_then(|o| o.as_float().ok()) {
                    h_scale = (scale as f64).max(1.0);
                }
            }
            "Ts" => {
                if let Some(rise) = op.operands.first().and_then(|o| o.as_float().ok()) {
                    text_rise = rise as f64;
                }
            }
            "Tr" => {
                if let Some(mode) = op.operands.first().and_then(|o| o.as_i64().ok()) {
                    render_mode = mode.clamp(0, 7) as u8;
                }
            }

            // Showing text: Tj, TJ, ', "
            "'" => {
                let offset = Matrix::translation(0.0, -leading);
                tlm = offset.multiply(&tlm);
                tm = tlm.clone();

                if let Some(Object::String(bytes, _)) = op.operands.first() {
                    process_text_string(
                        bytes,
                        &state,
                        &mut tm,
                        &current_font_tag,
                        current_font_size,
                        char_spacing,
                        word_spacing,
                        h_scale,
                        text_rise,
                        render_mode,
                        &encodings,
                        &font_infos,
                        options,
                        &mut raw_spans,
                    );
                }
            }

            "\"" => {
                if op.operands.len() >= 3 {
                    if let (Ok(ws), Ok(cs)) = (op.operands[0].as_float(), op.operands[1].as_float())
                    {
                        word_spacing = ws as f64;
                        char_spacing = cs as f64;
                    }
                    let offset = Matrix::translation(0.0, -leading);
                    tlm = offset.multiply(&tlm);
                    tm = tlm.clone();

                    if let Object::String(bytes, _) = &op.operands[2] {
                        process_text_string(
                            bytes,
                            &state,
                            &mut tm,
                            &current_font_tag,
                            current_font_size,
                            char_spacing,
                            word_spacing,
                            h_scale,
                            text_rise,
                            render_mode,
                            &encodings,
                            &font_infos,
                            options,
                            &mut raw_spans,
                        );
                    }
                }
            }

            "Tj" => {
                if let Some(Object::String(bytes, _)) = op.operands.first() {
                    process_text_string(
                        bytes,
                        &state,
                        &mut tm,
                        &current_font_tag,
                        current_font_size,
                        char_spacing,
                        word_spacing,
                        h_scale,
                        text_rise,
                        render_mode,
                        &encodings,
                        &font_infos,
                        options,
                        &mut raw_spans,
                    );
                }
            }

            "TJ" => {
                if let Some(Object::Array(arr)) = op.operands.first() {
                    for item in arr {
                        match item {
                            Object::String(bytes, _) => {
                                process_text_string(
                                    bytes,
                                    &state,
                                    &mut tm,
                                    &current_font_tag,
                                    current_font_size,
                                    char_spacing,
                                    word_spacing,
                                    h_scale,
                                    text_rise,
                                    render_mode,
                                    &encodings,
                                    &font_infos,
                                    options,
                                    &mut raw_spans,
                                );
                            }
                            Object::Integer(kern) => {
                                let kern_advance = -(*kern as f64 / 1000.0)
                                    * current_font_size
                                    * (h_scale / 100.0);
                                tm = Matrix::translation(kern_advance, 0.0).multiply(&tm);
                            }
                            Object::Real(kern) => {
                                let kern_advance = -(*kern as f64 / 1000.0)
                                    * current_font_size
                                    * (h_scale / 100.0);
                                tm = Matrix::translation(kern_advance, 0.0).multiply(&tm);
                            }
                            _ => {}
                        }
                    }
                }
            }

            _ => {}
        }
    }

    raw_spans
}

fn process_text_string(
    bytes: &[u8],
    state: &GraphicsState,
    tm: &mut Matrix,
    font_tag: &[u8],
    font_size: f64,
    char_spacing: f64,
    word_spacing: f64,
    h_scale: f64,
    text_rise: f64,
    render_mode: u8,
    encodings: &BTreeMap<Vec<u8>, Encoding>,
    font_infos: &BTreeMap<Vec<u8>, FontInfo>,
    options: &TextExtractionOptions,
    out_spans: &mut Vec<TextSpan>,
) {
    if bytes.is_empty() {
        return;
    }
    if !options.include_invisible && render_mode == 3 {
        return;
    }

    // Decode bytes to text
    let text = if let Some(enc) = encodings.get(font_tag) {
        enc.bytes_to_string(bytes).unwrap_or_else(|_| {
            String::from_utf8(bytes.to_vec())
                .unwrap_or_else(|_| bytes.iter().map(|&b| b as char).collect())
        })
    } else {
        String::from_utf8(bytes.to_vec())
            .unwrap_or_else(|_| bytes.iter().map(|&b| b as char).collect())
    };

    if text.is_empty() {
        return;
    }

    let font_info = font_infos.get(font_tag);
    let font_name = font_info
        .map(|i| i.clean_name.clone())
        .unwrap_or_else(|| "Helvetica".to_string());
    let is_bold = font_info.map(|i| i.is_bold).unwrap_or(false);
    let is_italic = font_info.map(|i| i.is_italic).unwrap_or(false);
    let is_monospace = font_info.map(|i| i.is_monospace).unwrap_or(false);

    // Calculate total advance across glyphs
    let mut total_advance = 0.0f64;
    for ch in text.chars() {
        let w1000 = get_glyph_width_1000(font_info, ch as u32);
        let char_advance = (w1000 / 1000.0 * font_size + char_spacing) * (h_scale / 100.0);
        let space_advance = if ch == ' ' {
            word_spacing * (h_scale / 100.0)
        } else {
            0.0
        };
        total_advance += char_advance + space_advance;
    }

    let eff_matrix = tm.multiply(&state.ctm);
    let rotation = eff_matrix.rotation_degrees();

    // 4 corners of text bounding box in text space
    let p0 = eff_matrix.transform_point(0.0, text_rise);
    let p1 = eff_matrix.transform_point(total_advance, text_rise);
    let p2 = eff_matrix.transform_point(total_advance, text_rise + font_size);
    let p3 = eff_matrix.transform_point(0.0, text_rise + font_size);

    let min_x = p0.0.min(p1.0).min(p2.0).min(p3.0);
    let min_y = p0.1.min(p1.1).min(p2.1).min(p3.1);
    let max_x = p0.0.max(p1.0).max(p2.0).max(p3.0);
    let max_y = p0.1.max(p1.1).max(p2.1).max(p3.1);

    let bbox = [min_x, min_y, max_x, max_y];

    let style = TextStyle {
        font_name,
        font_size,
        is_bold,
        is_italic,
        is_monospace,
        color: state.fill_color,
        color_hex: color_to_hex(state.fill_color),
        render_mode,
        rotation,
    };

    // Subdivide into TextWords if granularity requires Word or Full
    let mut words = Vec::new();
    if matches!(
        options.granularity,
        TextGranularity::Full | TextGranularity::Words
    ) {
        let mut cur_idx = 0usize;
        let total_chars = text.chars().count().max(1);
        let span_w = (max_x - min_x).abs().max(1.0);

        for word_str in text.split_whitespace() {
            if let Some(pos) = text[cur_idx..].find(word_str) {
                let start_char = text[..cur_idx + pos].chars().count();
                let word_char_count = word_str.chars().count();
                let end_char = start_char + word_char_count;

                let w_min_x = min_x + (start_char as f64 / total_chars as f64) * span_w;
                let w_max_x = min_x + (end_char as f64 / total_chars as f64) * span_w;

                words.push(TextWord {
                    text: word_str.to_string(),
                    bbox: [w_min_x, min_y, w_max_x, max_y],
                    style: style.clone(),
                });

                cur_idx += pos + word_str.len();
            }
        }
    }

    out_spans.push(TextSpan {
        text,
        bbox,
        style,
        words,
    });

    // Advance text matrix horizontally by total advance
    *tm = Matrix::translation(total_advance, 0.0).multiply(tm);
}

// ---------------------------------------------------------------------------
// Reading Order & Hierarchy Reconstruction
// ---------------------------------------------------------------------------

fn group_spans_into_lines(
    mut spans: Vec<TextSpan>,
    options: &TextExtractionOptions,
) -> Vec<TextLine> {
    if spans.is_empty() {
        return Vec::new();
    }

    if options.sort_reading_order {
        // In PDF coordinates, origin is bottom-left. Higher Y = top of page.
        // Sort primary Y descending (top to bottom), secondary X ascending (left to right).
        spans.sort_by(|a, b| {
            let y_diff = b.bbox[1] - a.bbox[1];
            if y_diff.abs() > options.line_tolerance {
                y_diff
                    .partial_cmp(&0.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            } else {
                a.bbox[0]
                    .partial_cmp(&b.bbox[0])
                    .unwrap_or(std::cmp::Ordering::Equal)
            }
        });
    }

    let mut lines: Vec<TextLine> = Vec::new();

    for span in spans {
        let span_baseline = span.bbox[1];
        let mut matched_line = None;

        for line in lines.iter_mut().rev() {
            let line_baseline = line.bbox[1];
            if (span_baseline - line_baseline).abs() <= options.line_tolerance {
                matched_line = Some(line);
                break;
            }
        }

        if let Some(line) = matched_line {
            // Update line bounding box union
            line.bbox[0] = line.bbox[0].min(span.bbox[0]);
            line.bbox[1] = line.bbox[1].min(span.bbox[1]);
            line.bbox[2] = line.bbox[2].max(span.bbox[2]);
            line.bbox[3] = line.bbox[3].max(span.bbox[3]);
            line.spans.push(span);
        } else {
            lines.push(TextLine {
                text: String::new(),
                bbox: span.bbox,
                spans: vec![span],
            });
        }
    }

    // Sort spans within each line left to right, assemble line text
    for line in &mut lines {
        if options.sort_reading_order {
            line.spans.sort_by(|a, b| {
                a.bbox[0]
                    .partial_cmp(&b.bbox[0])
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        let mut line_text = String::new();
        let mut prev_urx = None;

        for span in &line.spans {
            if let Some(last_x) = prev_urx {
                let gap = span.bbox[0] - last_x;
                let font_sz = span.style.font_size.max(8.0);
                if gap > options.word_gap_factor * font_sz
                    && !line_text.ends_with(' ')
                    && !span.text.starts_with(' ')
                {
                    line_text.push(' ');
                }
            }
            line_text.push_str(&span.text);
            prev_urx = Some(span.bbox[2]);
        }

        if options.normalize_whitespace {
            line.text = line_text.trim().to_string();
        } else {
            line.text = line_text;
        }
    }

    lines
}

fn group_lines_into_blocks(
    lines: Vec<TextLine>,
    _options: &TextExtractionOptions,
) -> Vec<TextBlock> {
    if lines.is_empty() {
        return Vec::new();
    }

    let mut blocks: Vec<TextBlock> = Vec::new();

    for line in lines {
        if line.text.is_empty() {
            continue;
        }

        let mut should_new_block = false;

        if let Some(last_block) = blocks.last_mut() {
            if let Some(last_line) = last_block.lines.last() {
                // Gap between bottom of previous line and top of current line
                let v_gap = (last_line.bbox[1] - line.bbox[3]).abs();
                let avg_font = last_line
                    .spans
                    .first()
                    .map(|s| s.style.font_size)
                    .unwrap_or(12.0);

                if v_gap > 1.8 * avg_font {
                    should_new_block = true;
                }
            }
        } else {
            should_new_block = true;
        }

        if should_new_block {
            blocks.push(TextBlock {
                text: line.text.clone(),
                bbox: line.bbox,
                lines: vec![line],
            });
        } else if let Some(last_block) = blocks.last_mut() {
            last_block.bbox[0] = last_block.bbox[0].min(line.bbox[0]);
            last_block.bbox[1] = last_block.bbox[1].min(line.bbox[1]);
            last_block.bbox[2] = last_block.bbox[2].max(line.bbox[2]);
            last_block.bbox[3] = last_block.bbox[3].max(line.bbox[3]);
            if !last_block.text.is_empty() {
                last_block.text.push('\n');
            }
            last_block.text.push_str(&line.text);
            last_block.lines.push(line);
        }
    }

    blocks
}

// ---------------------------------------------------------------------------
// High-Level Public APIs
// ---------------------------------------------------------------------------

/// Extracts structured text with bounding boxes and typography styles from an existing Document.
pub fn extract_text_from_doc(
    doc: &Document,
    options: &TextExtractionOptions,
) -> Result<DocumentTextReport, String> {
    let all_pages = doc.get_pages();
    let total_pages = all_pages.len() as u32;

    let mut pages_text = Vec::new();
    let mut total_chars = 0usize;
    let mut total_words = 0usize;

    for (&page_num, &page_id) in &all_pages {
        if !options.pages.matches(page_num, total_pages) {
            continue;
        }

        let m_box = get_page_mediabox(doc, page_id);
        let width = (m_box[2] - m_box[0]).abs();
        let height = (m_box[3] - m_box[1]).abs();

        let raw_spans = extract_page_raw_spans(doc, page_id, options);
        let lines = group_spans_into_lines(raw_spans, options);
        let blocks = group_lines_into_blocks(lines, options);

        let mut page_full_text = String::new();
        for block in &blocks {
            if !page_full_text.is_empty() {
                page_full_text.push_str("\n\n");
            }
            page_full_text.push_str(&block.text);
        }

        let p_chars = page_full_text.chars().count();
        let p_words = page_full_text.split_whitespace().count();
        total_chars += p_chars;
        total_words += p_words;

        pages_text.push(PageText {
            page: page_num,
            dimensions: [width, height],
            text: page_full_text,
            blocks,
        });
    }

    Ok(DocumentTextReport {
        total_pages,
        extracted_pages: pages_text.len() as u32,
        total_characters: total_chars,
        total_words,
        pages: pages_text,
    })
}

/// Extracts structured text with bounding boxes and typography styles from raw PDF bytes.
pub fn extract_text_structured(
    pdf_bytes: &[u8],
    options: &TextExtractionOptions,
) -> Result<DocumentTextReport, String> {
    let doc = Document::load_mem(pdf_bytes)
        .map_err(|e| format!("Failed to parse PDF for text extraction: {e}"))?;
    extract_text_from_doc(&doc, options)
}

/// Extracts clean plain text string from raw PDF bytes.
pub fn extract_text(pdf_bytes: &[u8], options: &TextExtractionOptions) -> Result<String, String> {
    let report = extract_text_structured(pdf_bytes, options)?;
    let mut result = String::new();

    for page in &report.pages {
        if !result.is_empty() {
            result.push_str("\n\n--- Page Break ---\n\n");
        }
        result.push_str(&page.text);
    }

    Ok(result)
}
