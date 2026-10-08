use lopdf::Document;

pub struct PdfAppearance;

impl PdfAppearance {
    pub fn set_signature_image(
        template: &[u8],
        field_name: &str,
        image: &str,
    ) -> Result<Vec<u8>, String> {
        let mut doc = Document::load_mem(template).map_err(|e| format!("PDF load failed: {e}"))?;
        let fields = super::collect_fields(&doc);
        let Some((field_id, field, field_type)) = fields.get(field_name) else {
            return Err(format!("Signature field '{field_name}' not found"));
        };
        if field_type.as_slice() != b"Sig" {
            return Err(format!("Field '{field_name}' is not a signature field"));
        }
        let value = serde_json::Value::String(image.to_owned());
        let bytes = super::parse_image(image)?;
        super::jpeg_size(&bytes)?;
        super::set_image(&mut doc, *field_id, field, &value)?;
        super::save_document(&mut doc)
    }
}
