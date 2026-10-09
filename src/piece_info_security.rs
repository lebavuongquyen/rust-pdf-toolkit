use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use hmac::{Hmac, Mac};
use lopdf::{Dictionary, Document, Object, StringFormat};
use serde_json::Value;
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PieceInfoUnlockStatus {
    /// Data is completely authentic, integrity verified, and decrypted successfully.
    Valid,
    /// Data or signature has been tampered with or modified.
    Tampered(String),
    /// PieceInfo was copied/transplanted from another PDF document.
    DocumentMismatch,
    /// Secret key is incorrect or cannot decrypt payload.
    WrongKey,
    /// No PieceInfo dictionary found for the requested application.
    NotFound,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PieceInfoUnlockResult {
    pub status: PieceInfoUnlockStatus,
    pub data: Option<Value>,
    pub app_name: String,
}

/// Computes a deterministic document fingerprint based on trailer and page tree structure.
/// If PieceInfo is copied/transplanted into a different PDF, this fingerprint will not match.
pub fn compute_doc_fingerprint(doc: &Document) -> [u8; 32] {
    let mut hasher = Sha256::new();
    if let Ok(id_obj) = doc.trailer.get(b"ID") {
        hasher.update(format!("{:?}", id_obj).as_bytes());
    }
    if let Ok(root_obj) = doc.trailer.get(b"Root") {
        hasher.update(format!("{:?}", root_obj).as_bytes());
    }

    let pages = doc.get_pages();
    hasher.update((pages.len() as u64).to_be_bytes());
    for (page_num, page_id) in pages {
        hasher.update(page_num.to_be_bytes());
        hasher.update(page_id.0.to_be_bytes());
        hasher.update(page_id.1.to_be_bytes());
        if let Ok(page_dict) = doc.get_object(page_id).and_then(|o| o.as_dict()) {
            if let Ok(contents) = page_dict.get(b"Contents") {
                hasher.update(format!("{:?}", contents).as_bytes());
            }
            if let Ok(media_box) = page_dict.get(b"MediaBox") {
                hasher.update(format!("{:?}", media_box).as_bytes());
            }
        }
    }
    hasher.finalize().into()
}

fn derive_keys(secret_key: &str) -> ([u8; 32], [u8; 32]) {
    let mut aes_hasher = Sha256::new();
    aes_hasher.update(secret_key.as_bytes());
    aes_hasher.update(b":pdffiller:pieceinfo:aes256:v1");
    let aes_key: [u8; 32] = aes_hasher.finalize().into();

    let mut hmac_hasher = Sha256::new();
    hmac_hasher.update(secret_key.as_bytes());
    hmac_hasher.update(b":pdffiller:pieceinfo:hmac:v1");
    let hmac_key: [u8; 32] = hmac_hasher.finalize().into();

    (aes_key, hmac_key)
}

/// Encrypts and cryptographically seals a JSON payload into the document's /PieceInfo catalog.
///
/// Security characteristics:
/// - **Stealth**: Stored entirely in Document Catalog (/Root /PieceInfo). Zero visual page rendering.
/// - **Normal Viewing**: No user password prompt; opens seamlessly in Adobe, Foxit, Chrome, Edge, iOS, Android.
/// - **Zero Antivirus Alerts**: Static PDF dictionary strings; no JavaScript, no /Launch, no binary attachments.
/// - **Confidentiality**: Encrypted with AES-256-GCM; plaintext invisible to external viewers.
/// - **Tamper-Proof**: Sealed with HMAC-SHA256.
/// - **Document Binding**: Bound to the PDF's structural fingerprint to prevent replay/copy-paste to other PDFs.
pub fn insert_locked_piece_info(
    doc: &mut Document,
    app_name: &str,
    data: &Value,
    secret_key: &str,
) -> Result<(), String> {
    if secret_key.trim().is_empty() {
        return Err("Secret key cannot be empty".to_string());
    }

    let catalog_id = match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => *id,
        _ => return Err("PDF Catalog not found in trailer".to_string()),
    };

    let (aes_key, hmac_key) = derive_keys(secret_key);

    // 1. Generate 96-bit (12 bytes) cryptographically secure random nonce
    let mut nonce_bytes = [0u8; 12];
    getrandom::getrandom(&mut nonce_bytes)
        .map_err(|e| format!("Failed to generate secure random nonce: {e}"))?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    // 2. Encrypt JSON payload with AES-256-GCM
    let plaintext = serde_json::to_vec(data)
        .map_err(|e| format!("Failed to serialize piece_info JSON: {e}"))?;
    let cipher = Aes256Gcm::new_from_slice(&aes_key)
        .map_err(|e| format!("Failed to initialize AES-256-GCM cipher: {e}"))?;
    let ciphertext_with_tag = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|e| format!("AES-256-GCM encryption failed: {e}"))?;

    // 3. Compute document binding fingerprint
    let doc_binding = compute_doc_fingerprint(doc);

    // 4. Compute HMAC-SHA256 signature
    let mut mac: HmacSha256 = KeyInit::new_from_slice(&hmac_key)
        .map_err(|e| format!("HMAC initialization failed: {e}"))?;
    mac.update(app_name.as_bytes());
    mac.update(&nonce_bytes);
    mac.update(&ciphertext_with_tag);
    mac.update(&doc_binding);
    let hmac_sig = mac.finalize().into_bytes();

    // 5. Build Private dictionary with standard PDF Hex strings
    let mut private_dict = Dictionary::new();
    private_dict.set(b"Version".as_slice(), Object::Integer(1));
    private_dict.set(
        b"Algorithm".as_slice(),
        Object::string_literal("AES-256-GCM+HMAC-SHA256"),
    );
    private_dict.set(
        b"Nonce".as_slice(),
        Object::String(nonce_bytes.to_vec(), StringFormat::Hexadecimal),
    );
    private_dict.set(
        b"Payload".as_slice(),
        Object::String(ciphertext_with_tag, StringFormat::Hexadecimal),
    );
    private_dict.set(
        b"DocBinding".as_slice(),
        Object::String(doc_binding.to_vec(), StringFormat::Hexadecimal),
    );
    private_dict.set(
        b"HMAC".as_slice(),
        Object::String(hmac_sig.to_vec(), StringFormat::Hexadecimal),
    );

    let mut app_dict = Dictionary::new();
    app_dict.set(
        b"LastModified".as_slice(),
        Object::string_literal("D:20261008000000Z"),
    );
    app_dict.set(b"Private".as_slice(), Object::Dictionary(private_dict));

    // 6. Insert or merge into Catalog /PieceInfo
    let existing_ref = {
        let catalog = doc
            .get_object(catalog_id)
            .map_err(|e| format!("Failed to get Catalog: {e}"))?
            .as_dict()
            .map_err(|e| format!("Catalog is not a dictionary: {e}"))?;
        match catalog.get(b"PieceInfo") {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        }
    };

    if let Some(r_id) = existing_ref {
        if let Ok(piece_dict) = doc.get_object_mut(r_id).and_then(|o| o.as_dict_mut()) {
            piece_dict.set(app_name.as_bytes().to_vec(), Object::Dictionary(app_dict));
            return Ok(());
        }
    }

    let catalog = doc
        .get_object_mut(catalog_id)
        .map_err(|e| format!("Failed to get Catalog: {e}"))?
        .as_dict_mut()
        .map_err(|e| format!("Catalog is not a dictionary: {e}"))?;

    match catalog.get_mut(b"PieceInfo") {
        Ok(Object::Dictionary(existing_dict)) => {
            existing_dict.set(app_name.as_bytes().to_vec(), Object::Dictionary(app_dict));
        }
        _ => {
            let mut piece_info_dict = Dictionary::new();
            piece_info_dict.set(app_name.as_bytes().to_vec(), Object::Dictionary(app_dict));
            catalog.set(b"PieceInfo".as_slice(), Object::Dictionary(piece_info_dict));
        }
    }

    Ok(())
}

