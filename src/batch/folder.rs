use crate::FillOptions;
use crate::fill_pdf_with_options;
use crate::ops::{
    CropOptions, MergeOptions, NumberingOptions, OutputImageFormat, RemovePagesOptions,
    RenderOptions, RotateOptions, SplitOptions, TextExtractionOptions, WatermarkOptions,
    apply_page_numbering, apply_watermark, crop_pdf_pages, extract_text, extract_text_structured,
    format_split_filename, merge_pdf_bytes_with_options, remove_pdf_pages,
    render_pdf_to_images_with_options, rotate_pdf_pages, split_pdf_bytes_with_options,
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchOptions {
    #[serde(default)]
    pub recursive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter_pattern: Option<String>,
    #[serde(default = "default_true")]
    pub continue_on_error: bool,
    #[serde(default = "default_true")]
    pub overwrite: bool,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_threads: Option<usize>,
}

fn default_true() -> bool {
    true
}

impl Default for BatchOptions {
    fn default() -> Self {
        Self {
            recursive: false,
            filter_pattern: None,
            continue_on_error: true,
            overwrite: true,
            dry_run: false,
            max_threads: None,
        }
    }
}

impl BatchOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    pub fn filter_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.filter_pattern = Some(pattern.into());
        self
    }

    pub fn continue_on_error(mut self, continue_on_error: bool) -> Self {
        self.continue_on_error = continue_on_error;
        self
    }

    pub fn overwrite(mut self, overwrite: bool) -> Self {
        self.overwrite = overwrite;
        self
    }

    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn max_threads(mut self, threads: usize) -> Self {
        self.max_threads = Some(threads);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BatchReport {
    pub total_scanned: usize,
    pub successful: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

fn matches_filter(filename: &str, pattern: Option<&str>) -> bool {
    let pat = match pattern {
        Some(p) => p.trim(),
        None => return true,
    };
    if pat.is_empty() || pat == "*" {
        return true;
    }

    let file_lower = filename.to_lowercase();
    let pat_lower = pat.to_lowercase();

    if pat_lower.contains('*') {
        let parts: Vec<&str> = pat_lower.split('*').filter(|s| !s.is_empty()).collect();
        let mut curr = 0;
        for part in parts {
            if let Some(pos) = file_lower[curr..].find(part) {
                curr += pos + part.len();
            } else {
                return false;
            }
        }
        true
    } else {
        file_lower.contains(&pat_lower)
    }
}

/// Scans a directory for PDF files with optional recursive and pattern filter.
pub fn scan_pdf_files<P: AsRef<Path>>(
    dir: P,
    recursive: bool,
    filter_pattern: Option<&str>,
) -> Result<Vec<PathBuf>, String> {
    let dir = dir.as_ref();
    if !dir.exists() {
        return Err(format!("Directory does not exist: {}", dir.display()));
    }
    if !dir.is_dir() {
        return Err(format!("Path is not a directory: {}", dir.display()));
    }

    let walker = if recursive {
        WalkDir::new(dir)
    } else {
        WalkDir::new(dir).max_depth(1)
    };

    let mut pdfs = Vec::new();
    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file()
            && let Some(ext) = path.extension()
            && ext.to_string_lossy().eq_ignore_ascii_case("pdf")
        {
            let file_name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if matches_filter(&file_name, filter_pattern) {
                pdfs.push(path.to_path_buf());
            }
        }
    }

    pdfs.sort();
    Ok(pdfs)
}

/// Merges all PDF files found in `input_dir` into `output_file` with customizable options.
pub fn batch_merge_dir_with_options<P: AsRef<Path>, Q: AsRef<Path>>(
    input_dir: P,
    output_file: Q,
    batch_opts: &BatchOptions,
    merge_opts: &MergeOptions,
) -> Result<BatchReport, String> {
    let pdf_paths = scan_pdf_files(
        &input_dir,
        batch_opts.recursive,
        batch_opts.filter_pattern.as_deref(),
    )?;

    if pdf_paths.is_empty() {
        return Err(format!(
            "No matching PDF files found in directory: {}",
            input_dir.as_ref().display()
        ));
    }

    let mut report = BatchReport {
        total_scanned: pdf_paths.len(),
        successful: 0,
        failed: 0,
        errors: Vec::new(),
    };

    if batch_opts.dry_run {
        report.successful = pdf_paths.len();
        return Ok(report);
    }

    let mut files_data = Vec::with_capacity(pdf_paths.len());
    for p in &pdf_paths {
        match fs::read(p) {
            Ok(bytes) => {
                files_data.push(bytes);
                report.successful += 1;
            }
            Err(e) => {
                let err_msg = format!("Failed to read '{}': {e}", p.display());
                report.failed += 1;
                report.errors.push(err_msg.clone());
                if !batch_opts.continue_on_error {
                    return Err(err_msg);
                }
            }
        }
    }

    if files_data.is_empty() {
        return Err("No readable PDF files could be loaded for merging".to_string());
    }

    let byte_slices: Vec<&[u8]> = files_data.iter().map(|b| b.as_slice()).collect();
    let merged_bytes = merge_pdf_bytes_with_options(&byte_slices, merge_opts)?;

    let out_path = output_file.as_ref();
    if out_path.exists() && !batch_opts.overwrite {
        return Err(format!(
            "Output file '{}' already exists and overwrite is disabled",
            out_path.display()
        ));
    }

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create output parent directory: {e}"))?;
    }

    fs::write(out_path, &merged_bytes).map_err(|e| {
        format!(
            "Failed to write merged output to '{}': {e}",
            out_path.display()
        )
    })?;

    Ok(report)
}

