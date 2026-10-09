use crate::{
    LockedPieceInfoConfig, flatten_form_fields, insert_locked_piece_info, insert_piece_info,
};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Representation of RGB color in range [0.0, 1.0].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ColorRgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

impl<'de> serde::Deserialize<'de> for ColorRgb {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum ColorHelper {
            Str(String),
            Rgb { r: f64, g: f64, b: f64 },
        }

        match ColorHelper::deserialize(deserializer)? {
            ColorHelper::Str(s) => ColorRgb::parse(&s).map_err(serde::de::Error::custom),
            ColorHelper::Rgb { r, g, b } => Ok(ColorRgb::new(r, g, b)),
        }
    }
}

impl Default for ColorRgb {
    fn default() -> Self {
        Self {
            r: 0.5,
            g: 0.5,
            b: 0.5,
        }
    }
}

impl ColorRgb {
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
    };
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
    };
    pub const GRAY: Self = Self {
        r: 0.5,
        g: 0.5,
        b: 0.5,
    };
    pub const RED: Self = Self {
        r: 0.85,
        g: 0.1,
        b: 0.1,
    };
    pub const BLUE: Self = Self {
        r: 0.1,
        g: 0.3,
        b: 0.85,
    };

    pub fn new(r: f64, g: f64, b: f64) -> Self {
        Self {
            r: r.clamp(0.0, 1.0),
            g: g.clamp(0.0, 1.0),
            b: b.clamp(0.0, 1.0),
        }
    }

    /// Parses color from hex ("#RRGGBB", "#RGB", "RRGGBB") or color names ("red", "black", "gray", "blue").
    pub fn parse(s: &str) -> Result<Self, String> {
        let clean = s.trim().to_lowercase();
        match clean.as_str() {
            "black" => Ok(Self::BLACK),
            "white" => Ok(Self::WHITE),
            "gray" | "grey" => Ok(Self::GRAY),
            "red" => Ok(Self::RED),
            "blue" => Ok(Self::BLUE),
            _ => {
                let hex = clean.strip_prefix('#').unwrap_or(&clean);
                if hex.len() == 6 {
                    let r = u8::from_str_radix(&hex[0..2], 16)
                        .map_err(|_| format!("Invalid hex color: {s}"))?
                        as f64
                        / 255.0;
                    let g = u8::from_str_radix(&hex[2..4], 16)
                        .map_err(|_| format!("Invalid hex color: {s}"))?
                        as f64
                        / 255.0;
                    let b = u8::from_str_radix(&hex[4..6], 16)
                        .map_err(|_| format!("Invalid hex color: {s}"))?
                        as f64
                        / 255.0;
                    Ok(Self::new(r, g, b))
                } else if hex.len() == 3 {
                    let r_char = &hex[0..1];
                    let g_char = &hex[1..2];
                    let b_char = &hex[2..3];
                    let r = u8::from_str_radix(&format!("{r_char}{r_char}"), 16)
                        .map_err(|_| format!("Invalid hex color: {s}"))?
                        as f64
                        / 255.0;
                    let g = u8::from_str_radix(&format!("{g_char}{g_char}"), 16)
                        .map_err(|_| format!("Invalid hex color: {s}"))?
                        as f64
                        / 255.0;
                    let b = u8::from_str_radix(&format!("{b_char}{b_char}"), 16)
                        .map_err(|_| format!("Invalid hex color: {s}"))?
                        as f64
                        / 255.0;
                    Ok(Self::new(r, g, b))
                } else {
                    Err(format!(
                        "Invalid color specification '{s}'. Expected hex like '#FF0000' or name ('red', 'gray', 'black')"
                    ))
                }
            }
        }
    }
}

/// Layer rendering mode for watermark / stamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LayerMode {
    #[default]
    Over,
    Under,
}

impl LayerMode {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "under" | "behind" | "background" => Self::Under,
            _ => Self::Over,
        }
    }
}

/// Page selection targeting which pages receive the watermark / numbering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PageSelection {
    #[default]
    All,
    Odd,
    Even,
    First,
    Last,
    Range(Vec<u32>),
}

impl PageSelection {
    pub fn parse(s: &str) -> Self {
        let clean = s.trim().to_lowercase();
        match clean.as_str() {
            "" | "all" => Self::All,
            "odd" => Self::Odd,
            "even" => Self::Even,
            "first" | "1" => Self::First,
            "last" => Self::Last,
            _ => {
                let mut pages = Vec::new();
                for part in clean.split([',', ';']) {
                    let part = part.trim();
                    if let Some((start_s, end_s)) = part.split_once('-') {
                        if let (Ok(start), Ok(end)) =
                            (start_s.trim().parse::<u32>(), end_s.trim().parse::<u32>())
                        {
                            for p in start..=end {
                                pages.push(p);
                            }
                        }
                    } else if let Ok(p) = part.parse::<u32>() {
                        pages.push(p);
                    }
                }
                if pages.is_empty() {
                    Self::All
                } else {
                    pages.sort_unstable();
                    pages.dedup();
                    Self::Range(pages)
                }
            }
        }
    }

