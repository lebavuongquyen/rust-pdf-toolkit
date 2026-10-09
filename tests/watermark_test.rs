use pdftoolkit_core::ops::{
    ColorRgb, LayerMode, NumberingOptions, NumberingPosition, PageSelection, WatermarkOptions,
    WatermarkPosition, apply_page_numbering, apply_watermark,
};
use std::fs;

#[test]
fn test_text_watermark_diagonal_auto_font_size() {
    let pdf_bytes = fs::read("reference/template.pdf").expect("template.pdf");

    let wm_opts = WatermarkOptions::new()
        .text("BẢN SAO / CONFIDENTIAL")
        .position(WatermarkPosition::Diagonal)
        .opacity(0.18)
        .color(ColorRgb::parse("#808080").unwrap())
        .layer(LayerMode::Over);

    let output = apply_watermark(&pdf_bytes, &wm_opts).expect("apply_watermark");
    assert!(!output.is_empty());
    assert!(output.len() > pdf_bytes.len());

    // Verify output is a valid loadable PDF
    let doc = lopdf::Document::load_mem(&output).expect("Load watermarked PDF");
    assert!(!doc.get_pages().is_empty());
}

#[test]
fn test_text_watermark_placeholders_and_options() {
    let pdf_bytes = fs::read("reference/template.pdf").expect("template.pdf");

    let wm_opts = WatermarkOptions::new()
        .text("DOC {filename} - Page {page}/{total} - {date}")
        .position(WatermarkPosition::Center)
        .rotation(15.0)
        .font("Times-Bold")
        .font_size(32.0)
        .opacity(0.25)
        .color(ColorRgb::parse("red").unwrap())
        .layer(LayerMode::Over)
        .template_var("custom_tag", "AUDITED_V1");

    let output = apply_watermark(&pdf_bytes, &wm_opts).expect("apply_watermark");
    let doc = lopdf::Document::load_mem(&output).expect("Load PDF");

    // Check that pages contain the F_WM font and GS_WM transparency state
    let pages = doc.get_pages();
    for (_num, id) in pages {
        let page_dict = doc.get_object(id).unwrap().as_dict().unwrap();
        let res = page_dict.get(b"Resources").unwrap().as_dict().unwrap();
        assert!(res.has(b"Font"));
        assert!(res.has(b"ExtGState"));
    }
}

#[test]
fn test_tiled_grid_watermark() {
    let pdf_bytes = fs::read("reference/template.pdf").expect("template.pdf");

    let wm_opts = WatermarkOptions::new()
        .text("DRAFT COPY")
        .position(WatermarkPosition::Tiled {
            step_x: 200.0,
            step_y: 200.0,
        })
        .font_size(16.0)
        .opacity(0.10);

    let output = apply_watermark(&pdf_bytes, &wm_opts).expect("apply_watermark tiled");
    let doc = lopdf::Document::load_mem(&output).expect("Load tiled PDF");
    assert!(!doc.get_pages().is_empty());
}

#[test]
fn test_image_watermark_png() {
    let pdf_bytes = fs::read("reference/template.pdf").expect("template.pdf");

    // Create a 64x64 RGBA PNG image in memory
    let img = image::RgbaImage::from_pixel(64, 64, image::Rgba([200, 50, 50, 255]));
    let mut png_buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut png_buf, image::ImageFormat::Png)
        .expect("write png");
    let png_bytes = png_buf.into_inner();

    let wm_opts = WatermarkOptions::new()
        .image(png_bytes)
        .position(WatermarkPosition::BottomRight)
        .opacity(0.35);

    let output = apply_watermark(&pdf_bytes, &wm_opts).expect("apply image watermark");
    let doc = lopdf::Document::load_mem(&output).expect("Load image watermarked PDF");

    // Check that XObject is registered on page
    let pages = doc.get_pages();
    let page_id = *pages.values().next().unwrap();
    let page_dict = doc.get_object(page_id).unwrap().as_dict().unwrap();
    let res = page_dict.get(b"Resources").unwrap().as_dict().unwrap();
    assert!(res.has(b"XObject"));
}

#[test]
fn test_page_numbering_bates_and_custom_margin() {
    let pdf_bytes = fs::read("reference/template.pdf").expect("template.pdf");

    let num_opts = NumberingOptions::new()
        .format("CONFIDENTIAL - BATES #{bates:06d}")
        .position(NumberingPosition::BottomRight)
        .start_page(1)
        .start_number(501)
        .margin(40.0, 20.0)
        .font("Helvetica-Bold")
        .font_size(9.0)
        .color(ColorRgb::parse("#222222").unwrap())
        .opacity(0.9);

    let output = apply_page_numbering(&pdf_bytes, &num_opts).expect("apply_page_numbering");
    let doc = lopdf::Document::load_mem(&output).expect("Load numbered PDF");

    let pages = doc.get_pages();
    for (_num, id) in pages {
        let page_dict = doc.get_object(id).unwrap().as_dict().unwrap();
        let res = page_dict.get(b"Resources").unwrap().as_dict().unwrap();
        assert!(res.has(b"Font"));
        assert!(res.has(b"ExtGState"));
    }
}

#[test]
fn test_page_selection_filtering() {
    let pdf_bytes = fs::read("reference/template.pdf").expect("template.pdf");

    // Target odd pages only
    let wm_opts = WatermarkOptions::new()
        .text("ODD ONLY")
        .pages(PageSelection::Odd);

    let output = apply_watermark(&pdf_bytes, &wm_opts).expect("apply odd watermark");
    assert!(!output.is_empty());
}
