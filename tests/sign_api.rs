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
