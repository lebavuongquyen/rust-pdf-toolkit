use lopdf::{Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractImageOptions {
    /// Specific 1-based page numbers to extract images from.
    /// If None, extracts from all pages.
    pub pages: Option<Vec<u32>>,
    /// Minimum width in pixels to include (filters out small icons/dots).
    pub min_width: u32,
    /// Minimum height in pixels to include (filters out small icons/dots).
    pub min_height: u32,
    /// Custom filename template, e.g. "image_p{page}_{index}.{ext}" or "{id}_{index}.{ext}".
    pub naming_pattern: Option<String>,
    /// If true, identical images (by PDF ObjectId) will only be extracted once. Default: true.
    pub deduplicate: bool,
}

impl Default for ExtractImageOptions {
    fn default() -> Self {
        Self {
            pages: None,
            min_width: 0,
            min_height: 0,
            naming_pattern: None,
            deduplicate: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedImage {
    pub page: u32,
    pub object_id: (u32, u16),
    pub width: u32,
    pub height: u32,
    pub format: String, // "jpg", "png", "jp2"
    pub file_name: String,
    #[serde(skip_serializing)]
    pub data: Vec<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_base64: Option<String>,
}

/// Extracts embedded images from raw PDF bytes.
pub fn extract_images_from_bytes(
    pdf_bytes: &[u8],
    options: &ExtractImageOptions,
) -> Result<Vec<ExtractedImage>, String> {
    let doc = Document::load_mem(pdf_bytes).map_err(|e| format!("Failed to parse PDF: {}", e))?;
    extract_images_from_doc(&doc, options)
}

/// Extracts embedded images from an existing lopdf Document.
pub fn extract_images_from_doc(
    doc: &Document,
    options: &ExtractImageOptions,
) -> Result<Vec<ExtractedImage>, String> {
    let pages_map = doc.get_pages();
    let total_pages = pages_map.len() as u32;

    let target_pages: Vec<u32> = match &options.pages {
        Some(list) => list
            .iter()
            .copied()
            .filter(|&p| p >= 1 && p <= total_pages)
            .collect(),
        None => (1..=total_pages).collect(),
    };

    let mut extracted = Vec::new();
    let mut seen_objects: HashSet<ObjectId> = HashSet::new();
    let mut image_counter = 0usize;

    for &page_num in &target_pages {
        if let Some(&page_id) = pages_map.get(&page_num) {
            let mut page_image_objects = Vec::new();
            collect_page_images(doc, page_id, &mut page_image_objects);

            for (obj_id, stream) in page_image_objects {
                if options.deduplicate && seen_objects.contains(&obj_id) {
                    continue;
                }

                if let Some(img) =
                    process_image_stream(doc, obj_id, stream, page_num, image_counter + 1, options)?
                {
                    seen_objects.insert(obj_id);
                    image_counter += 1;
                    extracted.push(img);
                }
            }
        }
    }

    Ok(extracted)
}

fn collect_page_images<'a>(
    doc: &'a Document,
    page_id: ObjectId,
    collected: &mut Vec<(ObjectId, &'a Stream)>,
) {
    let page_obj = match doc.get_object(page_id) {
        Ok(obj) => obj,
        Err(_) => return,
    };
    let page_dict = match page_obj.as_dict() {
        Ok(dict) => dict,
        Err(_) => return,
    };

    // 1. Scan direct Page /Resources
    if let Ok(res_obj) = page_dict.get(b"Resources") {
        collect_images_from_resources(doc, res_obj, collected, 0);
    }

    // 2. Scan Page /Annots (Widgets, Stamps, Appearances)
    if let Ok(annots_obj) = page_dict.get(b"Annots") {
        collect_images_from_annots(doc, annots_obj, collected, 0);
    }
}

fn collect_images_from_resources<'a>(
    doc: &'a Document,
    res_obj: &'a Object,
    collected: &mut Vec<(ObjectId, &'a Stream)>,
    depth: usize,
) {
    if depth > 8 {
        return;
    }

    let res_dict = match res_obj {
        Object::Dictionary(dict) => dict,
        Object::Reference(id) => match doc.get_object(*id) {
            Ok(Object::Dictionary(dict)) => dict,
            _ => return,
        },
        _ => return,
    };

    if let Ok(xobject_obj) = res_dict.get(b"XObject") {
        let xobj_dict = match xobject_obj {
            Object::Dictionary(dict) => Some(dict),
            Object::Reference(id) => match doc.get_object(*id) {
                Ok(Object::Dictionary(dict)) => Some(dict),
                _ => None,
            },
            _ => None,
        };

        if let Some(dict) = xobj_dict {
            for (_name, val) in dict.iter() {
                let (target_id, target_obj) = match val {
                    Object::Reference(id) => match doc.get_object(*id) {
                        Ok(obj) => (*id, obj),
                        _ => continue,
                    },
                    _ => continue,
                };

                if let Object::Stream(stream) = target_obj {
                    let subtype = stream
                        .dict
                        .get(b"Subtype")
                        .ok()
                        .and_then(|x| x.as_name().ok());
                    if subtype == Some(b"Image") {
                        if !collected.iter().any(|(id, _)| *id == target_id) {
                            collected.push((target_id, stream));
                        }
                    } else if subtype == Some(b"Form") {
                        // Recursively check Form XObject resources
                        if let Ok(form_res) = stream.dict.get(b"Resources") {
                            collect_images_from_resources(doc, form_res, collected, depth + 1);
                        }
                    }
                }
            }
        }
    }
}

fn collect_images_from_annots<'a>(
    doc: &'a Document,
    annots_obj: &'a Object,
    collected: &mut Vec<(ObjectId, &'a Stream)>,
    depth: usize,
) {
    if depth > 8 {
        return;
    }

    let annots_list = match annots_obj {
        Object::Array(arr) => arr,
        Object::Reference(id) => match doc.get_object(*id) {
            Ok(Object::Array(arr)) => arr,
            _ => return,
        },
        _ => return,
    };

    for annot_ref in annots_list {
        let annot_dict = match annot_ref {
            Object::Dictionary(dict) => Some(dict),
            Object::Reference(id) => match doc.get_object(*id) {
                Ok(Object::Dictionary(dict)) => Some(dict),
                _ => None,
            },
            _ => None,
        };

        if let Some(dict) = annot_dict {
            // Check /AP (Appearance dictionary)
            if let Ok(ap_obj) = dict.get(b"AP") {
                let ap_dict = match ap_obj {
                    Object::Dictionary(d) => Some(d),
                    Object::Reference(id) => match doc.get_object(*id) {
                        Ok(Object::Dictionary(d)) => Some(d),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(d) = ap_dict {
                    for key in &[b"N", b"D", b"R"] {
                        if let Ok(ap_sub) = d.get(*key) {
                            collect_from_ap_sub(doc, ap_sub, collected, depth + 1);
                        }
                    }
                }
            }

            // Check /MK /I (Icon)
            if let Ok(mk_obj) = dict.get(b"MK") {
                let mk_dict = match mk_obj {
                    Object::Dictionary(d) => Some(d),
                    Object::Reference(id) => match doc.get_object(*id) {
                        Ok(Object::Dictionary(d)) => Some(d),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(d) = mk_dict {
                    if let Ok(icon_obj) = d.get(b"I") {
                        collect_from_ap_sub(doc, icon_obj, collected, depth + 1);
                    }
                }
            }
        }
    }
}

fn collect_from_ap_sub<'a>(
    doc: &'a Document,
    sub_obj: &'a Object,
    collected: &mut Vec<(ObjectId, &'a Stream)>,
    depth: usize,
) {
    if depth > 8 {
        return;
    }
    match sub_obj {
        Object::Reference(id) => {
            if let Ok(obj) = doc.get_object(*id) {
                match obj {
                    Object::Stream(stream) => {
                        let subtype = stream
                            .dict
                            .get(b"Subtype")
                            .ok()
                            .and_then(|x| x.as_name().ok());
                        if subtype == Some(b"Image") {
                            if !collected.iter().any(|(cid, _)| *cid == *id) {
                                collected.push((*id, stream));
                            }
                        } else if subtype == Some(b"Form") {
                            if let Ok(res) = stream.dict.get(b"Resources") {
                                collect_images_from_resources(doc, res, collected, depth + 1);
                            }
                        }
                    }
                    Object::Dictionary(_) => {
                        collect_images_from_resources(doc, obj, collected, depth + 1);
                    }
                    _ => {}
                }
            }
        }
        Object::Dictionary(_) => {
            collect_images_from_resources(doc, sub_obj, collected, depth + 1);
        }
        _ => {}
    }
}

fn process_image_stream(
    doc: &Document,
    obj_id: ObjectId,
    stream: &Stream,
    page: u32,
    index: usize,
    options: &ExtractImageOptions,
) -> Result<Option<ExtractedImage>, String> {
    let width = stream
        .dict
        .get(b"Width")
        .ok()
        .and_then(|x| x.as_i64().ok())
        .unwrap_or(0) as u32;
    let height = stream
        .dict
        .get(b"Height")
        .ok()
        .and_then(|x| x.as_i64().ok())
        .unwrap_or(0) as u32;

    if width < options.min_width || height < options.min_height {
        return Ok(None);
    }

    let filter_name = stream.dict.get(b"Filter").ok().and_then(|x| match x {
        Object::Name(n) => Some(n.as_slice()),
        Object::Array(arr) => arr.first().and_then(|first| first.as_name().ok()),
        _ => None,
    });

    let (format, data) =
        if stream.content.starts_with(&[0xff, 0xd8]) || filter_name == Some(b"DCTDecode") {
            // Raw JPEG stream: already valid JPEG
            ("jpg".to_string(), stream.content.clone())
        } else if filter_name == Some(b"JPXDecode") {
            // Raw JPEG 2000
            ("jp2".to_string(), stream.content.clone())
        } else {
            // Decompress & convert raster to PNG
            let raw = stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone());

            // Decode into PNG
            let png_bytes = raster_to_png(doc, &stream.dict, &raw, width, height)?;
            ("png".to_string(), png_bytes)
        };

    let ext = &format;
    let file_name = if let Some(ref pattern) = options.naming_pattern {
        pattern
            .replace("{page}", &format!("{:03}", page))
            .replace("{index}", &format!("{:03}", index))
            .replace("{id}", &format!("{}_{}", obj_id.0, obj_id.1))
            .replace("{ext}", ext)
    } else {
        format!("image_p{:03}_{:03}.{}", page, index, ext)
    };

    Ok(Some(ExtractedImage {
        page,
        object_id: (obj_id.0, obj_id.1),
        width,
        height,
        format,
        file_name,
        data,
        data_base64: None,
    }))
}

fn raster_to_png(
    doc: &Document,
    dict: &lopdf::Dictionary,
    raw: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 {
        return Err("Invalid image dimensions (0x0)".to_string());
    }

    let color_space = dict
        .get(b"ColorSpace")
        .ok()
        .and_then(|cs| match cs {
            Object::Name(n) => Some(n.as_slice()),
            Object::Array(arr) => arr.first().and_then(|f| f.as_name().ok()),
            _ => None,
        })
        .unwrap_or(b"DeviceRGB");

    let bpc = dict
        .get(b"BitsPerComponent")
        .ok()
        .and_then(|x| x.as_i64().ok())
        .unwrap_or(8) as u32;

    // Check optional Soft Mask (/SMask) for alpha channel
    let smask_alpha: Option<Vec<u8>> = dict.get(b"SMask").ok().and_then(|smask_obj| {
        let smask_id = match smask_obj {
            Object::Reference(id) => Some(*id),
            _ => None,
        }?;
        let smask_stream = doc.get_object(smask_id).ok()?.as_stream().ok()?;
        let decompressed = smask_stream.decompressed_content().ok()?;
        Some(decompressed)
    });

    let pixel_count = (width * height) as usize;

    if color_space == b"DeviceGray" {
        if let Some(alpha) = smask_alpha {
            if raw.len() >= pixel_count && alpha.len() >= pixel_count {
                let mut rgba = Vec::with_capacity(pixel_count * 4);
                for i in 0..pixel_count {
                    let g = raw[i];
                    let a = alpha[i];
                    rgba.extend_from_slice(&[g, g, g, a]);
                }
                return encode_image_buffer_to_png(
                    image::RgbaImage::from_raw(width, height, rgba)
                        .ok_or("Failed to create RGBA buffer")?,
                );
            }
        }
        if raw.len() >= pixel_count {
            let gray_buf = raw[..pixel_count].to_vec();
            return encode_gray_buffer_to_png(
                image::GrayImage::from_raw(width, height, gray_buf)
                    .ok_or("Failed to create Gray buffer")?,
            );
        }
    } else if color_space == b"DeviceCMYK" {
        if raw.len() >= pixel_count * 4 {
            let mut rgb = Vec::with_capacity(pixel_count * 3);
            for chunk in raw.chunks_exact(4).take(pixel_count) {
                let c = chunk[0] as f32 / 255.0;
                let m = chunk[1] as f32 / 255.0;
                let y = chunk[2] as f32 / 255.0;
                let k = chunk[3] as f32 / 255.0;

                let r = ((1.0 - c) * (1.0 - k) * 255.0).clamp(0.0, 255.0) as u8;
                let g = ((1.0 - m) * (1.0 - k) * 255.0).clamp(0.0, 255.0) as u8;
                let b = ((1.0 - y) * (1.0 - k) * 255.0).clamp(0.0, 255.0) as u8;
                rgb.extend_from_slice(&[r, g, b]);
            }
            return encode_rgb_buffer_to_png(
                image::RgbImage::from_raw(width, height, rgb)
                    .ok_or("Failed to create RGB buffer")?,
            );
        }
    } else {
        // Default DeviceRGB or ICCBased
        if let Some(alpha) = smask_alpha {
            if raw.len() >= pixel_count * 3 && alpha.len() >= pixel_count {
                let mut rgba = Vec::with_capacity(pixel_count * 4);
                for i in 0..pixel_count {
                    let r = raw[i * 3];
                    let g = raw[i * 3 + 1];
                    let b = raw[i * 3 + 2];
                    let a = alpha[i];
                    rgba.extend_from_slice(&[r, g, b, a]);
                }
                return encode_image_buffer_to_png(
                    image::RgbaImage::from_raw(width, height, rgba)
                        .ok_or("Failed to create RGBA buffer")?,
                );
            }
        }

        if raw.len() >= pixel_count * 3 {
            let rgb_buf = raw[..pixel_count * 3].to_vec();
            return encode_rgb_buffer_to_png(
                image::RgbImage::from_raw(width, height, rgb_buf)
                    .ok_or("Failed to create RGB buffer")?,
            );
        }
    }

    // Fallback: try loading directly via image crate in case stream is already a known format
    if let Ok(img) = image::load_from_memory(raw) {
        let mut out = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .map_err(|e| format!("PNG encode error: {}", e))?;
        return Ok(out);
    }

    Err(format!(
        "Unsupported raster format: ColorSpace={:?}, bpc={}, size={}",
        String::from_utf8_lossy(color_space),
        bpc,
        raw.len()
    ))
}

fn encode_rgb_buffer_to_png(img: image::RgbImage) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let dynamic = image::DynamicImage::ImageRgb8(img);
    dynamic
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| format!("PNG write error: {}", e))?;
    Ok(out)
}

fn encode_gray_buffer_to_png(img: image::GrayImage) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let dynamic = image::DynamicImage::ImageLuma8(img);
    dynamic
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| format!("PNG write error: {}", e))?;
    Ok(out)
}

fn encode_image_buffer_to_png(img: image::RgbaImage) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let dynamic = image::DynamicImage::ImageRgba8(img);
    dynamic
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| format!("PNG write error: {}", e))?;
    Ok(out)
}