/// Simple wrapper for backward compatibility.
pub fn batch_merge_dir<P: AsRef<Path>, Q: AsRef<Path>>(
    input_dir: P,
    output_file: Q,
    recursive: bool,
) -> Result<usize, String> {
    let b_opts = BatchOptions::new().recursive(recursive);
    let m_opts = MergeOptions::default();
    let report = batch_merge_dir_with_options(input_dir, output_file, &b_opts, &m_opts)?;
    Ok(report.successful)
}

/// Splits all PDF files in `input_dir` with customizable options.
pub fn batch_split_dir_with_options<P: AsRef<Path>, Q: AsRef<Path>>(
    input_dir: P,
    output_dir: Q,
    batch_opts: &BatchOptions,
    split_opts: &SplitOptions,
) -> Result<BatchReport, String> {
    let pdf_paths = scan_pdf_files(
        &input_dir,
        batch_opts.recursive,
        batch_opts.filter_pattern.as_deref(),
    )?;

    if pdf_paths.is_empty() {
        return Err(format!(
            "No matching PDF files found in directory: {}",
            input_dir.as_ref().display()
        ));
    }

    let out_dir = output_dir.as_ref();
    if !batch_opts.dry_run {
        fs::create_dir_all(out_dir).map_err(|e| {
            format!(
                "Failed to create output directory '{}': {e}",
                out_dir.display()
            )
        })?;
    }

    let results: Vec<Result<(), String>> = pdf_paths
        .par_iter()
        .map(|pdf_path| {
            if batch_opts.dry_run {
                return Ok(());
            }

            let stem = pdf_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "doc".to_string());

            let bytes = fs::read(pdf_path)
                .map_err(|e| format!("Failed to read '{}': {e}", pdf_path.display()))?;

            let parts = split_pdf_bytes_with_options(&bytes, split_opts)?;
            for (idx, (label, part_bytes)) in parts.iter().enumerate() {
                let out_name = format_split_filename(
                    &split_opts.naming_pattern,
                    &stem,
                    label,
                    idx,
                    (idx + 1) as u32,
                );
                let out_file = out_dir.join(out_name);
                if out_file.exists() && !batch_opts.overwrite {
                    continue;
                }
                fs::write(&out_file, part_bytes)
                    .map_err(|e| format!("Failed to write '{}': {e}", out_file.display()))?;
            }
            Ok(())
        })
        .collect();

    let mut report = BatchReport {
        total_scanned: pdf_paths.len(),
        successful: 0,
        failed: 0,
        errors: Vec::new(),
    };

    for res in results {
        match res {
            Ok(_) => report.successful += 1,
            Err(e) => {
                report.failed += 1;
                report.errors.push(e);
            }
        }
    }

    if report.failed > 0 && !batch_opts.continue_on_error {
        return Err(report.errors.join("; "));
    }

    Ok(report)
}