    pub fn matches(&self, page_num: u32, total_pages: u32) -> bool {
        match self {
            Self::All => true,
            Self::Odd => page_num % 2 == 1,
            Self::Even => page_num.is_multiple_of(2),
            Self::First => page_num == 1,
            Self::Last => page_num == total_pages,
            Self::Range(list) => list.contains(&page_num),
        }
    }
}

/// Watermark placement position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum WatermarkPosition {
    Center,
    #[default]
    Diagonal,
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    Custom {
        x: f64,
        y: f64,
    },
    Tiled {
        step_x: f64,
        step_y: f64,
    },
}

impl WatermarkPosition {
    pub fn parse(s: &str) -> Self {
        let clean = s.trim().to_lowercase();
        match clean.as_str() {
            "diagonal" => Self::Diagonal,
            "center" | "middle" => Self::Center,
            "top-left" | "topleft" => Self::TopLeft,
            "top-center" | "topcenter" | "top" => Self::TopCenter,
            "top-right" | "topright" => Self::TopRight,
            "center-left" | "centerleft" | "left" => Self::CenterLeft,
            "center-right" | "centerright" | "right" => Self::CenterRight,
            "bottom-left" | "bottomleft" => Self::BottomLeft,
            "bottom-center" | "bottomcenter" | "bottom" => Self::BottomCenter,
            "bottom-right" | "bottomright" => Self::BottomRight,
            "tiled" | "tile" | "grid" => Self::Tiled {
                step_x: 220.0,
                step_y: 220.0,
            },
            _ => {
                if let Some((xs, ys)) = clean.split_once(',')
                    && let (Ok(x), Ok(y)) = (xs.trim().parse::<f64>(), ys.trim().parse::<f64>())
                {
                    return Self::Custom { x, y };
                }
                Self::Diagonal
            }
        }
    }
}

/// Comprehensive options for applying a watermark to a PDF.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatermarkOptions {
    /// Watermark text string. Supports placeholders: {page}, {total}, {date}, {time}, {filename}.
    pub text: Option<String>,
    /// Optional raw image bytes (JPEG or PNG) for image stamp/watermark.
    #[serde(skip)]
    pub image_bytes: Option<Vec<u8>>,
    /// Target page selection: "all", "odd", "even", "first", "last", "1,3-5".
    #[serde(default)]
    pub pages: PageSelection,
    /// Watermark placement on the page.
    #[serde(default)]
    pub position: WatermarkPosition,
    /// Rotation in degrees. If None, auto-calculated (e.g. angle of page diagonal for Diagonal).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f64>,
    /// Opacity in range [0.0, 1.0]. Default 0.15 (15%) for subtle watermark.
    #[serde(default = "default_watermark_opacity")]
    pub opacity: f64,
    /// Standard PDF BaseFont: "Helvetica-Bold", "Helvetica", "Times-Bold", "Courier-Bold", etc.
    #[serde(default = "default_font_name")]
    pub font_name: String,
    /// Font size in points. If None, dynamically scales to fit page diagonal without overflow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    /// Text color in RGB.
    #[serde(default)]
    pub color: ColorRgb,
    /// Layer mode: Over (on top of page contents) or Under (behind page contents).
    #[serde(default)]
    pub layer: LayerMode,
    /// Optional image dimensions when embedding an image watermark.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_height: Option<f64>,
    /// Custom template placeholder variables.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub template_vars: HashMap<String, String>,
    /// Flatten form fields after watermarking.
    #[serde(default)]
    pub flatten: bool,
    /// Optional PieceInfo metadata to inject.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub piece_info: Option<Value>,
    /// Optional locked PieceInfo metadata to inject.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked_piece_info: Option<LockedPieceInfoConfig>,
}

fn default_watermark_opacity() -> f64 {
    0.15
}

fn default_font_name() -> String {
    "Helvetica-Bold".to_string()
}

impl Default for WatermarkOptions {
    fn default() -> Self {
        Self {
            text: None,
            image_bytes: None,
            pages: PageSelection::All,
            position: WatermarkPosition::Diagonal,
            rotation: None,
            opacity: 0.15,
            font_name: default_font_name(),
            font_size: None,
            color: ColorRgb::GRAY,
            layer: LayerMode::Over,
            image_width: None,
            image_height: None,
            template_vars: HashMap::new(),
            flatten: false,
            piece_info: None,
            locked_piece_info: None,
        }
    }
}

impl WatermarkOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn image(mut self, bytes: Vec<u8>) -> Self {
        self.image_bytes = Some(bytes);
        self
    }

    pub fn pages(mut self, pages: PageSelection) -> Self {
        self.pages = pages;
        self
    }

    pub fn position(mut self, pos: WatermarkPosition) -> Self {
        self.position = pos;
        self
    }

    pub fn rotation(mut self, degrees: f64) -> Self {
        self.rotation = Some(degrees);
        self
    }

    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    pub fn font(mut self, font_name: impl Into<String>) -> Self {
        self.font_name = font_name.into();
        self
    }

    pub fn font_size(mut self, size: f64) -> Self {
        self.font_size = Some(size.max(1.0));
        self
    }

    pub fn color(mut self, color: ColorRgb) -> Self {
        self.color = color;
        self
    }

    pub fn layer(mut self, layer: LayerMode) -> Self {
        self.layer = layer;
        self
    }

    pub fn flatten(mut self, flatten: bool) -> Self {
        self.flatten = flatten;
        self
    }

    pub fn template_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.template_vars.insert(key.into(), value.into());
        self
    }
}

