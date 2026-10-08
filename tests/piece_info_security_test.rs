use pdffiller_core::{
    insert_locked_piece_info, verify_and_unlock_piece_info, CertificateSigner, FillOptions,
    PdfSigner, PieceInfoUnlockStatus,
};
use std::fs;

#[test]
fn test_locked_piece_info_roundtrip_valid_and_wrong_key() {
    let template = fs::read("reference/template.pdf").expect("template.pdf");
    let secret = "MyBankSecureKey@2026";
    let app_name = "CoreBanking";

    let payload = serde_json::json!({
        "account_id": "ACC-998877",
        "customer_cif": "CIF-123456",
        "transaction_amount": 50000000,
        "currency": "VND",
        "approved": true
    });

    // 1. Chèn locked PieceInfo vào PDF thông qua FillOptions
    let options = FillOptions::new()
        .flatten(false)
        .locked_piece_info(app_name, payload.clone(), secret);

    let (filled_pdf, report) = pdffiller_core::fill_pdf_with_options(
        &template,
        r#"{"name":"Le Ba Vuong Quyen"}"#,
        &options,
    )
    .expect("fill_pdf_with_options");

    assert_eq!(report.filled_count(), 1);

    // 2. Mở khóa thành công với đúng Secret Key
    let result_valid = verify_and_unlock_piece_info(&filled_pdf, app_name, secret)
        .expect("verify_and_unlock_piece_info");

    assert_eq!(result_valid.status, PieceInfoUnlockStatus::Valid);
    assert_eq!(result_valid.app_name, app_name);
    let unlocked_data = result_valid.data.expect("data exists");
    assert_eq!(unlocked_data["account_id"], "ACC-998877");
    assert_eq!(unlocked_data["customer_cif"], "CIF-123456");
    assert_eq!(unlocked_data["transaction_amount"], 50000000);
    assert_eq!(unlocked_data["currency"], "VND");
    assert_eq!(unlocked_data["approved"], true);

    // 3. Thất bại khi dùng sai Secret Key
    let result_wrong_key = verify_and_unlock_piece_info(&filled_pdf, app_name, "HackerWrongKey@999")
        .expect("verify with wrong key");

    assert_eq!(result_wrong_key.status, PieceInfoUnlockStatus::WrongKey);
    assert!(result_wrong_key.data.is_none());

    // 4. Kiểm tra app_name không tồn tại
    let result_not_found = verify_and_unlock_piece_info(&filled_pdf, "UnknownApp", secret)
        .expect("verify non-existent app");

    assert_eq!(result_not_found.status, PieceInfoUnlockStatus::NotFound);
}

#[test]
fn test_locked_piece_info_detects_document_transplant() {
    let doc_a_bytes = fs::read("reference/template.pdf").expect("template.pdf");
    let doc_b_bytes = fs::read("reference/template_8field.pdf").expect("template_8field.pdf");
    let secret = "TopSecret@2026";
    let app_name = "LegalVault";

    let payload = serde_json::json!({
        "contract_id": "HD-2026-VIP",
        "value": 1000000000
    });

    // Tạo PDF A chứa locked piece_info
    let mut doc_a = lopdf::Document::load_mem(&doc_a_bytes).expect("load doc A");
    insert_locked_piece_info(&mut doc_a, app_name, &payload, secret).expect("insert in A");
    let mut pdf_a_locked = Vec::new();
    doc_a.save_to(&mut pdf_a_locked).expect("save doc A");

    // Xác thực PDF A hợp lệ
    let res_a = verify_and_unlock_piece_info(&pdf_a_locked, app_name, secret).expect("verify A");
    assert_eq!(res_a.status, PieceInfoUnlockStatus::Valid);

    // BÂY GIỜ KẺ GIAN CỐ TÌNH CẮT /PieceInfo TỪ PDF A ĐEM DÁN SANG PDF B (Document Swapping Attack)
    let piece_info_from_a = {
        let loaded_a = lopdf::Document::load_mem(&pdf_a_locked).unwrap();
        let cat_id = match loaded_a.trailer.get(b"Root").unwrap() {
            lopdf::Object::Reference(id) => *id,
            _ => panic!("no root"),
        };
        let cat = loaded_a.get_object(cat_id).unwrap().as_dict().unwrap();
        cat.get(b"PieceInfo").unwrap().clone()
    };

    let mut doc_b = lopdf::Document::load_mem(&doc_b_bytes).expect("load doc B");
    let cat_b_id = match doc_b.trailer.get(b"Root").unwrap() {
        lopdf::Object::Reference(id) => *id,
        _ => panic!("no root"),
    };
    let cat_b = doc_b.get_object_mut(cat_b_id).unwrap().as_dict_mut().unwrap();
    cat_b.set(b"PieceInfo".as_slice(), piece_info_from_a);
    let mut pdf_b_tampered = Vec::new();
    doc_b.save_to(&mut pdf_b_tampered).expect("save doc B");

    // HỆ THỐNG PHẢI PHÁT HIỆN RA NGAY LẬP TỨC: DocumentMismatch!
    let res_b = verify_and_unlock_piece_info(&pdf_b_tampered, app_name, secret).expect("verify B");
    assert_eq!(
        res_b.status,
        PieceInfoUnlockStatus::DocumentMismatch,
        "He thong phai phat hien ra PieceInfo bi trao tu PDF khac sang!"
    );
    assert!(res_b.data.is_none());
}

