use pdffiller_core::{CertificateSigner, EcdsaSigner, PdfSigner};
use std::fs;

#[test]
fn sign_api_creates_incremental_rsa_signature() {
    let template = fs::read("reference/template.pdf").expect("template");
    let certificate = fs::read("tests/fixtures/test-signing.cert.der").expect("certificate");
    let private_key = fs::read("tests/fixtures/test-signing.key.der").expect("private key");
    let signer =
        CertificateSigner::from_pkcs8_der(certificate, &private_key).expect("certificate signer");

    let signed = PdfSigner::new()
        .field("Signature_0")
        .signer(signer)
        .reason("Integration test")
        .location("Test")
        .sign(&template)
        .expect("sign");

    assert_signature_pdf(&template, &signed, b"adbe.pkcs7.detached");

    fs::create_dir_all("output").expect("output directory");
    fs::write("output/sign_api-rsa-test.pdf", &signed).expect("write signed pdf");
}

#[test]
fn sign_api_creates_foxit_compatible_ecdsa_signature() {
    let template = fs::read("reference/template.pdf").expect("template");
    let certificate = fs::read("tests/fixtures/test-ecdsa-signing.cert.der").expect("certificate");
    let private_key = fs::read("tests/fixtures/test-ecdsa-signing.key.der").expect("private key");
    let signer =
        EcdsaSigner::from_pkcs8_der(certificate, &private_key).expect("ECDSA certificate signer");

    let signed = PdfSigner::new()
        .field("Signature_0")
        .reason("I am the author of this document")
        .location("")
        .signer(signer)
        .sign(&template)
        .expect("ECDSA sign");

    assert_signature_pdf(&template, &signed, b"adbe.pkcs7.detached");

    fs::create_dir_all("output").expect("output directory");
    fs::write("output/sign_api-ecdsa-test.pdf", &signed).expect("write ECDSA signed pdf");
    assert!(
        signed
            .windows(b"2A8648CE3D0201".len())
            .any(|window| window == b"2A8648CE3D0201")
    );
}

fn assert_signature_pdf(template: &[u8], signed: &[u8], expected_subfilter: &[u8]) {
    assert!(signed.len() > template.len());
    assert!(signed.starts_with(template));
    assert!(
        signed
            .windows(b"/ByteRange".len())
            .any(|window| window == b"/ByteRange")
    );
    assert!(
        signed
            .windows(b"/SubFilter/".len())
            .any(|window| window == b"/SubFilter/")
    );
    assert!(
        signed
            .windows(expected_subfilter.len())
            .any(|window| window == expected_subfilter)
    );

    let doc = lopdf::Document::load_mem(signed).expect("signed pdf parses");
    let signature_fields = doc
        .objects
        .values()
        .filter_map(|object| object.as_dict().ok())
        .filter(|dict| {
            matches!(
                dict.get(b"FT").ok().and_then(|value| value.as_name().ok()),
                Some(b"Sig")
            )
        })
        .count();

    assert!(signature_fields >= 1);
}


#[test]
fn sign_api_validates_signature_field_before_signing() {
    let template = fs::read("reference/template.pdf").expect("template");
    let certificate = fs::read("tests/fixtures/test-signing.cert.der").expect("certificate");
    let private_key = fs::read("tests/fixtures/test-signing.key.der").expect("private key");
    let signer = CertificateSigner::from_pkcs8_der(certificate, &private_key).expect("certificate signer");
    let result = PdfSigner::new().field("Missing").signer(signer).validate(&template);
    assert!(matches!(result, Err(pdffiller_core::SignError::SignatureFieldNotFound(_))));
}


#[test]
fn sign_api_rejects_already_signed_field() {
    let template = fs::read("reference/template.pdf").expect("template");
    let certificate = fs::read("tests/fixtures/test-signing.cert.der").expect("certificate");
    let private_key = fs::read("tests/fixtures/test-signing.key.der").expect("private key");
    let signer = CertificateSigner::from_pkcs8_der(certificate, &private_key).expect("certificate signer");
    let signed = PdfSigner::new().field("Signature_0").signer(signer).sign(&template).expect("sign");

    let certificate = fs::read("tests/fixtures/test-signing.cert.der").expect("certificate");
    let private_key = fs::read("tests/fixtures/test-signing.key.der").expect("private key");
    let signer = CertificateSigner::from_pkcs8_der(certificate, &private_key).expect("certificate signer");
    let result = PdfSigner::new().field("Signature_0").signer(signer).validate(&signed);
    assert!(matches!(result, Err(pdffiller_core::SignError::InvalidSignatureField(_))));
}

#[test]
fn sign_api_supports_flatten_and_custom_appearance() {
    use base64::Engine;
    use pdffiller_core::{
        GraphicPosition, SignatureAppearanceOptions, SignatureFont, TextAlign, get_form_fields,
    };

    let template = fs::read("reference/template.pdf").expect("template.pdf");
    let certificate = fs::read("tests/fixtures/test-signing.cert.der").expect("certificate");
    let private_key = fs::read("tests/fixtures/test-signing.key.der").expect("private key");
    let signer =
        CertificateSigner::from_pkcs8_der(certificate, &private_key).expect("certificate signer");

    let img_bytes = fs::read("output/extracted-image.jpg").expect("sample image");
    let b64 = base64::engine::general_purpose::STANDARD.encode(&img_bytes);
    let data_url = format!("data:image/jpeg;base64,{}", b64);

    let appearance = SignatureAppearanceOptions {
        image: Some(data_url),
        position: GraphicPosition::Left,
        font: SignatureFont::Times,
        font_size: Some(8.0),
        bold: true,
        italic: false,
        align: TextAlign::Left,
        text_color: Some([20, 20, 80]),
        show_signer_name: true,
        signer_name: Some("Nguyen Van A".into()),
        show_date: true,
        date: Some("2026-10-08 22:30:00".into()),
        show_reason: true,
        reason: Some("Signed and Flattened".into()),
        show_location: true,
        location: Some("Vietnam".into()),
        labels: None,
        extra_lines: vec!["Ref: 999888".into()],
        ..Default::default()
    };

    let signed = PdfSigner::new()
        .field("Signature_0")
        .signer(signer)
        .reason("Signed and Flattened")
        .location("Vietnam")
        .flatten(true)
        .appearance(appearance)
        .sign(&template)
        .expect("sign with flatten and appearance");

    assert!(
        signed
            .windows(b"/ByteRange".len())
            .any(|window| window == b"/ByteRange")
    );
    assert!(
        signed
            .windows(b"adbe.pkcs7.detached".len())
            .any(|window| window == b"adbe.pkcs7.detached")
    );

    // Verify PDF is valid lopdf document
    let _doc = lopdf::Document::load_mem(&signed).expect("signed pdf parses");

    // Check fields on the signed & flattened PDF
    let fields = get_form_fields(&signed).expect("fields");
    assert_eq!(fields.len(), 1);
    let sig_f = &fields[0];
    assert_eq!(sig_f.name, "Signature_0");
    assert_eq!(sig_f.signed, Some(true));
    assert!(sig_f.read_only);

    fs::create_dir_all("output").expect("output directory");
    fs::write("output/sign_api-flatten-test.pdf", &signed).expect("write output");
}
