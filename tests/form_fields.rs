extern crate pdftoolkit_core as pdffiller_core;
use pdffiller_core::{FormFieldType, get_form_fields};
use std::fs;

#[test]
fn discovers_all_form_fields_with_exact_types() {
    let template = fs::read("reference/template_8field.pdf").expect("template");
    let fields = get_form_fields(&template).expect("fields");

    let expected = [
        ("Push Button0", FormFieldType::Button),
        ("Check Box0", FormFieldType::Checkbox),
        ("Radio Button0", FormFieldType::Radio),
        ("Signature_0", FormFieldType::Signature),
        ("Text Field0", FormFieldType::Text),
        ("Combo Box0", FormFieldType::ComboBox),
        ("List Box0", FormFieldType::ListBox),
        ("Barcode Field0", FormFieldType::Barcode),
        ("Image Field0", FormFieldType::Image),
        ("Date Field0", FormFieldType::Date),
    ];

    assert_eq!(fields.len(), expected.len());

    for (name, field_type) in expected {
        let field = fields
            .iter()
            .find(|field| field.name == name)
            .unwrap_or_else(|| panic!("missing field: {name}"));

        assert_eq!(field.field_type, field_type, "wrong type for {name}");
        assert_eq!(field.page, Some(1));
        assert!(field.rect.is_some());
        assert_eq!(field.locations.len(), 1);
        assert_eq!(field.locations[0].page, 1);
    }
}

