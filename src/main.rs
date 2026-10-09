use clap::{Args, Parser, Subcommand, ValueEnum};
use pdftoolkit_core::{
    CertificateSigner, EcdsaSigner, FillOptions, PdfSigner, Signer,
    batch::{
        BatchOptions, batch_crop_dir_with_options, batch_extract_images_dir_with_options,
        batch_extract_text_dir_with_options, batch_fill_records_with_options,
        batch_merge_dir_with_options, batch_number_dir_with_options,
        batch_remove_pages_dir_with_options, batch_rotate_dir_with_options,
        batch_split_dir_with_options, batch_to_image_dir_with_options,
        batch_watermark_dir_with_options,
    },
    fill_pdf_with_options, form_fields_json,
    ops::{
        AddSignatureFieldOptions, ColorRgb, CropOptions, ExtractImageOptions, FieldPresetPosition,
        LayerMode, MergeOptions, NumberingOptions, NumberingPosition, OutputImageFormat, PageMode,
        PageSelection, RemovePagesOptions, RemoveSignatureFieldOptions, RenderOptions,
        RotateOptions, RotationDirection, RotationMode, SignaturePlacement, SplitOptions,
        TargetBox, TargetOrientation, TextExtractionOptions, TextGranularity, WatermarkOptions,
        WatermarkPosition, add_signature_field, crop_pdf_pages, extract_images_from_bytes,
        extract_text, extract_text_structured, get_image_metadata, merge_pdf_bytes_with_options,
        parse_split_ranges, remove_pdf_pages, remove_signature_field,
        render_pdf_to_images_with_options, rotate_pdf_pages, split_pdf_bytes_with_options,
        verify_pdf_signatures,
    },
    report_json,
};
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "pdftoolkit",
    author = "rust-pdf-toolkit contributors",
    version = "0.2.0",
    about = "High-performance all-in-one PDF toolkit: Fill, Merge, Split, Convert to Image, Watermark, Page Numbering, and Batch Folder Processing",
    subcommand_required = false,
    arg_required_else_help = false
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Positional args for backward-compatible fallback: [template.pdf] [data.json] [output.pdf] [piece_info.json]
    #[arg(trailing_var_arg = true)]
    raw_args: Vec<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Fill PDF form fields from JSON data
    Fill(FillArgs),

    /// Merge multiple PDF files into a single PDF
    Merge(MergeArgs),

    /// Split a PDF into multiple parts or individual pages
    Split(SplitArgs),

    /// Convert PDF pages to images (PNG or JPEG) with optional metadata injection
    #[command(name = "to-image")]
    ToImage(ToImageArgs),

    /// Extract embedded images (photos, logos, signatures) directly from PDF pages
    #[command(name = "extract-images")]
    ExtractImages(ExtractImagesArgs),

    /// Apply customizable text, image, or template watermark to PDF pages
    Watermark(WatermarkArgs),

    /// Apply Bates / Page numbering header and footer to PDF pages
    Number(NumberArgs),

    /// Rotate PDF pages by angle, normalize orientation (portrait/landscape), or auto-detect text direction
    Rotate(RotateArgs),

    /// Add a new empty signature field (invisible or custom position) to a PDF
    #[command(name = "add-sig-field")]
    AddSigField(AddSigFieldArgs),

    /// Remove signature fields from a PDF
    #[command(name = "remove-sig-field")]
    RemoveSigField(RemoveSigFieldArgs),

    /// Digitally sign a PDF using RSA or ECDSA certificate
    Sign(SignArgs),

    /// Verify digital signatures and cryptographic integrity of a PDF
    Verify(VerifyArgs),

    /// Remove pages (cover, back cover, blank pages, or specific ranges)
    #[command(name = "remove-pages")]
    RemovePages(RemovePagesArgs),

    /// Crop PDF pages by margins, explicit box, or auto-detected content bounding box
    Crop(CropArgs),

    /// Extract text with precise bounding boxes, typography styles, and hierarchy
    #[command(name = "extract-text")]
    ExtractText(ExtractTextArgs),

    /// Batch process entire folders (Fill, Merge, Split, Convert, Extract, Watermark, Number, RemovePages, Crop, ExtractText)
    Batch(BatchArgs),

    /// Inspect form fields and metadata of a PDF
    Inspect(InspectArgs),

    /// Inspect embedded PieceInfo metadata from a PNG or JPEG image
    #[command(name = "inspect-image")]
    InspectImage(InspectImageArgs),
}

#[derive(Args, Debug)]
struct FillArgs {
    /// Template PDF file path
    #[arg(short, long)]
    template: PathBuf,

    /// Form data JSON file path
    #[arg(short, long)]
    data: PathBuf,

    /// Output PDF file path
    #[arg(short, long)]
    output: PathBuf,

    /// Optional piece_info JSON file path
    #[arg(short, long)]
    piece_info: Option<PathBuf>,

    /// Flatten form fields after filling
    #[arg(long, default_value_t = false)]
    flatten: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
enum PageModeArg {
    #[default]
    Outlines,
    Thumbs,
    Fullscreen,
    None,
}

impl From<PageModeArg> for PageMode {
    fn from(arg: PageModeArg) -> Self {
        match arg {
            PageModeArg::Outlines => PageMode::UseOutlines,
            PageModeArg::Thumbs => PageMode::UseThumbs,
            PageModeArg::Fullscreen => PageMode::FullScreen,
            PageModeArg::None => PageMode::UseNone,
        }
    }
}

#[derive(Args, Debug)]
struct MergeArgs {
    /// List of input PDF files (in merge order)
    #[arg(short, long, num_args = 1..)]
    inputs: Option<Vec<PathBuf>>,

    /// Alternatively, directory containing PDF files to merge
    #[arg(long)]
    dir: Option<PathBuf>,

    /// Output merged PDF file path
    #[arg(short, long)]
    output: PathBuf,

    /// Scan directory recursively if --dir is used
    #[arg(short, long, default_value_t = false)]
    recursive: bool,

    /// Create Bookmarks / Table of Contents in the merged PDF
    #[arg(long, default_value_t = true)]
    bookmarks: bool,

    /// Initial PDF page mode when opened in viewer
    #[arg(long, value_enum, default_value_t = PageModeArg::Outlines)]
    page_mode: PageModeArg,

    /// Flatten form fields before merging
    #[arg(long, default_value_t = false)]
    flatten: bool,

