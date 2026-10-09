use pdftoolkit_core::batch::{BatchOptions, batch_merge_dir_with_options, scan_pdf_files};
use pdftoolkit_core::ops::{
    ImageFormatType, MergeOptions, OutputImageFormat, PageMode, RenderOptions, SplitOptions,
    get_image_metadata, inject_image_metadata, merge_pdf_bytes_with_options,
    render_pdf_page_with_options, render_pdf_to_images_with_options, split_pdf_bytes_with_options,
};
use pdftoolkit_core::{get_piece_info, insert_piece_info};
use serde_json::json;
use std::fs;

#[test]
fn test_image_metadata_roundtrip_png_and_jpeg() {
    let test_meta = json!({
        "app": "PDFAuditPro",
        "doc_id": "DOC-99882",
        "verified": true,
        "tags": ["financial", "confidential"]
    });

    // 1. Render a clean PNG from PDF
    let pdf = fs::read("reference/template.pdf").expect("template.pdf");
    let clean_png = render_pdf_page_with_options(
        &pdf,
        1,
        &RenderOptions::new()
            .format(OutputImageFormat::Png)
            .inherit_pdf_piece_info(false),
    )
    .expect("Render clean PNG");

    // Initially no metadata
    assert_eq!(get_image_metadata(&clean_png).unwrap(), None);

    // Inject PieceInfo into PNG
    let tagged_png = inject_image_metadata(&clean_png, &test_meta, ImageFormatType::Png)
        .expect("Inject PNG metadata");
    assert!(!tagged_png.is_empty());

    // Extract PieceInfo from PNG
    let extracted_png_meta = get_image_metadata(&tagged_png)
        .unwrap()
        .expect("Extract PNG metadata");
    assert_eq!(extracted_png_meta, test_meta);

    // 2. Render a clean JPEG
    let clean_jpg = render_pdf_page_with_options(
        &pdf,
        1,
        &RenderOptions::new()
            .format(OutputImageFormat::Jpeg)
            .inherit_pdf_piece_info(false),
    )
    .expect("Render clean JPEG");

    // Inject PieceInfo into JPEG
    let tagged_jpg = inject_image_metadata(&clean_jpg, &test_meta, ImageFormatType::Jpeg)
        .expect("Inject JPEG metadata");

    // Extract PieceInfo from JPEG
    let extracted_jpg_meta = get_image_metadata(&tagged_jpg)
        .unwrap()
        .expect("Extract JPEG metadata");
    assert_eq!(extracted_jpg_meta, test_meta);
}

#[test]
fn test_merge_options_with_piece_info_and_bookmarks() {
    let pdf1 = fs::read("reference/template.pdf").expect("template.pdf");
    let pdf2 = fs::read("reference/template_8field.pdf").expect("template_8field.pdf");

    let merge_meta = json!({
        "pipeline": "contract_assembly",
        "merged_at": "2026-10-09",
        "security_level": "top_secret"
    });

    let opts = MergeOptions::new()
        .create_bookmarks(true)
        .page_mode(PageMode::UseOutlines)
        .flatten(false)
        .piece_info(merge_meta.clone());

    let merged_bytes = merge_pdf_bytes_with_options(&[&pdf1, &pdf2], &opts).expect("Merge failed");

    // Verify PieceInfo was injected into merged document
    let extracted_meta = get_piece_info(&merged_bytes)
        .unwrap()
        .expect("Extracted piece info");
    assert_eq!(extracted_meta, merge_meta);

    let doc = lopdf::Document::load_mem(&merged_bytes).expect("Load doc");
    assert_eq!(doc.get_pages().len(), 2);
}

#[test]
fn test_split_options_with_inherited_piece_info_and_naming() {
    let pdf = fs::read("reference/template.pdf").expect("template.pdf");
    let mut doc = lopdf::Document::load_mem(&pdf).expect("load doc");

    let source_meta = json!({
        "parent_contract": "MASTER-2026",
        "tenant_id": "tenant_abc"
    });
    insert_piece_info(&mut doc, &source_meta).expect("insert piece info");

    let mut doc_with_meta_bytes = Vec::new();
    doc.save_to(&mut doc_with_meta_bytes).expect("save doc");

    // Split with inherit_piece_info = true
    let split_opts = SplitOptions::new()
        .ranges("all")
        .naming_pattern("sub_{stem}_{label}.pdf")
        .inherit_piece_info(true);

    let parts =
        split_pdf_bytes_with_options(&doc_with_meta_bytes, &split_opts).expect("split failed");
    assert_eq!(parts.len(), 1);

    // Verify sub-document inherited PieceInfo from source document
    let part_meta = get_piece_info(&parts[0].1)
        .unwrap()
        .expect("Subdoc should inherit piece info");
    assert_eq!(part_meta, source_meta);
}