/// Simple wrapper for backward compatibility.
pub fn batch_split_dir<P: AsRef<Path>, Q: AsRef<Path>>(
    input_dir: P,
    output_dir: Q,
    range_spec: &str,
    recursive: bool,
) -> Result<usize, String> {
    let b_opts = BatchOptions::new().recursive(recursive);
    let s_opts = SplitOptions::new().ranges(range_spec);
    let rep = batch_split_dir_with_options(input_dir, output_dir, &b_opts, &s_opts)?;
    Ok(rep.successful)
}

/// Converts all PDF files in `input_dir` to images with customizable options.
pub fn batch_to_image_dir_with_options<P: AsRef<Path>, Q: AsRef<Path>>(
    input_dir: P,
    output_dir: Q,
    batch_opts: &BatchOptions,
    render_opts: &RenderOptions,
) -> Result<BatchReport, String> {
    let pdf_paths = scan_pdf_files(
        &input_dir,
        batch_opts.recursive,
        batch_opts.filter_pattern.as_deref(),
    )?;

    if pdf_paths.is_empty() {
        return Err(format!(
            "No matching PDF files found in directory: {}",
            input_dir.as_ref().display()
        ));
    }

    let out_dir = output_dir.as_ref();
    if !batch_opts.dry_run {
        fs::create_dir_all(out_dir).map_err(|e| {
            format!(
                "Failed to create output directory '{}': {e}",
                out_dir.display()
            )
        })?;
    }

    let results: Vec<Result<(), String>> = pdf_paths
        .par_iter()
        .map(|pdf_path| {
            if batch_opts.dry_run {
                return Ok(());
            }

            let stem = pdf_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "doc".to_string());

            let bytes = fs::read(pdf_path)
                .map_err(|e| format!("Failed to read '{}': {e}", pdf_path.display()))?;

            let images = render_pdf_to_images_with_options(&bytes, render_opts)?;
            let ext = render_opts.format.extension();
            for (page_num, img_bytes) in images {
                let out_name = format!("{stem}_page_{page_num:03}.{ext}");
                let out_file = out_dir.join(out_name);
                if out_file.exists() && !batch_opts.overwrite {
                    continue;
                }
                fs::write(&out_file, &img_bytes)
                    .map_err(|e| format!("Failed to write '{}': {e}", out_file.display()))?;
            }
            Ok(())
        })
        .collect();

    let mut report = BatchReport {
        total_scanned: pdf_paths.len(),
        successful: 0,
        failed: 0,
        errors: Vec::new(),
    };

    for res in results {
        match res {
            Ok(_) => report.successful += 1,
            Err(e) => {
                report.failed += 1;
                report.errors.push(e);
            }
        }
    }

    if report.failed > 0 && !batch_opts.continue_on_error {
        return Err(report.errors.join("; "));
    }

    Ok(report)
}

/// Simple wrapper for backward compatibility.
pub fn batch_to_image_dir<P: AsRef<Path>, Q: AsRef<Path>>(
    input_dir: P,
    output_dir: Q,
    dpi: f32,
    format: OutputImageFormat,
    recursive: bool,
) -> Result<usize, String> {
    let b_opts = BatchOptions::new().recursive(recursive);
    let r_opts = RenderOptions::new().dpi(dpi).format(format);
    let rep = batch_to_image_dir_with_options(input_dir, output_dir, &b_opts, &r_opts)?;
    Ok(rep.successful)
}

