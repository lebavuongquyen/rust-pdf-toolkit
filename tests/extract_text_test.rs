use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, dictionary};
use pdftoolkit_core::batch::{BatchOptions, batch_extract_text_dir_with_options};
use pdftoolkit_core::ops::{
    ColorRgb, PageSelection, TextExtractionOptions, TextGranularity, extract_text,
    extract_text_structured,
};
use std::fs;

/// Helper to create a multi-page test PDF with distinct font styles and text operators.
fn create_styled_pdf(
    pages_spec: &[(f64, f64, &str, f64, ColorRgb, Vec<(&str, f64, f64)>)],
) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let mut page_ids = Vec::new();

    for (w, h, font_name, font_size, color, text_items) in pages_spec {
        // Create font dictionary
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => Object::Name(font_name.as_bytes().to_vec()),
        });

        let mut operations = Vec::new();

        // Color setting: r g b rg
        operations.push(Operation::new(
            "rg",
            vec![color.r.into(), color.g.into(), color.b.into()],
        ));

        // Font setting: /F1 size Tf
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), (*font_size).into()],
        ));

        for (text, x, y) in text_items {
            operations.push(Operation::new("BT", vec![]));
            operations.push(Operation::new(
                "Tm",
                vec![
                    1.0.into(),
                    0.0.into(),
                    0.0.into(),
                    1.0.into(),
                    (*x).into(),
                    (*y).into(),
                ],
            ));
            operations.push(Operation::new(
                "Tj",
                vec![Object::string_literal(text.as_bytes())],
            ));
            operations.push(Operation::new("ET", vec![]));
        }

        let content = Content { operations };
        let content_bytes = content.encode().unwrap_or_default();
        let stream = Stream::new(dictionary! {}, content_bytes);
        let stream_id = doc.add_object(stream);

        let font_res = dictionary! {
            "F1" => Object::Reference(font_id),
        };
        let resources = dictionary! {
            "Font" => font_res,
        };

        let page_dict = dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), (*w).into(), (*h).into()],
            "Resources" => resources,
            "Contents" => Object::Reference(stream_id),
        };

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
    doc.save_to(&mut out).expect("save styled pdf");
    out
}

#[test]
fn test_extract_plain_text() {
    let pdf_bytes = create_styled_pdf(&[
        (
            595.0,
            842.0,
            "Helvetica",
            12.0,
            ColorRgb::BLACK,
            vec![
                ("First header line", 50.0, 750.0),
                ("Paragraph body text", 50.0, 700.0),
            ],
        ),
        (
            595.0,
            842.0,
            "Helvetica",
            12.0,
            ColorRgb::BLACK,
            vec![("Page 2 content line", 50.0, 750.0)],
        ),
    ]);

    let opts = TextExtractionOptions::new();
    let plain_text = extract_text(&pdf_bytes, &opts).expect("extract plain text");

    assert!(plain_text.contains("First header line"));
    assert!(plain_text.contains("Paragraph body text"));
    assert!(plain_text.contains("Page 2 content line"));
    assert!(plain_text.contains("--- Page Break ---"));
}

#[test]
fn test_extract_text_with_bounding_boxes() {
    let pdf_bytes = create_styled_pdf(&[(
        595.0,
        842.0,
        "Helvetica",
        16.0,
        ColorRgb::BLACK,
        vec![("Title Bounding Box Test", 72.0, 600.0)],
    )]);

    let opts = TextExtractionOptions::new().granularity(TextGranularity::Full);
    let report = extract_text_structured(&pdf_bytes, &opts).expect("extract structured");

    assert_eq!(report.extracted_pages, 1);
    let page = &report.pages[0];
    assert_eq!(page.dimensions, [595.0, 842.0]);
    assert_eq!(page.blocks.len(), 1);

    let block = &page.blocks[0];
    let line = &block.lines[0];
    assert_eq!(line.text, "Title Bounding Box Test");

    // Check line bounding box: starts at x=72.0, y=600.0
    assert_eq!(line.bbox[0], 72.0);
    assert_eq!(line.bbox[1], 600.0);
    assert!(line.bbox[2] > 72.0); // urx > llx
    assert_eq!(line.bbox[3], 616.0); // 600.0 + 16.0 font size

    // Check individual words
    let span = &line.spans[0];
    assert_eq!(span.words.len(), 4);
    assert_eq!(span.words[0].text, "Title");
    assert_eq!(span.words[1].text, "Bounding");
    assert_eq!(span.words[2].text, "Box");
    assert_eq!(span.words[3].text, "Test");

    // Word bboxes are progressive along X
    assert!(span.words[0].bbox[0] < span.words[1].bbox[0]);
    assert!(span.words[1].bbox[0] < span.words[2].bbox[0]);
    assert!(span.words[2].bbox[0] < span.words[3].bbox[0]);
}

