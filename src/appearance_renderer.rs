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
