use crate::{
    LockedPieceInfoConfig, flatten_form_fields, insert_locked_piece_info, insert_piece_info,
    ops::PageSelection,
};
use lopdf::content::Content;
use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};

fn default_true() -> bool {
    true
}

fn default_blank_max_bytes() -> usize {
    100
}

fn default_min_retained() -> usize {
    1
}

/// Options to configure smart blank page detection heuristics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlankDetectionOptions {
    /// Maximum content stream bytes to consider potentially blank (default: 100 bytes).
    #[serde(default = "default_blank_max_bytes")]
    pub max_content_bytes: usize,

    /// If true, pages with text containing only whitespace (' ', '\t', '\r', '\n')
    /// are treated as blank (default: true).
    #[serde(default = "default_true")]
    pub ignore_whitespace_only_text: bool,

    /// If true, pages with visible annotations (Widget, Stamp, Ink, Text, Highlight, etc.)
    /// are NOT treated as blank (default: true).
    #[serde(default = "default_true")]
    pub check_annotations: bool,

    /// If true, graphics state setup operators (q, Q, cm, gs) without any stroke,
    /// fill, text, or XObject painting are treated as blank (default: true).
    #[serde(default = "default_true")]
    pub treat_empty_paths_as_blank: bool,
}

impl Default for BlankDetectionOptions {
    fn default() -> Self {
        Self {
            max_content_bytes: default_blank_max_bytes(),
            ignore_whitespace_only_text: true,
            check_annotations: true,
            treat_empty_paths_as_blank: true,
        }
    }
}

impl BlankDetectionOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn max_content_bytes(mut self, bytes: usize) -> Self {
        self.max_content_bytes = bytes;
        self
    }

    pub fn ignore_whitespace_only_text(mut self, ignore: bool) -> Self {
        self.ignore_whitespace_only_text = ignore;
        self
    }

    pub fn check_annotations(mut self, check: bool) -> Self {
        self.check_annotations = check;
        self
    }

    pub fn treat_empty_paths_as_blank(mut self, treat: bool) -> Self {
        self.treat_empty_paths_as_blank = treat;
        self
    }
}

/// Options for removing pages from a PDF document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemovePagesOptions {
    /// Remove the first page (cover). Default: false.
    #[serde(default)]
    pub remove_cover: bool,

    /// Remove the last page (back cover). Default: false.
    #[serde(default)]
    pub remove_back_cover: bool,

    /// Remove detected blank/empty pages. Default: false.
    #[serde(default)]
    pub remove_blank_pages: bool,

    /// Configuration for blank page detection heuristics.
    #[serde(default)]
    pub blank_detection: BlankDetectionOptions,

    /// Explicit pages or range to remove (1-indexed, e.g. "2", "3-5", "odd", "even").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<PageSelection>,

    /// Explicit pages or range to keep (whitelist override). Pages matching this
    /// are protected and will never be removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_pages: Option<PageSelection>,

    /// Minimum number of pages that must be retained (safety guard, default: 1).
    /// If removal would leave fewer pages than this, an error is returned.
    #[serde(default = "default_min_retained")]
    pub min_pages_retained: usize,

    /// Flatten form fields before saving. Default: false.
    #[serde(default)]
    pub flatten: bool,

    /// Optional PieceInfo metadata JSON to embed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub piece_info: Option<serde_json::Value>,

    /// Optional cryptographic locked PieceInfo configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked_piece_info: Option<LockedPieceInfoConfig>,
}

impl Default for RemovePagesOptions {
    fn default() -> Self {
        Self {
            remove_cover: false,
            remove_back_cover: false,
            remove_blank_pages: false,
            blank_detection: BlankDetectionOptions::default(),
            pages: None,
            keep_pages: None,
            min_pages_retained: default_min_retained(),
            flatten: false,
            piece_info: None,
            locked_piece_info: None,
        }
    }
}

