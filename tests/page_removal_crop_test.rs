use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};
use pdftoolkit_core::batch::{
    BatchOptions, batch_crop_dir_with_options, batch_remove_pages_dir_with_options,
};
use pdftoolkit_core::ops::{
    CropOptions, PageSelection, RemovePagesOptions, TargetBox, crop_pdf_pages, remove_pdf_pages,
};
use std::fs;

/// Helper to create a multi-page test PDF with distinct page contents.
fn create_multipage_pdf(pages_spec: &[(f64, f64, Option<Vec<Operation>>)]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let mut page_ids = Vec::new();

    for (w, h, ops_opt) in pages_spec {
        let mut page_dict = dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), (*w).into(), (*h).into()],
        };

        if let Some(ops) = ops_opt {
            let content = Content {
                operations: ops.clone(),
            };
            let content_bytes = content.encode().unwrap_or_default();
            let stream = Stream::new(dictionary! {}, content_bytes);
            let stream_id = doc.add_object(stream);
            page_dict.set("Contents", Object::Reference(stream_id));
        }

        let page_id = doc.add_object(page_dict);
        page_ids.push(page_id);
    }

    let pages_dict = dictionary! {
        "Type" => "Pages",
        "Kids" => page_ids.iter().map(|&id| Object::Reference(id)).collect::<Vec<_>>(),
        "Count" => page_ids.len() as u32,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save multipage test pdf");
    out
}

#[test]
fn test_remove_cover_and_back_cover() {
    let text_ops = vec![Operation::new("Tj", vec![Object::string_literal("Hello")])];
    let pdf_bytes = create_multipage_pdf(&[
        (595.0, 842.0, Some(text_ops.clone())), // Page 1: Cover
        (595.0, 842.0, Some(text_ops.clone())), // Page 2: Body
        (595.0, 842.0, Some(text_ops.clone())), // Page 3: Body
        (595.0, 842.0, Some(text_ops.clone())), // Page 4: Back Cover
    ]);

    // 1. Remove cover only
    let opts_cover = RemovePagesOptions::new().remove_cover(true);
    let (out_cover, rep_cover) = remove_pdf_pages(&pdf_bytes, &opts_cover).expect("remove cover");
    assert_eq!(rep_cover.original_pages, 4);
    assert_eq!(rep_cover.retained_pages, 3);
    assert_eq!(rep_cover.removed_pages, vec![1]);
    assert!(rep_cover.cover_removed);
    assert!(!rep_cover.back_cover_removed);
    let doc_cover = Document::load_mem(&out_cover).expect("load out");
    assert_eq!(doc_cover.get_pages().len(), 3);

    // 2. Remove back cover only
    let opts_back = RemovePagesOptions::new().remove_back_cover(true);
    let (_out_back, rep_back) =
        remove_pdf_pages(&pdf_bytes, &opts_back).expect("remove back cover");
    assert_eq!(rep_back.retained_pages, 3);
    assert_eq!(rep_back.removed_pages, vec![4]);
    assert!(!rep_back.cover_removed);
    assert!(rep_back.back_cover_removed);

    // 3. Remove both cover and back cover
    let opts_both = RemovePagesOptions::new()
        .remove_cover(true)
        .remove_back_cover(true);
    let (out_both, rep_both) = remove_pdf_pages(&pdf_bytes, &opts_both).expect("remove both");
    assert_eq!(rep_both.retained_pages, 2);
    assert_eq!(rep_both.removed_pages, vec![1, 4]);
    assert!(rep_both.cover_removed);
    assert!(rep_both.back_cover_removed);
    let doc_both = Document::load_mem(&out_both).expect("load both");
    assert_eq!(doc_both.get_pages().len(), 2);
}

