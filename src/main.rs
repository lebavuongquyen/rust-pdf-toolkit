use clap::{Args, Parser, Subcommand, ValueEnum};
use pdftoolkit_core::{
    FillOptions,
    batch::{
        BatchOptions, batch_extract_images_dir_with_options, batch_fill_records_with_options,
        batch_merge_dir_with_options, batch_split_dir_with_options,
        batch_to_image_dir_with_options,
    },
    fill_pdf_with_options, form_fields_json,
    ops::{
        ExtractImageOptions, MergeOptions, OutputImageFormat, PageMode, RenderOptions,
        SplitOptions, extract_images_from_bytes, get_image_metadata, merge_pdf_bytes_with_options,
        parse_split_ranges, render_pdf_to_images_with_options, split_pdf_bytes_with_options,
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
    about = "High-performance all-in-one PDF toolkit: Fill, Merge, Split, Convert to Image, and Batch Folder Processing",
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

    /// Batch process entire folders (Fill, Merge, Split, Convert, Extract)
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
        if let Some(pi_str) = piece_info.as_deref() {
            if let Ok(val) = serde_json::from_str(pi_str) {
                opts = opts.piece_info(val);
            }
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

        None => {
            use clap::CommandFactory;
            Cli::command().print_help()?;
            println!();
        }
    }

    Ok(())
}