/// Page numbering alignment / position on page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NumberingPosition {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    #[default]
    BottomCenter,
    BottomRight,
}

impl NumberingPosition {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "top-left" | "topleft" => Self::TopLeft,
            "top-center" | "topcenter" | "top" => Self::TopCenter,
            "top-right" | "topright" => Self::TopRight,
            "bottom-left" | "bottomleft" => Self::BottomLeft,
            "bottom-right" | "bottomright" => Self::BottomRight,
            _ => Self::BottomCenter,
        }
    }
}

/// Comprehensive options for Bates / Page Numbering header & footer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NumberingOptions {
    /// Format template string. Placeholders: {page}, {total}, {bates}, {date}, {time}, {filename}.
    /// Example: "Trang {page} / {total}" or "Page {page} of {total}" or "CONFIDENTIAL-BATES-{page:04}".
    #[serde(default = "default_numbering_format")]
    pub format: String,
    /// Placement position: BottomCenter, BottomRight, TopRight, etc.
    #[serde(default)]
    pub position: NumberingPosition,
    /// Horizontal margin from page edge in points. Default 36.0 (0.5 inch).
    #[serde(default = "default_margin_x")]
    pub margin_x: f64,
    /// Vertical margin from page edge in points. Default 24.0.
    #[serde(default = "default_margin_y")]
    pub margin_y: f64,
    /// Page selection: "all", "odd", "even", range.
    #[serde(default)]
    pub pages: PageSelection,
    /// Physical 1-based page index to start numbering (e.g. 2 to skip cover page). Default 1.
    #[serde(default = "default_start_page")]
    pub start_page: u32,
    /// Starting counter value for numbering. Default 1.
    #[serde(default = "default_start_number")]
    pub start_number: u32,
    /// Standard PDF BaseFont: "Helvetica", "Times-Roman", "Courier", etc.
    #[serde(default = "default_number_font")]
    pub font_name: String,
    /// Font size in points. Default 10.0.
    #[serde(default = "default_number_font_size")]
    pub font_size: f64,
    /// Text color in RGB.
    #[serde(default = "default_number_color")]
    pub color: ColorRgb,
    /// Opacity in range [0.0, 1.0]. Default 1.0 (fully opaque).
    #[serde(default = "default_opaque")]
    pub opacity: f64,
    /// Layer mode: Over (default) or Under.
    #[serde(default)]
    pub layer: LayerMode,
    /// Custom template placeholder variables.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub template_vars: HashMap<String, String>,
    /// Flatten form fields after numbering.
    #[serde(default)]
    pub flatten: bool,
    /// Optional PieceInfo metadata to inject.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub piece_info: Option<Value>,
    /// Optional locked PieceInfo metadata to inject.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked_piece_info: Option<LockedPieceInfoConfig>,
}

fn default_numbering_format() -> String {
    "Trang {page} / {total}".to_string()
}
fn default_margin_x() -> f64 {
    36.0
}
fn default_margin_y() -> f64 {
    24.0
}
fn default_start_page() -> u32 {
    1
}
fn default_start_number() -> u32 {
    1
}
fn default_number_font() -> String {
    "Helvetica".to_string()
}
fn default_number_font_size() -> f64 {
    10.0
}
fn default_number_color() -> ColorRgb {
    ColorRgb::new(0.25, 0.25, 0.25)
}
fn default_opaque() -> f64 {
    1.0
}

impl Default for NumberingOptions {
    fn default() -> Self {
        Self {
            format: default_numbering_format(),
            position: NumberingPosition::BottomCenter,
            margin_x: default_margin_x(),
            margin_y: default_margin_y(),
            pages: PageSelection::All,
            start_page: default_start_page(),
            start_number: default_start_number(),
            font_name: default_number_font(),
            font_size: default_number_font_size(),
            color: default_number_color(),
            opacity: default_opaque(),
            layer: LayerMode::Over,
            template_vars: HashMap::new(),
            flatten: false,
            piece_info: None,
            locked_piece_info: None,
        }
    }
}