#[test]
fn test_remove_blank_pages() {
    let text_ops = vec![Operation::new(
        "Tj",
        vec![Object::string_literal("Content")],
    )];
    let whitespace_ops = vec![Operation::new(
        "Tj",
        vec![Object::string_literal("   \t\n  ")],
    )];

    let pdf_bytes = create_multipage_pdf(&[
        (595.0, 842.0, Some(text_ops.clone())), // Page 1: Has visible text
        (595.0, 842.0, None),                   // Page 2: No Contents stream (blank)
        (595.0, 842.0, Some(whitespace_ops.clone())), // Page 3: Whitespace only text (blank)
        (595.0, 842.0, Some(text_ops.clone())), // Page 4: Has visible text
    ]);

    let opts = RemovePagesOptions::new().remove_blank_pages(true);
    let (out_bytes, report) = remove_pdf_pages(&pdf_bytes, &opts).expect("remove blank pages");

    assert_eq!(report.original_pages, 4);
    assert_eq!(report.blank_pages_detected, vec![2, 3]);
    assert_eq!(report.removed_pages, vec![2, 3]);
    assert_eq!(report.retained_pages, 2);

    let doc = Document::load_mem(&out_bytes).expect("load result");
    assert_eq!(doc.get_pages().len(), 2);
}

#[test]
fn test_remove_pages_range_and_keep_protection() {
    let text_ops = vec![Operation::new("Tj", vec![Object::string_literal("Page")])];
    let pdf_bytes = create_multipage_pdf(&[
        (595.0, 842.0, Some(text_ops.clone())), // 1
        (595.0, 842.0, Some(text_ops.clone())), // 2
        (595.0, 842.0, Some(text_ops.clone())), // 3
        (595.0, 842.0, Some(text_ops.clone())), // 4
        (595.0, 842.0, Some(text_ops.clone())), // 5
    ]);

    // Target remove pages 2-4, but protect page 3 with keep_pages
    let opts = RemovePagesOptions::new()
        .pages_str("2-4")
        .keep_pages_str("3");

    let (out_bytes, report) = remove_pdf_pages(&pdf_bytes, &opts).expect("remove with keep");
    assert_eq!(report.removed_pages, vec![2, 4]);
    assert_eq!(report.retained_pages, 3); // 1, 3, 5

    let doc = Document::load_mem(&out_bytes).expect("load doc");
    assert_eq!(doc.get_pages().len(), 3);
}

#[test]
fn test_min_pages_retained_safety() {
    let text_ops = vec![Operation::new("Tj", vec![Object::string_literal("Only")])];
    let pdf_bytes = create_multipage_pdf(&[
        (595.0, 842.0, Some(text_ops.clone())), // 1
        (595.0, 842.0, Some(text_ops.clone())), // 2
    ]);

    // Try to remove all pages when min_pages_retained = 1
    let opts = RemovePagesOptions::new()
        .pages(PageSelection::All)
        .min_pages_retained(1);

    let res = remove_pdf_pages(&pdf_bytes, &opts);
    assert!(res.is_err());
    let err_msg = res.err().unwrap();
    assert!(err_msg.contains("min_pages_retained"));
}

#[test]
fn test_crop_margins_absolute_and_relative() {
    let pdf_bytes = create_multipage_pdf(&[(600.0, 800.0, None)]);

    // 1. Margin crop in points: top=50, bottom=50, left=40, right=40
    let opts_abs = CropOptions::new()
        .margins(50.0, 50.0, 40.0, 40.0)
        .target_box(TargetBox::CropBox);

    let (out_abs, rep_abs) = crop_pdf_pages(&pdf_bytes, &opts_abs).expect("crop abs");
    assert_eq!(rep_abs.cropped_pages, 1);
    let detail = &rep_abs.pages_details[0];
    assert_eq!(detail.new_box, [40.0, 50.0, 560.0, 750.0]);

    let doc_abs = Document::load_mem(&out_abs).expect("load abs");
    let p_id = *doc_abs.get_pages().values().next().unwrap();
    let crop_arr = doc_abs
        .get_object(p_id)
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"CropBox")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(crop_arr.len(), 4);

    // 2. Relative margin crop: 10% on all sides (left=60, right=60, bottom=80, top=80)
    let opts_rel = CropOptions::new().margins_relative(0.1, 0.1, 0.1, 0.1);
    let (_out_rel, rep_rel) = crop_pdf_pages(&pdf_bytes, &opts_rel).expect("crop rel");
    let detail_rel = &rep_rel.pages_details[0];
    assert_eq!(detail_rel.new_box, [60.0, 80.0, 540.0, 720.0]);
}