/// Extracts embedded images from all PDF files in `input_dir` with customizable options.
pub fn batch_extract_images_dir_with_options<P: AsRef<Path>, Q: AsRef<Path>>(
    input_dir: P,
    output_dir: Q,
    batch_opts: &BatchOptions,
    extract_opts: &crate::ops::ExtractImageOptions,
) -> Result<BatchReport, String> {
    let pdf_paths = scan_pdf_files(
        &input_dir,
        batch_opts.recursive,
        batch_opts.filter_pattern.as_deref(),
    )?;

    if pdf_paths.is_empty() {
        return Err(format!(
            "No matching PDF files found in directory: {}",
            input_dir.as_ref().display()
        ));
    }

    let out_dir = output_dir.as_ref();
    if !batch_opts.dry_run {
        fs::create_dir_all(out_dir).map_err(|e| {
            format!(
                "Failed to create output directory '{}': {e}",
                out_dir.display()
            )
        })?;
    }

    let results: Vec<Result<(), String>> = pdf_paths
        .par_iter()
        .map(|pdf_path| {
            if batch_opts.dry_run {
                return Ok(());
            }

            let pdf_bytes = fs::read(pdf_path)
                .map_err(|e| format!("Failed to read '{}': {e}", pdf_path.display()))?;

            let images = crate::ops::extract_images_from_bytes(&pdf_bytes, extract_opts)
                .map_err(|e| format!("Extract failed for '{}': {e}", pdf_path.display()))?;

            let stem = pdf_path.file_stem().unwrap_or_default().to_string_lossy();
            for img in images {
                let out_name = format!("{stem}_{}", img.file_name);
                let out_file = out_dir.join(out_name);
                if out_file.exists() && !batch_opts.overwrite {
                    continue;
                }
                fs::write(&out_file, &img.data)
                    .map_err(|e| format!("Failed to write '{}': {e}", out_file.display()))?;
            }
            Ok(())
        })
        .collect();

    let mut report = BatchReport {
        total_scanned: pdf_paths.len(),
        successful: 0,
        failed: 0,
        errors: Vec::new(),
    };

    for res in results {
        match res {
            Ok(_) => report.successful += 1,
            Err(e) => {
                report.failed += 1;
                report.errors.push(e);
            }
        }
    }

    if report.failed > 0 && !batch_opts.continue_on_error {
        return Err(report.errors.join("; "));
    }

    Ok(report)
}

/// Fills a single template PDF with an array of records from a JSON file with full options.
pub fn batch_fill_records_with_options<P: AsRef<Path>, Q: AsRef<Path>, R: AsRef<Path>>(
    template_path: P,
    data_json_path: Q,
    output_dir: R,
    fill_opts: &FillOptions,
    batch_opts: &BatchOptions,
) -> Result<BatchReport, String> {
    let template_bytes =
        fs::read(&template_path).map_err(|e| format!("Failed to read template PDF: {e}"))?;

    let json_content = fs::read_to_string(&data_json_path)
        .map_err(|e| format!("Failed to read data JSON: {e}"))?;

    let parsed_val: serde_json::Value =
        serde_json::from_str(&json_content).map_err(|e| format!("Invalid data JSON: {e}"))?;

    let records: Vec<serde_json::Value> = match parsed_val {
        serde_json::Value::Array(arr) => arr,
        obj @ serde_json::Value::Object(_) => vec![obj],
        _ => return Err("Data JSON must be an object or an array of objects".to_string()),
    };

    let total = records.len();
    if total == 0 {
        return Err("No data records found in JSON".to_string());
    }

    let out_dir = output_dir.as_ref();
    if !batch_opts.dry_run {
        fs::create_dir_all(out_dir)
            .map_err(|e| format!("Failed to create output directory: {e}"))?;
    }

    let template_stem = template_path
        .as_ref()
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "document".to_string());

    let results: Vec<Result<(), String>> = records
        .par_iter()
        .enumerate()
        .map(|(idx, record)| {
            if batch_opts.dry_run {
                return Ok(());
            }

            let record_json = record.to_string();
            let (filled_bytes, _report) =
                fill_pdf_with_options(&template_bytes, &record_json, fill_opts)?;

            let out_file = out_dir.join(format!("{template_stem}_{:04}.pdf", idx + 1));
            if out_file.exists() && !batch_opts.overwrite {
                return Ok(());
            }

            fs::write(&out_file, &filled_bytes)
                .map_err(|e| format!("Failed to write output '{}': {e}", out_file.display()))?;
            Ok(())
        })
        .collect();

    let mut report = BatchReport {
        total_scanned: total,
        successful: 0,
        failed: 0,
        errors: Vec::new(),
    };

    for res in results {
        match res {
            Ok(_) => report.successful += 1,
            Err(e) => {
                report.failed += 1;
                report.errors.push(e);
            }
        }
    }

    if report.failed > 0 && !batch_opts.continue_on_error {
        return Err(report.errors.join("; "));
    }

    Ok(report)
}