impl NumberingOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn format(mut self, format: impl Into<String>) -> Self {
        self.format = format.into();
        self
    }

    pub fn position(mut self, pos: NumberingPosition) -> Self {
        self.position = pos;
        self
    }

    pub fn margin(mut self, x: f64, y: f64) -> Self {
        self.margin_x = x.max(0.0);
        self.margin_y = y.max(0.0);
        self
    }

    pub fn pages(mut self, pages: PageSelection) -> Self {
        self.pages = pages;
        self
    }

    pub fn start_page(mut self, page: u32) -> Self {
        self.start_page = page.max(1);
        self
    }

    pub fn start_number(mut self, num: u32) -> Self {
        self.start_number = num;
        self
    }

    pub fn font(mut self, font_name: impl Into<String>) -> Self {
        self.font_name = font_name.into();
        self
    }

    pub fn font_size(mut self, size: f64) -> Self {
        self.font_size = size.max(1.0);
        self
    }

    pub fn color(mut self, color: ColorRgb) -> Self {
        self.color = color;
        self
    }

    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    pub fn flatten(mut self, flatten: bool) -> Self {
        self.flatten = flatten;
        self
    }
}

// ---------------------------------------------------------------------------
// Core Rendering & Processing Engine
// ---------------------------------------------------------------------------

/// Applies a watermark to PDF bytes and returns the modified PDF bytes.
pub fn apply_watermark(pdf_bytes: &[u8], options: &WatermarkOptions) -> Result<Vec<u8>, String> {
    let mut doc = Document::load_mem(pdf_bytes).map_err(|e| format!("Failed to load PDF: {e}"))?;
    apply_watermark_to_doc(&mut doc, options, None)?;
    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| format!("Failed to save watermarked PDF: {e}"))?;
    Ok(out)
}

/// Applies page numbering to PDF bytes and returns the modified PDF bytes.
pub fn apply_page_numbering(
    pdf_bytes: &[u8],
    options: &NumberingOptions,
) -> Result<Vec<u8>, String> {
    let mut doc = Document::load_mem(pdf_bytes).map_err(|e| format!("Failed to load PDF: {e}"))?;
    apply_page_numbering_to_doc(&mut doc, options, None)?;
    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| format!("Failed to save numbered PDF: {e}"))?;
    Ok(out)
}

/// Applies watermark directly to a lopdf Document in place.
pub fn apply_watermark_to_doc(
    doc: &mut Document,
    options: &WatermarkOptions,
    filename: Option<&str>,
) -> Result<(), String> {
    if options.text.is_none() && options.image_bytes.is_none() {
        return Err("Watermark requires either text or image_bytes to be specified".to_string());
    }

    let pages_map = doc.get_pages();
    let total_pages = pages_map.len() as u32;
    if total_pages == 0 {
        return Err("PDF document contains 0 pages".to_string());
    }

    // Prepare image XObject if image watermark is provided
    let image_xobject_id = if let Some(ref img_bytes) = options.image_bytes {
        Some(create_watermark_image_xobject(doc, img_bytes)?)
    } else {
        None
    };

    let filename_stem = filename.unwrap_or("document");

    for (page_num, page_id) in pages_map {
        if !options.pages.matches(page_num, total_pages) {
            continue;
        }

        let (llx, lly, width, height) = get_page_box(doc, page_id)?;

        // Ensure Resources has font, ExtGState and/or Image XObject
        let font_alias = "F_WM";
        let gs_alias = "GS_WM";
        let img_alias = "Im_WM";

        ensure_page_resources(
            doc,
            page_id,
            &options.font_name,
            font_alias,
            options.opacity,
            gs_alias,
            image_xobject_id,
            img_alias,
        )?;

        let stream_bytes = if let Some((_, img_w, img_h)) = image_xobject_id {
            generate_image_watermark_stream(
                &options.position,
                options.rotation,
                img_alias,
                img_w,
                img_h,
                options.image_width,
                options.image_height,
                gs_alias,
                llx,
                lly,
                width,
                height,
            )
        } else if let Some(ref raw_text) = options.text {
            let rendered_text = interpolate_placeholders(
                raw_text,
                page_num,
                total_pages,
                page_num,
                filename_stem,
                &options.template_vars,
            );

            // Smart font size calculation: if not specified, dynamically scale text to fill ~65% of page diagonal
            let font_size = options.font_size.unwrap_or_else(|| {
                calculate_adaptive_font_size(&rendered_text, width, height, &options.position)
            });

            generate_text_watermark_stream(
                &rendered_text,
                &options.position,
                options.rotation,
                font_alias,
                font_size,
                &options.color,
                gs_alias,
                llx,
                lly,
                width,
                height,
            )
        } else {
            Vec::new()
        };

        if !stream_bytes.is_empty() {
            insert_page_content_stream(doc, page_id, stream_bytes, options.layer)?;
        }
    }

    if options.flatten {
        flatten_form_fields(doc, true)?;
    }
    if let Some(ref p_info) = options.piece_info {
        insert_piece_info(doc, p_info)?;
    }
    if let Some(ref locked) = options.locked_piece_info {
        insert_locked_piece_info(doc, &locked.app_name, &locked.data, &locked.secret_key)?;
    }

    Ok(())
}