    /// Optional PieceInfo JSON file to inject into the merged document
    #[arg(short, long)]
    piece_info: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct SplitArgs {
    /// Input PDF file path
    #[arg(short, long)]
    input: PathBuf,

    /// Output directory for split files
    #[arg(short, long)]
    output_dir: PathBuf,

    /// Split range specification ("all", "1-3,4-5", "1,3,5")
    #[arg(short, long, default_value = "all")]
    range: String,

    /// Naming pattern for output files ({stem}, {label}, {index:03}, {page})
    #[arg(long, default_value = "{stem}_{label}.pdf")]
    naming_pattern: String,

    /// Inherit PieceInfo metadata from the source PDF
    #[arg(long, default_value_t = true)]
    inherit_piece_info: bool,

    /// Override PieceInfo JSON file to inject into each split part
    #[arg(long)]
    piece_info: Option<PathBuf>,

    /// Flatten form fields before splitting
    #[arg(long, default_value_t = false)]
    flatten: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
enum ImageFormatArg {
    #[default]
    Png,
    Jpg,
    Jpeg,
}

impl From<ImageFormatArg> for OutputImageFormat {
    fn from(arg: ImageFormatArg) -> Self {
        match arg {
            ImageFormatArg::Png => OutputImageFormat::Png,
            ImageFormatArg::Jpg | ImageFormatArg::Jpeg => OutputImageFormat::Jpeg,
        }
    }
}

#[derive(Args, Debug)]
struct ToImageArgs {
    /// Input PDF file path
    #[arg(short, long)]
    input: PathBuf,

    /// Output directory for generated images
    #[arg(short, long)]
    output_dir: PathBuf,

    /// Rendering resolution DPI (e.g. 72, 150, 300)
    #[arg(long, default_value_t = 150.0)]
    dpi: f32,

    /// Image format: png or jpg
    #[arg(short, long, value_enum, default_value_t = ImageFormatArg::Png)]
    format: ImageFormatArg,

    /// JPEG compression quality (1-100)
    #[arg(long, default_value_t = 85)]
    quality: u8,

    /// Optional page filter (e.g. "1, 3-5")
    #[arg(long)]
    pages: Option<String>,

    /// Maximum image width constraint in pixels
    #[arg(long)]
    max_width: Option<u32>,

    /// Maximum image height constraint in pixels
    #[arg(long)]
    max_height: Option<u32>,

    /// Transparent background for PNG images
    #[arg(long, default_value_t = false)]
    transparent: bool,

    /// Disable rendering of annotations and signatures
    #[arg(long, default_value_t = false)]
    no_annotations: bool,

    /// Custom PieceInfo JSON file to inject into the image
    #[arg(long)]
    piece_info: Option<PathBuf>,

    /// Disable inheriting PieceInfo from source PDF
    #[arg(long, default_value_t = false)]
    no_inherit_piece_info: bool,
}

#[derive(Args, Debug)]
struct ExtractImagesArgs {
    /// Input PDF file path
    #[arg(short, long)]
    input: PathBuf,

    /// Output directory for extracted images
    #[arg(short, long)]
    output_dir: PathBuf,

    /// Optional page filter (e.g. "1, 3-5")
    #[arg(long)]
    pages: Option<String>,

    /// Minimum image width filter in pixels
    #[arg(long, default_value_t = 0)]
    min_width: u32,

    /// Minimum image height filter in pixels
    #[arg(long, default_value_t = 0)]
    min_height: u32,

    /// Custom naming pattern (e.g. "img_p{page}_{index}.{ext}")
    #[arg(long)]
    naming_pattern: Option<String>,

    /// Allow duplicate images (default deduplicates by object id)
    #[arg(long, default_value_t = false)]
    keep_duplicates: bool,
}

#[derive(Args, Debug)]
struct WatermarkArgs {
    /// Input PDF file path
    #[arg(short, long)]
    input: PathBuf,

    /// Output PDF file path
    #[arg(short, long)]
    output: PathBuf,

    /// Watermark text string. Supports placeholders: {page}, {total}, {date}, {time}, {filename}
    #[arg(short, long)]
    text: Option<String>,

    /// Image watermark file path (PNG or JPEG)
    #[arg(long)]
    image: Option<PathBuf>,

    /// Target pages: "all", "odd", "even", "first", "last", "1,3-5"
    #[arg(short, long, default_value = "all")]
    pages: String,

    /// Watermark position: "diagonal", "center", "top-left", "bottom-right", "tiled", "x,y"
    #[arg(long, default_value = "diagonal")]
    position: String,

    /// Custom rotation angle in degrees (default: auto diagonal angle for diagonal, 0 for center)
    #[arg(long)]
    rotation: Option<f64>,

    /// Opacity in range [0.0, 1.0]. Default: 0.15 (15%)
    #[arg(long, default_value_t = 0.15)]
    opacity: f64,

    /// Standard PDF BaseFont: "Helvetica-Bold", "Helvetica", "Times-Bold", "Courier-Bold", etc.
    #[arg(long, default_value = "Helvetica-Bold")]
    font: String,

    /// Font size in points (default: auto dynamically scaled to fit page diagonal)
    #[arg(long)]
    font_size: Option<f64>,

    /// Text color: hex "#RRGGBB" or name ("gray", "red", "black", "blue")
    #[arg(long, default_value = "gray")]
    color: String,

    /// Layer: "over" (on top of page contents) or "under" (behind page contents)
    #[arg(long, default_value = "over")]
    layer: String,

    /// Load watermark options from a JSON template file
    #[arg(long)]
    template_json: Option<PathBuf>,

    /// Flatten form fields
    #[arg(long, default_value_t = false)]
    flatten: bool,

    /// Optional piece_info JSON file path
    #[arg(long)]
    piece_info: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct NumberArgs {
    /// Input PDF file path
    #[arg(short, long)]
    input: PathBuf,

    /// Output PDF file path
    #[arg(short, long)]
    output: PathBuf,

    /// Format template: "Trang {page} / {total}", "Page {page} of {total}", "Bates #{bates:06d}"
    #[arg(short, long, default_value = "Trang {page} / {total}")]
    format: String,

    /// Position: "bottom-center", "bottom-right", "bottom-left", "top-center", "top-right", "top-left"
    #[arg(long, default_value = "bottom-center")]
    position: String,

    /// Target pages: "all", "odd", "even", "2-end"
    #[arg(short, long, default_value = "all")]
    pages: String,

    /// Physical page to start numbering from (e.g. 2 to skip cover). Default: 1
    #[arg(long, default_value_t = 1)]
    start_page: u32,

    /// Starting sequence number. Default: 1
    #[arg(long, default_value_t = 1)]
    start_number: u32,

    /// Margin X from page edge in points. Default: 36.0 (0.5 inch)
    #[arg(long, default_value_t = 36.0)]
    margin_x: f64,

    /// Margin Y from page edge in points. Default: 24.0
    #[arg(long, default_value_t = 24.0)]
    margin_y: f64,

    /// Standard PDF BaseFont: "Helvetica", "Times-Roman", "Courier", etc.
    #[arg(long, default_value = "Helvetica")]
    font: String,

    /// Font size in points. Default: 10.0
    #[arg(long, default_value_t = 10.0)]
    font_size: f64,

    /// Text color: hex "#RRGGBB" or name ("black", "gray")
    #[arg(long, default_value = "#404040")]
    color: String,

    /// Opacity in range [0.0, 1.0]. Default: 1.0
    #[arg(long, default_value_t = 1.0)]
    opacity: f64,

    /// Layer: "over" or "under"
    #[arg(long, default_value = "over")]
    layer: String,

    /// Flatten form fields
    #[arg(long, default_value_t = false)]
    flatten: bool,

    /// Optional piece_info JSON file path
    #[arg(long)]
    piece_info: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct AddSigFieldArgs {
    /// Input PDF file path
    #[arg(short, long)]
    input: PathBuf,

    /// Output PDF file path
    #[arg(short, long)]
    output: PathBuf,

    /// Signature field name (default: "Signature1")
    #[arg(short, long, default_value = "Signature1")]
    name: String,

    /// 1-based target page (0 or omitted = last page)
    #[arg(short, long)]
    page: Option<u32>,

    /// Create invisible cryptographic signature field (Rect [0, 0, 0, 0])
    #[arg(long, default_value_t = false)]
    invisible: bool,

    /// Explicit rectangle coordinates in points: "llx,lly,urx,ury"
    #[arg(long)]
    rect: Option<String>,

    /// Visual preset position: "bottom-right", "bottom-left", "bottom-center", "top-right", "top-left", "center"
    #[arg(long)]
    position: Option<String>,

    /// Preset field width in points (default: 150.0)
    #[arg(long, default_value_t = 150.0)]
    width: f64,

    /// Preset field height in points (default: 50.0)
    #[arg(long, default_value_t = 50.0)]
    height: f64,

    /// Preset margin X in points (default: 36.0)
    #[arg(long, default_value_t = 36.0)]
    margin_x: f64,

    /// Preset margin Y in points (default: 36.0)
    #[arg(long, default_value_t = 36.0)]
    margin_y: f64,
}

#[derive(Args, Debug)]
struct RemoveSigFieldArgs {
    /// Input PDF file path
    #[arg(short, long)]
    input: PathBuf,

    /// Output PDF file path
    #[arg(short, long)]
    output: PathBuf,

    /// Target signature field name to remove
    #[arg(short, long)]
    name: Option<String>,

    /// Remove all unsigned signature fields
    #[arg(long, default_value_t = false)]
    all_unsigned: bool,

    /// Remove all signature fields
    #[arg(long, default_value_t = false)]
    all: bool,
}

#[derive(Args, Debug)]
struct SignArgs {
    /// Input PDF file path
    #[arg(short, long)]
    input: PathBuf,

    /// Output PDF file path
    #[arg(short, long)]
    output: PathBuf,

    /// Signature field name to sign
    #[arg(short, long, default_value = "Signature1")]
    field: String,

    /// Certificate file path (PKCS#12 .p12/.pfx, or DER/PEM certificate)
    #[arg(short, long)]
    cert: PathBuf,

    /// Private key file path (if separate PKCS#8 DER/PEM file)
    #[arg(short, long)]
    key: Option<PathBuf>,

    /// Password for PKCS#12 (.p12/.pfx) certificate
    #[arg(long)]
    password: Option<String>,

    /// Signing reason (e.g. "I approve this document")
    #[arg(long)]
    reason: Option<String>,

    /// Signing location (e.g. "Hanoi, Vietnam")
    #[arg(long)]
    location: Option<String>,

    /// Signer contact information
    #[arg(long)]
    contact: Option<String>,

    /// Automatically create the signature field if it does not already exist
    #[arg(long, default_value_t = false)]
    auto_create_field: bool,

    /// 1-based target page for auto-created field (0 or omitted = last page)
    #[arg(short, long)]
    page: Option<u32>,

    /// Create invisible cryptographic signature (Rect [0, 0, 0, 0])
    #[arg(long, default_value_t = false)]
    invisible: bool,

    /// Explicit rectangle coordinates in points: "llx,lly,urx,ury"
    #[arg(long)]
    rect: Option<String>,

    /// Visual preset position: "bottom-right", "bottom-left", "bottom-center", "top-right", "top-left", "center"
    #[arg(long)]
    position: Option<String>,

    /// Visual signature image or stamp file path (PNG/JPEG) or Base64 data URI
    #[arg(long)]
    image: Option<String>,

    /// Visual graphic layout inside signature box: "left", "right", "behind", "image-only", "text-only"
    #[arg(long, default_value = "left")]
    graphic_position: String,

    /// Flatten form fields before signing
    #[arg(long, default_value_t = false)]
    flatten: bool,
}

#[derive(Args, Debug)]
struct VerifyArgs {
    /// Input PDF file path to verify
    #[arg(short, long)]
    input: PathBuf,

    /// Output results in JSON format
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Args, Debug)]
struct RotateArgs {
    /// Input PDF file path
    #[arg(short, long)]
    input: PathBuf,

    /// Output PDF file path
    #[arg(short, long)]
    output: PathBuf,

    /// Rotation angle in degrees: 90, 180, 270, -90 (default: 90 if no orientation/auto mode)
    #[arg(short, long)]
    angle: Option<i32>,

    /// Target orientation to normalize pages: "portrait" or "landscape"
    #[arg(long)]
    orientation: Option<String>,

    /// Direction when normalizing orientation: "cw" (clockwise) or "ccw" (counter-clockwise)
    #[arg(long, default_value = "cw")]
    direction: String,

    /// Smart text orientation detection: inspect content stream matrices to make text upright
    #[arg(long, default_value_t = false)]
    auto_text: bool,

    /// Target pages: "all", "odd", "even", "first", "last", "1-3, 5"
    #[arg(short, long, default_value = "all")]
    pages: String,

    /// Treat angle as absolute (set exactly) rather than relative (add to current rotation)
    #[arg(long, default_value_t = false)]
    absolute: bool,

    /// Fallback angle if auto-text mode detects no text on the page
    #[arg(long)]
    fallback_angle: Option<i32>,

    /// Flatten form fields
    #[arg(long, default_value_t = false)]
    flatten: bool,

    /// Optional piece_info JSON file path
    #[arg(long)]
    piece_info: Option<PathBuf>,

    /// Output report in JSON format
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Args, Debug)]
struct RemovePagesArgs {
    /// Input PDF file path
    #[arg(short, long)]
    input: PathBuf,

    /// Output PDF file path
    #[arg(short, long)]
    output: PathBuf,

    /// Remove first page (cover)
    #[arg(long, default_value_t = false)]
    cover: bool,

    /// Remove last page (back cover)
    #[arg(long, default_value_t = false)]
    back_cover: bool,

    /// Remove detected blank/empty pages
    #[arg(long, default_value_t = false)]
    blank: bool,

    /// Explicit pages or range to remove: "2", "3-5", "odd", "even"
    #[arg(short, long)]
    pages: Option<String>,

    /// Explicit pages or range to keep / protect from removal
    #[arg(long)]
    keep_pages: Option<String>,

    /// Minimum pages that must remain in document (safety default: 1)
    #[arg(long, default_value_t = 1)]
    min_pages: usize,

    /// Flatten form fields
    #[arg(long, default_value_t = false)]
    flatten: bool,

    /// Optional piece_info JSON file path
    #[arg(long)]
    piece_info: Option<PathBuf>,

    /// Output report in JSON format
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Args, Debug)]
struct CropArgs {
    /// Input PDF file path
    #[arg(short, long)]
    input: PathBuf,