#[test]
fn extracts_date_format_for_date_fields() {
    let template8 = fs::read("reference/template_8field.pdf").expect("template_8field");
    let fields8 = get_form_fields(&template8).expect("fields");

    let date_field8 = fields8
        .iter()
        .find(|f| f.name == "Date Field0")
        .expect("Date Field0");
    assert_eq!(date_field8.field_type, FormFieldType::Date);
    assert_eq!(date_field8.date_format.as_deref(), Some("m/d/yy"));

    // Other non-date fields should not have a date_format
    let text_field = fields8
        .iter()
        .find(|f| f.name == "Text Field0")
        .expect("Text Field0");
    assert_eq!(text_field.date_format, None);

    let template_ref = fs::read("reference/template.pdf").expect("template");
    let fields_ref = get_form_fields(&template_ref).expect("fields");
    let date_field_ref = fields_ref.iter().find(|f| f.name == "date").expect("date");
    assert_eq!(date_field_ref.field_type, FormFieldType::Date);
    assert_eq!(date_field_ref.date_format.as_deref(), Some("dd/mm/yyyy"));

    // Verify JSON serialization includes date_format
    let json = serde_json::to_string(&fields8).expect("json");
    assert!(json.contains(r#""date_format":"m/d/yy""#));
}

#[test]
fn extracts_image_field_value_when_filled() {
    use base64::Engine;

    let template = fs::read("reference/template_8field.pdf").expect("template");
    let fields_before = get_form_fields(&template).expect("fields");
    let image_field_before = fields_before
        .iter()
        .find(|f| f.name == "Image Field0")
        .expect("Image Field0");
    assert_eq!(image_field_before.field_type, FormFieldType::Image);
    assert_eq!(image_field_before.value, None);

    // Read a test JPEG image
    let img_bytes = fs::read("tests/fixtures/extracted-image.jpg").expect("sample image");
    let b64 = base64::engine::general_purpose::STANDARD.encode(&img_bytes);
    let data_url = format!("data:image/jpeg;base64,{}", b64);

    let fill_json = serde_json::json!({
        "Image Field0": data_url
    })
    .to_string();

    let (filled_bytes, report) =
        pdffiller_core::fill_pdf(&template, &fill_json, None).expect("fill");
    assert_eq!(report.filled_count(), 1);

    let fields_after = get_form_fields(&filled_bytes).expect("fields");
    let image_field_after = fields_after
        .iter()
        .find(|f| f.name == "Image Field0")
        .expect("Image Field0");
    assert_eq!(image_field_after.field_type, FormFieldType::Image);
    assert!(image_field_after.value.is_some());
    let val_str = image_field_after.value.as_ref().unwrap().as_str().unwrap();
    assert!(val_str.starts_with("data:image/jpeg;base64,"));

    // Verify fixed-image.pdf appearance extraction
    let fixed_bytes = fs::read("tests/fixtures/fixed-image.pdf").expect("fixed-image");
    let fixed_fields = get_form_fields(&fixed_bytes).expect("fields");
    let avatar_field = fixed_fields
        .iter()
        .find(|f| f.name == "avatar")
        .expect("avatar");
    assert_eq!(avatar_field.field_type, FormFieldType::Image);
    assert!(avatar_field.value.is_some());
    let avatar_val = avatar_field.value.as_ref().unwrap().as_str().unwrap();
    assert!(avatar_val.starts_with("data:image/jpeg;base64,"));
}

#[test]
fn extracts_signature_info_and_image_from_signed_pdf() {
    let bytes = fs::read("reference/template_signed.pdf").expect("read template_signed.pdf");
    let fields = pdffiller_core::get_form_fields(&bytes).expect("get_form_fields");

    let sig_field = fields
        .iter()
        .find(|f| f.field_type == FormFieldType::Signature)
        .expect("Signature field not found");

    assert_eq!(sig_field.name, "Signature_0");
    assert_eq!(sig_field.signed, Some(true));

    // Verify visual image
    assert!(sig_field.value.is_some());
    let value_str = sig_field.value.as_ref().unwrap().as_str().unwrap();
    assert!(
        value_str.starts_with("data:image/jpeg;base64,"),
        "Expected value to contain JPEG data URL"
    );

    // Verify signature info struct
    let sig_info = sig_field
        .signature
        .as_ref()
        .expect("SignatureInfo should be present");
    assert_eq!(
        sig_info.reason.as_deref(),
        Some("I am the author of this document")
    );
    assert_eq!(sig_info.filter.as_deref(), Some("Adobe.PPKLite"));
    assert_eq!(sig_info.sub_filter.as_deref(), Some("adbe.pkcs7.detached"));
    assert_eq!(
        sig_info.signing_time.as_deref(),
        Some("D:20261008162001+07'00'")
    );
    assert!(sig_info.image.is_some());
    assert!(
        sig_info
            .image
            .as_ref()
            .unwrap()
            .starts_with("data:image/jpeg;base64,")
    );

    // Verify certificate info
    assert_eq!(
        sig_info.signer_name.as_deref(),
        Some("e8f4f4a3-fcad-44aa-af38-715f6ae7f8af")
    );
    assert_eq!(
        sig_info.issuer.as_deref(),
        Some("e8f4f4a3-fcad-44aa-af38-715f6ae7f8af")
    );
    assert!(sig_info.not_before.is_some());
    assert!(sig_info.not_after.is_some());
    assert!(sig_info.serial_number.is_some());
}

#[test]
fn extracts_all_field_types_values_when_filled() {
    use base64::Engine;

    let template = fs::read("reference/template_8field.pdf").expect("template_8field.pdf");
    let fields = get_form_fields(&template).expect("get_form_fields");

    let combo = fields
        .iter()
        .find(|f| f.name == "Combo Box0")
        .expect("combo");
    let combo_val = combo
        .options
        .first()
        .map(|o| o.value.clone())
        .unwrap_or_else(|| "Item1".into());

    let list = fields.iter().find(|f| f.name == "List Box0").expect("list");
    let list_val = list
        .options
        .first()
        .map(|o| o.value.clone())
        .unwrap_or_else(|| "Item1".into());

    let radio = fields
        .iter()
        .find(|f| f.name == "Radio Button0")
        .expect("radio");
    let radio_val = radio
        .options
        .first()
        .map(|o| o.value.clone())
        .unwrap_or_else(|| "Choice1".into());

    let img_bytes = fs::read("tests/fixtures/extracted-image.jpg").expect("sample image");
    let b64 = base64::engine::general_purpose::STANDARD.encode(&img_bytes);
    let data_url = format!("data:image/jpeg;base64,{}", b64);

    let fill_data = serde_json::json!({
        "Text Field0": "Nguyen Van A",
        "Date Field0": "12/25/2026",
        "Check Box0": true,
        "Radio Button0": radio_val,
        "Combo Box0": combo_val,
        "List Box0": list_val,
        "Image Field0": data_url,
    });

    let (filled_bytes, report) =
        pdffiller_core::fill_pdf(&template, &fill_data.to_string(), None).expect("fill");
    assert!(report.filled_count() >= 6);

    let filled_fields = get_form_fields(&filled_bytes).expect("fields after fill");

    let text_f = filled_fields
        .iter()
        .find(|f| f.name == "Text Field0")
        .unwrap();
    assert_eq!(
        text_f.value,
        Some(serde_json::Value::String("Nguyen Van A".into()))
    );

    let date_f = filled_fields
        .iter()
        .find(|f| f.name == "Date Field0")
        .unwrap();
    assert_eq!(
        date_f.value,
        Some(serde_json::Value::String("12/25/2026".into()))
    );

    let check_f = filled_fields
        .iter()
        .find(|f| f.name == "Check Box0")
        .unwrap();
    assert!(check_f.value.is_some());
    assert_ne!(check_f.value.as_ref().unwrap().as_str().unwrap(), "Off");

    let combo_f = filled_fields
        .iter()
        .find(|f| f.name == "Combo Box0")
        .unwrap();
    assert_eq!(combo_f.value, Some(serde_json::Value::String(combo_val)));

    let list_f = filled_fields
        .iter()
        .find(|f| f.name == "List Box0")
        .unwrap();
    assert_eq!(list_f.value, Some(serde_json::Value::String(list_val)));

    let img_f = filled_fields
        .iter()
        .find(|f| f.name == "Image Field0")
        .unwrap();
    assert!(img_f.value.is_some());
    assert!(
        img_f
            .value
            .as_ref()
            .unwrap()
            .as_str()
            .unwrap()
            .starts_with("data:image/jpeg;base64,")
    );
}

#[test]
fn fill_pdf_with_options_flattens_fields_except_signature() {
    let template = fs::read("reference/template_8field.pdf").expect("template_8field.pdf");
    let fields_before = get_form_fields(&template).expect("get_form_fields");
    assert_eq!(fields_before.len(), 10);

    let fill_data = serde_json::json!({
        "Text Field0": "Flattened Text",
        "Check Box0": true,
    });

    // 1. When flatten is false: all 10 fields remain in AcroForm
    let (unflat_bytes, _) = pdffiller_core::fill_pdf_with_options(
        &template,
        &fill_data.to_string(),
        &pdffiller_core::FillOptions {
            flatten: false,
            ..Default::default()
        },
    )
    .expect("fill unflat");
    let unflat_fields = get_form_fields(&unflat_bytes).expect("fields unflat");
    assert_eq!(unflat_fields.len(), 10);

    // 2. When flatten is true: non-signature fields are flattened, Signature_0 remains interactive
    let (flat_bytes, report) = pdffiller_core::fill_pdf_with_options(
        &template,
        &fill_data.to_string(),
        &pdffiller_core::FillOptions {
            flatten: true,
            ..Default::default()
        },
    )
    .expect("fill flat");
    assert_eq!(report.filled_count(), 2);

    let flat_fields = get_form_fields(&flat_bytes).expect("fields flat");
    // Only Signature_0 remains as a form field!
    assert_eq!(flat_fields.len(), 1);
    assert_eq!(flat_fields[0].name, "Signature_0");
    assert_eq!(flat_fields[0].field_type, FormFieldType::Signature);
    assert_eq!(flat_fields[0].signed, Some(true));

    // 3. Test on template.pdf where signature is unsigned:
    let template_ref = fs::read("reference/template.pdf").expect("template.pdf");
    let (flat_ref_bytes, _) = pdffiller_core::fill_pdf_with_options(
        &template_ref,
        r#"{"full_name":"Test User"}"#,
        &pdffiller_core::FillOptions {
            flatten: true,
            ..Default::default()
        },
    )
    .expect("fill flat template.pdf");
    let flat_ref_fields = get_form_fields(&flat_ref_bytes).expect("fields flat template.pdf");
    let sig_ref = flat_ref_fields
        .iter()
        .find(|f| f.field_type == FormFieldType::Signature)
        .expect("Signature field in template.pdf");
    assert_eq!(sig_ref.signed, Some(false));
}

#[test]
fn pdf_appearance_sets_custom_signature_layout() {
    use base64::Engine;

    let template = fs::read("reference/template_8field.pdf").expect("template_8field.pdf");
    let img_bytes = fs::read("tests/fixtures/extracted-image.jpg").expect("sample image");
    let b64 = base64::engine::general_purpose::STANDARD.encode(&img_bytes);
    let data_url = format!("data:image/jpeg;base64,{}", b64);

    let options = pdffiller_core::SignatureAppearanceOptions {
        image: Some(data_url),
        position: pdffiller_core::GraphicPosition::Left,
        font: pdffiller_core::SignatureFont::Helvetica,
        font_size: Some(8.5),
        bold: true,
        italic: false,
        align: pdffiller_core::TextAlign::Left,
        text_color: Some([10, 10, 10]),
        show_signer_name: true,
        signer_name: Some("Tran Van B".into()),
        show_date: true,
        date: Some("2026-10-08".into()),
        show_reason: true,
        reason: Some("Approved document".into()),
        show_location: true,
        location: Some("Da Nang, VN".into()),
        labels: None,
        extra_lines: vec!["Custom line 1".into()],
        ..Default::default()
    };

    let result =
        pdffiller_core::PdfAppearance::set_signature_appearance(&template, "Signature_0", &options)
            .expect("set_signature_appearance");

    let fields = get_form_fields(&result).expect("fields");
    let sig_f = fields
        .iter()
        .find(|f| f.name == "Signature_0")
        .expect("Signature_0");
    assert_eq!(sig_f.field_type, FormFieldType::Signature);
    assert!(sig_f.value.is_some());
}

#[test]
fn test_fill_pdf_piece_info_insertion_and_extraction() {
    let template = fs::read("reference/template.pdf").expect("template.pdf");

    // 1. None piece_info -> None extracted
    let (pdf_no_piece, _) =
        pdffiller_core::fill_pdf(&template, r#"{"name":"Test"}"#, None).expect("fill");
    assert_eq!(pdffiller_core::get_piece_info(&pdf_no_piece).unwrap(), None);

    // 2. Some piece_info as JSON string
    let piece_str = r#"{"AppMeta":{"doc_uuid":"uuid-123-abc","version":2,"active":true}}"#;
    let (pdf_with_piece, report) =
        pdffiller_core::fill_pdf(&template, r#"{"name":"Test"}"#, Some(piece_str))
            .expect("fill with piece");
    assert_eq!(report.filled_count(), 1);

    let extracted = pdffiller_core::get_piece_info(&pdf_with_piece)
        .unwrap()
        .expect("piece_info");
    assert_eq!(extracted["AppMeta"]["doc_uuid"], "uuid-123-abc");
    assert_eq!(extracted["AppMeta"]["version"], 2);
    assert_eq!(extracted["AppMeta"]["active"], true);

    // 3. Flattened PDF with piece_info
    let opt = pdffiller_core::FillOptions::new()
        .flatten(true)
        .piece_info(serde_json::json!({
            "FlattenMeta": { "status": "archived", "pages": 1 }
        }));
    let (pdf_flattened, _) =
        pdffiller_core::fill_pdf_with_options(&template, r#"{"name":"Test"}"#, &opt)
            .expect("fill flat");
    let ext_flat = pdffiller_core::get_piece_info(&pdf_flattened)
        .unwrap()
        .expect("flat piece_info");
    assert_eq!(ext_flat["FlattenMeta"]["status"], "archived");
    assert_eq!(ext_flat["FlattenMeta"]["pages"], 1);
}

#[test]
fn test_template_8field_choice_fields_discovery() {
    let template = fs::read("reference/template_8field.pdf").expect("template_8field");
    let fields = get_form_fields(&template).expect("fields");

    let combo = fields
        .iter()
        .find(|f| f.name == "Combo Box0")
        .expect("combo");
    assert_eq!(combo.field_type, FormFieldType::ComboBox);
    assert!(combo.multi_select.is_some());
    assert!(combo.editable.is_some());
    assert_eq!(combo.editable, combo.custom_option);

    let list = fields.iter().find(|f| f.name == "List Box0").expect("list");
    assert_eq!(list.field_type, FormFieldType::ListBox);
    assert!(list.multi_select.is_some());
    assert!(list.editable.is_some());
    assert_eq!(list.editable, list.custom_option);

    // Non-choice field has None
    let text = fields
        .iter()
        .find(|f| f.name == "Text Field0")
        .expect("text");
    assert_eq!(text.multi_select, None);
    assert_eq!(text.editable, None);
    assert_eq!(text.custom_option, None);
}

#[test]
fn test_choice_fields_multi_select_and_custom_options() {
    use lopdf::{Dictionary, Document, Object};

    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();

    // 1. Multi-select ListBox with custom option allowed (Bit 19 Edit: 1<<18, Bit 22 MultiSelect: 1<<21)
    let list_flags = (1 << 18) | (1 << 21);
    let mut list_dict = Dictionary::new();
    list_dict.set("Type", Object::Name(b"Annot".to_vec()));
    list_dict.set("Subtype", Object::Name(b"Widget".to_vec()));
    list_dict.set("FT", Object::Name(b"Ch".to_vec()));
    list_dict.set("T", Object::string_literal("multi_list"));
    list_dict.set("Ff", Object::Integer(list_flags as i64));
    list_dict.set(
        "Opt",
        Object::Array(vec![
            Object::string_literal("A"),
            Object::string_literal("B"),
            Object::string_literal("C"),
        ]),
    );
    list_dict.set(
        "Rect",
        Object::Array(vec![10.into(), 10.into(), 100.into(), 100.into()]),
    );
    let list_id = doc.add_object(list_dict);

    // 2. Single-select ComboBox with editable / custom option allowed (Bit 18 Combo: 1<<17, Bit 19 Edit: 1<<18)
    let combo_flags = (1 << 17) | (1 << 18);
    let mut combo_dict = Dictionary::new();
    combo_dict.set("Type", Object::Name(b"Annot".to_vec()));
    combo_dict.set("Subtype", Object::Name(b"Widget".to_vec()));
    combo_dict.set("FT", Object::Name(b"Ch".to_vec()));
    combo_dict.set("T", Object::string_literal("editable_combo"));
    combo_dict.set("Ff", Object::Integer(combo_flags as i64));
    combo_dict.set(
        "Opt",
        Object::Array(vec![
            Object::string_literal("X"),
            Object::string_literal("Y"),
        ]),
    );
    combo_dict.set(
        "Rect",
        Object::Array(vec![10.into(), 110.into(), 100.into(), 150.into()]),
    );
    let combo_id = doc.add_object(combo_dict);

    // 3. Regular non-editable single-select ComboBox (Bit 18 Combo: 1<<17)
    let standard_combo_flags = 1 << 17;
    let mut s_combo_dict = Dictionary::new();
    s_combo_dict.set("Type", Object::Name(b"Annot".to_vec()));
    s_combo_dict.set("Subtype", Object::Name(b"Widget".to_vec()));
    s_combo_dict.set("FT", Object::Name(b"Ch".to_vec()));
    s_combo_dict.set("T", Object::string_literal("standard_combo"));
    s_combo_dict.set("Ff", Object::Integer(standard_combo_flags as i64));
    s_combo_dict.set(
        "Opt",
        Object::Array(vec![
            Object::string_literal("Opt1"),
            Object::string_literal("Opt2"),
        ]),
    );
    s_combo_dict.set(
        "Rect",
        Object::Array(vec![10.into(), 160.into(), 100.into(), 200.into()]),
    );
    let s_combo_id = doc.add_object(s_combo_dict);

    // Page
    let mut page_dict = Dictionary::new();
    page_dict.set("Type", Object::Name(b"Page".to_vec()));
    page_dict.set("Parent", Object::Reference(pages_id));
    page_dict.set(
        "MediaBox",
        Object::Array(vec![0.into(), 0.into(), 600.into(), 800.into()]),
    );
    page_dict.set(
        "Annots",
        Object::Array(vec![
            Object::Reference(list_id),
            Object::Reference(combo_id),
            Object::Reference(s_combo_id),
        ]),
    );
    doc.objects.insert(page_id, Object::Dictionary(page_dict));

    // Pages
    let mut pages_dict = Dictionary::new();
    pages_dict.set("Type", Object::Name(b"Pages".to_vec()));
    pages_dict.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    pages_dict.set("Count", Object::Integer(1));
    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));

    // Catalog & AcroForm
    let acroform_id = doc.new_object_id();
    let mut acroform_dict = Dictionary::new();
    acroform_dict.set(
        "Fields",
        Object::Array(vec![
            Object::Reference(list_id),
            Object::Reference(combo_id),
            Object::Reference(s_combo_id),
        ]),
    );
    doc.objects
        .insert(acroform_id, Object::Dictionary(acroform_dict));

    let catalog_id = doc.new_object_id();
    let mut catalog_dict = Dictionary::new();
    catalog_dict.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog_dict.set("Pages", Object::Reference(pages_id));
    catalog_dict.set("AcroForm", Object::Reference(acroform_id));
    doc.objects
        .insert(catalog_id, Object::Dictionary(catalog_dict));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut pdf_bytes = Vec::new();
    doc.save_to(&mut pdf_bytes).expect("save pdf");

    // Discover fields
    let fields = get_form_fields(&pdf_bytes).expect("get_form_fields");
    assert_eq!(fields.len(), 3);

    // Verify multi_list
    let list_f = fields.iter().find(|f| f.name == "multi_list").unwrap();
    assert_eq!(list_f.field_type, FormFieldType::ListBox);
    assert_eq!(list_f.multi_select, Some(true));
    assert_eq!(list_f.editable, Some(true));
    assert_eq!(list_f.custom_option, Some(true));

    // Verify editable_combo
    let combo_f = fields.iter().find(|f| f.name == "editable_combo").unwrap();
    assert_eq!(combo_f.field_type, FormFieldType::ComboBox);
    assert_eq!(combo_f.multi_select, Some(false));
    assert_eq!(combo_f.editable, Some(true));
    assert_eq!(combo_f.custom_option, Some(true));

    // Verify standard_combo
    let s_combo_f = fields.iter().find(|f| f.name == "standard_combo").unwrap();
    assert_eq!(s_combo_f.field_type, FormFieldType::ComboBox);
    assert_eq!(s_combo_f.multi_select, Some(false));
    assert_eq!(s_combo_f.editable, Some(false));
    assert_eq!(s_combo_f.custom_option, Some(false));

    // Fill test: custom option allowed in editable fields
    let fill_data = serde_json::json!({
        "editable_combo": "Custom Value Z",
        "multi_list": ["A", "Custom Value 123"],
    });
    let (filled_bytes, report) = pdffiller_core::fill_pdf(&pdf_bytes, &fill_data.to_string(), None)
        .expect("fill custom options");
    assert_eq!(report.filled_count(), 2);

    let filled_fields = get_form_fields(&filled_bytes).expect("get filled fields");
    let filled_combo = filled_fields
        .iter()
        .find(|f| f.name == "editable_combo")
        .unwrap();
    assert_eq!(
        filled_combo.value,
        Some(serde_json::Value::String("Custom Value Z".into()))
    );

    // Fill test: standard_combo marks invalid when providing non-existent option
    let invalid_fill = serde_json::json!({
        "standard_combo": "Not In List",
    });
    let (_pdf, report_invalid) =
        pdffiller_core::fill_pdf(&pdf_bytes, &invalid_fill.to_string(), None)
            .expect("fill returns report");
    assert_eq!(report_invalid.filled_count(), 0);
    assert_eq!(report_invalid.fields.len(), 1);
    assert_eq!(
        report_invalid.fields[0].status,
        pdffiller_core::FieldStatus::Invalid
    );
    assert_eq!(
        report_invalid.fields[0].reason.as_deref(),
        Some("Choice option not found")
    );
}