#[test]
fn test_locked_piece_info_with_pdf_signer() {
    let template = fs::read("reference/template.pdf").expect("template");
    let certificate = fs::read("tests/fixtures/test-signing.cert.der").expect("certificate");
    let private_key = fs::read("tests/fixtures/test-signing.key.der").expect("private key");
    let signer =
        CertificateSigner::from_pkcs8_der(certificate, &private_key).expect("certificate signer");

    let secret = "SignerAuthKey@2026";
    let app_name = "SignAudit";
    let audit_data = serde_json::json!({
        "audit_event": "DIGITAL_SIGNATURE_APPLIED",
        "signer_ip": "14.161.20.55",
        "device": "macOS-Antigravity-Agent"
    });

    let pdf_signer = PdfSigner::new()
        .field("Signature_0")
        .signer(signer)
        .reason("Testing Locked PieceInfo in Signer")
        .locked_piece_info(app_name, audit_data.clone(), secret);

    let signed_pdf = pdf_signer.sign(&template).expect("sign pdf with locked piece_info");

    // Mở khóa từ file PDF đã ký số
    let verify_res = verify_and_unlock_piece_info(&signed_pdf, app_name, secret).expect("verify signed");
    assert_eq!(verify_res.status, PieceInfoUnlockStatus::Valid);
    let extracted = verify_res.data.unwrap();
    assert_eq!(extracted["audit_event"], "DIGITAL_SIGNATURE_APPLIED");
    assert_eq!(extracted["signer_ip"], "14.161.20.55");
}

#[test]
fn test_fill_pdf_with_locked_json_parameter() {
    let template = fs::read("reference/template.pdf").expect("template.pdf");
    let secret = "DynamicPass123";
    let app_name = "QuickLock";

    let piece_info_with_lock = serde_json::json!({
        "__locked": {
            "app_name": app_name,
            "data": { "session_id": "sess_abc_123" },
            "secret_key": secret
        }
    })
    .to_string();

    let (filled_pdf, report) =
        pdffiller_core::fill_pdf(&template, r#"{"name":"Test User"}"#, Some(&piece_info_with_lock))
            .expect("fill_pdf with __locked");
    assert_eq!(report.filled_count(), 1);

    let verify_res = verify_and_unlock_piece_info(&filled_pdf, app_name, secret).expect("verify");
    assert_eq!(verify_res.status, PieceInfoUnlockStatus::Valid);
    let data = verify_res.data.unwrap();
    assert_eq!(data["session_id"], "sess_abc_123");
}

#[test]
fn test_unmarked_file_returns_not_found_and_empty_list() {
    let template = fs::read("reference/template.pdf").expect("template.pdf");

    // File PDF gốc chưa từng qua thư viện sẽ không có PieceInfo
    let apps = pdffiller_core::list_piece_info_applications(&template).expect("list apps");
    assert!(apps.is_empty(), "File PDF goc phai tra ve danh sach rong");

    // Kiểm tra trên file không có PieceInfo -> Trả về NotFound an toàn
    let res = verify_and_unlock_piece_info(&template, "AnyApp", "AnySecretKey").expect("verify");
    assert_eq!(res.status, PieceInfoUnlockStatus::NotFound);
    assert!(res.data.is_none());
}