/// Simple wrapper for backward compatibility.
pub fn batch_fill_records<P: AsRef<Path>, Q: AsRef<Path>, R: AsRef<Path>>(
    template_path: P,
    data_json_path: Q,
    output_dir: R,
    piece_info_json_path: Option<&Path>,
) -> Result<usize, String> {
    let mut fill_opts = FillOptions::default();
    if let Some(p) = piece_info_json_path {
        let content =
            fs::read_to_string(p).map_err(|e| format!("Failed to read piece_info JSON: {e}"))?;
        let val =
            serde_json::from_str(&content).map_err(|e| format!("Invalid piece_info JSON: {e}"))?;
        fill_opts = fill_opts.piece_info(val);
    }
    let b_opts = BatchOptions::default();
    let rep = batch_fill_records_with_options(
        template_path,
        data_json_path,
        output_dir,
        &fill_opts,
        &b_opts,
    )?;
    Ok(rep.successful)
}

/// Batch applies a watermark across all PDF files in a directory.
pub fn batch_watermark_dir_with_options<P: AsRef<Path>, Q: AsRef<Path>>(
    input_dir: P,
    output_dir: Q,
    wm_opts: &WatermarkOptions,
    batch_opts: &BatchOptions,
) -> Result<BatchReport, String> {
    let pdf_paths = scan_pdf_files(
        &input_dir,
        batch_opts.recursive,
        batch_opts.filter_pattern.as_deref(),
    )?;

    if pdf_paths.is_empty() {
        return Err(format!(
            "No matching PDF files found in directory: {}",
            input_dir.as_ref().display()
        ));
    }

    let out_dir = output_dir.as_ref();
    if !batch_opts.dry_run {
        fs::create_dir_all(out_dir).map_err(|e| {
            format!(
                "Failed to create output directory '{}': {e}",
                out_dir.display()
            )
        })?;
    }

    let total = pdf_paths.len();
    let results: Vec<Result<(), String>> = pdf_paths
        .par_iter()
        .map(|pdf_path| {
            if batch_opts.dry_run {
                return Ok(());
            }

            let file_name = pdf_path
                .file_name()
                .ok_or_else(|| format!("Invalid file path: {}", pdf_path.display()))?;
            let out_file = out_dir.join(file_name);

            if out_file.exists() && !batch_opts.overwrite {
                return Ok(());
            }

            let bytes = fs::read(pdf_path)
                .map_err(|e| format!("Failed to read '{}': {e}", pdf_path.display()))?;

            let watermarked_bytes = apply_watermark(&bytes, wm_opts)
                .map_err(|e| format!("Failed to watermark '{}': {e}", pdf_path.display()))?;

            fs::write(&out_file, &watermarked_bytes)
                .map_err(|e| format!("Failed to write output '{}': {e}", out_file.display()))?;
            Ok(())
        })
        .collect();

    let mut report = BatchReport {
        total_scanned: total,
        successful: 0,
        failed: 0,
        errors: Vec::new(),
    };

    for res in results {
        match res {
            Ok(_) => report.successful += 1,
            Err(e) => {
                report.failed += 1;
                report.errors.push(e);
            }
        }
    }

    if report.failed > 0 && !batch_opts.continue_on_error {
        return Err(report.errors.join("; "));
    }

    Ok(report)
}