/// Applies page numbering directly to a lopdf Document in place.
pub fn apply_page_numbering_to_doc(
    doc: &mut Document,
    options: &NumberingOptions,
    filename: Option<&str>,
) -> Result<(), String> {
    let pages_map = doc.get_pages();
    let total_pages = pages_map.len() as u32;
    if total_pages == 0 {
        return Err("PDF document contains 0 pages".to_string());
    }

    let filename_stem = filename.unwrap_or("document");

    for (page_num, page_id) in pages_map {
        if page_num < options.start_page {
            continue;
        }
        if !options.pages.matches(page_num, total_pages) {
            continue;
        }

        let current_number = options.start_number + (page_num - options.start_page);
        let rendered_text = interpolate_placeholders(
            &options.format,
            current_number,
            total_pages,
            current_number,
            filename_stem,
            &options.template_vars,
        );

        let (llx, lly, width, height) = get_page_box(doc, page_id)?;

        let font_alias = "F_NUM";
        let gs_alias = "GS_NUM";

        ensure_page_resources(
            doc,
            page_id,
            &options.font_name,
            font_alias,
            options.opacity,
            gs_alias,
            None,
            "",
        )?;

        let stream_bytes = generate_numbering_stream(
            &rendered_text,
            options.position,
            font_alias,
            options.font_size,
            &options.color,
            gs_alias,
            options.margin_x,
            options.margin_y,
            llx,
            lly,
            width,
            height,
        );

        if !stream_bytes.is_empty() {
            insert_page_content_stream(doc, page_id, stream_bytes, options.layer)?;
        }
    }

    if options.flatten {
        flatten_form_fields(doc, true)?;
    }
    if let Some(ref p_info) = options.piece_info {
        insert_piece_info(doc, p_info)?;
    }
    if let Some(ref locked) = options.locked_piece_info {
        insert_locked_piece_info(doc, &locked.app_name, &locked.data, &locked.secret_key)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Mathematical Calculation & PDF Graphics Generation
// ---------------------------------------------------------------------------

/// Interpolates standard and custom placeholders: {page}, {total}, {bates}, {bates:04d}, {date}, {time}, {filename}.
fn interpolate_placeholders(
    template: &str,
    page: u32,
    total: u32,
    bates: u32,
    filename: &str,
    custom: &HashMap<String, String>,
) -> String {
    let mut res = template.to_string();
    res = res.replace("{page}", &page.to_string());
    res = res.replace("{total}", &total.to_string());
    res = res.replace("{filename}", filename);

    // Bates formatting support: {bates:06d}, {bates:04d}, {bates}
    res = res.replace("{bates:08d}", &format!("{:08}", bates));
    res = res.replace("{bates:06d}", &format!("{:06}", bates));
    res = res.replace("{bates:05d}", &format!("{:05}", bates));
    res = res.replace("{bates:04d}", &format!("{:04}", bates));
    res = res.replace("{bates}", &bates.to_string());

    // Current date and time formatting
    let date_str = "2026-10-09";
    let time_str = "13:00";
    res = res.replace("{date}", date_str);
    res = res.replace("{time}", time_str);

    for (k, v) in custom {
        res = res.replace(&format!("{{{k}}}"), v);
    }
    res
}

/// Escapes special characters in PDF string literal syntax `(...)`.
fn escape_pdf_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for b in s.bytes() {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'(' => out.push_str("\\("),
            b')' => out.push_str("\\)"),
            b'\r' => out.push_str("\\r"),
            b'\n' => out.push_str("\\n"),
            _ => out.push(b as char),
        }
    }
    out
}

/// Approximate string width for standard PDF Type 1 fonts (Helvetica / Times / Courier).
fn estimate_text_width(text: &str, font_size: f64) -> f64 {
    let char_count = text.chars().count().max(1) as f64;
    // Standard Latin proportional font glyphs average ~0.52em in width
    char_count * 0.52 * font_size
}

/// Calculates smart adaptive font size if user omitted `--font-size`.
/// Uses page diagonal and text length to fit comfortably without overflowing.
fn calculate_adaptive_font_size(
    text: &str,
    width: f64,
    height: f64,
    position: &WatermarkPosition,
) -> f64 {
    let char_count = text.chars().count().max(1) as f64;
    match position {
        WatermarkPosition::Diagonal => {
            let diagonal = (width * width + height * height).sqrt();
            // Target text length spanning ~65% of page diagonal
            let target_span = diagonal * 0.65;
            let computed = target_span / (0.52 * char_count);
            computed.clamp(14.0, 110.0)
        }
        WatermarkPosition::Center => {
            let target_span = width * 0.70;
            let computed = target_span / (0.52 * char_count);
            computed.clamp(14.0, 72.0)
        }
        WatermarkPosition::Tiled { .. } => 18.0,
        _ => 24.0,
    }
}

