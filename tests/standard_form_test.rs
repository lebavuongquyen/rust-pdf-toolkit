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

#[test]
fn test_standard_form_flatten_all_fields_even_without_any_values() {
    let pdf_bytes = fs::read("reference/standard_form.pdf").expect("standard_form.pdf");
    
    // Flatten without filling ANY fields at all
    let mut doc = lopdf::Document::load_mem(&pdf_bytes).expect("load doc");
    pdftoolkit_core::flatten_form_fields(&mut doc, true).expect("flatten all fields");
    let mut flattened_bytes = Vec::new();
    doc.save_to(&mut flattened_bytes).expect("save doc");

    let fields_after = get_form_fields(&flattened_bytes).expect("fields after flattening");
    // ALL 11 fields (whether empty or not) are flattened! Only 2 signature fields remain.
    assert_eq!(fields_after.len(), 2);
    assert!(fields_after.iter().all(|f| f.field_type == FormFieldType::Signature));
    assert_eq!(fields_after[0].name, "Signature_Applicant");
    assert_eq!(fields_after[1].name, "Signature_Manager");
}

#[test]
fn test_standard_form_flatten_partial_filled_flattens_all_non_signature_fields() {
    let pdf_bytes = fs::read("reference/standard_form.pdf").expect("standard_form.pdf");
    
    // Only fill 1 single field out of 13
    let fill_data = r#"{"full_name": "Only One Field Filled"}"#;
    let options = pdftoolkit_core::FillOptions::new().flatten(true);
    let (filled_bytes, report) =
        pdftoolkit_core::fill_pdf_with_options(&pdf_bytes, fill_data, &options).expect("fill");

    assert_eq!(report.filled_count(), 1);

    // ALL non-signature fields (both the 1 filled and the 10 unfilled) are flattened!
    let fields_after = get_form_fields(&filled_bytes).expect("fields after flattening");
    assert_eq!(fields_after.len(), 2);
    assert!(fields_after.iter().all(|f| f.field_type == FormFieldType::Signature));
}

#[test]
fn test_standard_form_signed_signature_is_readonly_and_never_flattened() {
    let pdf_bytes = fs::read("reference/standard_form.pdf").expect("standard_form.pdf");
    let cert = fs::read("tests/fixtures/test-signing.cert.der").expect("cert");
    let key = fs::read("tests/fixtures/test-signing.key.der").expect("key");

    let signer = pdftoolkit_core::CertificateSigner::from_pkcs8_der(cert, &key).expect("signer");
    let signed_bytes = pdftoolkit_core::PdfSigner::new()
        .field("Signature_Applicant")
        .signer(signer)
        .reason("Approved Application")
        .sign(&pdf_bytes)
        .expect("sign");

    let fields_before_flatten = get_form_fields(&signed_bytes).expect("fields before flatten");
    assert_eq!(fields_before_flatten.len(), 13);

    // Flatten the entire document
    let mut doc = lopdf::Document::load_mem(&signed_bytes).expect("load doc");
    pdftoolkit_core::flatten_form_fields(&mut doc, true).expect("flatten all fields");
    let mut flattened_signed = Vec::new();
    doc.save_to(&mut flattened_signed).expect("save doc");

    let fields = get_form_fields(&flattened_signed).expect("fields");
    assert_eq!(fields.len(), 2);

    let sig_applicant = fields.iter().find(|f| f.name == "Signature_Applicant").unwrap();
    assert_eq!(sig_applicant.field_type, FormFieldType::Signature);
    assert_eq!(sig_applicant.signed, Some(true));
    assert!(sig_applicant.read_only, "Signed signature MUST be read-only!");

    let sig_manager = fields.iter().find(|f| f.name == "Signature_Manager").unwrap();
    assert_eq!(sig_manager.field_type, FormFieldType::Signature);
    assert_eq!(sig_manager.signed, Some(false));
    assert!(!sig_manager.read_only, "Unsigned signature must remain signable!");
}
