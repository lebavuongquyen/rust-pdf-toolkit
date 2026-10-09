use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};
use pdftoolkit_core::batch::{BatchOptions, batch_rotate_dir_with_options};
use pdftoolkit_core::ops::{RotateOptions, get_page_rotation, rotate_pdf_pages};
use std::fs;

/// Helper to create a minimal valid PDF in memory with custom page dimensions, rotation, and text stream.
fn create_test_pdf(
    width: f64,
    height: f64,
    rotation: i32,
    text_stream_ops: Option<Vec<Operation>>,
) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let mut page_dict = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
    };
    if rotation != 0 {
        page_dict.set("Rotate", Object::Integer(rotation as i64));
    }

    if let Some(ops) = text_stream_ops {
        let content = Content { operations: ops };
        let content_bytes = content.encode().unwrap_or_default();
        let stream = Stream::new(dictionary! {}, content_bytes);
        let stream_id = doc.add_object(stream);
        page_dict.set("Contents", Object::Reference(stream_id));
    }

    let page_id = doc.add_object(page_dict);

    let pages_dict = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page_id.into()],
        "Count" => 1,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save test pdf");
    out
}

#[test]
fn test_rotate_explicit_angle_relative_and_absolute() {
    let template = fs::read("reference/template.pdf").expect("template");

    // 1. Initial rotation should be 0
    let doc_init = Document::load_mem(&template).expect("load init");
    let first_page = *doc_init.get_pages().values().next().unwrap();
    assert_eq!(get_page_rotation(&doc_init, first_page), 0);

    // 2. Rotate by +90 relative
    let (step1_bytes, rep1) =
        rotate_pdf_pages(&template, &RotateOptions::new().angle(90)).expect("rotate 90");
    assert_eq!(rep1.rotated_pages, 1);
    let doc1 = Document::load_mem(&step1_bytes).expect("load 1");
    let p1 = *doc1.get_pages().values().next().unwrap();
    assert_eq!(get_page_rotation(&doc1, p1), 90);

    // 3. Rotate by +90 relative again -> should become 180
    let (step2_bytes, rep2) =
        rotate_pdf_pages(&step1_bytes, &RotateOptions::new().angle(90)).expect("rotate 90 again");
    assert_eq!(rep2.rotated_pages, 1);
    let doc2 = Document::load_mem(&step2_bytes).expect("load 2");
    let p2 = *doc2.get_pages().values().next().unwrap();
    assert_eq!(get_page_rotation(&doc2, p2), 180);

    // 4. Rotate by 270 absolute -> sets directly to 270
    let (step3_bytes, rep3) =
        rotate_pdf_pages(&step2_bytes, &RotateOptions::new().angle_absolute(270))
            .expect("rotate 270 absolute");
    assert_eq!(rep3.rotated_pages, 1);
    let doc3 = Document::load_mem(&step3_bytes).expect("load 3");
    let p3 = *doc3.get_pages().values().next().unwrap();
    assert_eq!(get_page_rotation(&doc3, p3), 270);
}

#[test]
fn test_normalize_to_portrait_and_landscape() {
    // Create a 612x792 (US Letter Portrait) PDF with rotation 0
    let portrait_pdf = create_test_pdf(612.0, 792.0, 0, None);

    // 1. Target is Landscape: should rotate 90° CW to make it landscape
    let (land_bytes, rep1) = rotate_pdf_pages(&portrait_pdf, &RotateOptions::new().to_landscape())
        .expect("to landscape");
    assert_eq!(rep1.rotated_pages, 1);
    let doc_land = Document::load_mem(&land_bytes).expect("load land");
    let p_land = *doc_land.get_pages().values().next().unwrap();
    assert_eq!(get_page_rotation(&doc_land, p_land), 90);

    // 2. Target is Landscape again on already-landscape PDF -> idempotent!
    let (_land2_bytes, rep2) = rotate_pdf_pages(&land_bytes, &RotateOptions::new().to_landscape())
        .expect("to landscape idempotent");
    assert_eq!(
        rep2.rotated_pages, 0,
        "Already landscape, should not rotate"
    );

    // 3. Target is Portrait on landscape PDF -> rotates back to portrait!
    let (back_port_bytes, rep3) =
        rotate_pdf_pages(&land_bytes, &RotateOptions::new().to_portrait())
            .expect("back to portrait");
    assert_eq!(rep3.rotated_pages, 1);
    let doc_back = Document::load_mem(&back_port_bytes).expect("load back portrait");
    let p_back = *doc_back.get_pages().values().next().unwrap();
    assert_eq!(get_page_rotation(&doc_back, p_back), 180); // 90 + 90 = 180, visual width & height swap back!
}