impl RemovePagesOptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set whether to remove the first page (cover).
    pub fn remove_cover(mut self, remove: bool) -> Self {
        self.remove_cover = remove;
        self
    }

    /// Set whether to remove the last page (back cover).
    pub fn remove_back_cover(mut self, remove: bool) -> Self {
        self.remove_back_cover = remove;
        self
    }

    /// Set whether to automatically detect and remove blank pages.
    pub fn remove_blank_pages(mut self, remove: bool) -> Self {
        self.remove_blank_pages = remove;
        self
    }

    /// Customize blank detection options.
    pub fn blank_detection(mut self, opts: BlankDetectionOptions) -> Self {
        self.blank_detection = opts;
        self
    }

    /// Specify pages to remove using `PageSelection`.
    pub fn pages(mut self, selection: PageSelection) -> Self {
        self.pages = Some(selection);
        self
    }

    /// Specify pages to remove via range string (e.g. "2", "3-5", "odd", "even").
    pub fn pages_str(mut self, s: &str) -> Self {
        self.pages = Some(PageSelection::parse(s));
        self
    }

    /// Specify protected pages to keep using `PageSelection`.
    pub fn keep_pages(mut self, selection: PageSelection) -> Self {
        self.keep_pages = Some(selection);
        self
    }

    /// Specify protected pages to keep via range string.
    pub fn keep_pages_str(mut self, s: &str) -> Self {
        self.keep_pages = Some(PageSelection::parse(s));
        self
    }

    /// Set safety minimum retained pages threshold (default: 1).
    pub fn min_pages_retained(mut self, min: usize) -> Self {
        self.min_pages_retained = min;
        self
    }

    /// Flatten form fields after page removal.
    pub fn flatten(mut self, flatten: bool) -> Self {
        self.flatten = flatten;
        self
    }

    /// Embed PieceInfo metadata JSON.
    pub fn piece_info(mut self, piece_info: serde_json::Value) -> Self {
        self.piece_info = Some(piece_info);
        self
    }

    /// Embed cryptographic locked PieceInfo.
    pub fn locked_piece_info(
        mut self,
        app_name: impl Into<String>,
        data: serde_json::Value,
        secret_key: impl Into<String>,
    ) -> Self {
        self.locked_piece_info = Some(LockedPieceInfoConfig {
            app_name: app_name.into(),
            data,
            secret_key: secret_key.into(),
        });
        self
    }
}

/// Execution report returned after page removal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRemovalReport {
    pub original_pages: u32,
    pub retained_pages: u32,
    pub removed_pages: Vec<u32>,
    pub blank_pages_detected: Vec<u32>,
    pub cover_removed: bool,
    pub back_cover_removed: bool,
}

// ---------------------------------------------------------------------------
// Smart Blank Page Detection
// ---------------------------------------------------------------------------