#[test]
fn test_crop_explicit_box_and_clamping() {
    let pdf_bytes = create_multipage_pdf(&[(600.0, 800.0, None)]);

    // Crop box that exceeds MediaBox bounds with clamp_to_media_box = true
    let opts = CropOptions::new()
        .crop_box([-50.0, -20.0, 700.0, 900.0])
        .clamp_to_media_box(true);

    let (_out, report) = crop_pdf_pages(&pdf_bytes, &opts).expect("crop clamp");
    let detail = &report.pages_details[0];
    // Clamped inside [0, 0, 600, 800]
    assert_eq!(detail.new_box, [0.0, 0.0, 600.0, 800.0]);
}

#[test]
fn test_crop_auto_content_detection() {
    // Path rectangle at (100, 150) with width 200, height 300
    let path_ops = vec![Operation::new(
        "re",
        vec![100.0.into(), 150.0.into(), 200.0.into(), 300.0.into()],
    )];

    let pdf_bytes = create_multipage_pdf(&[(600.0, 800.0, Some(path_ops))]);

    // Auto content crop with padding = 10.0
    let opts = CropOptions::new().auto_detect_content(10.0);
    let (_out, report) = crop_pdf_pages(&pdf_bytes, &opts).expect("auto crop");

    assert_eq!(report.cropped_pages, 1);
    let detail = &report.pages_details[0];
    assert!(detail.detected_content_bbox.is_some());
    let detected = detail.detected_content_bbox.unwrap();
    assert_eq!(detected, [100.0, 150.0, 300.0, 450.0]);
    // With padding 10.0: [90, 140, 310, 460]
    assert_eq!(detail.new_box, [90.0, 140.0, 310.0, 460.0]);
}

#[test]
fn test_batch_remove_and_crop() {
    let tmp_dir = std::env::temp_dir().join("pdftoolkit_batch_mod4_test");
    let in_dir = tmp_dir.join("input");
    let out_remove_dir = tmp_dir.join("output_remove");
    let out_crop_dir = tmp_dir.join("output_crop");

    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&in_dir).expect("create in dir");

    let text_ops = vec![Operation::new("Tj", vec![Object::string_literal("Batch")])];
    let pdf1 = create_multipage_pdf(&[
        (595.0, 842.0, Some(text_ops.clone())),
        (595.0, 842.0, None), // blank
    ]);
    let pdf2 = create_multipage_pdf(&[
        (600.0, 800.0, Some(text_ops.clone())),
        (600.0, 800.0, Some(text_ops.clone())),
    ]);

    fs::write(in_dir.join("doc1.pdf"), &pdf1).expect("write doc1");
    fs::write(in_dir.join("doc2.pdf"), &pdf2).expect("write doc2");

    let batch_opts = BatchOptions::new();

    // 1. Batch remove blank pages
    let remove_opts = RemovePagesOptions::new().remove_blank_pages(true);
    let rep_rem =
        batch_remove_pages_dir_with_options(&in_dir, &out_remove_dir, &remove_opts, &batch_opts)
            .expect("batch remove");
    assert_eq!(rep_rem.successful, 2);

    let doc1_rem = Document::load(out_remove_dir.join("doc1.pdf")).expect("load doc1 rem");
    assert_eq!(doc1_rem.get_pages().len(), 1); // Blank page was removed!

    // 2. Batch crop margins
    let crop_opts = CropOptions::new().margins_uniform(20.0);
    let rep_crop = batch_crop_dir_with_options(&in_dir, &out_crop_dir, &crop_opts, &batch_opts)
        .expect("batch crop");
    assert_eq!(rep_crop.successful, 2);

    let _ = fs::remove_dir_all(&tmp_dir);
}
