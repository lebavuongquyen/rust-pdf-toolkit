use lopdf::Document;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum GraphicPosition {
    #[default]
    #[serde(alias = "left", alias = "LEFT")]
    Left,
    #[serde(alias = "right", alias = "RIGHT")]
    Right,
    #[serde(alias = "behind", alias = "BEHIND", alias = "background", alias = "Background")]
    Behind,
    #[serde(alias = "image_only", alias = "image-only", alias = "IMAGE_ONLY", alias = "imageOnly", alias = "image", alias = "Image")]
    ImageOnly,
    #[serde(alias = "text_only", alias = "text-only", alias = "TEXT_ONLY", alias = "textOnly", alias = "text", alias = "Text")]
    TextOnly,
}

impl std::str::FromStr for GraphicPosition {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().replace('-', "_").as_str() {
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            "behind" | "background" => Ok(Self::Behind),
            "image_only" | "imageonly" | "image" => Ok(Self::ImageOnly),
            "text_only" | "textonly" | "text" => Ok(Self::TextOnly),
            other => Err(format!(
                "Unknown graphic position '{other}'. Allowed: left, right, behind, image-only, text-only"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SignatureFont {
    #[default]
    Helvetica,
    Times,
    Courier,
}

impl SignatureFont {
    pub fn pdf_base_font(&self, bold: bool, italic: bool) -> &'static str {
        match self {
            Self::Helvetica => {
                if bold && italic {
                    "Helvetica-BoldOblique"
                } else if bold {
                    "Helvetica-Bold"
                } else if italic {
                    "Helvetica-Oblique"
                } else {
                    "Helvetica"
                }
            }
            Self::Times => {
                if bold && italic {
                    "Times-BoldItalic"
                } else if bold {
                    "Times-Bold"
                } else if italic {
                    "Times-Italic"
                } else {
                    "Times-Roman"
                }
            }
            Self::Courier => {
                if bold && italic {
                    "Courier-BoldOblique"
                } else if bold {
                    "Courier-Bold"
                } else if italic {
                    "Courier-Oblique"
                } else {
                    "Courier"
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureLabels {
    #[serde(default = "default_signed_by")]
    pub signed_by: String,
    #[serde(default = "default_date")]
    pub date: String,
    #[serde(default = "default_reason")]
    pub reason: String,
    #[serde(default = "default_location")]
    pub location: String,
}

fn default_signed_by() -> String {
    "Ký bởi: ".into()
}
fn default_date() -> String {
    "Ngày: ".into()
}
fn default_reason() -> String {
    "Lý do: ".into()
}
fn default_location() -> String {
    "Địa điểm: ".into()
}

impl Default for SignatureLabels {
    fn default() -> Self {
        Self {
            signed_by: default_signed_by(),
            date: default_date(),
            reason: default_reason(),
            location: default_location(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SignatureTextLine {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_rgb: Option<[u8; 3]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SignatureDesign {
    #[serde(default)]
    pub position: GraphicPosition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_bounds: Option<[f64; 4]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub text_lines: Vec<SignatureTextLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SignatureAppearanceOptions {
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub position: GraphicPosition,
    #[serde(default)]
    pub image_width_ratio: Option<f64>,
    #[serde(default)]
    pub margin: Option<f64>,
    #[serde(default)]
    pub font: SignatureFont,
    #[serde(default)]
    pub font_size: Option<f64>,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub align: TextAlign,
    #[serde(default)]
    pub text_color: Option<[u8; 3]>,
    #[serde(default)]
    pub show_signer_name: bool,
    #[serde(default)]
    pub signer_name: Option<String>,
    #[serde(default)]
    pub show_date: bool,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub show_reason: bool,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub show_location: bool,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub labels: Option<SignatureLabels>,
    #[serde(default)]
    pub extra_lines: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub design: Option<SignatureDesign>,
}

impl From<SignatureDesign> for SignatureAppearanceOptions {
    fn from(design: SignatureDesign) -> Self {
        Self {
            image: design.image.clone(),
            position: design.position,
            design: Some(design),
            ..Default::default()
        }
    }
}

pub struct PdfAppearance;

impl PdfAppearance {
    pub fn set_signature_image(
        template: &[u8],
        field_name: &str,
        image: &str,
    ) -> Result<Vec<u8>, String> {
        let options = SignatureAppearanceOptions {
            image: Some(image.to_string()),
            position: GraphicPosition::ImageOnly,
            ..Default::default()
        };
        Self::set_signature_appearance(template, field_name, &options)
    }

    pub fn set_signature_appearance(
        template: &[u8],
        field_name: &str,
        options: &SignatureAppearanceOptions,
    ) -> Result<Vec<u8>, String> {
        let mut doc = Document::load_mem(template).map_err(|e| format!("PDF load failed: {e}"))?;
        let fields = super::collect_fields(&doc);
        let Some((field_id, field, field_type)) = fields.get(field_name) else {
            return Err(format!("Signature field '{field_name}' not found"));
        };
        if field_type.as_slice() != b"Sig" {
            return Err(format!("Field '{field_name}' is not a signature field"));
        }

        let widgets = super::widget_ids(&doc, *field_id, field);
        if widgets.is_empty() {
            return Err(format!("Signature field '{field_name}' has no widget"));
        }

        for widget_id in widgets {
            let widget = doc
                .get_object(widget_id)
                .map_err(|e| e.to_string())?
                .as_dict()
                .map_err(|e| e.to_string())?;
            let (bw, bh) = super::rect(widget)?;
            let appearance_id =
                super::appearance_renderer::render_signature_appearance(&mut doc, bw, bh, options)?;
            let widget_mut = doc
                .get_object_mut(widget_id)
                .map_err(|e| e.to_string())?
                .as_dict_mut()
                .map_err(|e| e.to_string())?;
            super::appearance_renderer::replace_appearance(widget_mut, appearance_id);
        }

        if let Some(encoded) = &options.image {
            if let Ok(field_mut) = doc.get_object_mut(*field_id).and_then(|x| x.as_dict_mut()) {
                field_mut.set("V", super::pdf_text(encoded));
            }
        }

        super::save_document(&mut doc)
    }
}