fn has_visible_annotations(doc: &Document, page_id: ObjectId) -> bool {
    let page_obj = match doc.get_object(page_id) {
        Ok(obj) => obj,
        Err(_) => return false,
    };
    let page_dict = match page_obj.as_dict() {
        Ok(dict) => dict,
        Err(_) => return false,
    };

    let annots = match page_dict.get(b"Annots") {
        Ok(Object::Array(arr)) => arr,
        Ok(Object::Reference(ref_id)) => {
            if let Ok(Object::Array(arr)) = doc.get_object(*ref_id) {
                arr
            } else {
                return false;
            }
        }
        _ => return false,
    };

    for annot_ref in annots {
        let annot_obj = match annot_ref {
            Object::Reference(id) => doc.get_object(*id).ok(),
            Object::Dictionary(_) => Some(annot_ref),
            _ => None,
        };

        if let Some(obj) = annot_obj {
            if let Ok(dict) = obj.as_dict() {
                if let Ok(subtype) = dict.get(b"Subtype").and_then(Object::as_name) {
                    // Visual annotation types that render content on screen
                    match subtype {
                        b"Widget" | b"Stamp" | b"Ink" | b"FreeText" | b"Text" | b"Highlight"
                        | b"Underline" | b"StrikeOut" | b"Squiggly" | b"Square" | b"Circle"
                        | b"Line" | b"Polygon" | b"PolyLine" | b"Caret" | b"Redact" => {
                            return true;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    false
}

fn is_whitespace_string(bytes: &[u8]) -> bool {
    bytes.iter().all(|&b| b.is_ascii_whitespace())
}

/// Inspects whether a given page in the PDF document is blank according to detection options.
pub fn is_page_blank(doc: &Document, page_id: ObjectId, options: &BlankDetectionOptions) -> bool {
    if options.check_annotations && has_visible_annotations(doc, page_id) {
        return false;
    }

    let content_bytes = doc.get_page_content(page_id);
    if content_bytes.is_empty() {
        return true;
    }

    // Quick whitespace check
    if is_whitespace_string(&content_bytes) {
        return true;
    }

    let content = match Content::decode(&content_bytes) {
        Ok(c) => c,
        Err(_) => {
            // If cannot decode, check if content byte length is within minimal threshold
            return content_bytes.len() <= options.max_content_bytes
                && is_whitespace_string(&content_bytes);
        }
    };

    let mut has_painting = false;

    for op in &content.operations {
        match op.operator.as_str() {
            // Path painting operators
            "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "sh" | "Do" | "BI" => {
                has_painting = true;
                break;
            }

            // Text painting operators
            "Tj" | "'" => {
                if let Some(s) = op.operands.first() {
                    match s {
                        Object::String(bytes, _) => {
                            if !options.ignore_whitespace_only_text || !is_whitespace_string(bytes)
                            {
                                has_painting = true;
                                break;
                            }
                        }
                        _ => {
                            has_painting = true;
                            break;
                        }
                    }
                }
            }

            "\"" => {
                if let Some(s) = op.operands.get(2) {
                    match s {
                        Object::String(bytes, _) => {
                            if !options.ignore_whitespace_only_text || !is_whitespace_string(bytes)
                            {
                                has_painting = true;
                                break;
                            }
                        }
                        _ => {
                            has_painting = true;
                            break;
                        }
                    }
                }
            }

            "TJ" => {
                if let Some(Object::Array(arr)) = op.operands.first() {
                    let mut visible_text = false;
                    for item in arr {
                        if let Object::String(bytes, _) = item {
                            if !options.ignore_whitespace_only_text || !is_whitespace_string(bytes)
                            {
                                visible_text = true;
                                break;
                            }
                        }
                    }
                    if visible_text {
                        has_painting = true;
                        break;
                    }
                }
            }

            _ => {}
        }
    }

    !has_painting
}

// ---------------------------------------------------------------------------
// Page Removal Execution
// ---------------------------------------------------------------------------

/// Removes pages from a PDF byte buffer according to the given options.
pub fn remove_pdf_pages(
    pdf_bytes: &[u8],
    options: &RemovePagesOptions,
) -> Result<(Vec<u8>, PageRemovalReport), String> {
    let mut doc = Document::load_mem(pdf_bytes).map_err(|e| format!("Failed to parse PDF: {e}"))?;
    let report = remove_pdf_pages_in_doc(&mut doc, options)?;
    let mut out_bytes = Vec::new();
    doc.save_to(&mut out_bytes)
        .map_err(|e| format!("Failed to serialize PDF: {e}"))?;
    Ok((out_bytes, report))
}

/// Removes pages directly in an existing lopdf Document.
pub fn remove_pdf_pages_in_doc(
    doc: &mut Document,
    options: &RemovePagesOptions,
) -> Result<PageRemovalReport, String> {
    let all_pages = doc.get_pages();
    let total_pages = all_pages.len() as u32;

    if total_pages == 0 {
        return Err("PDF document contains no pages".to_string());
    }

    let mut blank_pages = Vec::new();
    for (&p_num, &p_id) in &all_pages {
        if is_page_blank(doc, p_id, &options.blank_detection) {
            blank_pages.push(p_num);
        }
    }

    let mut to_remove_set = BTreeSet::new();

    for (&p_num, _) in &all_pages {
        let mut should_remove = false;

        if options.remove_cover && p_num == 1 {
            should_remove = true;
        }

        if options.remove_back_cover && p_num == total_pages {
            should_remove = true;
        }

        if options.remove_blank_pages && blank_pages.contains(&p_num) {
            should_remove = true;
        }

        if let Some(ref sel) = options.pages {
            if sel.matches(p_num, total_pages) {
                should_remove = true;
            }
        }

        // Whitelist / Keep protection override
        if let Some(ref keep_sel) = options.keep_pages {
            if keep_sel.matches(p_num, total_pages) {
                should_remove = false;
            }
        }

        if should_remove {
            to_remove_set.insert(p_num);
        }
    }

    let cover_removed = to_remove_set.contains(&1);
    let back_cover_removed = to_remove_set.contains(&total_pages);
    let retained_count = total_pages.saturating_sub(to_remove_set.len() as u32);

    if (retained_count as usize) < options.min_pages_retained {
        return Err(format!(
            "Removing specified pages would leave {} page(s), but min_pages_retained is {}",
            retained_count, options.min_pages_retained
        ));
    }

    let removed_pages: Vec<u32> = to_remove_set.iter().copied().collect();

    // If nothing to remove, return report early
    if to_remove_set.is_empty() {
        if options.flatten {
            flatten_form_fields(doc, true)?;
        }
        if let Some(ref pi) = options.piece_info {
            insert_piece_info(doc, pi)?;
        }
        if let Some(ref lpi) = options.locked_piece_info {
            insert_locked_piece_info(doc, &lpi.app_name, &lpi.data, &lpi.secret_key)?;
        }
        return Ok(PageRemovalReport {
            original_pages: total_pages,
            retained_pages: total_pages,
            removed_pages,
            blank_pages_detected: blank_pages,
            cover_removed,
            back_cover_removed,
        });
    }

    // Locate Catalog and root Pages dictionary
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|e| format!("Invalid PDF trailer Root: {e}"))?;

    let pages_id = {
        let catalog_obj = doc
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

    let mut kept_page_ids = Vec::new();
    let mut removed_page_ids = HashSet::new();

    for (&p_num, &p_id) in &all_pages {
        if to_remove_set.contains(&p_num) {
            removed_page_ids.insert(p_id);
            doc.objects.remove(&p_id);
        } else {
            kept_page_ids.push(p_id);
        }
    }

    // Clean up AcroForm fields pointing to removed pages
    let mut fields_to_remove = HashSet::new();
    for (&id, obj) in &doc.objects {
        if let Ok(dict) = obj.as_dict() {
            if let Ok(p_ref) = dict.get(b"P").and_then(Object::as_reference) {
                if removed_page_ids.contains(&p_ref) {
                    fields_to_remove.insert(id);
                }
            }
        }
    }

    if let Ok(catalog_dict) = doc.get_object(catalog_id).and_then(|o| o.as_dict()) {
        if let Ok(acro_ref) = catalog_dict.get(b"AcroForm").and_then(Object::as_reference) {
            if let Ok(acro_dict) = doc.get_object_mut(acro_ref).and_then(|o| o.as_dict_mut()) {
                if let Ok(Object::Array(fields)) = acro_dict.get_mut(b"Fields") {
                    fields.retain(|field_obj| {
                        if let Ok(field_ref) = field_obj.as_reference() {
                            !fields_to_remove.contains(&field_ref)
                        } else {
                            true
                        }
                    });
                }
            }
        }
    }

    // Update root Pages dictionary
    if let Some(pages_obj) = doc.objects.get_mut(&pages_id) {
        if let Ok(pages_dict) = pages_obj.as_dict_mut() {
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
    }

    // Ensure all kept pages have their Parent pointing to root Pages ID
    for &page_id in &kept_page_ids {
        if let Some(page_obj) = doc.objects.get_mut(&page_id) {
            if let Ok(page_dict) = page_obj.as_dict_mut() {
                page_dict.set("Parent", pages_id);
            }
        }
    }

    // Clean up unreferenced objects and compact
    doc.prune_objects();
    doc.renumber_objects();

    if options.flatten {
        flatten_form_fields(doc, true)?;
    }

    if let Some(ref pi) = options.piece_info {
        insert_piece_info(doc, pi)?;
    }

    if let Some(ref lpi) = options.locked_piece_info {
        insert_locked_piece_info(doc, &lpi.app_name, &lpi.data, &lpi.secret_key)?;
    }

    Ok(PageRemovalReport {
        original_pages: total_pages,
        retained_pages: kept_page_ids.len() as u32,
        removed_pages,
        blank_pages_detected: blank_pages,
        cover_removed,
        back_cover_removed,
    })
}
