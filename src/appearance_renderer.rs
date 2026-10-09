use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

pub trait AppearanceRenderer {
    fn render_image(
        &self,
        doc: &mut Document,
        image: ObjectId,
        width: f64,
        height: f64,
        image_width: f64,
        image_height: f64,
    ) -> ObjectId;
}

pub struct ImageAppearanceRenderer;

impl AppearanceRenderer for ImageAppearanceRenderer {
    fn render_image(
        &self,
        doc: &mut Document,
        image: ObjectId,
        width: f64,
        height: f64,
        image_width: f64,
        image_height: f64,
    ) -> ObjectId {
        let margin = 2.0;
        let available_width = (width - margin * 2.0).max(0.0);
        let available_height = (height - margin * 2.0).max(0.0);
        let scale = (available_width / image_width).min(available_height / image_height);
        let draw_width = image_width * scale;
        let draw_height = image_height * scale;
        let x = margin + (available_width - draw_width) / 2.0;
        let y = margin + (available_height - draw_height) / 2.0;

        let image_form = doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "FormType" => 1,
                "BBox" => vec![
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Real(image_width as f32),
                    Object::Real(image_height as f32),
                ],
                "Matrix" => vec![
                    Object::Integer(1),
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Integer(1),
                    Object::Integer(0),
                    Object::Integer(0),
                ],
                "Resources" => dictionary! {
                    "XObject" => dictionary! {
                        "img0" => image,
                    }
                },
            },
            format!("q\n{image_width:.6} 0 0 {image_height:.6} 0 0 cm\n/img0 Do\nQ\n").into_bytes(),
        ));

        let content = format!(
            "q Q q {margin:.6} {margin:.6} {available_width:.6} {available_height:.6} re W n q {scale:.6} 0 0 {scale:.6} {x:.6} {y:.6} cm /FRM Do Q Q\n"
        );

        doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "FormType" => 1,
                "BBox" => vec![
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Real(width as f32),
                    Object::Real(height as f32),
                ],
                "Matrix" => vec![
                    Object::Integer(1),
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Integer(1),
                    Object::Integer(0),
                    Object::Integer(0),
                ],
                "Resources" => dictionary! {
                    "XObject" => dictionary! {
                        "FRM" => image_form,
                    }
                },
            },
            content.into_bytes(),
        ))
    }
}

pub fn replace_appearance(field: &mut Dictionary, appearance_id: ObjectId) {
    let mut ap = match field.get(b"AP") {
        Ok(Object::Dictionary(existing)) => existing.clone(),
        _ => Dictionary::new(),
    };
    ap.set("N", appearance_id);
    field.set("AP", ap);
}

use crate::appearance::{GraphicPosition, SignatureAppearanceOptions, TextAlign};

pub fn escape_pdf_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out
}

