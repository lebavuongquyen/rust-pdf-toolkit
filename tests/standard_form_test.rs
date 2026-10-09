use pdftoolkit_core::{get_form_fields, FormFieldType};
use std::fs;

#[test]
fn test_standard_form_has_all_13_fields_with_exact_types() {
    let pdf_bytes = fs::read("reference/standard_form.pdf").expect("standard_form.pdf");
    let fields = get_form_fields(&pdf_bytes).expect("fields discovery");

    assert_eq!(fields.len(), 13, "Expected 13 fields, got {}", fields.len());

    let field_map: std::collections::HashMap<_, _> =
        fields.into_iter().map(|f| (f.name, f.field_type)).collect();

    // 1. Text fields
    assert_eq!(field_map.get("full_name"), Some(&FormFieldType::Text));
    assert_eq!(field_map.get("email"), Some(&FormFieldType::Text));
    assert_eq!(field_map.get("phone"), Some(&FormFieldType::Text));
    assert_eq!(field_map.get("job_title"), Some(&FormFieldType::Text));

    // 2. Date field
    assert_eq!(field_map.get("birth_date"), Some(&FormFieldType::Date));

    // 3. ComboBox & ListBox
    assert_eq!(field_map.get("department"), Some(&FormFieldType::ComboBox));
    assert_eq!(field_map.get("skills"), Some(&FormFieldType::ListBox));

    // 4. Buttons (Image, Radio, Checkbox)
    assert_eq!(field_map.get("photo"), Some(&FormFieldType::Image));
    assert_eq!(field_map.get("gender"), Some(&FormFieldType::Radio));
    assert_eq!(field_map.get("agree_terms"), Some(&FormFieldType::Checkbox));
    assert_eq!(field_map.get("newsletter"), Some(&FormFieldType::Checkbox));

    // 5. Digital Signature fields
    assert_eq!(
        field_map.get("Signature_Applicant"),
        Some(&FormFieldType::Signature)
    );
    assert_eq!(
        field_map.get("Signature_Manager"),
        Some(&FormFieldType::Signature)
    );
}

#[test]
fn test_standard_form_filling_and_flattening() {
    let pdf_bytes = fs::read("reference/standard_form.pdf").expect("standard_form.pdf");
    let fill_data = serde_json::json!({
        "full_name": "Nguyen Van A",
        "email": "nguyen@example.com",
        "phone": "+84 901 234 567",
        "birth_date": "15/08/1995",
        "job_title": "Senior Rust Engineer",
        "department": "Engineering",
        "skills": "Rust",
        "gender": "Male",
        "agree_terms": true,
        "newsletter": false
    })
    .to_string();

    let options = pdftoolkit_core::FillOptions::new().flatten(true);
    let (filled_bytes, report) =
        pdftoolkit_core::fill_pdf_with_options(&pdf_bytes, &fill_data, &options).expect("fill");

    assert_eq!(report.filled_count(), 10);
    assert!(filled_bytes.len() > 5000);

    // After flattening non-signature fields, the 2 signature fields must remain intact
    let fields_after = get_form_fields(&filled_bytes).expect("fields after flattening");
    assert_eq!(fields_after.len(), 2);
    assert!(fields_after.iter().all(|f| f.field_type == FormFieldType::Signature));
}
