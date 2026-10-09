#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use crate::get_piece_info;
    use crate::ops::image_metadata::{ImageFormatType, inject_image_metadata};
    use crate::ops::split::parse_split_ranges;
    use image::ImageFormat;
    use image::codecs::jpeg::JpegEncoder;
    use pdfium_bundled::bind_pdfium_silent;
    use pdfium_bundled::pdfium_render::prelude::*;
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use std::io::Cursor;
    use std::sync::Mutex;

    static PDFIUM_INIT: Mutex<()> = Mutex::new(());

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum OutputImageFormat {
        #[default]
        Png,
        Jpeg,
    }

    impl OutputImageFormat {
        pub fn extension(&self) -> &'static str {
            match self {
                Self::Png => "png",
                Self::Jpeg => "jpg",
            }
        }

        pub fn to_image_crate_format(&self) -> ImageFormat {
            match self {
                Self::Png => ImageFormat::Png,
                Self::Jpeg => ImageFormat::Jpeg,
            }
        }

        pub fn to_metadata_format(&self) -> ImageFormatType {
            match self {
                Self::Png => ImageFormatType::Png,
                Self::Jpeg => ImageFormatType::Jpeg,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct RenderOptions {
        #[serde(default = "default_dpi")]
        pub dpi: f32,
        #[serde(default)]
        pub format: OutputImageFormat,
        #[serde(default = "default_quality")]
        pub jpeg_quality: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub pages: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub max_width: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub max_height: Option<u32>,
        #[serde(default)]
        pub transparent_background: bool,
        #[serde(default = "default_true")]
        pub render_annotations: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub piece_info: Option<Value>,
        #[serde(default = "default_true")]
        pub inherit_pdf_piece_info: bool,
    }

    fn default_dpi() -> f32 {
        150.0
    }

    fn default_quality() -> u8 {
        85
    }

    fn default_true() -> bool {
        true
    }

    impl Default for RenderOptions {
        fn default() -> Self {
            Self {
                dpi: default_dpi(),
                format: OutputImageFormat::Png,
                jpeg_quality: default_quality(),
                pages: None,
                max_width: None,
                max_height: None,
                transparent_background: false,
                render_annotations: true,
                piece_info: None,
                inherit_pdf_piece_info: true,
            }
        }
    }

    impl RenderOptions {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn dpi(mut self, dpi: f32) -> Self {
            self.dpi = dpi;
            self
        }

        pub fn format(mut self, format: OutputImageFormat) -> Self {
            self.format = format;
            self
        }

        pub fn jpeg_quality(mut self, quality: u8) -> Self {
            self.jpeg_quality = quality.clamp(1, 100);
            self
        }

        pub fn pages(mut self, pages_spec: impl Into<String>) -> Self {
            self.pages = Some(pages_spec.into());
            self
        }

        pub fn max_width(mut self, width: u32) -> Self {
            self.max_width = Some(width);
            self
        }

        pub fn max_height(mut self, height: u32) -> Self {
            self.max_height = Some(height);
            self
        }

        pub fn transparent_background(mut self, transparent: bool) -> Self {
            self.transparent_background = transparent;
            self
        }

        pub fn render_annotations(mut self, enabled: bool) -> Self {
            self.render_annotations = enabled;
            self
        }

        pub fn piece_info(mut self, piece_info: Value) -> Self {
            self.piece_info = Some(piece_info);
            self
        }

        pub fn inherit_pdf_piece_info(mut self, inherit: bool) -> Self {
            self.inherit_pdf_piece_info = inherit;
            self
        }
    }

    /// Obtains a ready-to-use Pdfium instance.
    pub fn get_pdfium() -> Result<Pdfium, String> {
        let _lock = PDFIUM_INIT.lock().map_err(|e| e.to_string())?;
        if let Ok(bindings) = Pdfium::bind_to_system_library() {
            return Ok(Pdfium::new(bindings));
        }
        match bind_pdfium_silent() {
            Ok(p) => Ok(p),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("PdfiumLibraryBindingsAlreadyInitialized") {
                    Ok(Pdfium::default())
                } else {
                    Err(format!("Failed to initialize PDFium engine: {err_str}"))
                }
            }
        }
    }

    fn build_render_config(options: &RenderOptions) -> PdfRenderConfig {
        let scale = if options.dpi > 0.0 {
            options.dpi / 72.0
        } else {
            2.0
        };
        let mut config = PdfRenderConfig::new()
            .scale_page_by_factor(scale)
            .render_form_data(true)
            .render_annotations(options.render_annotations);

        if let Some(w) = options.max_width {
            config = config.set_target_width(w as i32);
        }
        if let Some(h) = options.max_height {
            config = config.set_maximum_height(h as i32);
        }

        if options.transparent_background && options.format == OutputImageFormat::Png {
            config = config.set_clear_color(PdfColor::new(0, 0, 0, 0));
        } else {
            config = config.set_clear_color(PdfColor::new(255, 255, 255, 255));
        }

        config
    }

    /// Renders a single PDF page (1-indexed) to image bytes with full options.
    pub fn render_pdf_page_with_options(
        pdf_bytes: &[u8],
        page_num: u32,
        options: &RenderOptions,
    ) -> Result<Vec<u8>, String> {
        let pdfium = get_pdfium()?;
        let document = pdfium
            .load_pdf_from_byte_slice(pdf_bytes, None)
            .map_err(|e| format!("Failed to parse PDF: {e}"))?;

        let pages = document.pages();
        let total = pages.len();
        if page_num == 0 || page_num > total as u32 {
            return Err(format!(
                "Page number {page_num} is out of bounds (1..={total})"
            ));
        }

        let page_index = (page_num - 1) as i32;
        let page = pages
            .get(page_index)
            .map_err(|e| format!("Failed to load page {page_num}: {e}"))?;

        let render_config = build_render_config(options);
        let bitmap = page
            .render_with_config(&render_config)
            .map_err(|e| format!("Failed to render page {page_num}: {e}"))?;

        let dynamic_image = bitmap
            .as_image()
            .map_err(|e| format!("Failed to convert bitmap to image: {e}"))?;

        let mut raw_buffer = Cursor::new(Vec::new());
        match options.format {
            OutputImageFormat::Png => {
                dynamic_image
                    .write_to(&mut raw_buffer, ImageFormat::Png)
                    .map_err(|e| format!("Failed to encode PNG image: {e}"))?;
            }
            OutputImageFormat::Jpeg => {
                let mut encoder =
                    JpegEncoder::new_with_quality(&mut raw_buffer, options.jpeg_quality);
                encoder
                    .encode_image(&dynamic_image)
                    .map_err(|e| format!("Failed to encode JPEG image: {e}"))?;
            }
        }

        let raw_bytes = raw_buffer.into_inner();

        // Metadata injection
        let metadata_to_inject = if let Some(ref pi) = options.piece_info {
            Some(pi.clone())
        } else if options.inherit_pdf_piece_info {
            get_piece_info(pdf_bytes).ok().flatten()
        } else {
            None
        };

        if let Some(ref meta) = metadata_to_inject {
            inject_image_metadata(&raw_bytes, meta, options.format.to_metadata_format())
        } else {
            Ok(raw_bytes)
        }
    }

    /// Renders a single PDF page (1-indexed) to image bytes with simple DPI and format.
    pub fn render_pdf_page(
        pdf_bytes: &[u8],
        page_num: u32,
        dpi: f32,
        format: OutputImageFormat,
    ) -> Result<Vec<u8>, String> {
        let opts = RenderOptions::new().dpi(dpi).format(format);
        render_pdf_page_with_options(pdf_bytes, page_num, &opts)
    }

    /// Renders pages of a PDF to a list of (page_num, image_bytes) with full options.
    pub fn render_pdf_to_images_with_options(
        pdf_bytes: &[u8],
        options: &RenderOptions,
    ) -> Result<Vec<(u32, Vec<u8>)>, String> {
        let pdfium = get_pdfium()?;
        let document = pdfium
            .load_pdf_from_byte_slice(pdf_bytes, None)
            .map_err(|e| format!("Failed to parse PDF: {e}"))?;

        let pages = document.pages();
        let total_pages = pages.len() as u32;

        let target_page_numbers: Vec<u32> = if let Some(ref page_spec) = options.pages {
            let split_ranges = parse_split_ranges(page_spec, total_pages)?;
            let mut list = Vec::new();
            for r in split_ranges {
                list.extend(r.pages);
            }
            list
        } else {
            (1..=total_pages).collect()
        };

        let render_config = build_render_config(options);
        let metadata_to_inject = if let Some(ref pi) = options.piece_info {
            Some(pi.clone())
        } else if options.inherit_pdf_piece_info {
            get_piece_info(pdf_bytes).ok().flatten()
        } else {
            None
        };

        let mut rendered_pages = Vec::with_capacity(target_page_numbers.len());

        for &page_num in &target_page_numbers {
            let page_index = (page_num - 1) as i32;
            let page = pages
                .get(page_index)
                .map_err(|e| format!("Failed to load page {page_num}: {e}"))?;

            let bitmap = page
                .render_with_config(&render_config)
                .map_err(|e| format!("Failed to render page {page_num}: {e}"))?;

            let dynamic_image = bitmap
                .as_image()
                .map_err(|e| format!("Failed to convert bitmap to image: {e}"))?;

            let mut raw_buffer = Cursor::new(Vec::new());
            match options.format {
                OutputImageFormat::Png => {
                    dynamic_image
                        .write_to(&mut raw_buffer, ImageFormat::Png)
                        .map_err(|e| format!("Failed to encode PNG image: {e}"))?;
                }
                OutputImageFormat::Jpeg => {
                    let mut encoder =
                        JpegEncoder::new_with_quality(&mut raw_buffer, options.jpeg_quality);
                    encoder
                        .encode_image(&dynamic_image)
                        .map_err(|e| format!("Failed to encode JPEG image: {e}"))?;
                }
            }

            let raw_bytes = raw_buffer.into_inner();
            let final_bytes = if let Some(ref meta) = metadata_to_inject {
                inject_image_metadata(&raw_bytes, meta, options.format.to_metadata_format())?
            } else {
                raw_bytes
            };

            rendered_pages.push((page_num, final_bytes));
        }

        Ok(rendered_pages)
    }

    /// Renders all pages of a PDF to a list of (page_num, image_bytes) with simple DPI and format.
    pub fn render_pdf_to_images(
        pdf_bytes: &[u8],
        dpi: f32,
        format: OutputImageFormat,
    ) -> Result<Vec<(u32, Vec<u8>)>, String> {
        let opts = RenderOptions::new().dpi(dpi).format(format);
        render_pdf_to_images_with_options(pdf_bytes, &opts)
    }
}
