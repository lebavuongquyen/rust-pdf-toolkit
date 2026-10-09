use std::fs;

#[test]
fn foxit_reference_is_the_golden_pdf_signature_shape() {
    let pdf = fs::read("reference/template_signed.pdf").expect("Foxit reference PDF");
    let text = String::from_utf8_lossy(&pdf);

    assert_eq!(pdf.len(), 59041);
    assert!(text.contains("/ByteRange[0 52115 54905 4136]"));
    assert!(text.contains("/SubFilter/adbe.pkcs7.detached"));
    assert!(text.contains("/Filter/Adobe.PPKLite"));
    assert!(text.contains("/Reason(I am the author of this document)"));

    let contents = extract_contents_hex(&pdf);
    assert_eq!(contents.len(), 1394);
    assert!(contents.starts_with(&[
        0x30, 0x82, 0x02, 0xB3, 0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02,
    ]));

    let digest_algorithm = [
        0x30, 0x0D, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05, 0x00,
    ];
    assert!(
        contents
            .windows(digest_algorithm.len())
            .any(|w| w == digest_algorithm)
    );

    let direct_ecdsa_signer_info = [
        0x30, 0x0B, 0x06, 0x07, 0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01, 0x05, 0x00, 0x04, 0x66,
    ];
    assert!(
        contents
            .windows(direct_ecdsa_signer_info.len())
            .any(|w| w == direct_ecdsa_signer_info)
    );
}

fn extract_contents_hex(pdf: &[u8]) -> Vec<u8> {
    let marker = b"/Contents<";
    let start = pdf
        .windows(marker.len())
        .position(|window| window == marker)
        .expect("Foxit Contents");
    let hex_start = start + marker.len();
    let hex_end = pdf[hex_start..]
        .iter()
        .position(|byte| *byte == b'>')
        .map(|offset| hex_start + offset)
        .expect("Foxit Contents end");

    let hex = &pdf[hex_start..hex_end];
    assert!(hex.len().is_multiple_of(2));
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(std::str::from_utf8(&hex[i..i + 2]).expect("hex"), 16)
                .expect("hex byte")
        })
        .collect()
}
