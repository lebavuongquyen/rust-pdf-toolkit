use pdffiller_core::validate_pdf;
use std::fs;

#[test]
fn validates_signature_field_as_signing_only() {
    let template = fs::read("reference/template.pdf").unwrap();
    let report = validate_pdf(&template, r#"{"Signature_0":"ignored"}"#).unwrap();
    assert!(report.contains("Digital signature fields must be handled by PdfSigner"));
    assert!(report.contains("\"valid\":false"));
}

#[test]
fn reports_missing_field() {
    let template = fs::read("reference/template.pdf").unwrap();
    let report = validate_pdf(&template, r#"{"does_not_exist":"value"}"#).unwrap();
    assert!(report.contains("Field not found"));
    assert!(report.contains("\"valid\":false"));
}
