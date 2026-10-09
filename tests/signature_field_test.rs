extern crate pdftoolkit_core as pdffiller_core;
use pdffiller_core::{
    CertificateSigner, FormFieldType, PdfSigner, get_form_fields,
    ops::{
        AddSignatureFieldOptions, FieldPresetPosition, RemoveSignatureFieldOptions,
        SignaturePlacement, add_signature_field, remove_signature_field, verify_pdf_signatures,
    },
};
use std::fs;

#[test]
fn test_add_invisible_signature_field() {
    let template = fs::read("reference/template.pdf").expect("template");

    // Add invisible signature field "Approval_Sig"
    let opts = AddSignatureFieldOptions::new("Approval_Sig").invisible();
    let updated_bytes =
        add_signature_field(&template, &opts).expect("add invisible signature field");

    let fields = get_form_fields(&updated_bytes).expect("get form fields");
    let target = fields.iter().find(|f| f.name == "Approval_Sig");
    assert!(target.is_some(), "Approval_Sig field should exist");
    let sig_field = target.unwrap();
    assert_eq!(sig_field.field_type, FormFieldType::Signature);
    // Invisible signature field has [0, 0, 0, 0] rect
    assert_eq!(sig_field.rect, Some([0.0, 0.0, 0.0, 0.0]));
}

#[test]
fn test_add_visible_preset_signature_field() {
    let template = fs::read("reference/template.pdf").expect("template");

    // Add visible bottom-right signature field
    let opts = AddSignatureFieldOptions::new("BottomRight_Sig").preset(
        FieldPresetPosition::BottomRight,
        180.0,
        50.0,
    );
    let updated_bytes = add_signature_field(&template, &opts).expect("add visible signature field");

    let fields = get_form_fields(&updated_bytes).expect("get form fields");
    let sig_field = fields
        .iter()
        .find(|f| f.name == "BottomRight_Sig")
        .expect("BottomRight_Sig found");
    assert_eq!(sig_field.field_type, FormFieldType::Signature);

    let [llx, lly, urx, ury] = sig_field.rect.expect("rect should be present");
    let width = urx - llx;
    let height = ury - lly;
    assert!(
        (width - 180.0).abs() < 1e-3,
        "Width should be 180: got {}",
        width
    );
    assert!(
        (height - 50.0).abs() < 1e-3,
        "Height should be 50: got {}",
        height
    );
}

#[test]
fn test_remove_signature_field_by_name() {
    let template = fs::read("reference/template.pdf").expect("template");

    // Add 2 fields: SigA and SigB
    let step1 = add_signature_field(
        &template,
        &AddSignatureFieldOptions::new("SigA").invisible(),
    )
    .expect("add SigA");
    let step2 = add_signature_field(&step1, &AddSignatureFieldOptions::new("SigB").invisible())
        .expect("add SigB");

    let fields_before = get_form_fields(&step2).expect("fields before");
    assert!(fields_before.iter().any(|f| f.name == "SigA"));
    assert!(fields_before.iter().any(|f| f.name == "SigB"));

    // Remove SigA by name
    let (after_bytes, removed_count) =
        remove_signature_field(&step2, &RemoveSignatureFieldOptions::by_name("SigA"))
            .expect("remove SigA");
    assert_eq!(removed_count, 1);

    let fields_after = get_form_fields(&after_bytes).expect("fields after");
    assert!(
        !fields_after.iter().any(|f| f.name == "SigA"),
        "SigA should be removed"
    );
    assert!(
        fields_after.iter().any(|f| f.name == "SigB"),
        "SigB should still exist"
    );
}

#[test]
fn test_remove_all_unsigned_signature_fields() {
    let template = fs::read("reference/template.pdf").expect("template");

    // Template already has "Signature_0" which is unsigned.
    // Add another unsigned field "Signature_Temp"
    let step1 = add_signature_field(
        &template,
        &AddSignatureFieldOptions::new("Signature_Temp").invisible(),
    )
    .expect("add field");

    let (after_bytes, removed_count) =
        remove_signature_field(&step1, &RemoveSignatureFieldOptions::all_unsigned())
            .expect("remove unsigned");
    assert!(
        removed_count >= 2,
        "Should remove at least 2 unsigned fields"
    );

    let fields_after = get_form_fields(&after_bytes).expect("fields after");
    let remaining_sigs = fields_after
        .iter()
        .filter(|f| f.field_type == FormFieldType::Signature)
        .count();
    assert_eq!(
        remaining_sigs, 0,
        "All unsigned signature fields should be gone"
    );
}

#[test]
fn test_auto_create_field_and_sign_and_verify() {
    let template = fs::read("reference/template.pdf").expect("template");
    let certificate = fs::read("tests/fixtures/test-signing.cert.der").expect("certificate");
    let private_key = fs::read("tests/fixtures/test-signing.key.der").expect("private key");
    let signer = CertificateSigner::from_pkcs8_der(certificate, &private_key).expect("signer");

    let auto_field_name = "AutoCreatedSigField";

    // Ensure template does NOT have this field
    let fields_initial = get_form_fields(&template).expect("initial fields");
    assert!(!fields_initial.iter().any(|f| f.name == auto_field_name));

    // Sign with auto_create_field = true
    let signed = PdfSigner::new()
        .field(auto_field_name)
        .signer(signer)
        .reason("Automated Approval")
        .location("Hanoi, VN")
        .auto_create_field(true)
        .placement(SignaturePlacement::visible_preset(
            FieldPresetPosition::BottomRight,
            150.0,
            50.0,
        ))
        .sign(&template)
        .expect("signing with auto_create_field should succeed");

    // Verify digital signature validity and cryptographic hash
    let verifications = verify_pdf_signatures(&signed).expect("verify signatures");
    assert_eq!(verifications.len(), 1, "Should find 1 digital signature");

    let sig_info = &verifications[0];
    assert_eq!(sig_info.field_name, auto_field_name);
    assert!(sig_info.is_valid, "Signature should be valid");
    assert!(
        sig_info.digest_matched,
        "Cryptographic SHA-256 digest should match"
    );
    assert_eq!(sig_info.reason.as_deref(), Some("Automated Approval"));
    assert_eq!(sig_info.location.as_deref(), Some("Hanoi, VN"));
    assert!(
        sig_info.signer_subject.is_some(),
        "Signer subject should be parsed"
    );

    // Test tamper detection: modify 1 byte in the PDF file outside the signature contents
    let mut tampered = signed.clone();
    // Tamper byte near beginning of PDF
    tampered[10] = if tampered[10] == b'a' { b'b' } else { b'a' };
    let tampered_verifications = verify_pdf_signatures(&tampered);
    if let Ok(results) = tampered_verifications
        && let Some(res) = results.first()
    {
        assert!(
            !res.digest_matched || !res.is_valid,
            "Tampered document must fail verification"
        );
    }
}
