use pdffiller_core::{FormFieldType, get_form_fields};
use std::fs;

#[test]
fn discovers_all_form_fields_with_exact_types() {
    let template = fs::read("reference/template_8field.pdf").expect("template");
    let fields = get_form_fields(&template).expect("fields");

    let expected = [
        ("Push Button0", FormFieldType::Button),
        ("Check Box0", FormFieldType::Checkbox),
        ("Radio Button0", FormFieldType::Radio),
        ("Signature_0", FormFieldType::Signature),
        ("Text Field0", FormFieldType::Text),
        ("Combo Box0", FormFieldType::ComboBox),
        ("List Box0", FormFieldType::ListBox),
        ("Barcode Field0", FormFieldType::Barcode),
        ("Image Field0", FormFieldType::Image),
        ("Date Field0", FormFieldType::Date),
    ];

    assert_eq!(fields.len(), expected.len());

    for (name, field_type) in expected {
        let field = fields
            .iter()
            .find(|field| field.name == name)
            .unwrap_or_else(|| panic!("missing field: {name}"));

        assert_eq!(field.field_type, field_type, "wrong type for {name}");
        assert_eq!(field.page, Some(1));
        assert!(field.rect.is_some());
        assert_eq!(field.locations.len(), 1);
        assert_eq!(field.locations[0].page, 1);
    }
}
