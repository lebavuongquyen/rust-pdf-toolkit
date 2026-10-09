extern crate pdftoolkit_core as pdffiller_core;
use pdffiller_core::{
    fill_pdf_with_options, get_form_fields, CertificateSigner, FillOptions, FormFieldType,
    PdfSigner,
};
use std::fs;

#[test]
fn test_golden_enterprise_form_discovery() {
    let bytes = fs::read("reference/golden_enterprise_form.pdf").expect("golden_enterprise_form.pdf");
    let fields = get_form_fields(&bytes).expect("get_form_fields");

    assert_eq!(fields.len(), 14, "Expected 14 fields in golden enterprise form");

    let field_map: std::collections::HashMap<_, _> =
        fields.into_iter().map(|f| (f.name.clone(), f)).collect();

    // 1. Text & Unicode Personal Info
    let name_f = field_map.get("full_name").expect("full_name");
    assert_eq!(name_f.field_type, FormFieldType::Text);
    assert_eq!(
        name_f.value,
        Some(serde_json::Value::String("Nguyễn Văn A".into()))
    );

    let email_f = field_map.get("email").expect("email");
    assert_eq!(email_f.field_type, FormFieldType::Text);
    assert_eq!(
        email_f.value,
        Some(serde_json::Value::String("nguyen.vana@enterprise.vn".into()))
    );

    let phone_f = field_map.get("phone").expect("phone");
    assert_eq!(phone_f.field_type, FormFieldType::Text);
    assert_eq!(
        phone_f.value,
        Some(serde_json::Value::String("+84 987 654 321".into()))
    );

    let title_f = field_map.get("job_title").expect("job_title");
    assert_eq!(title_f.field_type, FormFieldType::Text);
    assert_eq!(
        title_f.value,
        Some(serde_json::Value::String("Senior Rust & Cloud Architect".into()))
    );

    // 2. Date field
    let date_f = field_map.get("birth_date").expect("birth_date");
    assert_eq!(date_f.field_type, FormFieldType::Date);
    assert_eq!(
        date_f.value,
        Some(serde_json::Value::String("16/08/1995".into()))
    );

    // 3. ComboBox with custom option
    let dept_f = field_map.get("department").expect("department");
    assert_eq!(dept_f.field_type, FormFieldType::ComboBox);
    assert_eq!(dept_f.multi_select, Some(false));
    assert_eq!(dept_f.editable, Some(true));
    assert_eq!(dept_f.custom_option, Some(true));
    assert_eq!(
        dept_f.value,
        Some(serde_json::Value::String("Cái gì vậy".into()))
    );
    assert_eq!(dept_f.options.len(), 6);

    // 4. ListBox with multi-selection
    let skills_f = field_map.get("skills").expect("skills");
    assert_eq!(skills_f.field_type, FormFieldType::ListBox);
    assert_eq!(skills_f.multi_select, Some(true));
    assert_eq!(skills_f.editable, Some(false));
    assert_eq!(skills_f.custom_option, Some(false));
    assert_eq!(
        skills_f.value,
        Some(serde_json::json!(["TypeScript", "Python"]))
    );
    assert_eq!(skills_f.options.len(), 6);

    // 5. Radio and Checkboxes
    let gender_f = field_map.get("gender").expect("gender");
    assert_eq!(gender_f.field_type, FormFieldType::Radio);
    assert_eq!(
        gender_f.value,
        Some(serde_json::Value::String("Male".into()))
    );

    let agree_f = field_map.get("agree_terms").expect("agree_terms");
    assert_eq!(agree_f.field_type, FormFieldType::Checkbox);
    assert_eq!(
        agree_f.value,
        Some(serde_json::Value::String("Yes".into()))
    );

    let news_f = field_map.get("newsletter").expect("newsletter");
    assert_eq!(news_f.field_type, FormFieldType::Checkbox);
    assert_eq!(
        news_f.value,
        Some(serde_json::Value::String("Yes".into()))
    );

    // 6. Signatures (Applicant & Manager)
    let sig_app = field_map.get("Signature_Applicant").expect("Signature_Applicant");
    assert_eq!(sig_app.field_type, FormFieldType::Signature);
    assert_eq!(sig_app.signed, Some(false));
    assert!(!sig_app.read_only);

    let sig_mgr = field_map.get("Signature_Manager").expect("Signature_Manager");
    assert_eq!(sig_mgr.field_type, FormFieldType::Signature);
    assert_eq!(sig_mgr.signed, Some(false));
    assert!(!sig_mgr.read_only);
}

#[test]
fn test_golden_enterprise_form_flattening_preserves_signatures() {
    let bytes = fs::read("reference/golden_enterprise_form.pdf").expect("golden_enterprise_form.pdf");

    // Flatten form fields
    let opt = FillOptions::new().flatten(true);
    let (flattened_bytes, report) =
        fill_pdf_with_options(&bytes, "{}", &opt).expect("flatten golden enterprise form");

    assert_eq!(report.fields.len(), 0);

    // Verify fields after flattening
    let remaining_fields = get_form_fields(&flattened_bytes).expect("fields after flatten");
    assert_eq!(
        remaining_fields.len(),
        2,
        "Only digital signatures should remain unflattened"
    );

    let remaining_names: Vec<_> = remaining_fields.iter().map(|f| f.name.as_str()).collect();
    assert!(remaining_names.contains(&"Signature_Applicant"));
    assert!(remaining_names.contains(&"Signature_Manager"));

    for f in &remaining_fields {
        assert_eq!(f.field_type, FormFieldType::Signature);
        assert_eq!(f.signed, Some(false));
        assert!(!f.read_only);
    }
}

#[test]
fn test_golden_enterprise_form_signing_after_flatten() {
    let bytes = fs::read("reference/golden_enterprise_form.pdf").expect("golden_enterprise_form.pdf");

    // Flatten first
    let opt = FillOptions::new().flatten(true);
    let (flattened_bytes, _) =
        fill_pdf_with_options(&bytes, "{}", &opt).expect("flatten golden enterprise form");

    // Sign the preserved Signature_Applicant field
    let cert = fs::read("tests/fixtures/test-signing.cert.der").expect("cert");
    let key = fs::read("tests/fixtures/test-signing.key.der").expect("key");
    let signer = CertificateSigner::from_pkcs8_der(cert, &key).expect("signer");

    let signed_bytes = PdfSigner::new()
        .field("Signature_Applicant")
        .signer(signer)
        .reason("Enterprise Employment Application Sign-off")
        .location("Ho Chi Minh City, VN")
        .sign(&flattened_bytes)
        .expect("signing applicant field");

    // Verify signature was applied
    let signed_fields = get_form_fields(&signed_bytes).expect("fields after sign");
    let applicant_f = signed_fields
        .iter()
        .find(|f| f.name == "Signature_Applicant")
        .expect("applicant field");

    assert_eq!(applicant_f.field_type, FormFieldType::Signature);
    assert_eq!(applicant_f.signed, Some(true));
    assert!(applicant_f.read_only);
    assert_eq!(
        applicant_f.signature.as_ref().and_then(|s| s.reason.as_deref()),
        Some("Enterprise Employment Application Sign-off")
    );

    // Manager signature should still remain unsigned and ready for second party
    let manager_f = signed_fields
        .iter()
        .find(|f| f.name == "Signature_Manager")
        .expect("manager field");
    assert_eq!(manager_f.field_type, FormFieldType::Signature);
    assert_eq!(manager_f.signed, Some(false));
    assert!(!manager_f.read_only);
}
