use crate::{
    LockedPieceInfoConfig, flatten_form_fields, get_piece_info, insert_locked_piece_info,
    insert_piece_info,
};
use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitRange {
    pub label: String,
    pub pages: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplitOptions {
    #[serde(default = "default_range")]
    pub ranges: String,
    #[serde(default = "default_pattern")]
    pub naming_pattern: String,
    #[serde(default = "default_true")]
    pub inherit_piece_info: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub piece_info: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked_piece_info: Option<LockedPieceInfoConfig>,
    #[serde(default)]
    pub flatten: bool,
}

fn default_range() -> String {
    "all".to_string()
}

fn default_pattern() -> String {
    "{stem}_{label}.pdf".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for SplitOptions {
    fn default() -> Self {
        Self {
            ranges: default_range(),
            naming_pattern: default_pattern(),
            inherit_piece_info: true,
            piece_info: None,
            locked_piece_info: None,
            flatten: false,
        }
    }
}

impl SplitOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ranges(mut self, ranges: impl Into<String>) -> Self {
        self.ranges = ranges.into();
        self
    }

    pub fn naming_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.naming_pattern = pattern.into();
        self
    }

    pub fn inherit_piece_info(mut self, inherit: bool) -> Self {
        self.inherit_piece_info = inherit;
        self
    }

    pub fn piece_info(mut self, piece_info: Value) -> Self {
        self.piece_info = Some(piece_info);
        self
    }

    pub fn locked_piece_info(mut self, locked_piece_info: LockedPieceInfoConfig) -> Self {
        self.locked_piece_info = Some(locked_piece_info);
        self
    }

    pub fn flatten(mut self, flatten: bool) -> Self {
        self.flatten = flatten;
        self
    }
}

/// Helper to render output filename with placeholders.
pub fn format_split_filename(
    pattern: &str,
    stem: &str,
    label: &str,
    index: usize,
    first_page: u32,
) -> String {
    let mut name = pattern.to_string();
    name = name.replace("{stem}", stem);
    name = name.replace("{label}", label);
    name = name.replace("{index:04}", &format!("{:04}", index + 1));
    name = name.replace("{index:03}", &format!("{:03}", index + 1));
    name = name.replace("{index:02}", &format!("{:02}", index + 1));
    name = name.replace("{index}", &format!("{}", index + 1));
    name = name.replace("{page:03}", &format!("{:03}", first_page));
    name = name.replace("{page}", &format!("{}", first_page));
    if !name.ends_with(".pdf") {
        name.push_str(".pdf");
    }
    name
}

/// Parses page range specification.
pub fn parse_split_ranges(spec: &str, total_pages: u32) -> Result<Vec<SplitRange>, String> {
    if total_pages == 0 {
        return Err("PDF has 0 pages".to_string());
    }

    let spec = spec.trim();
    if spec.eq_ignore_ascii_case("all") {
        return Ok((1..=total_pages)
            .map(|p| SplitRange {
                label: format!("page_{p}"),
                pages: vec![p],
            })
            .collect());
    }

    let parts: Vec<&str> = spec
        .split([',', ';'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return Err("Empty split range specification".to_string());
    }

    let mut ranges = Vec::new();

    for part in parts {
        if let Some((start_s, end_s)) = part.split_once('-') {
            let start = if start_s.trim().is_empty() {
                1
            } else {
                start_s
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| format!("Invalid page number: '{start_s}' in range '{part}'"))?
            };
            let end = if end_s.trim().is_empty() {
                total_pages
            } else {
                end_s
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| format!("Invalid page number: '{end_s}' in range '{part}'"))?
            };

            if start == 0 || end == 0 {
                return Err("Page numbers must be 1-indexed (>= 1)".to_string());
            }
            if start > end {
                return Err(format!(
                    "Start page {start} cannot be greater than end page {end}"
                ));
            }
            if end > total_pages {
                return Err(format!(
                    "Requested page {end} exceeds total pages {total_pages}"
                ));
            }

            let pages: Vec<u32> = (start..=end).collect();
            let label = if start == end {
                format!("page_{start}")
            } else {
                format!("pages_{start}_{end}")
            };
            ranges.push(SplitRange { label, pages });
        } else {
            let p = part
                .parse::<u32>()
                .map_err(|_| format!("Invalid page number '{part}'"))?;
            if p == 0 || p > total_pages {
                return Err(format!("Page {p} out of range (1..={total_pages})"));
            }
            ranges.push(SplitRange {
                label: format!("page_{p}"),
                pages: vec![p],
            });
        }
    }

    Ok(ranges)
}