#[test]
fn test_render_options_with_inherited_piece_info_into_image() {
    let pdf = fs::read("reference/template.pdf").expect("template.pdf");
    let mut doc = lopdf::Document::load_mem(&pdf).expect("load doc");

    let audit_meta = json!({
        "watermark_id": "WM-7788",
        "author": "Antigravity Engineering"
    });
    insert_piece_info(&mut doc, &audit_meta).expect("insert piece info");

    let mut doc_bytes = Vec::new();
    doc.save_to(&mut doc_bytes).expect("save doc");

    // Render with inherit_pdf_piece_info = true
    let render_opts = RenderOptions::new()
        .dpi(150.0)
        .format(OutputImageFormat::Png)
        .inherit_pdf_piece_info(true);

    let images =
        render_pdf_to_images_with_options(&doc_bytes, &render_opts).expect("render failed");
    assert_eq!(images.len(), 1);

    // Image must have PieceInfo automatically embedded from the PDF!
    let img_meta = get_image_metadata(&images[0].1)
        .unwrap()
        .expect("Image should contain PieceInfo");
    assert_eq!(img_meta, audit_meta);
}

#[test]
fn test_batch_options_dry_run_and_filter() {
    let temp_dir = std::env::temp_dir().join("pdftoolkit_advanced_batch_test");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let input_dir = temp_dir.join("inputs");
    fs::create_dir_all(&input_dir).unwrap();

    fs::copy("reference/template.pdf", input_dir.join("invoice_01.pdf")).unwrap();
    fs::copy("reference/template.pdf", input_dir.join("invoice_02.pdf")).unwrap();
    fs::copy(
        "reference/template_8field.pdf",
        input_dir.join("contract_01.pdf"),
    )
    .unwrap();

    // 1. Filter pattern test
    let filtered = scan_pdf_files(&input_dir, false, Some("invoice_*")).unwrap();
    assert_eq!(filtered.len(), 2);

    let contracts = scan_pdf_files(&input_dir, false, Some("contract_*")).unwrap();
    assert_eq!(contracts.len(), 1);

    // 2. Dry-run test
    let batch_opts = BatchOptions::new()
        .dry_run(true)
        .filter_pattern("invoice_*");
    let merge_opts = MergeOptions::default();
    let dry_out = temp_dir.join("should_not_exist.pdf");

    let rep = batch_merge_dir_with_options(&input_dir, &dry_out, &batch_opts, &merge_opts).unwrap();
    assert_eq!(rep.total_scanned, 2);
    assert_eq!(rep.successful, 2);
    assert!(!dry_out.exists(), "Dry-run should not write to disk");

    // Cleanup
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_extract_images_from_pdf() {
    use pdftoolkit_core::ops::{ExtractImageOptions, extract_images_from_bytes};

    let pdf_bytes = fs::read("tests/fixtures/poc-filled.pdf").expect("read poc-filled.pdf");

    // 1. Extract all images
    let mut opts = ExtractImageOptions::default();
    let images = extract_images_from_bytes(&pdf_bytes, &opts).expect("Extract all images");
    assert!(!images.is_empty(), "Should extract at least 1 image");

    let first = &images[0];
    assert_eq!(first.page, 1);
    assert!(!first.data.is_empty());
    assert!(first.width > 0);
    assert!(first.height > 0);
    assert!(first.format == "jpg" || first.format == "png");

    // 2. Filter by minimum width/height
    opts.min_width = 500;
    let filtered = extract_images_from_bytes(&pdf_bytes, &opts).expect("Extract filtered images");
    for img in &filtered {
        assert!(img.width >= 500);
    }

    // 3. Page filter
    opts.min_width = 0;
    opts.pages = Some(vec![1]);
    let page1_images = extract_images_from_bytes(&pdf_bytes, &opts).expect("Extract page 1 images");
    assert!(!page1_images.is_empty());
}