#[test]
fn test_auto_detect_text_orientation_upright() {
    // Create horizontal text: Tm [1 0 0 1 100 200]
    let ops = vec![
        Operation::new("BT", vec![]),
        Operation::new(
            "Tm",
            vec![
                1.0.into(),
                0.0.into(),
                0.0.into(),
                1.0.into(),
                100.0.into(),
                200.0.into(),
            ],
        ),
        Operation::new(
            "Tj",
            vec![Object::string_literal("Upright Horizontal Text")],
        ),
        Operation::new("ET", vec![]),
    ];
    let pdf = create_test_pdf(612.0, 792.0, 0, Some(ops));

    // Auto-detect text orientation on already-horizontal page -> detected 0 deg, no rotation needed!
    let (_out_bytes, report) =
        rotate_pdf_pages(&pdf, &RotateOptions::new().auto_detect_text()).expect("auto detect text");
    assert_eq!(report.rotated_pages, 0);
    assert_eq!(report.pages_details[0].detected_text_angle, Some(0));
    assert!(!report.pages_details[0].modified);
}

#[test]
fn test_auto_detect_text_orientation_rotated_90() {
    // Create text tilted 90 degrees CCW in PDF user space:
    // Tm [cos(90) sin(90) -sin(90) cos(90) x y] = [0 1 -1 0 100 200]
    let ops = vec![
        Operation::new("BT", vec![]),
        Operation::new(
            "Tm",
            vec![
                0.0.into(),
                1.0.into(),
                (-1.0).into(),
                0.0.into(),
                100.0.into(),
                200.0.into(),
            ],
        ),
        Operation::new("Tj", vec![Object::string_literal("Sideways Vertical Text")]),
        Operation::new("ET", vec![]),
    ];
    let pdf = create_test_pdf(612.0, 792.0, 0, Some(ops));

    // Auto-detect should detect text angle 90° and set page /Rotate to 90° to make text upright!
    let (out_bytes, report) = rotate_pdf_pages(&pdf, &RotateOptions::new().auto_detect_text())
        .expect("auto detect 90 deg text");
    assert_eq!(report.rotated_pages, 1);
    assert_eq!(report.pages_details[0].detected_text_angle, Some(90));
    assert_eq!(report.pages_details[0].new_rotation, 90);
    assert!(report.pages_details[0].modified);

    let doc = Document::load_mem(&out_bytes).expect("load out");
    let p = *doc.get_pages().values().next().unwrap();
    assert_eq!(get_page_rotation(&doc, p), 90);
}

#[test]
fn test_auto_detect_fallback_angle_when_no_text() {
    // Create page without text stream
    let pdf = create_test_pdf(612.0, 792.0, 0, None);

    // Auto-detect without fallback leaves page at 0
    let (_out1, rep1) = rotate_pdf_pages(&pdf, &RotateOptions::new().auto_detect_text())
        .expect("no text no fallback");
    assert_eq!(rep1.rotated_pages, 0);
    assert_eq!(rep1.pages_details[0].detected_text_angle, None);

    // Auto-detect WITH fallback angle 180 rotates to 180
    let (out2, rep2) = rotate_pdf_pages(
        &pdf,
        &RotateOptions::new().auto_detect_text_with_fallback(180),
    )
    .expect("no text with fallback");
    assert_eq!(rep2.rotated_pages, 1);
    let doc = Document::load_mem(&out2).expect("load");
    let p = *doc.get_pages().values().next().unwrap();
    assert_eq!(get_page_rotation(&doc, p), 180);
}

#[test]
fn test_batch_rotate_dir() {
    let temp_in = "output/test_batch_rotate_in";
    let temp_out = "output/test_batch_rotate_out";
    fs::create_dir_all(temp_in).expect("create in");

    let pdf1 = create_test_pdf(612.0, 792.0, 0, None);
    let pdf2 = create_test_pdf(792.0, 612.0, 0, None);
    fs::write(format!("{temp_in}/doc1.pdf"), &pdf1).expect("write 1");
    fs::write(format!("{temp_in}/doc2.pdf"), &pdf2).expect("write 2");

    let rot_opts = RotateOptions::new().to_portrait();
    let batch_opts = BatchOptions::new();
    let rep = batch_rotate_dir_with_options(temp_in, temp_out, &rot_opts, &batch_opts)
        .expect("batch rotate");
    assert_eq!(rep.total_scanned, 2);
    assert_eq!(rep.successful, 2);
    assert_eq!(rep.failed, 0);

    // Clean up temporary test files
    let _ = fs::remove_dir_all(temp_in);
    let _ = fs::remove_dir_all(temp_out);
}