/// Splits a document into multiple documents with customizable options.
pub fn split_document_with_options(
    doc: &Document,
    options: &SplitOptions,
) -> Result<Vec<(String, Document)>, String> {
    let all_pages = doc.get_pages();
    let total_pages = all_pages.len() as u32;
    if total_pages == 0 {
        return Err("PDF document contains no pages".to_string());
    }

    let ranges = parse_split_ranges(&options.ranges, total_pages)?;
    let mut results = Vec::new();

    // Check inherited piece info if needed
    let mut inherited_piece_info = None;
    if options.inherit_piece_info && options.piece_info.is_none() {
        let mut temp_bytes = Vec::new();
        if doc.clone().save_to(&mut temp_bytes).is_ok()
            && let Ok(Some(pi)) = get_piece_info(&temp_bytes)
        {
            inherited_piece_info = Some(pi);
        }
    }

    let piece_info_to_inject = options.piece_info.clone().or(inherited_piece_info);

    for range in ranges {
        let kept_page_ids: Vec<ObjectId> = range
            .pages
            .iter()
            .filter_map(|p| all_pages.get(p).copied())
            .collect();

        if kept_page_ids.is_empty() {
            continue;
        }

        let mut sub_doc = doc.clone();

        if options.flatten {
            let _ = flatten_form_fields(&mut sub_doc, true);
        }

        // Find Catalog and root Pages ID
        let catalog_id = sub_doc
            .trailer
            .get(b"Root")
            .and_then(Object::as_reference)
            .map_err(|e| format!("Invalid PDF trailer Root: {e}"))?;

        let pages_id = {
            let catalog_obj = sub_doc
                .get_object(catalog_id)
                .map_err(|e| format!("Catalog object not found: {e}"))?;
            let catalog_dict = catalog_obj
                .as_dict()
                .map_err(|e| format!("Catalog is not a dictionary: {e}"))?;
            catalog_dict
                .get(b"Pages")
                .and_then(Object::as_reference)
                .map_err(|e| format!("Pages reference not found in Catalog: {e}"))?
        };

        // Determine all page IDs that should be removed
        let kept_set: HashSet<ObjectId> = kept_page_ids.iter().copied().collect();
        for &page_id in all_pages.values() {
            if !kept_set.contains(&page_id) {
                sub_doc.objects.remove(&page_id);
            }
        }

        // Update Pages dictionary with new Kids and Count
        if let Some(pages_obj) = sub_doc.objects.get_mut(&pages_id)
            && let Ok(pages_dict) = pages_obj.as_dict_mut()
        {
            pages_dict.set("Count", kept_page_ids.len() as u32);
            pages_dict.set(
                "Kids",
                kept_page_ids
                    .iter()
                    .copied()
                    .map(Object::Reference)
                    .collect::<Vec<_>>(),
            );
        }

        // Ensure all kept pages have their Parent set to the root Pages ID
        for &page_id in &kept_page_ids {
            if let Some(page_obj) = sub_doc.objects.get_mut(&page_id)
                && let Ok(page_dict) = page_obj.as_dict_mut()
            {
                page_dict.set("Parent", pages_id);
            }
        }

        // Prune unreferenced objects from the deleted pages
        sub_doc.prune_objects();
        sub_doc.renumber_objects();

        // Inject piece info if available
        if let Some(ref pi) = piece_info_to_inject {
            let _ = insert_piece_info(&mut sub_doc, pi);
        }
        if let Some(ref locked_pi) = options.locked_piece_info {
            let _ = insert_locked_piece_info(
                &mut sub_doc,
                &locked_pi.app_name,
                &locked_pi.data,
                &locked_pi.secret_key,
            );
        }

        results.push((range.label, sub_doc));
    }

    Ok(results)
}

/// Splits a document into multiple documents with default options.
pub fn split_document(doc: &Document, range_spec: &str) -> Result<Vec<(String, Document)>, String> {
    let opts = SplitOptions::new().ranges(range_spec);
    split_document_with_options(doc, &opts)
}

/// Splits PDF bytes into multiple PDF byte vectors with customizable options.
pub fn split_pdf_bytes_with_options(
    pdf_bytes: &[u8],
    options: &SplitOptions,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let doc = Document::load_mem(pdf_bytes)
        .map_err(|e| format!("Failed to parse PDF document for splitting: {e}"))?;

    let split_docs = split_document_with_options(&doc, options)?;
    let mut outputs = Vec::with_capacity(split_docs.len());

    for (label, mut sub_doc) in split_docs {
        let mut out = Vec::new();
        sub_doc
            .save_to(&mut out)
            .map_err(|e| format!("Failed to save split PDF part '{label}': {e}"))?;
        outputs.push((label, out));
    }

    Ok(outputs)
}

/// Splits PDF bytes into multiple PDF byte vectors with default options.
pub fn split_pdf_bytes(
    pdf_bytes: &[u8],
    range_spec: &str,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let opts = SplitOptions::new().ranges(range_spec);
    split_pdf_bytes_with_options(pdf_bytes, &opts)
}