pub fn render_signature_appearance(
    doc: &mut Document,
    width: f64,
    height: f64,
    options: &SignatureAppearanceOptions,
) -> Result<ObjectId, String> {
    let labels = options.labels.clone().unwrap_or_default();
    let mut lines = Vec::new();
    if options.show_signer_name {
        if let Some(name) = &options.signer_name {
            lines.push(format!("{}{}", labels.signed_by, name));
        }
    }
    if options.show_date {
        if let Some(date) = &options.date {
            lines.push(format!("{}{}", labels.date, date));
        }
    }
    if options.show_reason {
        if let Some(reason) = &options.reason {
            lines.push(format!("{}{}", labels.reason, reason));
        }
    }
    if options.show_location {
        if let Some(loc) = &options.location {
            lines.push(format!("{}{}", labels.location, loc));
        }
    }
    lines.extend(options.extra_lines.clone());

    let mut image_info = None;
    let image_encoded = options
        .image
        .as_deref()
        .or_else(|| options.design.as_ref().and_then(|d| d.image.as_deref()));
    if let Some(encoded) = image_encoded {
        let image_bytes = crate::parse_image(encoded)?;
        let (iw, ih) = crate::jpeg_size(&image_bytes)?;
        let image_id = crate::create_image(doc, image_bytes, iw, ih);
        image_info = Some((image_id, iw, ih));
    }

    let has_image = image_info.is_some();
    let has_text = !lines.is_empty()
        || options
            .design
            .as_ref()
            .map(|d| !d.text_lines.is_empty())
            .unwrap_or(false);

    let raw_pos = if let Some(d) = &options.design {
        if options.position == GraphicPosition::Left && d.position != GraphicPosition::Left {
            d.position
        } else {
            options.position
        }
    } else {
        options.position
    };

    let position = match raw_pos {
        GraphicPosition::ImageOnly => GraphicPosition::ImageOnly,
        GraphicPosition::TextOnly => GraphicPosition::TextOnly,
        _ if has_image && !has_text => GraphicPosition::ImageOnly,
        _ if !has_image && has_text => GraphicPosition::TextOnly,
        pos => pos,
    };

    if position == GraphicPosition::ImageOnly {
        if let Some((image_id, iw, ih)) = image_info {
            return Ok(ImageAppearanceRenderer.render_image(doc, image_id, width, height, iw, ih));
        }
    }

    let m = options.margin.unwrap_or(2.0).max(0.0);
    let total_w = (width - 2.0 * m).max(1.0);
    let total_h = (height - 2.0 * m).max(1.0);
    let ratio = options.image_width_ratio.unwrap_or(0.40).clamp(0.1, 0.9);

    let (img_box, txt_box) = match position {
        GraphicPosition::Left => {
            let iw_box = total_w * ratio;
            let tw_box = total_w * (1.0 - ratio) - 2.0;
            (
                Some((m, m, iw_box, total_h)),
                Some((m + iw_box + 2.0, m, tw_box.max(1.0), total_h)),
            )
        }
        GraphicPosition::Right => {
            let iw_box = total_w * ratio;
            let tw_box = total_w * (1.0 - ratio) - 2.0;
            (
                Some((m + tw_box + 2.0, m, iw_box, total_h)),
                Some((m, m, tw_box.max(1.0), total_h)),
            )
        }
        GraphicPosition::Behind => (
            Some((m, m, total_w, total_h)),
            Some((m, m, total_w, total_h)),
        ),
        GraphicPosition::TextOnly => (None, Some((m, m, total_w, total_h))),
        GraphicPosition::ImageOnly => (Some((m, m, total_w, total_h)), None),
    };

    let mut stream_content = String::new();

    let custom_design_lines = options
        .design
        .as_ref()
        .map(|d| &d.text_lines)
        .filter(|tl| !tl.is_empty());

    let has_custom_lines = custom_design_lines.is_some();
    let has_text = !lines.is_empty() || has_custom_lines;

    // 1. Render Image
    if let Some((_, iw, ih)) = image_info {
        if let Some([ix, iy, dw, dh]) = options.design.as_ref().and_then(|d| d.image_bounds) {
            stream_content.push_str(&format!(
                "q {dw:.4} 0 0 {dh:.4} {ix:.4} {iy:.4} cm /img0 Do Q\n"
            ));
        } else if let Some((bx, by, bw, bh)) = img_box {
            let scale = (bw / iw).min(bh / ih);
            let dw = iw * scale;
            let dh = ih * scale;
            let ix = bx + (bw - dw) / 2.0;
            let iy = by + (bh - dh) / 2.0;
            stream_content.push_str(&format!(
                "q {dw:.4} 0 0 {dh:.4} {ix:.4} {iy:.4} cm /img0 Do Q\n"
            ));
        }
    }

    // 2. Render Text
    if let Some(custom_lines) = custom_design_lines {
        let (tx, ty, tw, th) = txt_box.unwrap_or((m, m, total_w, total_h));
        let n = custom_lines.len();
        let default_fs = options
            .font_size
            .unwrap_or_else(|| (th / (n.max(1) as f64 * 1.35)).clamp(6.0, 11.0));
        let lh = default_fs * 1.3;
        let tot_h = n as f64 * lh;
        let default_start_y = ty + (th + tot_h) / 2.0 - default_fs;

        for (i, line) in custom_lines.iter().enumerate() {
            let clean_line = escape_pdf_string(&line.text);
            let fs = line.font_size.or(options.font_size).unwrap_or(default_fs);
            let [r, g, b] = line
                .color_rgb
                .or(options.text_color)
                .unwrap_or([20, 20, 20]);
            let r_f = r as f64 / 255.0;
            let g_f = g as f64 / 255.0;
            let b_f = b as f64 / 255.0;

            let y = line.y.unwrap_or(default_start_y - (i as f64 * lh));
            let x = if let Some(cx) = line.x {
                cx
            } else {
                let lw = line.text.chars().count() as f64 * (fs * 0.52);
                match options.align {
                    TextAlign::Left => tx,
                    TextAlign::Center => tx + (tw - lw).max(0.0) / 2.0,
                    TextAlign::Right => tx + (tw - lw).max(0.0),
                }
            };

            stream_content.push_str(&format!(
                "BT /F1 {fs:.2} Tf {r_f:.3} {g_f:.3} {b_f:.3} rg 1 0 0 1 {x:.2} {y:.2} Tm ({clean_line}) Tj ET\n"
            ));
        }
    } else if let Some((tx, ty, tw, th)) = txt_box {
        if has_text {
            let n = lines.len();
            let fs = options
                .font_size
                .unwrap_or_else(|| (th / (n.max(1) as f64 * 1.35)).clamp(6.0, 11.0));
            let lh = fs * 1.3;
            let tot_h = n as f64 * lh;
            let start_y = ty + (th + tot_h) / 2.0 - fs;
            let [r, g, b] = options.text_color.unwrap_or([20, 20, 20]);
            let r_f = r as f64 / 255.0;
            let g_f = g as f64 / 255.0;
            let b_f = b as f64 / 255.0;

            for (i, line) in lines.iter().enumerate() {
                let y = start_y - (i as f64 * lh);
                let clean_line = escape_pdf_string(line);
                let lw = line.chars().count() as f64 * (fs * 0.52);
                let x = match options.align {
                    TextAlign::Left => tx,
                    TextAlign::Center => tx + (tw - lw).max(0.0) / 2.0,
                    TextAlign::Right => tx + (tw - lw).max(0.0),
                };
                stream_content.push_str(&format!(
                    "BT /F1 {fs:.2} Tf {r_f:.3} {g_f:.3} {b_f:.3} rg 1 0 0 1 {x:.2} {y:.2} Tm ({clean_line}) Tj ET\n"
                ));
            }
        }
    }

    let mut resources = Dictionary::new();
    if has_text {
        let font_name = options.font.pdf_base_font(options.bold, options.italic);
        resources.set(
            "Font",
            dictionary! {
                "F1" => dictionary! {
                    "Type" => "Font",
                    "Subtype" => "Type1",
                    "BaseFont" => font_name,
                    "Encoding" => "WinAnsiEncoding",
                }
            },
        );
    }
    if let Some((img_id, _, _)) = image_info {
        resources.set(
            "XObject",
            dictionary! {
                "img0" => img_id,
            },
        );
    }

    let form_stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => vec![
                Object::Integer(0), Object::Integer(0),
                Object::Real(width as f32), Object::Real(height as f32),
            ],
            "Matrix" => vec![
                Object::Integer(1), Object::Integer(0),
                Object::Integer(0), Object::Integer(1),
                Object::Integer(0), Object::Integer(0),
            ],
            "Resources" => resources,
        },
        stream_content.into_bytes(),
    );

    Ok(doc.add_object(form_stream))
}