fn extract_bytes_from_object<'a>(obj: &'a Object) -> Option<&'a [u8]> {
    match obj {
        Object::String(bytes, _) => Some(bytes.as_slice()),
        _ => None,
    }
}

/// Verifies and unlocks a cryptographically sealed /PieceInfo from a PDF file.
pub fn verify_and_unlock_piece_info(
    pdf_bytes: &[u8],
    app_name: &str,
    secret_key: &str,
) -> Result<PieceInfoUnlockResult, String> {
    let doc = Document::load_mem(pdf_bytes).map_err(|e| format!("PDF load failed: {e}"))?;
    let catalog_id = match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => *id,
        _ => {
            return Ok(PieceInfoUnlockResult {
                status: PieceInfoUnlockStatus::NotFound,
                data: None,
                app_name: app_name.into(),
            });
        }
    };

    let catalog = match doc.get_object(catalog_id).and_then(|o| o.as_dict()) {
        Ok(d) => d,
        Err(_) => {
            return Ok(PieceInfoUnlockResult {
                status: PieceInfoUnlockStatus::NotFound,
                data: None,
                app_name: app_name.into(),
            });
        }
    };

    let piece_info_dict = match catalog.get(b"PieceInfo") {
        Ok(Object::Dictionary(d)) => d,
        Ok(Object::Reference(id)) => match doc.get_object(*id).and_then(|o| o.as_dict()) {
            Ok(d) => d,
            Err(_) => {
                return Ok(PieceInfoUnlockResult {
                    status: PieceInfoUnlockStatus::NotFound,
                    data: None,
                    app_name: app_name.into(),
                });
            }
        },
        _ => {
            return Ok(PieceInfoUnlockResult {
                status: PieceInfoUnlockStatus::NotFound,
                data: None,
                app_name: app_name.into(),
            });
        }
    };

    let app_entry = match piece_info_dict.get(app_name.as_bytes()) {
        Ok(Object::Dictionary(d)) => d,
        Ok(Object::Reference(id)) => match doc.get_object(*id).and_then(|o| o.as_dict()) {
            Ok(d) => d,
            Err(_) => {
                return Ok(PieceInfoUnlockResult {
                    status: PieceInfoUnlockStatus::NotFound,
                    data: None,
                    app_name: app_name.into(),
                });
            }
        },
        _ => {
            return Ok(PieceInfoUnlockResult {
                status: PieceInfoUnlockStatus::NotFound,
                data: None,
                app_name: app_name.into(),
            });
        }
    };

    let private_dict = match app_entry.get(b"Private") {
        Ok(Object::Dictionary(d)) => d,
        Ok(Object::Reference(id)) => match doc.get_object(*id).and_then(|o| o.as_dict()) {
            Ok(d) => d,
            Err(_) => {
                return Ok(PieceInfoUnlockResult {
                    status: PieceInfoUnlockStatus::NotFound,
                    data: None,
                    app_name: app_name.into(),
                });
            }
        },
        _ => {
            return Ok(PieceInfoUnlockResult {
                status: PieceInfoUnlockStatus::NotFound,
                data: None,
                app_name: app_name.into(),
            });
        }
    };

    let nonce_bytes = match private_dict
        .get(b"Nonce")
        .ok()
        .and_then(extract_bytes_from_object)
    {
        Some(b) if b.len() == 12 => b,
        _ => {
            return Ok(PieceInfoUnlockResult {
                status: PieceInfoUnlockStatus::Tampered(
                    "Invalid or missing Nonce in PieceInfo".into(),
                ),
                data: None,
                app_name: app_name.into(),
            });
        }
    };

    let payload_bytes = match private_dict
        .get(b"Payload")
        .ok()
        .and_then(extract_bytes_from_object)
    {
        Some(b) => b,
        _ => {
            return Ok(PieceInfoUnlockResult {
                status: PieceInfoUnlockStatus::Tampered(
                    "Invalid or missing Payload in PieceInfo".into(),
                ),
                data: None,
                app_name: app_name.into(),
            });
        }
    };

    let stored_doc_binding = match private_dict
        .get(b"DocBinding")
        .ok()
        .and_then(extract_bytes_from_object)
    {
        Some(b) if b.len() == 32 => b,
        _ => {
            return Ok(PieceInfoUnlockResult {
                status: PieceInfoUnlockStatus::Tampered(
                    "Invalid or missing DocBinding in PieceInfo".into(),
                ),
                data: None,
                app_name: app_name.into(),
            });
        }
    };

    let stored_hmac = match private_dict
        .get(b"HMAC")
        .ok()
        .and_then(extract_bytes_from_object)
    {
        Some(b) => b,
        _ => {
            return Ok(PieceInfoUnlockResult {
                status: PieceInfoUnlockStatus::Tampered(
                    "Invalid or missing HMAC in PieceInfo".into(),
                ),
                data: None,
                app_name: app_name.into(),
            });
        }
    };

    // 1. Verify Document Binding
    let current_doc_binding = compute_doc_fingerprint(&doc);
    if stored_doc_binding != current_doc_binding.as_slice() {
        return Ok(PieceInfoUnlockResult {
            status: PieceInfoUnlockStatus::DocumentMismatch,
            data: None,
            app_name: app_name.into(),
        });
    }

    // 2. Verify HMAC Signature
    let (aes_key, hmac_key) = derive_keys(secret_key);
    let mut mac: HmacSha256 = KeyInit::new_from_slice(&hmac_key)
        .map_err(|e| format!("HMAC initialization failed: {e}"))?;
    mac.update(app_name.as_bytes());
    mac.update(nonce_bytes);
    mac.update(payload_bytes);
    mac.update(stored_doc_binding);

    if mac.verify_slice(stored_hmac).is_err() {
        return Ok(PieceInfoUnlockResult {
            status: PieceInfoUnlockStatus::WrongKey,
            data: None,
            app_name: app_name.into(),
        });
    }

    // 3. Decrypt AES-256-GCM payload
    let nonce = Nonce::from_slice(nonce_bytes);
    let cipher = match Aes256Gcm::new_from_slice(&aes_key) {
        Ok(c) => c,
        Err(_) => {
            return Ok(PieceInfoUnlockResult {
                status: PieceInfoUnlockStatus::WrongKey,
                data: None,
                app_name: app_name.into(),
            });
        }
    };

    let plaintext = match cipher.decrypt(nonce, payload_bytes) {
        Ok(pt) => pt,
        Err(_) => {
            return Ok(PieceInfoUnlockResult {
                status: PieceInfoUnlockStatus::WrongKey,
                data: None,
                app_name: app_name.into(),
            });
        }
    };

    let json_val: Value = match serde_json::from_slice(&plaintext) {
        Ok(v) => v,
        Err(e) => {
            return Ok(PieceInfoUnlockResult {
                status: PieceInfoUnlockStatus::Tampered(format!("Corrupted JSON plaintext: {e}")),
                data: None,
                app_name: app_name.into(),
            });
        }
    };

    Ok(PieceInfoUnlockResult {
        status: PieceInfoUnlockStatus::Valid,
        data: Some(json_val),
        app_name: app_name.into(),
    })
}

/// Lists all application identifier names currently registered under /PieceInfo in the PDF.
/// Returns an empty Vec if the document has no /PieceInfo dictionary.
pub fn list_piece_info_applications(pdf_bytes: &[u8]) -> Result<Vec<String>, String> {
    let doc = Document::load_mem(pdf_bytes).map_err(|e| format!("PDF load failed: {e}"))?;
    let catalog_id = match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => *id,
        _ => return Ok(Vec::new()),
    };
    let catalog = match doc.get_object(catalog_id).and_then(|o| o.as_dict()) {
        Ok(d) => d,
        Err(_) => return Ok(Vec::new()),
    };
    let piece_info_dict = match catalog.get(b"PieceInfo") {
        Ok(Object::Dictionary(d)) => d,
        Ok(Object::Reference(id)) => match doc.get_object(*id).and_then(|o| o.as_dict()) {
            Ok(d) => d,
            Err(_) => return Ok(Vec::new()),
        },
        _ => return Ok(Vec::new()),
    };
    let apps: Vec<String> = piece_info_dict
        .iter()
        .map(|(k, _)| String::from_utf8_lossy(k).into_owned())
        .collect();
    Ok(apps)
}