/// Batch applies page numbering across all PDF files in a directory.
pub fn batch_number_dir_with_options<P: AsRef<Path>, Q: AsRef<Path>>(
    input_dir: P,
    output_dir: Q,
    num_opts: &NumberingOptions,
    batch_opts: &BatchOptions,
) -> Result<BatchReport, String> {
    let pdf_paths = scan_pdf_files(
        &input_dir,
        batch_opts.recursive,
        batch_opts.filter_pattern.as_deref(),
    )?;

    if pdf_paths.is_empty() {
        return Err(format!(
            "No matching PDF files found in directory: {}",
            input_dir.as_ref().display()
        ));
    }

    let out_dir = output_dir.as_ref();
    if !batch_opts.dry_run {
        fs::create_dir_all(out_dir).map_err(|e| {
            format!(
                "Failed to create output directory '{}': {e}",
                out_dir.display()
            )
        })?;
    }

    let total = pdf_paths.len();
    let results: Vec<Result<(), String>> = pdf_paths
        .par_iter()
        .map(|pdf_path| {
            if batch_opts.dry_run {
                return Ok(());
            }

            let file_name = pdf_path
                .file_name()
                .ok_or_else(|| format!("Invalid file path: {}", pdf_path.display()))?;
            let out_file = out_dir.join(file_name);

            if out_file.exists() && !batch_opts.overwrite {
                return Ok(());
            }

            let bytes = fs::read(pdf_path)
                .map_err(|e| format!("Failed to read '{}': {e}", pdf_path.display()))?;

            let numbered_bytes = apply_page_numbering(&bytes, num_opts)
                .map_err(|e| format!("Failed to number '{}': {e}", pdf_path.display()))?;

            fs::write(&out_file, &numbered_bytes)
                .map_err(|e| format!("Failed to write output '{}': {e}", out_file.display()))?;
            Ok(())
        })
        .collect();

    let mut report = BatchReport {
        total_scanned: total,
        successful: 0,
        failed: 0,
        errors: Vec::new(),
    };

    for res in results {
        match res {
            Ok(_) => report.successful += 1,
            Err(e) => {
                report.failed += 1;
                report.errors.push(e);
            }
        }
    }

    if report.failed > 0 && !batch_opts.continue_on_error {
        return Err(report.errors.join("; "));
    }

    Ok(report)
}

/// Batch rotates all PDF files in a folder according to RotateOptions.
pub fn batch_rotate_dir_with_options<P: AsRef<Path>, Q: AsRef<Path>>(
    input_dir: P,
    output_dir: Q,
    rot_opts: &RotateOptions,
    batch_opts: &BatchOptions,
) -> Result<BatchReport, String> {
    let pdf_paths = scan_pdf_files(
        &input_dir,
        batch_opts.recursive,
        batch_opts.filter_pattern.as_deref(),
    )?;

    let out_dir = output_dir.as_ref();
    if !batch_opts.dry_run {
        fs::create_dir_all(out_dir).map_err(|e| {
            format!(
                "Failed to create output directory '{}': {e}",
                out_dir.display()
            )
        })?;
    }

    let total = pdf_paths.len();
    let results: Vec<Result<(), String>> = pdf_paths
        .par_iter()
        .map(|pdf_path| {
            if batch_opts.dry_run {
                return Ok(());
            }

            let file_name = pdf_path
                .file_name()
                .ok_or_else(|| format!("Invalid file path: {}", pdf_path.display()))?;
            let out_file = out_dir.join(file_name);

            if out_file.exists() && !batch_opts.overwrite {
                return Ok(());
            }

            let bytes = fs::read(pdf_path)
                .map_err(|e| format!("Failed to read '{}': {e}", pdf_path.display()))?;

            let (rotated_bytes, _report) = rotate_pdf_pages(&bytes, rot_opts)
                .map_err(|e| format!("Failed to rotate '{}': {e}", pdf_path.display()))?;

            fs::write(&out_file, &rotated_bytes)
                .map_err(|e| format!("Failed to write output '{}': {e}", out_file.display()))?;
            Ok(())
        })
        .collect();

    let mut report = BatchReport {
        total_scanned: total,
        successful: 0,
        failed: 0,
        errors: Vec::new(),
    };

    for res in results {
        match res {
            Ok(_) => report.successful += 1,
            Err(e) => {
                report.failed += 1;
                report.errors.push(e);
            }
        }
    }

    if report.failed > 0 && !batch_opts.continue_on_error {
        return Err(report.errors.join("; "));
    }

    Ok(report)
}