/// Generates PDF stream commands for text watermark.
fn generate_text_watermark_stream(
    text: &str,
    position: &WatermarkPosition,
    custom_rotation: Option<f64>,
    font_alias: &str,
    font_size: f64,
    color: &ColorRgb,
    gs_alias: &str,
    llx: f64,
    lly: f64,
    width: f64,
    height: f64,
) -> Vec<u8> {
    let escaped = escape_pdf_string(text);
    let est_w = estimate_text_width(text, font_size);
    let cap_h = font_size * 0.70;

    let mut stream = String::new();
    stream.push_str("q\n");
    // Activate transparency ExtGState
    stream.push_str(&format!("/{gs_alias} gs\n"));
    // Set fill color
    stream.push_str(&format!(
        "{:.4} {:.4} {:.4} rg\n",
        color.r, color.g, color.b
    ));
    // Set font
    stream.push_str(&format!("/{font_alias} {:.2} Tf\n", font_size));

    match position {
        WatermarkPosition::Diagonal => {
            // Auto diagonal angle: theta = atan(height / width)
            let angle_rad = custom_rotation
                .map(|deg| deg.to_radians())
                .unwrap_or_else(|| (height / width).atan());
            let cos = angle_rad.cos();
            let sin = angle_rad.sin();
            let cx = llx + width / 2.0;
            let cy = lly + height / 2.0;

            // Transform matrix: center of page, rotate by angle, center text
            stream.push_str(&format!(
                "1 0 0 1 {:.4} {:.4} cm\n{:.6} {:.6} {:.6} {:.6} 0 0 cm\n1 0 0 1 {:.4} {:.4} cm\n({escaped}) Tj\n",
                cx,
                cy,
                cos,
                sin,
                -sin,
                cos,
                -est_w / 2.0,
                -cap_h / 2.0
            ));
        }
        WatermarkPosition::Center => {
            let angle_rad = custom_rotation.unwrap_or(0.0).to_radians();
            let cos = angle_rad.cos();
            let sin = angle_rad.sin();
            let cx = llx + width / 2.0;
            let cy = lly + height / 2.0;

            stream.push_str(&format!(
                "1 0 0 1 {:.4} {:.4} cm\n{:.6} {:.6} {:.6} {:.6} 0 0 cm\n1 0 0 1 {:.4} {:.4} cm\n({escaped}) Tj\n",
                cx,
                cy,
                cos,
                sin,
                -sin,
                cos,
                -est_w / 2.0,
                -cap_h / 2.0
            ));
        }
        WatermarkPosition::Tiled { step_x, step_y } => {
            let angle_rad = custom_rotation.unwrap_or(45.0).to_radians();
            let cos = angle_rad.cos();
            let sin = angle_rad.sin();
            let step_x = step_x.max(80.0);
            let step_y = step_y.max(80.0);

            let mut cur_y = lly + 30.0;
            while cur_y < lly + height {
                let mut cur_x = llx + 30.0;
                while cur_x < llx + width {
                    stream.push_str("q\n");
                    stream.push_str(&format!(
                        "1 0 0 1 {:.4} {:.4} cm\n{:.6} {:.6} {:.6} {:.6} 0 0 cm\n1 0 0 1 {:.4} {:.4} cm\n({escaped}) Tj\nQ\n",
                        cur_x,
                        cur_y,
                        cos,
                        sin,
                        -sin,
                        cos,
                        -est_w / 2.0,
                        -cap_h / 2.0
                    ));
                    cur_x += step_x;
                }
                cur_y += step_y;
            }
        }
        _ => {
            // Anchor-based positions
            let (target_x, target_y) = match position {
                WatermarkPosition::TopLeft => (llx + 36.0, lly + height - 36.0 - cap_h),
                WatermarkPosition::TopCenter => {
                    (llx + (width - est_w) / 2.0, lly + height - 36.0 - cap_h)
                }
                WatermarkPosition::TopRight => {
                    (llx + width - 36.0 - est_w, lly + height - 36.0 - cap_h)
                }
                WatermarkPosition::CenterLeft => (llx + 36.0, lly + (height - cap_h) / 2.0),
                WatermarkPosition::CenterRight => {
                    (llx + width - 36.0 - est_w, lly + (height - cap_h) / 2.0)
                }
                WatermarkPosition::BottomLeft => (llx + 36.0, lly + 36.0),
                WatermarkPosition::BottomCenter => (llx + (width - est_w) / 2.0, lly + 36.0),
                WatermarkPosition::BottomRight => (llx + width - 36.0 - est_w, lly + 36.0),
                WatermarkPosition::Custom { x, y } => (*x, *y),
                _ => (llx + 36.0, lly + 36.0),
            };

            let angle_rad = custom_rotation.unwrap_or(0.0).to_radians();
            let cos = angle_rad.cos();
            let sin = angle_rad.sin();

            stream.push_str(&format!(
                "1 0 0 1 {:.4} {:.4} cm\n{:.6} {:.6} {:.6} {:.6} 0 0 cm\n({escaped}) Tj\n",
                target_x, target_y, cos, sin, -sin, cos
            ));
        }
    }

    stream.push_str("Q\n");
    stream.into_bytes()
}

