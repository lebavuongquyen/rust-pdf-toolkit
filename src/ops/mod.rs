pub mod crop;
pub mod extract_images;
pub mod extract_text;
pub mod image_metadata;
pub mod merge;
pub mod page_removal;
pub mod rotate;
pub mod signature_field;
pub mod split;
pub mod watermark;

#[cfg(not(target_arch = "wasm32"))]
pub mod render;
#[cfg(not(target_arch = "wasm32"))]
pub mod verify_signature;

pub use crop::{
    CropMode, CropOptions, PageCropDetail, PageCropReport, TargetBox, crop_pdf_pages,
    crop_pdf_pages_in_doc, detect_page_content_bbox, get_page_mediabox, get_page_target_box,
};
pub use extract_images::{
    ExtractImageOptions, ExtractedImage, extract_images_from_bytes, extract_images_from_doc,
};
pub use extract_text::{
    DocumentTextReport, PageText, TextBlock, TextExtractionOptions, TextGranularity, TextLine,
    TextSpan, TextStyle, TextWord, extract_text, extract_text_from_doc, extract_text_structured,
};
pub use image_metadata::{ImageFormatType, get_image_metadata, inject_image_metadata};
pub use merge::{
    MergeOptions, PageMode, merge_documents, merge_documents_with_options, merge_pdf_bytes,
    merge_pdf_bytes_with_options,
};
pub use page_removal::{
    BlankDetectionOptions, PageRemovalReport, RemovePagesOptions, is_page_blank, remove_pdf_pages,
    remove_pdf_pages_in_doc,
};
pub use rotate::{
    PageRotationDetail, PageRotationReport, RotateOptions, RotationDirection, RotationMode,
    TargetOrientation, detect_page_text_orientation, get_page_box, get_page_rotation,
    normalize_degrees, rotate_pdf_pages, rotate_pdf_pages_in_doc,
};
pub use signature_field::{
    AddSignatureFieldOptions, FieldPresetPosition, RemoveSignatureFieldOptions, SignaturePlacement,
    add_signature_field, add_signature_field_to_doc, remove_signature_field,
    remove_signature_field_from_doc,
};
pub use split::{
    SplitOptions, SplitRange, format_split_filename, parse_split_ranges, split_document,
    split_document_with_options, split_pdf_bytes, split_pdf_bytes_with_options,
};
pub use watermark::{
    ColorRgb, LayerMode, NumberingOptions, NumberingPosition, PageSelection, WatermarkOptions,
    WatermarkPosition, apply_page_numbering, apply_page_numbering_to_doc, apply_watermark,
    apply_watermark_to_doc,
};

#[cfg(not(target_arch = "wasm32"))]
pub use render::native::{
    OutputImageFormat, RenderOptions, get_pdfium, render_pdf_page, render_pdf_page_with_options,
    render_pdf_to_images, render_pdf_to_images_with_options,
};
#[cfg(not(target_arch = "wasm32"))]
pub use verify_signature::{SignatureVerification, verify_pdf_signatures};