/// Batch removes pages from all PDFs in a folder according to RemovePagesOptions.
pub fn batch_remove_pages_dir_with_options<P: AsRef<Path>, Q: AsRef<Path>>(
    input_dir: P,
    output_dir: Q,
    remove_opts: &RemovePagesOptions,
    batch_opts: &BatchOptions,
) -> Result<BatchReport, String> {
    let pdf_paths = scan_pdf_files(
        &input_dir,
        batch_opts.recursive,
        batch_opts.filter_pattern.as_deref(),
    )?;

    let out_dir = output_dir.as_ref();
    if !batch_opts.dry_run {
        fs::create_dir_all(out_dir).map_err(|e| {
            format!(
                "Failed to create output directory '{}': {e}",
                out_dir.display()
            )
        })?;
    }

    let total = pdf_paths.len();
    let results: Vec<Result<(), String>> = pdf_paths
        .par_iter()
        .map(|pdf_path| {
            if batch_opts.dry_run {
                return Ok(());
            }

            let file_name = pdf_path
                .file_name()
                .ok_or_else(|| format!("Invalid file path: {}", pdf_path.display()))?;
            let out_file = out_dir.join(file_name);

            if out_file.exists() && !batch_opts.overwrite {
                return Ok(());
            }

            let bytes = fs::read(pdf_path)
                .map_err(|e| format!("Failed to read '{}': {e}", pdf_path.display()))?;

            let (cleaned_bytes, _report) = remove_pdf_pages(&bytes, remove_opts).map_err(|e| {
                format!("Failed to remove pages from '{}': {e}", pdf_path.display())
            })?;

            fs::write(&out_file, &cleaned_bytes)
                .map_err(|e| format!("Failed to write output '{}': {e}", out_file.display()))?;
            Ok(())
        })
        .collect();

    let mut report = BatchReport {
        total_scanned: total,
        successful: 0,
        failed: 0,
        errors: Vec::new(),
    };

    for res in results {
        match res {
            Ok(_) => report.successful += 1,
            Err(e) => {
                report.failed += 1;
                report.errors.push(e);
            }
        }
    }

    if report.failed > 0 && !batch_opts.continue_on_error {
        return Err(report.errors.join("; "));
    }

    Ok(report)
}