    /// Output PDF file path
    #[arg(short, long)]
    output: PathBuf,

    /// Margins in points: "top,bottom,left,right" or uniform "36"
    #[arg(long)]
    margins: Option<String>,

    /// Relative margin fractions (0.0 to 1.0): "top,bottom,left,right"
    #[arg(long)]
    margins_relative: Option<String>,

    /// Explicit bounding box in points: "llx,lly,urx,ury"
    #[arg(long)]
    r#box: Option<String>,

    /// Automatically crop around content bounding box (text, paths, images)
    #[arg(long, default_value_t = false)]
    auto_content: bool,

    /// Padding in points when using --auto-content (default: 18.0)
    #[arg(long, default_value_t = 18.0)]
    padding: f64,

    /// Target PDF box to modify: "crop", "media", "trim", "bleed", "all"
    #[arg(long, default_value = "crop")]
    target_box: String,

    /// Target pages: "all", "odd", "even", "first", "last", "1-3, 5"
    #[arg(short, long, default_value = "all")]
    pages: String,

    /// Disable clamping resulting box to MediaBox bounds
    #[arg(long, default_value_t = false)]
    no_clamp: bool,

    /// Flatten form fields
    #[arg(long, default_value_t = false)]
    flatten: bool,

    /// Optional piece_info JSON file path
    #[arg(long)]
    piece_info: Option<PathBuf>,

    /// Output report in JSON format
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Args, Debug)]
struct ExtractTextArgs {
    /// Input PDF file path
    #[arg(short, long)]
    input: PathBuf,

    /// Output text or JSON file path (if omitted, prints to stdout)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Target pages to extract: "all", "1-3", "odd", "even", "2"
    #[arg(short, long, default_value = "all")]
    pages: String,

    /// Output granularity: "full", "blocks", "lines", "spans", "words"
    #[arg(long, default_value = "full")]
    granularity: String,

    /// Output structured JSON with bounding boxes and typography styles
    #[arg(long, default_value_t = false)]
    json: bool,

    /// Disable natural reading order sorting (top-to-bottom, left-to-right)
    #[arg(long, default_value_t = false)]
    no_reading_order: bool,

    /// Vertical tolerance in points to group text spans into the same line (default: 3.5)
    #[arg(long, default_value_t = 3.5)]
    line_tolerance: f64,

    /// Word gap factor relative to font size (default: 0.25)
    #[arg(long, default_value_t = 0.25)]
    word_gap: f64,

    /// Include invisible text (e.g. OCR hidden text layer)
    #[arg(long, default_value_t = false)]
    include_invisible: bool,

    /// Disable whitespace normalization
    #[arg(long, default_value_t = false)]
    no_normalize_whitespace: bool,
}

#[derive(Args, Debug)]
struct BatchArgs {
    #[command(subcommand)]
    sub: BatchSubcommands,
}

#[derive(Subcommand, Debug)]
enum BatchSubcommands {
    /// Batch fill a template PDF with a JSON array of records
    Fill {
        #[arg(short, long)]
        template: PathBuf,
        #[arg(short, long)]
        data: PathBuf,
        #[arg(short, long)]
        output_dir: PathBuf,
        #[arg(short, long)]
        piece_info: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        flatten: bool,
        #[arg(long)]
        threads: Option<usize>,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Batch merge all PDFs in a folder into a single PDF
    Merge {
        #[arg(short, long)]
        dir: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(short, long, default_value_t = false)]
        recursive: bool,
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value_t = true)]
        bookmarks: bool,
        #[arg(long, value_enum, default_value_t = PageModeArg::Outlines)]
        page_mode: PageModeArg,
        #[arg(long, default_value_t = false)]
        flatten: bool,
        #[arg(long)]
        piece_info: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Batch split all PDFs in a folder
    Split {
        #[arg(short, long)]
        dir: PathBuf,
        #[arg(short, long)]
        output_dir: PathBuf,
        #[arg(short, long, default_value = "all")]
        range: String,
        #[arg(long, default_value = "{stem}_{label}.pdf")]
        naming_pattern: String,
        #[arg(short, long, default_value_t = false)]
        recursive: bool,
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value_t = true)]
        inherit_piece_info: bool,
        #[arg(long)]
        piece_info: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Batch convert all PDFs in a folder to images
    ToImage {
        #[arg(short, long)]
        dir: PathBuf,
        #[arg(short, long)]
        output_dir: PathBuf,
        #[arg(long, default_value_t = 150.0)]
        dpi: f32,
        #[arg(short, long, value_enum, default_value_t = ImageFormatArg::Png)]
        format: ImageFormatArg,
        #[arg(long, default_value_t = 85)]
        quality: u8,
        #[arg(long)]
        pages: Option<String>,
        #[arg(long)]
        max_width: Option<u32>,
        #[arg(long)]
        max_height: Option<u32>,
        #[arg(long, default_value_t = false)]
        transparent: bool,
        #[arg(short, long, default_value_t = false)]
        recursive: bool,
        #[arg(long)]
        filter: Option<String>,
        #[arg(long)]
        piece_info: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Batch extract embedded images from all PDFs in a folder
    #[command(name = "extract-images")]
    ExtractImages {
        #[arg(short, long)]
        dir: PathBuf,
        #[arg(short, long)]
        output_dir: PathBuf,
        #[arg(short, long, default_value_t = false)]
        recursive: bool,
        #[arg(long)]
        filter: Option<String>,
        #[arg(long)]
        pages: Option<String>,
        #[arg(long, default_value_t = 0)]
        min_width: u32,
        #[arg(long, default_value_t = 0)]
        min_height: u32,
        #[arg(long, default_value_t = false)]
        keep_duplicates: bool,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Batch apply watermark across all PDFs in a folder
    Watermark {
        #[arg(short, long)]
        dir: PathBuf,
        #[arg(short, long)]
        output_dir: PathBuf,
        #[arg(short, long)]
        text: Option<String>,
        #[arg(long)]
        image: Option<PathBuf>,
        #[arg(short, long, default_value = "all")]
        pages: String,
        #[arg(long, default_value = "diagonal")]
        position: String,
        #[arg(long)]
        rotation: Option<f64>,
        #[arg(long, default_value_t = 0.15)]
        opacity: f64,
        #[arg(long, default_value = "Helvetica-Bold")]
        font: String,
        #[arg(long)]
        font_size: Option<f64>,
        #[arg(long, default_value = "gray")]
        color: String,
        #[arg(long, default_value = "over")]
        layer: String,
        #[arg(short, long, default_value_t = false)]
        recursive: bool,
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value_t = false)]
        flatten: bool,
        #[arg(long)]
        piece_info: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Batch apply page numbering across all PDFs in a folder
    Number {
        #[arg(short, long)]
        dir: PathBuf,
        #[arg(short, long)]
        output_dir: PathBuf,
        #[arg(short, long, default_value = "Trang {page} / {total}")]
        format: String,
        #[arg(long, default_value = "bottom-center")]
        position: String,
        #[arg(short, long, default_value = "all")]
        pages: String,
        #[arg(long, default_value_t = 1)]
        start_page: u32,
        #[arg(long, default_value_t = 1)]
        start_number: u32,
        #[arg(long, default_value_t = 36.0)]
        margin_x: f64,
        #[arg(long, default_value_t = 24.0)]
        margin_y: f64,
        #[arg(long, default_value = "Helvetica")]
        font: String,
        #[arg(long, default_value_t = 10.0)]
        font_size: f64,
        #[arg(long, default_value = "#404040")]
        color: String,
        #[arg(long, default_value_t = 1.0)]
        opacity: f64,
        #[arg(short, long, default_value_t = false)]
        recursive: bool,
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value_t = false)]
        flatten: bool,
        #[arg(long)]
        piece_info: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Batch rotate PDF pages across all PDFs in a folder
    Rotate {
        #[arg(short, long)]
        dir: PathBuf,
        #[arg(short, long)]
        output_dir: PathBuf,
        /// Rotation angle in degrees (e.g. 90, 180, 270, -90)
        #[arg(short, long)]
        angle: Option<i32>,
        /// Target orientation: "portrait" or "landscape"
        #[arg(long)]
        orientation: Option<String>,
        /// Direction for orientation normalization: "cw" or "ccw"
        #[arg(long, default_value = "cw")]
        direction: String,
        /// Auto-detect text orientation via content stream transformation matrices
        #[arg(long, default_value_t = false)]
        auto_text: bool,
        /// Target pages: "all", "odd", "even", "first", "last", "1,3-5"
        #[arg(short, long, default_value = "all")]
        pages: String,
        /// Set absolute rotation instead of relative
        #[arg(long, default_value_t = false)]
        absolute: bool,
        /// Fallback angle for auto-text mode if page has no text
        #[arg(long)]
        fallback_angle: Option<i32>,
        #[arg(short, long, default_value_t = false)]
        recursive: bool,
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value_t = false)]
        flatten: bool,
        #[arg(long)]
        piece_info: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Batch remove pages across all PDFs in a folder
    #[command(name = "remove-pages")]
    RemovePages {
        #[arg(short, long)]
        dir: PathBuf,
        #[arg(short, long)]
        output_dir: PathBuf,
        /// Remove first page (cover)
        #[arg(long, default_value_t = false)]
        cover: bool,
        /// Remove last page (back cover)
        #[arg(long, default_value_t = false)]
        back_cover: bool,
        /// Remove detected blank/empty pages
        #[arg(long, default_value_t = false)]
        blank: bool,
        /// Explicit pages or range to remove: "2", "3-5", "odd", "even"
        #[arg(short, long)]
        pages: Option<String>,
        /// Explicit pages or range to keep / protect from removal
        #[arg(long)]
        keep_pages: Option<String>,
        /// Minimum pages that must remain in document (default: 1)
        #[arg(long, default_value_t = 1)]
        min_pages: usize,
        #[arg(short, long, default_value_t = false)]
        recursive: bool,
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value_t = false)]
        flatten: bool,
        #[arg(long)]
        piece_info: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Batch crop pages across all PDFs in a folder
    Crop {
        #[arg(short, long)]
        dir: PathBuf,
        #[arg(short, long)]
        output_dir: PathBuf,
        /// Margins in points: "top,bottom,left,right" or uniform "36"
        #[arg(long)]
        margins: Option<String>,
        /// Relative margin fractions (0.0 to 1.0): "top,bottom,left,right"
        #[arg(long)]
        margins_relative: Option<String>,
        /// Explicit bounding box in points: "llx,lly,urx,ury"
        #[arg(long)]
        r#box: Option<String>,
        /// Automatically crop around content bounding box
        #[arg(long, default_value_t = false)]
        auto_content: bool,
        /// Padding in points when using --auto-content (default: 18.0)
        #[arg(long, default_value_t = 18.0)]
        padding: f64,
        /// Target PDF box to modify: "crop", "media", "trim", "bleed", "all"
        #[arg(long, default_value = "crop")]
        target_box: String,
        /// Target pages: "all", "odd", "even", "1-3"
        #[arg(short, long, default_value = "all")]
        pages: String,
        /// Disable clamping resulting box to MediaBox bounds
        #[arg(long, default_value_t = false)]
        no_clamp: bool,
        #[arg(short, long, default_value_t = false)]
        recursive: bool,
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value_t = false)]
        flatten: bool,
        #[arg(long)]
        piece_info: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    /// Batch extract text across all PDFs in a folder
    #[command(name = "extract-text")]
    ExtractText {
        #[arg(short, long)]
        dir: PathBuf,
        #[arg(short, long)]
        output_dir: PathBuf,
        /// Target pages to extract: "all", "1-3", "odd", "even", "2"
        #[arg(short, long, default_value = "all")]
        pages: String,
        /// Output granularity: "full", "blocks", "lines", "spans", "words"
        #[arg(long, default_value = "full")]
        granularity: String,
        /// Output structured JSON with bounding boxes instead of plain text (.txt)
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Disable natural reading order sorting
        #[arg(long, default_value_t = false)]
        no_reading_order: bool,
        /// Vertical tolerance in points to group text spans into the same line (default: 3.5)
        #[arg(long, default_value_t = 3.5)]
        line_tolerance: f64,
        /// Word gap factor relative to font size (default: 0.25)
        #[arg(long, default_value_t = 0.25)]
        word_gap: f64,
        /// Include invisible text
        #[arg(long, default_value_t = false)]
        include_invisible: bool,
        /// Disable whitespace normalization
        #[arg(long, default_value_t = false)]
        no_normalize_whitespace: bool,
        #[arg(short, long, default_value_t = false)]
        recursive: bool,
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
}