/// Generates PDF stream commands for page numbering (header / footer).
fn generate_numbering_stream(
    text: &str,
    position: NumberingPosition,
    font_alias: &str,
    font_size: f64,
    color: &ColorRgb,
    gs_alias: &str,
    margin_x: f64,
    margin_y: f64,
    llx: f64,
    lly: f64,
    width: f64,
    height: f64,
) -> Vec<u8> {
    let escaped = escape_pdf_string(text);
    let est_w = estimate_text_width(text, font_size);
    let cap_h = font_size * 0.70;

    let (x, y) = match position {
        NumberingPosition::BottomCenter => (llx + (width - est_w) / 2.0, lly + margin_y),
        NumberingPosition::BottomLeft => (llx + margin_x, lly + margin_y),
        NumberingPosition::BottomRight => (llx + width - margin_x - est_w, lly + margin_y),
        NumberingPosition::TopCenter => {
            (llx + (width - est_w) / 2.0, lly + height - margin_y - cap_h)
        }
        NumberingPosition::TopLeft => (llx + margin_x, lly + height - margin_y - cap_h),
        NumberingPosition::TopRight => (
            llx + width - margin_x - est_w,
            lly + height - margin_y - cap_h,
        ),
    };

    format!(
        "q\n/{gs_alias} gs\n{:.4} {:.4} {:.4} rg\n/{font_alias} {:.2} Tf\n1 0 0 1 {:.4} {:.4} cm\n({escaped}) Tj\nQ\n",
        color.r, color.g, color.b, font_size, x, y
    )
    .into_bytes()
}

/// Generates PDF stream commands for image watermark / stamp.
fn generate_image_watermark_stream(
    position: &WatermarkPosition,
    custom_rotation: Option<f64>,
    img_alias: &str,
    native_w: f64,
    native_h: f64,
    req_w: Option<f64>,
    req_h: Option<f64>,
    gs_alias: &str,
    llx: f64,
    lly: f64,
    page_w: f64,
    page_h: f64,
) -> Vec<u8> {
    // Determine target image draw width and height
    let (draw_w, draw_h) = match (req_w, req_h) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, native_h * (w / native_w.max(1.0))),
        (None, Some(h)) => (native_w * (h / native_h.max(1.0)), h),
        (None, None) => {
            // Default: fit image into ~40% of page dimension
            let scale = ((page_w * 0.40) / native_w.max(1.0))
                .min((page_h * 0.40) / native_h.max(1.0))
                .min(1.0);
            (native_w * scale, native_h * scale)
        }
    };

    let (cx, cy) = match position {
        WatermarkPosition::Center | WatermarkPosition::Diagonal => {
            (llx + page_w / 2.0, lly + page_h / 2.0)
        }
        WatermarkPosition::TopLeft => (
            llx + 36.0 + draw_w / 2.0,
            lly + page_h - 36.0 - draw_h / 2.0,
        ),
        WatermarkPosition::TopCenter => (llx + page_w / 2.0, lly + page_h - 36.0 - draw_h / 2.0),
        WatermarkPosition::TopRight => (
            llx + page_w - 36.0 - draw_w / 2.0,
            lly + page_h - 36.0 - draw_h / 2.0,
        ),
        WatermarkPosition::BottomLeft => (llx + 36.0 + draw_w / 2.0, lly + 36.0 + draw_h / 2.0),
        WatermarkPosition::BottomCenter => (llx + page_w / 2.0, lly + 36.0 + draw_h / 2.0),
        WatermarkPosition::BottomRight => (
            llx + page_w - 36.0 - draw_w / 2.0,
            lly + 36.0 + draw_h / 2.0,
        ),
        WatermarkPosition::Custom { x, y } => (*x + draw_w / 2.0, *y + draw_h / 2.0),
        _ => (llx + page_w / 2.0, lly + page_h / 2.0),
    };

    let angle_rad = custom_rotation.unwrap_or(0.0).to_radians();
    let cos = angle_rad.cos();
    let sin = angle_rad.sin();

    format!(
        "q\n/{gs_alias} gs\n1 0 0 1 {:.4} {:.4} cm\n{:.6} {:.6} {:.6} {:.6} 0 0 cm\n1 0 0 1 {:.4} {:.4} cm\n{:.4} 0 0 {:.4} 0 0 cm\n/{img_alias} Do\nQ\n",
        cx,
        cy,
        cos,
        sin,
        -sin,
        cos,
        -draw_w / 2.0,
        -draw_h / 2.0,
        draw_w,
        draw_h
    )
    .into_bytes()
}

// ---------------------------------------------------------------------------
// Low-Level PDF Document Manipulation Helpers
// ---------------------------------------------------------------------------

/// Gets page boundary coordinates (llx, lly, width, height) from MediaBox or CropBox.
fn get_page_box(doc: &Document, page_id: ObjectId) -> Result<(f64, f64, f64, f64), String> {
    let page_obj = doc
        .get_object(page_id)
        .map_err(|e| format!("Failed to get page object {page_id:?}: {e}"))?;
    let page_dict = page_obj
        .as_dict()
        .map_err(|e| format!("Page object is not a dictionary: {e}"))?;

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

    // Default standard A4 in points
    Ok((0.0, 0.0, 595.28, 841.89))
}