/// Batch crops pages in all PDFs in a folder according to CropOptions.
pub fn batch_crop_dir_with_options<P: AsRef<Path>, Q: AsRef<Path>>(
    input_dir: P,
    output_dir: Q,
    crop_opts: &CropOptions,
    batch_opts: &BatchOptions,
) -> Result<BatchReport, String> {
    let pdf_paths = scan_pdf_files(
        &input_dir,
        batch_opts.recursive,
        batch_opts.filter_pattern.as_deref(),
    )?;

    let out_dir = output_dir.as_ref();
    if !batch_opts.dry_run {
        fs::create_dir_all(out_dir).map_err(|e| {
            format!(
                "Failed to create output directory '{}': {e}",
                out_dir.display()
            )
        })?;
    }

    let total = pdf_paths.len();
    let results: Vec<Result<(), String>> = pdf_paths
        .par_iter()
        .map(|pdf_path| {
            if batch_opts.dry_run {
                return Ok(());
            }

            let file_name = pdf_path
                .file_name()
                .ok_or_else(|| format!("Invalid file path: {}", pdf_path.display()))?;
            let out_file = out_dir.join(file_name);

            if out_file.exists() && !batch_opts.overwrite {
                return Ok(());
            }

            let bytes = fs::read(pdf_path)
                .map_err(|e| format!("Failed to read '{}': {e}", pdf_path.display()))?;

            let (cropped_bytes, _report) = crop_pdf_pages(&bytes, crop_opts)
                .map_err(|e| format!("Failed to crop '{}': {e}", pdf_path.display()))?;

            fs::write(&out_file, &cropped_bytes)
                .map_err(|e| format!("Failed to write output '{}': {e}", out_file.display()))?;
            Ok(())
        })
        .collect();

    let mut report = BatchReport {
        total_scanned: total,
        successful: 0,
        failed: 0,
        errors: Vec::new(),
    };

    for res in results {
        match res {
            Ok(_) => report.successful += 1,
            Err(e) => {
                report.failed += 1;
                report.errors.push(e);
            }
        }
    }

    if report.failed > 0 && !batch_opts.continue_on_error {
        return Err(report.errors.join("; "));
    }

    Ok(report)
}

/// Batch extracts text from all PDFs in a folder according to TextExtractionOptions.
pub fn batch_extract_text_dir_with_options<P: AsRef<Path>, Q: AsRef<Path>>(
    input_dir: P,
    output_dir: Q,
    text_opts: &TextExtractionOptions,
    save_json: bool,
    batch_opts: &BatchOptions,
) -> Result<BatchReport, String> {
    let pdf_paths = scan_pdf_files(
        &input_dir,
        batch_opts.recursive,
        batch_opts.filter_pattern.as_deref(),
    )?;

    let out_dir = output_dir.as_ref();
    if !batch_opts.dry_run {
        fs::create_dir_all(out_dir).map_err(|e| {
            format!(
                "Failed to create output directory '{}': {e}",
                out_dir.display()
            )
        })?;
    }

    let total = pdf_paths.len();
    let results: Vec<Result<(), String>> = pdf_paths
        .par_iter()
        .map(|pdf_path| {
            if batch_opts.dry_run {
                return Ok(());
            }

            let stem = pdf_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("document");

            let out_ext = if save_json { "json" } else { "txt" };
            let out_file = out_dir.join(format!("{stem}.{out_ext}"));

            if out_file.exists() && !batch_opts.overwrite {
                return Ok(());
            }

            let bytes = fs::read(pdf_path)
                .map_err(|e| format!("Failed to read '{}': {e}", pdf_path.display()))?;

            if save_json {
                let report = extract_text_structured(&bytes, text_opts).map_err(|e| {
                    format!(
                        "Failed to extract structured text from '{}': {e}",
                        pdf_path.display()
                    )
                })?;
                let json_str = serde_json::to_string_pretty(&report).map_err(|e| {
                    format!(
                        "Failed to serialize report for '{}': {e}",
                        pdf_path.display()
                    )
                })?;
                fs::write(&out_file, json_str)
                    .map_err(|e| format!("Failed to write output '{}': {e}", out_file.display()))?;
            } else {
                let text = extract_text(&bytes, text_opts).map_err(|e| {
                    format!("Failed to extract text from '{}': {e}", pdf_path.display())
                })?;
                fs::write(&out_file, text)
                    .map_err(|e| format!("Failed to write output '{}': {e}", out_file.display()))?;
            }

            Ok(())
        })
        .collect();

    let mut report = BatchReport {
        total_scanned: total,
        successful: 0,
        failed: 0,
        errors: Vec::new(),
    };

    for res in results {
        match res {
            Ok(_) => report.successful += 1,
            Err(e) => {
                report.failed += 1;
                report.errors.push(e);
            }
        }
    }

    if report.failed > 0 && !batch_opts.continue_on_error {
        return Err(report.errors.join("; "));
    }

    Ok(report)
}