#[derive(Args, Debug)]
struct InspectArgs {
    /// PDF file path to inspect
    #[arg(short, long)]
    input: PathBuf,
}

#[derive(Args, Debug)]
struct InspectImageArgs {
    /// Image file path (PNG or JPEG) to inspect embedded PieceInfo
    #[arg(short, long)]
    input: PathBuf,
}

fn decode_der_or_pem(data: &[u8]) -> Result<Vec<u8>, String> {
    if let Ok(text) = std::str::from_utf8(data)
        && text.contains("-----BEGIN")
    {
        let mut b64 = String::new();
        let mut in_block = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("-----BEGIN") {
                in_block = true;
                b64.clear();
            } else if trimmed.starts_with("-----END") {
                break;
            } else if in_block {
                b64.push_str(trimmed);
            }
        }
        if !b64.is_empty() {
            use base64::Engine;
            return base64::engine::general_purpose::STANDARD
                .decode(&b64)
                .map_err(|e| format!("Base64 decode failed for PEM: {e}"));
        }
    }
    Ok(data.to_vec())
}

fn extract_private_key_pem(data: &[u8]) -> Option<Vec<u8>> {
    if let Ok(text) = std::str::from_utf8(data) {
        let mut b64 = String::new();
        let mut in_block = false;
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("-----BEGIN") && trimmed.contains("PRIVATE KEY") {
                in_block = true;
                b64.clear();
            } else if trimmed.starts_with("-----END") && trimmed.contains("PRIVATE KEY") {
                use base64::Engine;
                if let Ok(der) = base64::engine::general_purpose::STANDARD.decode(&b64) {
                    return Some(der);
                }
                in_block = false;
            } else if in_block {
                b64.push_str(trimmed);
            }
        }
    }
    None
}

