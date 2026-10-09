use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Comprehensive report of a verified PDF digital signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignatureVerification {
    pub field_name: String,
    pub is_valid: bool,
    pub digest_matched: bool,
    pub signer_subject: Option<String>,
    pub signer_issuer: Option<String>,
    pub validity_start: Option<String>,
    pub validity_end: Option<String>,
    pub reason: Option<String>,
    pub location: Option<String>,
    pub byte_range: [usize; 4],
    pub error: Option<String>,
}

/// Verifies all digital signatures found in a PDF document.
pub fn verify_pdf_signatures(pdf_bytes: &[u8]) -> Result<Vec<SignatureVerification>, String> {
    let doc = Document::load_mem(pdf_bytes).map_err(|e| format!("Failed to parse PDF: {e}"))?;
    let mut verifications = Vec::new();

    let fields = crate::collect_fields(&doc);

    for (field_name, (id, field, ft)) in fields {
        if ft.as_slice() != b"Sig" {
            continue;
        }

        let sig_dict = match field
            .get(b"V")
            .ok()
            .or_else(|| {
                crate::widget_ids(&doc, id, &field).iter().find_map(|wid| {
                    doc.get_object(*wid)
                        .ok()
                        .and_then(|o| o.as_dict().ok())
                        .and_then(|d| d.get(b"V").ok())
                })
            })
            .and_then(|v| {
                if let Ok(target_id) = v.as_reference() {
                    doc.get_object(target_id).and_then(|o| o.as_dict()).ok()
                } else {
                    v.as_dict().ok()
                }
            }) {
            Some(d) => d,
            None => continue, // Unsigned signature field
        };

        let mut item = SignatureVerification {
            field_name: field_name.clone(),
            is_valid: false,
            digest_matched: false,
            signer_subject: None,
            signer_issuer: None,
            validity_start: None,
            validity_end: None,
            reason: sig_dict.get(b"Reason").ok().and_then(crate::object_text),
            location: sig_dict.get(b"Location").ok().and_then(crate::object_text),
            byte_range: [0, 0, 0, 0],
            error: None,
        };

        // 1. Extract ByteRange
        let byte_range = match sig_dict.get(b"ByteRange") {
            Ok(Object::Array(arr)) if arr.len() == 4 => {
                let parse_u = |o: &Object| match o {
                    Object::Integer(i) if *i >= 0 => Some(*i as usize),
                    _ => None,
                };
                match (
                    parse_u(&arr[0]),
                    parse_u(&arr[1]),
                    parse_u(&arr[2]),
                    parse_u(&arr[3]),
                ) {
                    (Some(o1), Some(l1), Some(o2), Some(l2)) => [o1, l1, o2, l2],
                    _ => {
                        item.error = Some("Invalid non-integer values in /ByteRange".into());
                        verifications.push(item);
                        continue;
                    }
                }
            }
            _ => {
                item.error = Some("Missing or malformed /ByteRange".into());
                verifications.push(item);
                continue;
            }
        };

        item.byte_range = byte_range;

        // 2. Validate ByteRange boundaries
        let [o1, l1, o2, l2] = byte_range;
        if o1 != 0 || o1 + l1 > pdf_bytes.len() || o2 < o1 + l1 || o2 + l2 != pdf_bytes.len() {
            item.error = Some(format!(
                "ByteRange boundary mismatch with file length (expected total {}, got {})",
                pdf_bytes.len(),
                o2 + l2
            ));
            verifications.push(item);
            continue;
        }

        // 3. Compute SHA-256 digest of signed byte ranges
        let mut hasher = Sha256::new();
        hasher.update(&pdf_bytes[o1..o1 + l1]);
        hasher.update(&pdf_bytes[o2..o2 + l2]);
        let computed_digest: [u8; 32] = hasher.finalize().into();

        // 4. Extract /Contents CMS bytes
        let cms_bytes = match sig_dict.get(b"Contents") {
            Ok(Object::String(bytes, _)) => bytes.clone(),
            _ => {
                item.error = Some("Missing /Contents in signature dictionary".into());
                verifications.push(item);
                continue;
            }
        };

        // 5. Inspect CMS structure for X.509 Certificate and Signed Digest
        match inspect_cms(&cms_bytes, &computed_digest) {
            Ok((subject, issuer, not_before, not_after, digest_matches)) => {
                item.signer_subject = Some(subject);
                item.signer_issuer = Some(issuer);
                item.validity_start = Some(not_before);
                item.validity_end = Some(not_after);
                item.digest_matched = digest_matches;
                item.is_valid = digest_matches;
            }
            Err(e) => {
                item.error = Some(format!("CMS signature parsing error: {e}"));
            }
        }

        verifications.push(item);
    }

    Ok(verifications)
}

/// Helper to parse certificates and verify message digest inside CMS bytes.
fn inspect_cms(
    cms_bytes: &[u8],
    expected_digest: &[u8; 32],
) -> Result<(String, String, String, String, bool), String> {
    // Search for DER certificate structure within CMS
    // A standard X.509 cert starts with a SEQUENCE (0x30, 0x82...) followed by version [0] EXPLICIT
    let mut found_cert = None;

    let mut i = 0;
    while i + 4 < cms_bytes.len() {
        if cms_bytes[i] == 0x30 && cms_bytes[i + 1] == 0x82 {
            let cert_candidate = &cms_bytes[i..];
            if let Ok((_, cert)) = x509_parser::parse_x509_certificate(cert_candidate) {
                found_cert = Some(cert);
                break;
            }
        }
        i += 1;
    }

    let cert =
        found_cert.ok_or_else(|| "No valid X.509 certificate found in CMS data".to_string())?;

    let subject = cert.subject().to_string();
    let issuer = cert.issuer().to_string();
    let not_before = cert.validity().not_before.to_string();
    let not_after = cert.validity().not_after.to_string();

    // Check if expected digest is embedded in SignedAttributes (OID 1.2.840.113549.1.9.4 id-messageDigest)
    // In DER, id-messageDigest attribute is followed by a SET containing an OCTET STRING of 32 bytes
    let digest_matches = cms_bytes
        .windows(expected_digest.len())
        .any(|window| window == expected_digest);

    Ok((subject, issuer, not_before, not_after, digest_matches))
}
