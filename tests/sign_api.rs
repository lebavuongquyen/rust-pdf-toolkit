use pdffiller_core::{CertificateSigner, PdfSigner};
use std::fs;

#[test]
fn sign_api_creates_incremental_signature() {
    let template = fs::read("reference/template.pdf").expect("template");
    let certificate = fs::read("tests/fixtures/test-signing.cert.der").expect("certificate");
    let private_key = fs::read("tests/fixtures/test-signing.key.der").expect("private key");
    let signer = CertificateSigner::from_pkcs8_der(certificate, &private_key)
        .expect("certificate signer");

    let signed = PdfSigner::new()
        .field("Signature_0")
        .signer(signer)
        .reason("Integration test")
        .location("Test")
        .sign(&template)
        .expect("sign");

    assert!(signed.len() > template.len());
    assert!(signed.starts_with(&template));

    fs::create_dir_all("output").expect("output directory");
    fs::write("output/sign_api-test.pdf", &signed).expect("write signed pdf");

    let doc = lopdf::Document::load_mem(&signed).expect("signed pdf parses");
    let signature_fields = doc.objects.values()
        .filter_map(|object| object.as_dict().ok())
        .filter(|dict| matches!(
            dict.get(b"FT").ok().and_then(|value| value.as_name().ok()),
            Some(b"Sig")
        ))
        .count();

    assert!(signature_fields >= 1);
    assert!(signed.windows(b"/ByteRange".len()).any(|window| window == b"/ByteRange"));
}