/// Ensures page Resources contains Font, ExtGState (transparency), and optional Image XObject.
fn ensure_page_resources(
    doc: &mut Document,
    page_id: ObjectId,
    font_name: &str,
    font_alias: &str,
    opacity: f64,
    gs_alias: &str,
    image_xobject: Option<(ObjectId, f64, f64)>,
    img_alias: &str,
) -> Result<(), String> {
    let resources_obj = {
        let page = doc
            .get_object(page_id)
            .map_err(|e| e.to_string())?
            .as_dict()
            .map_err(|e| e.to_string())?;
        match page.get(b"Resources") {
            Ok(Object::Dictionary(d)) => Some(d.clone()),
            Ok(Object::Reference(res_id)) => doc
                .get_object(*res_id)
                .ok()
                .and_then(|o| o.as_dict().ok())
                .cloned(),
            _ => None,
        }
    };

    let mut resources = resources_obj.unwrap_or_else(Dictionary::new);

    // 1. Font dictionary
    let mut font_dict = match resources.get(b"Font") {
        Ok(Object::Dictionary(d)) => d.clone(),
        _ => Dictionary::new(),
    };
    if !font_dict.has(font_alias.as_bytes()) {
        font_dict.set(
            font_alias,
            dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => font_name,
            },
        );
        resources.set("Font", Object::Dictionary(font_dict));
    }

    // 2. ExtGState dictionary (transparency /ca and /CA)
    let mut ext_gstate_dict = match resources.get(b"ExtGState") {
        Ok(Object::Dictionary(d)) => d.clone(),
        _ => Dictionary::new(),
    };
    ext_gstate_dict.set(
        gs_alias,
        dictionary! {
            "Type" => "ExtGState",
            "ca" => Object::Real(opacity as f32),
            "CA" => Object::Real(opacity as f32),
        },
    );
    resources.set("ExtGState", Object::Dictionary(ext_gstate_dict));

    // 3. XObject dictionary (if image watermark present)
    if let Some((img_obj_id, _, _)) = image_xobject {
        let mut xobj_dict = match resources.get(b"XObject") {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };
        xobj_dict.set(img_alias, Object::Reference(img_obj_id));
        resources.set("XObject", Object::Dictionary(xobj_dict));
    }

    // Re-attach resources dictionary to page
    let page_mut = doc
        .get_object_mut(page_id)
        .map_err(|e| e.to_string())?
        .as_dict_mut()
        .map_err(|e| e.to_string())?;
    page_mut.set("Resources", Object::Dictionary(resources));

    Ok(())
}

/// Creates a PDF Image XObject from raw image bytes (JPEG or PNG via image crate).
fn create_watermark_image_xobject(
    doc: &mut Document,
    img_bytes: &[u8],
) -> Result<(ObjectId, f64, f64), String> {
    // Try fast JPEG detection first
    if img_bytes.starts_with(&[0xFF, 0xD8])
        && let Ok(dyn_img) = image::load_from_memory(img_bytes)
    {
        let width = dyn_img.width() as f64;
        let height = dyn_img.height() as f64;
        let obj_id = doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => width as i64,
                "Height" => height as i64,
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
                "Filter" => "DCTDecode",
            },
            img_bytes.to_vec(),
        ));
        return Ok((obj_id, width, height));
    }

    // Generic fallback: load with image crate and encode to JPEG
    let dyn_img = image::load_from_memory(img_bytes)
        .map_err(|e| format!("Failed to decode watermark image: {e}"))?;
    let width = dyn_img.width() as f64;
    let height = dyn_img.height() as f64;

    let mut jpeg_buf = std::io::Cursor::new(Vec::new());
    dyn_img
        .write_to(&mut jpeg_buf, image::ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode image to JPEG: {e}"))?;
    let jpeg_bytes = jpeg_buf.into_inner();

    let obj_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => width as i64,
            "Height" => height as i64,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "Filter" => "DCTDecode",
        },
        jpeg_bytes,
    ));

    Ok((obj_id, width, height))
}

/// Appends or prepends a new content stream to a page's /Contents.
fn insert_page_content_stream(
    doc: &mut Document,
    page_id: ObjectId,
    content_bytes: Vec<u8>,
    layer: LayerMode,
) -> Result<(), String> {
    let new_stream_id = doc.add_object(Stream::new(Dictionary::new(), content_bytes));
    let page = doc
        .get_object_mut(page_id)
        .map_err(|e| e.to_string())?
        .as_dict_mut()
        .map_err(|e| e.to_string())?;

    match page.get_mut(b"Contents") {
        Ok(Object::Array(arr)) => match layer {
            LayerMode::Over => arr.push(Object::Reference(new_stream_id)),
            LayerMode::Under => arr.insert(0, Object::Reference(new_stream_id)),
        },
        Ok(Object::Reference(existing_id)) => {
            let existing = *existing_id;
            let new_arr = match layer {
                LayerMode::Over => vec![
                    Object::Reference(existing),
                    Object::Reference(new_stream_id),
                ],
                LayerMode::Under => vec![
                    Object::Reference(new_stream_id),
                    Object::Reference(existing),
                ],
            };
            page.set("Contents", Object::Array(new_arr));
        }
        _ => {
            page.set("Contents", Object::Reference(new_stream_id));
        }
    }
    Ok(())
}