fn handle_legacy_args(args: &[String]) -> Result<bool, Box<dyn std::error::Error>> {
    if (args.len() == 4 || args.len() == 5) && args[1].ends_with(".pdf") {
        let template = fs::read(&args[1])?;
        let json = fs::read_to_string(&args[2])?;
        let piece_info = if args.len() == 5 {
            Some(fs::read_to_string(&args[4])?)
        } else {
            None
        };
        let mut opts = FillOptions::new();
        if let Some(pi_str) = piece_info.as_deref()
            && let Ok(val) = serde_json::from_str(pi_str)
        {
            opts = opts.piece_info(val);
        }
        let (output, report) =
            fill_pdf_with_options(&template, &json, &opts).map_err(std::io::Error::other)?;
        fs::write(&args[3], &output)?;
        println!("{}", report_json(&report));
        println!("Output: {}", args[3]);
        return Ok(true);
    }
    Ok(false)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env_args: Vec<String> = std::env::args().collect();
    if env_args.len() > 1 && handle_legacy_args(&env_args)? {
        return Ok(());
    }

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Fill(args)) => {
            let template = fs::read(&args.template)?;
            let json = fs::read_to_string(&args.data)?;
            let mut opts = FillOptions::new().flatten(args.flatten);
            if let Some(pi_path) = args.piece_info {
                let pi_str = fs::read_to_string(pi_path)?;
                let val = serde_json::from_str(&pi_str)?;
                opts = opts.piece_info(val);
            }
            let (output, report) =
                fill_pdf_with_options(&template, &json, &opts).map_err(std::io::Error::other)?;
            fs::write(&args.output, &output)?;
            println!("{}", report_json(&report));
            println!("Filled PDF saved to: {}", args.output.display());
        }

        Some(Commands::Merge(args)) => {
            let mut merge_opts = MergeOptions::new()
                .create_bookmarks(args.bookmarks)
                .page_mode(args.page_mode.into())
                .flatten(args.flatten);

            if let Some(pi_path) = args.piece_info {
                let pi_str = fs::read_to_string(pi_path)?;
                let val = serde_json::from_str(&pi_str)?;
                merge_opts = merge_opts.piece_info(val);
            }

            if let Some(dir) = args.dir {
                let batch_opts = BatchOptions::new().recursive(args.recursive);
                let rep =
                    batch_merge_dir_with_options(&dir, &args.output, &batch_opts, &merge_opts)
                        .map_err(std::io::Error::other)?;
                println!(
                    "Successfully merged {} PDF files from '{}' into '{}'",
                    rep.successful,
                    dir.display(),
                    args.output.display()
                );
            } else if let Some(inputs) = args.inputs {
                if inputs.is_empty() {
                    eprintln!("Error: Please provide at least one input PDF or use --dir");
                    std::process::exit(1);
                }
                let mut data = Vec::with_capacity(inputs.len());
                for path in &inputs {
                    data.push(fs::read(path)?);
                }
                let slices: Vec<&[u8]> = data.iter().map(|d| d.as_slice()).collect();
                let merged = merge_pdf_bytes_with_options(&slices, &merge_opts)
                    .map_err(std::io::Error::other)?;
                fs::write(&args.output, &merged)?;
                println!(
                    "Successfully merged {} PDF files into '{}'",
                    inputs.len(),
                    args.output.display()
                );
            } else {
                eprintln!("Error: Specify either --inputs or --dir");
                std::process::exit(1);
            }
        }

        Some(Commands::Split(args)) => {
            let bytes = fs::read(&args.input)?;
            let mut split_opts = SplitOptions::new()
                .ranges(args.range)
                .naming_pattern(args.naming_pattern)
                .inherit_piece_info(args.inherit_piece_info)
                .flatten(args.flatten);

            if let Some(pi_path) = args.piece_info {
                let pi_str = fs::read_to_string(pi_path)?;
                let val = serde_json::from_str(&pi_str)?;
                split_opts = split_opts.piece_info(val);
            }

            let parts =
                split_pdf_bytes_with_options(&bytes, &split_opts).map_err(std::io::Error::other)?;
            fs::create_dir_all(&args.output_dir)?;

            let stem = args.input.file_stem().unwrap_or_default().to_string_lossy();
            let total = parts.len();
            for (idx, (label, part_bytes)) in parts.iter().enumerate() {
                let out_name = pdftoolkit_core::ops::format_split_filename(
                    &split_opts.naming_pattern,
                    &stem,
                    label,
                    idx,
                    (idx + 1) as u32,
                );
                let out_file = args.output_dir.join(out_name);
                fs::write(&out_file, part_bytes)?;
                println!("Exported: {}", out_file.display());
            }
            println!(
                "Split completed: generated {total} parts into '{}'",
                args.output_dir.display()
            );
        }

        Some(Commands::ToImage(args)) => {
            let bytes = fs::read(&args.input)?;
            let format: OutputImageFormat = args.format.into();
            let mut render_opts = RenderOptions::new()
                .dpi(args.dpi)
                .format(format)
                .jpeg_quality(args.quality)
                .transparent_background(args.transparent)
                .render_annotations(!args.no_annotations)
                .inherit_pdf_piece_info(!args.no_inherit_piece_info);

            if let Some(p) = args.pages {
                render_opts = render_opts.pages(p);
            }
            if let Some(w) = args.max_width {
                render_opts = render_opts.max_width(w);
            }
            if let Some(h) = args.max_height {
                render_opts = render_opts.max_height(h);
            }
            if let Some(pi_path) = args.piece_info {
                let pi_str = fs::read_to_string(pi_path)?;
                let val = serde_json::from_str(&pi_str)?;
                render_opts = render_opts.piece_info(val);
            }

            let images = render_pdf_to_images_with_options(&bytes, &render_opts)
                .map_err(std::io::Error::other)?;
            fs::create_dir_all(&args.output_dir)?;

            let stem = args.input.file_stem().unwrap_or_default().to_string_lossy();
            let ext = format.extension();
            let total = images.len();
            for (page_num, img_bytes) in images {
                let out_file = args
                    .output_dir
                    .join(format!("{stem}_page_{page_num:03}.{ext}"));
                fs::write(&out_file, &img_bytes)?;
                println!("Saved page image: {}", out_file.display());
            }
            println!(
                "Converted {total} pages to images in '{}'",
                args.output_dir.display()
            );
        }

        Some(Commands::ExtractImages(args)) => {
            let bytes = fs::read(&args.input)?;
            let mut opts = ExtractImageOptions::default();
            if let Some(ref p_str) = args.pages {
                let doc = lopdf::Document::load_mem(&bytes)
                    .map_err(|e| std::io::Error::other(format!("Failed to parse PDF: {e}")))?;
                let total_pages = doc.get_pages().len() as u32;
                let ranges =
                    parse_split_ranges(p_str, total_pages).map_err(std::io::Error::other)?;
                let mut page_list = Vec::new();
                for r in ranges {
                    page_list.extend(r.pages);
                }
                opts.pages = Some(page_list);
            }
            opts.min_width = args.min_width;
            opts.min_height = args.min_height;
            opts.deduplicate = !args.keep_duplicates;
            opts.naming_pattern = args.naming_pattern;

            let extracted =
                extract_images_from_bytes(&bytes, &opts).map_err(std::io::Error::other)?;
            fs::create_dir_all(&args.output_dir)?;

            let total = extracted.len();
            for img in &extracted {
                let out_file = args.output_dir.join(&img.file_name);
                fs::write(&out_file, &img.data)?;
                println!(
                    "Extracted image [page {}, {}x{}, {}]: {}",
                    img.page,
                    img.width,
                    img.height,
                    img.format,
                    out_file.display()
                );
            }
            println!(
                "Extracted {total} embedded images into '{}'",
                args.output_dir.display()
            );
        }

        Some(Commands::Watermark(args)) => {
            let bytes = fs::read(&args.input)?;
            let mut wm_opts = if let Some(ref tmpl_path) = args.template_json {
                let tmpl_str = fs::read_to_string(tmpl_path)?;
                serde_json::from_str::<WatermarkOptions>(&tmpl_str)?
            } else {
                WatermarkOptions::new()
            };

            if let Some(t) = args.text {
                wm_opts.text = Some(t);
            }
            if let Some(img_path) = args.image {
                let img_bytes = fs::read(&img_path)?;
                wm_opts.image_bytes = Some(img_bytes);
            }
            wm_opts.pages = PageSelection::parse(&args.pages);
            wm_opts.position = WatermarkPosition::parse(&args.position);
            if let Some(rot) = args.rotation {
                wm_opts.rotation = Some(rot);
            }
            wm_opts.opacity = args.opacity;
            wm_opts.font_name = args.font;
            if let Some(fs) = args.font_size {
                wm_opts.font_size = Some(fs);
            }
            wm_opts.color = ColorRgb::parse(&args.color).map_err(std::io::Error::other)?;
            wm_opts.layer = LayerMode::parse(&args.layer);
            wm_opts.flatten = args.flatten;

            if let Some(pi_path) = args.piece_info {
                let pi_str = fs::read_to_string(pi_path)?;
                let val = serde_json::from_str(&pi_str)?;
                wm_opts.piece_info = Some(val);
            }

            let filename = args.input.file_stem().and_then(|s| s.to_str());
            let mut doc = lopdf::Document::load_mem(&bytes)
                .map_err(|e| std::io::Error::other(format!("Failed to parse PDF: {e}")))?;
            pdftoolkit_core::ops::apply_watermark_to_doc(&mut doc, &wm_opts, filename)
                .map_err(std::io::Error::other)?;

            let mut out = Vec::new();
            doc.save_to(&mut out)
                .map_err(|e| std::io::Error::other(format!("Failed to save PDF: {e}")))?;

            if let Some(parent) = args.output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&args.output, &out)?;
            println!("Watermark applied: {}", args.output.display());
        }

        Some(Commands::Number(args)) => {
            let bytes = fs::read(&args.input)?;
            let mut num_opts = NumberingOptions::new()
                .format(args.format)
                .position(NumberingPosition::parse(&args.position))
                .pages(PageSelection::parse(&args.pages))
                .start_page(args.start_page)
                .start_number(args.start_number)
                .margin(args.margin_x, args.margin_y)
                .font(args.font)
                .font_size(args.font_size)
                .color(ColorRgb::parse(&args.color).map_err(std::io::Error::other)?)
                .opacity(args.opacity)
                .flatten(args.flatten);

            num_opts.layer = LayerMode::parse(&args.layer);

            if let Some(pi_path) = args.piece_info {
                let pi_str = fs::read_to_string(pi_path)?;
                let val = serde_json::from_str(&pi_str)?;
                num_opts.piece_info = Some(val);
            }

            let filename = args.input.file_stem().and_then(|s| s.to_str());
            let mut doc = lopdf::Document::load_mem(&bytes)
                .map_err(|e| std::io::Error::other(format!("Failed to parse PDF: {e}")))?;
            pdftoolkit_core::ops::apply_page_numbering_to_doc(&mut doc, &num_opts, filename)
                .map_err(std::io::Error::other)?;

            let mut out = Vec::new();
            doc.save_to(&mut out)
                .map_err(|e| std::io::Error::other(format!("Failed to save PDF: {e}")))?;

            if let Some(parent) = args.output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&args.output, &out)?;
            println!("Page numbering applied: {}", args.output.display());
        }

        Some(Commands::Rotate(args)) => {
            let bytes = fs::read(&args.input)?;
            let mode = if args.auto_text {
                RotationMode::AutoDetectText {
                    fallback_angle: args.fallback_angle,
                }
            } else if let Some(ref ori_str) = args.orientation {
                let target = match TargetOrientation::parse(ori_str) {
                    Some(t) => t,
                    None => {
                        eprintln!("Invalid orientation: expected 'portrait' or 'landscape'");
                        std::process::exit(1);
                    }
                };
                let dir = RotationDirection::parse(&args.direction);
                RotationMode::ToOrientation {
                    target,
                    direction: dir,
                }
            } else {
                let deg = args.angle.unwrap_or(90);
                RotationMode::Angle {
                    degrees: deg,
                    relative: !args.absolute,
                }
            };

            let mut opts = RotateOptions::new()
                .pages(PageSelection::parse(&args.pages))
                .flatten(args.flatten);
            opts.mode = mode;

            if let Some(p) = args.piece_info {
                let pi_str = fs::read_to_string(p)?;
                let val = serde_json::from_str(&pi_str)?;
                opts.piece_info = Some(val);
            }

            let (rotated_bytes, report) =
                rotate_pdf_pages(&bytes, &opts).map_err(std::io::Error::other)?;

            if let Some(parent) = args.output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&args.output, rotated_bytes)?;

            if args.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Successfully rotated {}/{} pages -> '{}'",
                    report.rotated_pages,
                    report.total_pages,
                    args.output.display()
                );
            }
        }

        Some(Commands::RemovePages(args)) => {
            let bytes = fs::read(&args.input)?;
            let mut opts = RemovePagesOptions::new()
                .remove_cover(args.cover)
                .remove_back_cover(args.back_cover)
                .remove_blank_pages(args.blank)
                .min_pages_retained(args.min_pages)
                .flatten(args.flatten);

            if let Some(p) = &args.pages {
                opts = opts.pages_str(p);
            }
            if let Some(kp) = &args.keep_pages {
                opts = opts.keep_pages_str(kp);
            }
            if let Some(pi_path) = args.piece_info {
                let pi_str = fs::read_to_string(pi_path)?;
                let val = serde_json::from_str(&pi_str)?;
                opts.piece_info = Some(val);
            }

            let (result_bytes, report) =
                remove_pdf_pages(&bytes, &opts).map_err(std::io::Error::other)?;

            if let Some(parent) = args.output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&args.output, result_bytes)?;

            if args.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Successfully removed {} page(s), retained {}/{} -> '{}'",
                    report.removed_pages.len(),
                    report.retained_pages,
                    report.original_pages,
                    args.output.display()
                );
            }
        }

        Some(Commands::Crop(args)) => {
            let bytes = fs::read(&args.input)?;
            let mut opts = CropOptions::new()
                .target_box(TargetBox::parse(&args.target_box))
                .pages_str(&args.pages)
                .clamp_to_media_box(!args.no_clamp)
                .flatten(args.flatten);

            if args.auto_content {
                opts = opts.auto_detect_content(args.padding);
            } else if let Some(box_str) = &args.r#box {
                let parts: Vec<f64> = box_str
                    .split(',')
                    .filter_map(|s| s.trim().parse::<f64>().ok())
                    .collect();
                if parts.len() != 4 {
                    eprintln!("Invalid box format: expected 'llx,lly,urx,ury'");
                    std::process::exit(1);
                }
                opts = opts.crop_box([parts[0], parts[1], parts[2], parts[3]]);
            } else if let Some(m_rel) = &args.margins_relative {
                let parts: Vec<f64> = m_rel
                    .split(',')
                    .filter_map(|s| s.trim().parse::<f64>().ok())
                    .collect();
                if parts.len() == 1 {
                    opts = opts.margins_relative(parts[0], parts[0], parts[0], parts[0]);
                } else if parts.len() == 4 {
                    opts = opts.margins_relative(parts[0], parts[1], parts[2], parts[3]);
                } else {
                    eprintln!(
                        "Invalid margins-relative format: expected 1 or 4 values (top,bottom,left,right)"
                    );
                    std::process::exit(1);
                }
            } else if let Some(m_str) = &args.margins {
                let parts: Vec<f64> = m_str
                    .split(',')
                    .filter_map(|s| s.trim().parse::<f64>().ok())
                    .collect();
                if parts.len() == 1 {
                    opts = opts.margins_uniform(parts[0]);
                } else if parts.len() == 4 {
                    opts = opts.margins(parts[0], parts[1], parts[2], parts[3]);
                } else {
                    eprintln!(
                        "Invalid margins format: expected 1 or 4 values (top,bottom,left,right)"
                    );
                    std::process::exit(1);
                }
            }

            if let Some(pi_path) = args.piece_info {
                let pi_str = fs::read_to_string(pi_path)?;
                let val = serde_json::from_str(&pi_str)?;
                opts.piece_info = Some(val);
            }

            let (cropped_bytes, report) =
                crop_pdf_pages(&bytes, &opts).map_err(std::io::Error::other)?;

            if let Some(parent) = args.output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&args.output, cropped_bytes)?;

            if args.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Successfully cropped {}/{} pages -> '{}'",
                    report.cropped_pages,
                    report.total_pages,
                    args.output.display()
                );
            }
        }

        Some(Commands::ExtractText(args)) => {
            let bytes = fs::read(&args.input)?;
            let opts = TextExtractionOptions::new()
                .pages(PageSelection::parse(&args.pages))
                .granularity(TextGranularity::parse(&args.granularity))
                .sort_reading_order(!args.no_reading_order)
                .line_tolerance(args.line_tolerance)
                .word_gap_factor(args.word_gap)
                .include_invisible(args.include_invisible)
                .normalize_whitespace(!args.no_normalize_whitespace);

            if args.json {
                let report =
                    extract_text_structured(&bytes, &opts).map_err(std::io::Error::other)?;
                let json_str = serde_json::to_string_pretty(&report)?;

                if let Some(out_path) = &args.output {
                    if let Some(parent) = out_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(out_path, &json_str)?;
                    println!(
                        "Successfully extracted text ({} characters, {} words) -> '{}'",
                        report.total_characters,
                        report.total_words,
                        out_path.display()
                    );
                } else {
                    println!("{json_str}");
                }
            } else {
                let text = extract_text(&bytes, &opts).map_err(std::io::Error::other)?;

                if let Some(out_path) = &args.output {
                    if let Some(parent) = out_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(out_path, &text)?;
                    println!("Successfully extracted text -> '{}'", out_path.display());
                } else {
                    print!("{text}");
                }
            }
        }

        Some(Commands::Batch(args)) => match args.sub {
            BatchSubcommands::Fill {
                template,
                data,
                output_dir,
                piece_info,
                flatten,
                threads,
                dry_run,
            } => {
                let mut fill_opts = FillOptions::new().flatten(flatten);
                if let Some(p) = piece_info {
                    let pi_str = fs::read_to_string(p)?;
                    let val = serde_json::from_str(&pi_str)?;
                    fill_opts = fill_opts.piece_info(val);
                }
                let mut batch_opts = BatchOptions::new().dry_run(dry_run);
                if let Some(t) = threads {
                    batch_opts = batch_opts.max_threads(t);
                }
                let rep = batch_fill_records_with_options(
                    &template,
                    &data,
                    &output_dir,
                    &fill_opts,
                    &batch_opts,
                )
                .map_err(std::io::Error::other)?;
                println!(
                    "Batch fill completed: generated {} files in '{}'",
                    rep.successful,
                    output_dir.display()
                );
            }
            BatchSubcommands::Merge {
                dir,
                output,
                recursive,
                filter,
                bookmarks,
                page_mode,
                flatten,
                piece_info,
                dry_run,
            } => {
                let mut batch_opts = BatchOptions::new().recursive(recursive).dry_run(dry_run);
                if let Some(f) = filter {
                    batch_opts = batch_opts.filter_pattern(f);
                }
                let mut merge_opts = MergeOptions::new()
                    .create_bookmarks(bookmarks)
                    .page_mode(page_mode.into())
                    .flatten(flatten);
                if let Some(p) = piece_info {
                    let pi_str = fs::read_to_string(p)?;
                    let val = serde_json::from_str(&pi_str)?;
                    merge_opts = merge_opts.piece_info(val);
                }
                let rep = batch_merge_dir_with_options(&dir, &output, &batch_opts, &merge_opts)
                    .map_err(std::io::Error::other)?;
                println!(
                    "Batch merge completed: combined {} files into '{}'",
                    rep.successful,
                    output.display()
                );
            }
            BatchSubcommands::Split {
                dir,
                output_dir,
                range,
                naming_pattern,
                recursive,
                filter,
                inherit_piece_info,
                piece_info,
                dry_run,
            } => {
                let mut batch_opts = BatchOptions::new().recursive(recursive).dry_run(dry_run);
                if let Some(f) = filter {
                    batch_opts = batch_opts.filter_pattern(f);
                }
                let mut split_opts = SplitOptions::new()
                    .ranges(range)
                    .naming_pattern(naming_pattern)
                    .inherit_piece_info(inherit_piece_info);
                if let Some(p) = piece_info {
                    let pi_str = fs::read_to_string(p)?;
                    let val = serde_json::from_str(&pi_str)?;
                    split_opts = split_opts.piece_info(val);
                }
                let rep = batch_split_dir_with_options(&dir, &output_dir, &batch_opts, &split_opts)
                    .map_err(std::io::Error::other)?;
                println!(
                    "Batch split completed for {} documents into '{}'",
                    rep.successful,
                    output_dir.display()
                );
            }
            BatchSubcommands::ToImage {
                dir,
                output_dir,
                dpi,
                format,
                quality,
                pages,
                max_width,
                max_height,
                transparent,
                recursive,
                filter,
                piece_info,
                dry_run,
            } => {
                let mut batch_opts = BatchOptions::new().recursive(recursive).dry_run(dry_run);
                if let Some(f) = filter {
                    batch_opts = batch_opts.filter_pattern(f);
                }
                let fmt: OutputImageFormat = format.into();
                let mut render_opts = RenderOptions::new()
                    .dpi(dpi)
                    .format(fmt)
                    .jpeg_quality(quality)
                    .transparent_background(transparent);
                if let Some(p) = pages {
                    render_opts = render_opts.pages(p);
                }
                if let Some(w) = max_width {
                    render_opts = render_opts.max_width(w);
                }
                if let Some(h) = max_height {
                    render_opts = render_opts.max_height(h);
                }
                if let Some(p) = piece_info {
                    let pi_str = fs::read_to_string(p)?;
                    let val = serde_json::from_str(&pi_str)?;
                    render_opts = render_opts.piece_info(val);
                }
                let rep =
                    batch_to_image_dir_with_options(&dir, &output_dir, &batch_opts, &render_opts)
                        .map_err(std::io::Error::other)?;
                println!(
                    "Batch image conversion completed for {} documents into '{}'",
                    rep.successful,
                    output_dir.display()
                );
            }
            BatchSubcommands::ExtractImages {
                dir,
                output_dir,
                recursive,
                filter,
                pages,
                min_width,
                min_height,
                keep_duplicates,
                dry_run,
            } => {
                let mut batch_opts = BatchOptions::new().recursive(recursive).dry_run(dry_run);
                if let Some(f) = filter {
                    batch_opts = batch_opts.filter_pattern(f);
                }
                let mut extract_opts = ExtractImageOptions::default();
                if let Some(ref p_str) = pages {
                    let mut page_list = Vec::new();
                    for part in p_str.split([',', ';']) {
                        let trimmed = part.trim();
                        if let Some((s, e)) = trimmed.split_once('-') {
                            if let (Ok(start), Ok(end)) =
                                (s.trim().parse::<u32>(), e.trim().parse::<u32>())
                            {
                                for p in start..=end {
                                    page_list.push(p);
                                }
                            }
                        } else if let Ok(p) = trimmed.parse::<u32>() {
                            page_list.push(p);
                        }
                    }
                    if !page_list.is_empty() {
                        extract_opts.pages = Some(page_list);
                    }
                }
                extract_opts.min_width = min_width;
                extract_opts.min_height = min_height;
                extract_opts.deduplicate = !keep_duplicates;

                let rep = batch_extract_images_dir_with_options(
                    &dir,
                    &output_dir,
                    &batch_opts,
                    &extract_opts,
                )
                .map_err(std::io::Error::other)?;
                println!(
                    "Batch image extraction completed for {} documents into '{}'",
                    rep.successful,
                    output_dir.display()
                );
            }

            BatchSubcommands::Watermark {
                dir,
                output_dir,
                text,
                image,
                pages,
                position,
                rotation,
                opacity,
                font,
                font_size,
                color,
                layer,
                recursive,
                filter,
                flatten,
                piece_info,
                dry_run,
            } => {
                let mut wm_opts = WatermarkOptions::new();
                wm_opts.text = text;
                if let Some(img_path) = image {
                    let img_bytes = fs::read(&img_path)?;
                    wm_opts.image_bytes = Some(img_bytes);
                }
                wm_opts.pages = PageSelection::parse(&pages);
                wm_opts.position = WatermarkPosition::parse(&position);
                wm_opts.rotation = rotation;
                wm_opts.opacity = opacity;
                wm_opts.font_name = font;
                wm_opts.font_size = font_size;
                wm_opts.color = ColorRgb::parse(&color).map_err(std::io::Error::other)?;
                wm_opts.layer = LayerMode::parse(&layer);
                wm_opts.flatten = flatten;

                if let Some(p) = piece_info {
                    let pi_str = fs::read_to_string(p)?;
                    let val = serde_json::from_str(&pi_str)?;
                    wm_opts.piece_info = Some(val);
                }

                let mut batch_opts = BatchOptions::new().recursive(recursive).dry_run(dry_run);
                if let Some(f) = filter {
                    batch_opts = batch_opts.filter_pattern(f);
                }

                let rep =
                    batch_watermark_dir_with_options(&dir, &output_dir, &wm_opts, &batch_opts)
                        .map_err(std::io::Error::other)?;

                println!(
                    "Batch watermarking completed for {} documents into '{}'",
                    rep.successful,
                    output_dir.display()
                );
            }

            BatchSubcommands::Number {
                dir,
                output_dir,
                format,
                position,
                pages,
                start_page,
                start_number,
                margin_x,
                margin_y,
                font,
                font_size,
                color,
                opacity,
                recursive,
                filter,
                flatten,
                piece_info,
                dry_run,
            } => {
                let mut num_opts = NumberingOptions::new()
                    .format(format)
                    .position(NumberingPosition::parse(&position))
                    .pages(PageSelection::parse(&pages))
                    .start_page(start_page)
                    .start_number(start_number)
                    .margin(margin_x, margin_y)
                    .font(font)
                    .font_size(font_size)
                    .color(ColorRgb::parse(&color).map_err(std::io::Error::other)?)
                    .opacity(opacity)
                    .flatten(flatten);

                if let Some(p) = piece_info {
                    let pi_str = fs::read_to_string(p)?;
                    let val = serde_json::from_str(&pi_str)?;
                    num_opts.piece_info = Some(val);
                }

                let mut batch_opts = BatchOptions::new().recursive(recursive).dry_run(dry_run);
                if let Some(f) = filter {
                    batch_opts = batch_opts.filter_pattern(f);
                }

                let rep = batch_number_dir_with_options(&dir, &output_dir, &num_opts, &batch_opts)
                    .map_err(std::io::Error::other)?;

                println!(
                    "Batch page numbering completed for {} documents into '{}'",
                    rep.successful,
                    output_dir.display()
                );
            }

            BatchSubcommands::Rotate {
                dir,
                output_dir,
                angle,
                orientation,
                direction,
                auto_text,
                pages,
                absolute,
                fallback_angle,
                recursive,
                filter,
                flatten,
                piece_info,
                dry_run,
            } => {
                let mode = if auto_text {
                    RotationMode::AutoDetectText { fallback_angle }
                } else if let Some(ref ori_str) = orientation {
                    let target = match TargetOrientation::parse(ori_str) {
                        Some(t) => t,
                        None => {
                            eprintln!("Invalid orientation: expected 'portrait' or 'landscape'");
                            std::process::exit(1);
                        }
                    };
                    let dir = RotationDirection::parse(&direction);
                    RotationMode::ToOrientation {
                        target,
                        direction: dir,
                    }
                } else {
                    let deg = angle.unwrap_or(90);
                    RotationMode::Angle {
                        degrees: deg,
                        relative: !absolute,
                    }
                };

                let mut rot_opts = RotateOptions::new()
                    .pages(PageSelection::parse(&pages))
                    .flatten(flatten);
                rot_opts.mode = mode;

                if let Some(p) = piece_info {
                    let pi_str = fs::read_to_string(p)?;
                    let val = serde_json::from_str(&pi_str)?;
                    rot_opts.piece_info = Some(val);
                }

                let mut batch_opts = BatchOptions::new().recursive(recursive).dry_run(dry_run);
                if let Some(f) = filter {
                    batch_opts = batch_opts.filter_pattern(f);
                }

                let rep = batch_rotate_dir_with_options(&dir, &output_dir, &rot_opts, &batch_opts)
                    .map_err(std::io::Error::other)?;

                println!(
                    "Batch page rotation completed for {} documents into '{}'",
                    rep.successful,
                    output_dir.display()
                );
            }

            BatchSubcommands::RemovePages {
                dir,
                output_dir,
                cover,
                back_cover,
                blank,
                pages,
                keep_pages,
                min_pages,
                recursive,
                filter,
                flatten,
                piece_info,
                dry_run,
            } => {
                let mut remove_opts = RemovePagesOptions::new()
                    .remove_cover(cover)
                    .remove_back_cover(back_cover)
                    .remove_blank_pages(blank)
                    .min_pages_retained(min_pages)
                    .flatten(flatten);

                if let Some(p) = pages {
                    remove_opts = remove_opts.pages_str(&p);
                }
                if let Some(kp) = keep_pages {
                    remove_opts = remove_opts.keep_pages_str(&kp);
                }
                if let Some(p) = piece_info {
                    let pi_str = fs::read_to_string(p)?;
                    let val = serde_json::from_str(&pi_str)?;
                    remove_opts.piece_info = Some(val);
                }

                let mut batch_opts = BatchOptions::new().recursive(recursive).dry_run(dry_run);
                if let Some(f) = filter {
                    batch_opts = batch_opts.filter_pattern(f);
                }

                let rep = batch_remove_pages_dir_with_options(
                    &dir,
                    &output_dir,
                    &remove_opts,
                    &batch_opts,
                )
                .map_err(std::io::Error::other)?;

                println!(
                    "Batch page removal completed for {} documents into '{}'",
                    rep.successful,
                    output_dir.display()
                );
            }

            BatchSubcommands::Crop {
                dir,
                output_dir,
                margins,
                margins_relative,
                r#box,
                auto_content,
                padding,
                target_box,
                pages,
                no_clamp,
                recursive,
                filter,
                flatten,
                piece_info,
                dry_run,
            } => {
                let mut crop_opts = CropOptions::new()
                    .target_box(TargetBox::parse(&target_box))
                    .pages_str(&pages)
                    .clamp_to_media_box(!no_clamp)
                    .flatten(flatten);

                if auto_content {
                    crop_opts = crop_opts.auto_detect_content(padding);
                } else if let Some(box_str) = &r#box {
                    let parts: Vec<f64> = box_str
                        .split(',')
                        .filter_map(|s| s.trim().parse::<f64>().ok())
                        .collect();
                    if parts.len() != 4 {
                        eprintln!("Invalid box format: expected 'llx,lly,urx,ury'");
                        std::process::exit(1);
                    }
                    crop_opts = crop_opts.crop_box([parts[0], parts[1], parts[2], parts[3]]);
                } else if let Some(m_rel) = &margins_relative {
                    let parts: Vec<f64> = m_rel
                        .split(',')
                        .filter_map(|s| s.trim().parse::<f64>().ok())
                        .collect();
                    if parts.len() == 1 {
                        crop_opts =
                            crop_opts.margins_relative(parts[0], parts[0], parts[0], parts[0]);
                    } else if parts.len() == 4 {
                        crop_opts =
                            crop_opts.margins_relative(parts[0], parts[1], parts[2], parts[3]);
                    } else {
                        eprintln!(
                            "Invalid margins-relative format: expected 1 or 4 values (top,bottom,left,right)"
                        );
                        std::process::exit(1);
                    }
                } else if let Some(m_str) = &margins {
                    let parts: Vec<f64> = m_str
                        .split(',')
                        .filter_map(|s| s.trim().parse::<f64>().ok())
                        .collect();
                    if parts.len() == 1 {
                        crop_opts = crop_opts.margins_uniform(parts[0]);
                    } else if parts.len() == 4 {
                        crop_opts = crop_opts.margins(parts[0], parts[1], parts[2], parts[3]);
                    } else {
                        eprintln!(
                            "Invalid margins format: expected 1 or 4 values (top,bottom,left,right)"
                        );
                        std::process::exit(1);
                    }
                }

                if let Some(p) = piece_info {
                    let pi_str = fs::read_to_string(p)?;
                    let val = serde_json::from_str(&pi_str)?;
                    crop_opts.piece_info = Some(val);
                }

                let mut batch_opts = BatchOptions::new().recursive(recursive).dry_run(dry_run);
                if let Some(f) = filter {
                    batch_opts = batch_opts.filter_pattern(f);
                }

                let rep = batch_crop_dir_with_options(&dir, &output_dir, &crop_opts, &batch_opts)
                    .map_err(std::io::Error::other)?;

                println!(
                    "Batch page cropping completed for {} documents into '{}'",
                    rep.successful,
                    output_dir.display()
                );
            }

            BatchSubcommands::ExtractText {
                dir,
                output_dir,
                pages,
                granularity,
                json,
                no_reading_order,
                line_tolerance,
                word_gap,
                include_invisible,
                no_normalize_whitespace,
                recursive,
                filter,
                dry_run,
            } => {
                let text_opts = TextExtractionOptions::new()
                    .pages(PageSelection::parse(&pages))
                    .granularity(TextGranularity::parse(&granularity))
                    .sort_reading_order(!no_reading_order)
                    .line_tolerance(line_tolerance)
                    .word_gap_factor(word_gap)
                    .include_invisible(include_invisible)
                    .normalize_whitespace(!no_normalize_whitespace);

                let mut batch_opts = BatchOptions::new().recursive(recursive).dry_run(dry_run);
                if let Some(f) = filter {
                    batch_opts = batch_opts.filter_pattern(f);
                }

                let rep = batch_extract_text_dir_with_options(
                    &dir,
                    &output_dir,
                    &text_opts,
                    json,
                    &batch_opts,
                )
                .map_err(std::io::Error::other)?;

                println!(
                    "Batch text extraction completed for {} documents into '{}'",
                    rep.successful,
                    output_dir.display()
                );
            }
        },

        Some(Commands::Inspect(args)) => {
            let bytes = fs::read(&args.input)?;
            let fields_json = form_fields_json(&bytes).map_err(std::io::Error::other)?;
            println!("{fields_json}");
        }

        Some(Commands::InspectImage(args)) => {
            let bytes = fs::read(&args.input)?;
            match get_image_metadata(&bytes) {
                Ok(Some(val)) => {
                    println!("{}", serde_json::to_string_pretty(&val)?);
                }
                Ok(None) => {
                    println!(
                        "No PieceInfo metadata found in image: {}",
                        args.input.display()
                    );
                }
                Err(e) => {
                    eprintln!("Error inspecting image metadata: {e}");
                    std::process::exit(1);
                }
            }
        }

        Some(Commands::AddSigField(args)) => {
            let bytes = fs::read(&args.input)?;
            let mut opts = AddSignatureFieldOptions::new(&args.name);
            if let Some(p) = args.page {
                opts = opts.page(p);
            }
            if args.invisible {
                opts = opts.invisible();
            } else if let Some(rect_str) = &args.rect {
                let parts: Vec<f64> = rect_str
                    .split(',')
                    .filter_map(|s| s.trim().parse::<f64>().ok())
                    .collect();
                if parts.len() != 4 {
                    eprintln!("Invalid rect format: expected 'llx,lly,urx,ury'");
                    std::process::exit(1);
                }
                opts = opts.rect(parts[0], parts[1], parts[2], parts[3]);
            } else if let Some(pos_str) = &args.position {
                let preset = FieldPresetPosition::parse(pos_str);
                opts = opts.preset_with_margins(
                    preset,
                    args.width,
                    args.height,
                    args.margin_x,
                    args.margin_y,
                );
            } else {
                opts = opts.preset_with_margins(
                    FieldPresetPosition::BottomRight,
                    args.width,
                    args.height,
                    args.margin_x,
                    args.margin_y,
                );
            }

            let result_bytes = add_signature_field(&bytes, &opts)
                .map_err(|e| std::io::Error::other(e.to_string()))?;

            if let Some(parent) = args.output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&args.output, result_bytes)?;
            println!(
                "Successfully added signature field '{}' -> '{}'",
                args.name,
                args.output.display()
            );
        }

        Some(Commands::RemoveSigField(args)) => {
            let bytes = fs::read(&args.input)?;
            let opts = if args.all {
                RemoveSignatureFieldOptions::all()
            } else if args.all_unsigned {
                RemoveSignatureFieldOptions::all_unsigned()
            } else if let Some(n) = &args.name {
                RemoveSignatureFieldOptions::by_name(n)
            } else {
                RemoveSignatureFieldOptions::all_unsigned()
            };

            let (result_bytes, removed_count) = remove_signature_field(&bytes, &opts)
                .map_err(|e| std::io::Error::other(e.to_string()))?;

            if let Some(parent) = args.output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&args.output, result_bytes)?;
            println!(
                "Removed {} signature field(s) -> '{}'",
                removed_count,
                args.output.display()
            );
        }

        Some(Commands::Sign(args)) => {
            let pdf_bytes = fs::read(&args.input)?;
            let cert_raw = fs::read(&args.cert)?;
            let cert_der = decode_der_or_pem(&cert_raw)
                .map_err(|e| std::io::Error::other(format!("Certificate error: {e}")))?;

            let key_der = if let Some(key_path) = &args.key {
                let key_raw = fs::read(key_path)?;
                decode_der_or_pem(&key_raw)
                    .map_err(|e| std::io::Error::other(format!("Private key error: {e}")))?
            } else if let Some(extracted_key) = extract_private_key_pem(&cert_raw) {
                extracted_key
            } else {
                eprintln!(
                    "Error: Private key not provided. Use --key <KEY_PATH> or bundle PEM in --cert"
                );
                std::process::exit(1);
            };

            let signer: Box<dyn Signer> = if let Ok(rsa_signer) =
                CertificateSigner::from_pkcs8_der(cert_der.clone(), &key_der)
            {
                Box::new(rsa_signer)
            } else if let Ok(ecdsa_signer) = EcdsaSigner::from_pkcs8_der(cert_der.clone(), &key_der)
            {
                Box::new(ecdsa_signer)
            } else {
                eprintln!("Error: Failed to parse private key as PKCS#8 RSA or ECDSA.");
                std::process::exit(1);
            };

            let mut builder = PdfSigner::new()
                .field(&args.field)
                .signer(signer)
                .flatten(args.flatten);

            if let Some(r) = &args.reason {
                builder = builder.reason(r);
            }
            if let Some(l) = &args.location {
                builder = builder.location(l);
            }
            if let Some(c) = &args.contact {
                builder = builder.contact(c);
            }

            if args.auto_create_field {
                builder = builder.auto_create_field(true);
                if let Some(p) = args.page {
                    builder = builder.page(p);
                }
                if args.invisible {
                    builder = builder.placement(SignaturePlacement::Invisible);
                } else if let Some(rect_str) = &args.rect {
                    let parts: Vec<f64> = rect_str
                        .split(',')
                        .filter_map(|s| s.trim().parse::<f64>().ok())
                        .collect();
                    if parts.len() == 4 {
                        builder = builder.placement(SignaturePlacement::Rect([
                            parts[0], parts[1], parts[2], parts[3],
                        ]));
                    }
                } else if let Some(pos_str) = &args.position {
                    let preset = FieldPresetPosition::parse(pos_str);
                    builder = builder.placement(SignaturePlacement::Preset {
                        position: preset,
                        width: 150.0,
                        height: 50.0,
                        margin_x: 36.0,
                        margin_y: 36.0,
                    });
                }
            }

            if args.image.is_some() || args.graphic_position.to_lowercase() != "left" {
                let img_data = if let Some(path_or_b64) = &args.image {
                    if path_or_b64.starts_with("data:image/") {
                        Some(path_or_b64.clone())
                    } else if let Ok(bytes) = fs::read(path_or_b64) {
                        use base64::Engine;
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                        let mime = if path_or_b64.ends_with(".png") {
                            "image/png"
                        } else {
                            "image/jpeg"
                        };
                        Some(format!("data:{mime};base64,{b64}"))
                    } else {
                        Some(path_or_b64.clone())
                    }
                } else {
                    None
                };

                let pos: pdftoolkit_core::appearance::GraphicPosition = args
                    .graphic_position
                    .parse()
                    .unwrap_or(pdftoolkit_core::appearance::GraphicPosition::Left);

                let app = pdftoolkit_core::appearance::SignatureAppearanceOptions {
                    image: img_data,
                    position: pos,
                    show_signer_name: true,
                    signer_name: args.reason.clone(),
                    show_date: true,
                    reason: args.reason.clone(),
                    location: args.location.clone(),
                    ..Default::default()
                };
                builder = builder.appearance(app);
            }

            let signed_bytes = builder
                .sign(&pdf_bytes)
                .map_err(|e| std::io::Error::other(e.to_string()))?;

            if let Some(parent) = args.output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&args.output, signed_bytes)?;
            println!(
                "Successfully digitally signed PDF -> '{}' (field: '{}')",
                args.output.display(),
                args.field
            );
        }

        Some(Commands::Verify(args)) => {
            let pdf_bytes = fs::read(&args.input)?;
            let results = verify_pdf_signatures(&pdf_bytes)
                .map_err(|e| std::io::Error::other(e.to_string()))?;

            if args.json {
                println!("{}", serde_json::to_string_pretty(&results)?);
            } else {
                if results.is_empty() {
                    println!("No digital signatures found in '{}'.", args.input.display());
                } else {
                    println!("=== Digital Signature Verification Report ===");
                    println!("File: {}", args.input.display());
                    println!("Signatures Found: {}", results.len());
                    println!("--------------------------------------------------");
                    for (idx, sig) in results.iter().enumerate() {
                        println!("[Signature #{}] Field: '{}'", idx + 1, sig.field_name);
                        println!(
                            "  Status:          {}",
                            if sig.is_valid {
                                "VALID ✓"
                            } else {
                                "INVALID ✗"
                            }
                        );
                        println!(
                            "  Digest Matched:  {}",
                            if sig.digest_matched { "YES" } else { "NO" }
                        );
                        if let Some(subj) = &sig.signer_subject {
                            println!("  Signer Subject:  {}", subj);
                        }
                        if let Some(issuer) = &sig.signer_issuer {
                            println!("  Issuer:          {}", issuer);
                        }
                        if let (Some(start), Some(end)) = (&sig.validity_start, &sig.validity_end) {
                            println!("  Validity:        {} -> {}", start, end);
                        }
                        if let Some(r) = &sig.reason {
                            println!("  Reason:          {}", r);
                        }
                        if let Some(l) = &sig.location {
                            println!("  Location:        {}", l);
                        }
                        println!("  ByteRange:       {:?}", sig.byte_range);
                        if let Some(err) = &sig.error {
                            println!("  Warning/Error:   {}", err);
                        }
                        println!("--------------------------------------------------");
                    }
                }
            }
        }

        None => {
            use clap::CommandFactory;
            Cli::command().print_help()?;
            println!();
        }
    }

    Ok(())
}
