use pdffiller_core::{get_form_fields, FormFieldType};
use std::fs;

#[test]
fn discovers_all_form_fields_with_page_metadata() {
    let template = fs::read("reference/template.pdf").expect("template");
    let fields = get_form_fields(&template).expect("fields");

    assert!(!fields.is_empty());

    let signature = fields
        .iter()
        .find(|field| field.name == "Signature_0")
        .expect("signature field");

    assert!(matches!(signature.field_type, FormFieldType::Signature));
    assert_eq!(signature.page, Some(1));
    assert!(signature.rect.is_some());
    assert_eq!(signature.locations.len(), 1);
    assert_eq!(signature.locations[0].page, 1);
}
