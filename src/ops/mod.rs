pub mod extract_images;
pub mod image_metadata;
pub mod merge;
pub mod split;

#[cfg(not(target_arch = "wasm32"))]
pub mod render;

pub use extract_images::{
    ExtractImageOptions, ExtractedImage, extract_images_from_bytes, extract_images_from_doc,
};
pub use image_metadata::{ImageFormatType, get_image_metadata, inject_image_metadata};
pub use merge::{
    MergeOptions, PageMode, merge_documents, merge_documents_with_options, merge_pdf_bytes,
    merge_pdf_bytes_with_options,
};
pub use split::{
    SplitOptions, SplitRange, format_split_filename, parse_split_ranges, split_document,
    split_document_with_options, split_pdf_bytes, split_pdf_bytes_with_options,
};

#[cfg(not(target_arch = "wasm32"))]
pub use render::native::{
    OutputImageFormat, RenderOptions, get_pdfium, render_pdf_page, render_pdf_page_with_options,
    render_pdf_to_images, render_pdf_to_images_with_options,
};
