use crate::{
    LockedPieceInfoConfig, flatten_form_fields, insert_locked_piece_info, insert_piece_info,
};
use lopdf::{Bookmark, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PageMode {
    #[default]
    UseOutlines,
    UseThumbs,
    FullScreen,
    UseNone,
}

impl PageMode {
    pub fn as_pdf_name(&self) -> &'static str {
        match self {
            Self::UseOutlines => "UseOutlines",
            Self::UseThumbs => "UseThumbs",
            Self::FullScreen => "FullScreen",
            Self::UseNone => "UseNone",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MergeOptions {
    #[serde(default = "default_true")]
    pub create_bookmarks: bool,
    #[serde(default)]
    pub page_mode: PageMode,
    #[serde(default)]
    pub flatten: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub piece_info: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked_piece_info: Option<LockedPieceInfoConfig>,
}

fn default_true() -> bool {
    true
}

impl MergeOptions {
    pub fn new() -> Self {
        Self {
            create_bookmarks: true,
            page_mode: PageMode::UseOutlines,
            flatten: false,
            piece_info: None,
            locked_piece_info: None,
        }
    }

    pub fn create_bookmarks(mut self, enabled: bool) -> Self {
        self.create_bookmarks = enabled;
        self
    }

    pub fn page_mode(mut self, mode: PageMode) -> Self {
        self.page_mode = mode;
        self
    }

    pub fn flatten(mut self, flatten: bool) -> Self {
        self.flatten = flatten;
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
}

/// Merges multiple PDF documents into a single document with customizable options.
pub fn merge_documents_with_options(
    docs: &[Document],
    options: &MergeOptions,
) -> Result<Document, String> {
    if docs.is_empty() {
        return Err("No documents provided for merging".to_string());
    }

    let mut processed_docs = Vec::with_capacity(docs.len());
    for doc in docs {
        let mut d = doc.clone();
        if options.flatten {
            let _ = flatten_form_fields(&mut d, true);
        }
        processed_docs.push(d);
    }

    let mut merged = Document::with_version("1.7");
    let mut documents_pages: BTreeMap<ObjectId, Object> = BTreeMap::new();
    let mut documents_objects: BTreeMap<ObjectId, Object> = BTreeMap::new();
    let mut page_ids_in_order: Vec<ObjectId> = Vec::new();
    let mut first_page_of_doc: Vec<ObjectId> = Vec::new();

    let mut max_id = 1;

    for (doc_idx, doc) in processed_docs.iter_mut().enumerate() {
        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;

        let pages = doc.get_pages();
        let mut doc_first_page = None;
        for (_page_num, object_id) in pages {
            page_ids_in_order.push(object_id);
            if doc_first_page.is_none() {
                doc_first_page = Some(object_id);
            }
            if let Ok(obj) = doc.get_object(object_id) {
                documents_pages.insert(object_id, obj.clone());
            }
        }

        if let Some(first_id) = doc_first_page {
            first_page_of_doc.push(first_id);
            if options.create_bookmarks {
                let title = format!("Document {}", doc_idx + 1);
                merged.add_bookmark(Bookmark::new(title, [0.0, 0.0, 0.0], 0, first_id), None);
            }
        }

        documents_objects.extend(doc.objects.clone());
    }

    let mut catalog_object: Option<(ObjectId, Object)> = None;
    let mut pages_object: Option<(ObjectId, Object)> = None;

    // Filter objects: Catalog & Pages are unified
    for (object_id, object) in documents_objects {
        match object.type_name().unwrap_or(b"") {
            b"Catalog" => {
                if catalog_object.is_none() {
                    catalog_object = Some((object_id, object));
                }
            }
            b"Pages" => {
                if let Ok(dictionary) = object.as_dict() {
                    let mut dictionary = dictionary.clone();
                    if let Some((_, ref old_pages_obj)) = pages_object {
                        if let Ok(old_dictionary) = old_pages_obj.as_dict() {
                            dictionary.extend(old_dictionary);
                        }
                    }
                    pages_object = Some((
                        if let Some((id, _)) = pages_object {
                            id
                        } else {
                            object_id
                        },
                        Object::Dictionary(dictionary),
                    ));
                }
            }
            b"Page" | b"Outlines" | b"Outline" => {}
            _ => {
                merged.objects.insert(object_id, object);
            }
        }
    }

    let pages_id = if let Some((pid, _)) = pages_object {
        pid
    } else {
        (max_id, 0)
    };
    max_id += 1;

    let catalog_id = if let Some((cid, _)) = catalog_object {
        cid
    } else {
        (max_id, 0)
    };

    // Link each Page dictionary's Parent to pages_id
    for (object_id, object) in documents_pages {
        if let Ok(dictionary) = object.as_dict() {
            let mut dictionary = dictionary.clone();
            dictionary.set("Parent", pages_id);
            merged
                .objects
                .insert(object_id, Object::Dictionary(dictionary));
        }
    }

    // Build root Pages object
    let mut pages_dict = lopdf::Dictionary::new();
    pages_dict.set("Type", Object::Name(b"Pages".to_vec()));
    pages_dict.set("Count", page_ids_in_order.len() as u32);
    pages_dict.set(
        "Kids",
        page_ids_in_order
            .iter()
            .copied()
            .map(Object::Reference)
            .collect::<Vec<_>>(),
    );
    merged
        .objects
        .insert(pages_id, Object::Dictionary(pages_dict));

    // Build Catalog object
    let mut catalog_dict = lopdf::Dictionary::new();
    catalog_dict.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog_dict.set("Pages", pages_id);
    catalog_dict.set("PageMode", options.page_mode.as_pdf_name());
    catalog_dict.remove(b"Outlines");

    // Collect and unify AcroForm fields across all documents if not flattening
    if !options.flatten {
        let mut combined_fields: Vec<Object> = Vec::new();
        let mut base_acroform_dict: Option<lopdf::Dictionary> = None;

        for doc in &processed_docs {
            if let Ok(root_id) = doc.trailer.get(b"Root").and_then(Object::as_reference) {
                if let Ok(c_dict) = doc.get_object(root_id).and_then(Object::as_dict) {
                    if let Ok(af_val) = c_dict.get(b"AcroForm") {
                        let af_dict = match af_val {
                            Object::Reference(ref_id) => {
                                doc.get_object(*ref_id).and_then(Object::as_dict).ok()
                            }
                            Object::Dictionary(d) => Some(d),
                            _ => None,
                        };
                        if let Some(d) = af_dict {
                            if base_acroform_dict.is_none() {
                                base_acroform_dict = Some(d.clone());
                            }
                            if let Ok(fields) = d.get(b"Fields").and_then(Object::as_array) {
                                for f in fields {
                                    if !combined_fields.contains(f) {
                                        combined_fields.push(f.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(mut af) = base_acroform_dict {
            if !combined_fields.is_empty() {
                af.set("Fields", Object::Array(combined_fields));
            }
            let af_id = (max_id, 0);
            merged.objects.insert(af_id, Object::Dictionary(af));
            catalog_dict.set("AcroForm", Object::Reference(af_id));
        }
    }

    merged
        .objects
        .insert(catalog_id, Object::Dictionary(catalog_dict));

    merged.trailer.set("Root", catalog_id);
    merged.max_id = merged.objects.len() as u32 + 2;
    merged.renumber_objects();

    // Build outlines/bookmarks if enabled
    if options.create_bookmarks {
        merged.adjust_zero_pages();
        if let Some(outline_id) = merged.build_outline() {
            if let Ok(Object::Dictionary(dict)) = merged.get_object_mut(catalog_id) {
                dict.set("Outlines", Object::Reference(outline_id));
            }
        }
    }

    // Inject PieceInfo if requested
    if let Some(ref pi) = options.piece_info {
        insert_piece_info(&mut merged, pi)?;
    }

    // Inject Locked PieceInfo if requested
    if let Some(ref locked_pi) = options.locked_piece_info {
        insert_locked_piece_info(
            &mut merged,
            &locked_pi.app_name,
            &locked_pi.data,
            &locked_pi.secret_key,
        )?;
    }

    Ok(merged)
}

/// Merges multiple PDF documents into a single document with default options.
pub fn merge_documents(docs: &[Document]) -> Result<Document, String> {
    merge_documents_with_options(docs, &MergeOptions::default())
}

/// Merges multiple PDF byte slices into a single PDF byte vector with customizable options.
pub fn merge_pdf_bytes_with_options(
    pdf_bytes_list: &[&[u8]],
    options: &MergeOptions,
) -> Result<Vec<u8>, String> {
    if pdf_bytes_list.is_empty() {
        return Err("No PDF byte slices provided for merging".to_string());
    }

    let mut docs = Vec::with_capacity(pdf_bytes_list.len());
    for (idx, bytes) in pdf_bytes_list.iter().enumerate() {
        let doc = Document::load_mem(bytes)
            .map_err(|e| format!("Failed to parse PDF document at index {idx}: {e}"))?;
        docs.push(doc);
    }

    let mut merged_doc = merge_documents_with_options(&docs, options)?;
    let mut out = Vec::new();
    merged_doc
        .save_to(&mut out)
        .map_err(|e| format!("Failed to save merged PDF: {e}"))?;
    Ok(out)
}

/// Merges multiple PDF byte slices into a single PDF byte vector with default options.
pub fn merge_pdf_bytes(pdf_bytes_list: &[&[u8]]) -> Result<Vec<u8>, String> {
    merge_pdf_bytes_with_options(pdf_bytes_list, &MergeOptions::default())
}