#[test]
fn test_extract_text_with_typography_styles() {
    let red_color = ColorRgb::new(0.9, 0.1, 0.1);
    let pdf_bytes = create_styled_pdf(&[(
        595.0,
        842.0,
        "Helvetica-Bold",
        18.0,
        red_color,
        vec![("Bold Red Headline", 100.0, 500.0)],
    )]);

    let opts = TextExtractionOptions::new();
    let report = extract_text_structured(&pdf_bytes, &opts).expect("extract structured");

    let span = &report.pages[0].blocks[0].lines[0].spans[0];
    assert_eq!(span.style.font_name, "Helvetica-Bold");
    assert_eq!(span.style.font_size, 18.0);
    assert!(span.style.is_bold);
    assert!(!span.style.is_italic);
    assert_eq!(span.style.color_hex, "#E51A1A");
    assert_eq!(span.style.rotation, 0.0);
}

#[test]
fn test_extract_text_reading_order_sorting() {
    // In PDF stream, write bottom text first (y=200.0), top text second (y=700.0)
    let pdf_bytes = create_styled_pdf(&[(
        595.0,
        842.0,
        "Helvetica",
        12.0,
        ColorRgb::BLACK,
        vec![
            ("Bottom Footer Text", 50.0, 200.0),
            ("Top Header Text", 50.0, 700.0),
        ],
    )]);

    // With sort_reading_order = true (default):
    let opts = TextExtractionOptions::new().sort_reading_order(true);
    let report = extract_text_structured(&pdf_bytes, &opts).expect("sorted");
    let page = &report.pages[0];

    assert_eq!(page.blocks.len(), 2);
    // Block 0 should be top header text, Block 1 should be bottom footer text
    assert_eq!(page.blocks[0].text, "Top Header Text");
    assert_eq!(page.blocks[1].text, "Bottom Footer Text");

    // Without reading order sorting (stream order):
    let opts_raw = TextExtractionOptions::new().sort_reading_order(false);
    let report_raw = extract_text_structured(&pdf_bytes, &opts_raw).expect("unsorted");
    let page_raw = &report_raw.pages[0];
    assert_eq!(page_raw.blocks[0].text, "Bottom Footer Text");
    assert_eq!(page_raw.blocks[1].text, "Top Header Text");
}

#[test]
fn test_extract_text_page_selection() {
    let pdf_bytes = create_styled_pdf(&[
        (
            595.0,
            842.0,
            "Helvetica",
            12.0,
            ColorRgb::BLACK,
            vec![("Page 1", 50.0, 700.0)],
        ),
        (
            595.0,
            842.0,
            "Helvetica",
            12.0,
            ColorRgb::BLACK,
            vec![("Page 2 Target", 50.0, 700.0)],
        ),
        (
            595.0,
            842.0,
            "Helvetica",
            12.0,
            ColorRgb::BLACK,
            vec![("Page 3", 50.0, 700.0)],
        ),
    ]);

    let opts = TextExtractionOptions::new().pages(PageSelection::parse("2"));
    let report = extract_text_structured(&pdf_bytes, &opts).expect("page 2 extract");

    assert_eq!(report.extracted_pages, 1);
    assert_eq!(report.pages[0].page, 2);
    assert!(report.pages[0].text.contains("Page 2 Target"));
}

#[test]
fn test_batch_extract_text() {
    let tmp_dir = std::env::temp_dir().join("pdftoolkit_batch_text_mod5_test");
    let in_dir = tmp_dir.join("input");
    let out_txt_dir = tmp_dir.join("output_txt");
    let out_json_dir = tmp_dir.join("output_json");

    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&in_dir).expect("create in dir");

    let pdf1 = create_styled_pdf(&[(
        595.0,
        842.0,
        "Helvetica",
        12.0,
        ColorRgb::BLACK,
        vec![("Doc 1 Text", 50.0, 700.0)],
    )]);
    let pdf2 = create_styled_pdf(&[(
        595.0,
        842.0,
        "Helvetica",
        14.0,
        ColorRgb::BLACK,
        vec![("Doc 2 Header", 50.0, 750.0)],
    )]);

    fs::write(in_dir.join("file1.pdf"), &pdf1).expect("write file1");
    fs::write(in_dir.join("file2.pdf"), &pdf2).expect("write file2");

    let batch_opts = BatchOptions::new();
    let text_opts = TextExtractionOptions::new();

    // 1. Batch extract plain text (.txt)
    let rep_txt =
        batch_extract_text_dir_with_options(&in_dir, &out_txt_dir, &text_opts, false, &batch_opts)
            .expect("batch txt");
    assert_eq!(rep_txt.successful, 2);
    let txt_content = fs::read_to_string(out_txt_dir.join("file1.txt")).expect("read file1.txt");
    assert!(txt_content.contains("Doc 1 Text"));

    // 2. Batch extract structured JSON (.json)
    let rep_json =
        batch_extract_text_dir_with_options(&in_dir, &out_json_dir, &text_opts, true, &batch_opts)
            .expect("batch json");
    assert_eq!(rep_json.successful, 2);
    let json_content =
        fs::read_to_string(out_json_dir.join("file2.json")).expect("read file2.json");
    assert!(json_content.contains("Doc 2 Header"));
    assert!(json_content.contains("bbox"));

    let _ = fs::remove_dir_all(&tmp_dir);
}
