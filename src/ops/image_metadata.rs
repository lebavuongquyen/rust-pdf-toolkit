use crc32fast::Hasher;
use serde_json::Value;

const PNG_MAGIC: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
const PIECE_INFO_KEYWORD: &str = "PieceInfo";
const JPEG_COM_PREFIX: &str = "PieceInfo:";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormatType {
    Png,
    Jpeg,
}

impl ImageFormatType {
    pub fn detect(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(PNG_MAGIC) {
            Some(Self::Png)
        } else if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
            Some(Self::Jpeg)
        } else {
            None
        }
    }
}

/// Injects PieceInfo JSON metadata into PNG or JPEG image bytes.
pub fn inject_image_metadata(
    image_bytes: &[u8],
    metadata: &Value,
    format: ImageFormatType,
) -> Result<Vec<u8>, String> {
    if metadata.is_null() {
        return Ok(image_bytes.to_vec());
    }
    let json_str = serde_json::to_string(metadata)
        .map_err(|e| format!("Failed to serialize metadata to JSON: {e}"))?;

    match format {
        ImageFormatType::Png => inject_png_metadata(image_bytes, PIECE_INFO_KEYWORD, &json_str),
        ImageFormatType::Jpeg => inject_jpeg_metadata(image_bytes, &json_str),
    }
}

/// Extracts PieceInfo JSON metadata from PNG or JPEG image bytes.
pub fn get_image_metadata(image_bytes: &[u8]) -> Result<Option<Value>, String> {
    let format = match ImageFormatType::detect(image_bytes) {
        Some(f) => f,
        None => return Ok(None),
    };

    match format {
        ImageFormatType::Png => extract_png_metadata(image_bytes, PIECE_INFO_KEYWORD),
        ImageFormatType::Jpeg => extract_jpeg_metadata(image_bytes),
    }
}

// ---------------------------------------------------------------------------
// PNG Implementation (tEXt Chunk Injection & Extraction)
// ---------------------------------------------------------------------------

fn inject_png_metadata(bytes: &[u8], keyword: &str, text: &str) -> Result<Vec<u8>, String> {
    if !bytes.starts_with(PNG_MAGIC) {
        return Err("Invalid PNG magic header".to_string());
    }

    // Prepare tEXt chunk data: <keyword> + 0x00 + <text>
    let mut chunk_data = Vec::with_capacity(keyword.len() + 1 + text.len());
    chunk_data.extend_from_slice(keyword.as_bytes());
    chunk_data.push(0x00);
    chunk_data.extend_from_slice(text.as_bytes());

    let chunk_len = chunk_data.len() as u32;
    let chunk_type = b"tEXt";

    let mut hasher = Hasher::new();
    hasher.update(chunk_type);
    hasher.update(&chunk_data);
    let crc = hasher.finalize();

    let mut chunk_bytes = Vec::with_capacity(12 + chunk_data.len());
    chunk_bytes.extend_from_slice(&chunk_len.to_be_bytes());
    chunk_bytes.extend_from_slice(chunk_type);
    chunk_bytes.extend_from_slice(&chunk_data);
    chunk_bytes.extend_from_slice(&crc.to_be_bytes());

    // Locate IEND chunk to insert before it
    let mut offset = 8;
    let mut insert_pos = None;

    while offset + 8 <= bytes.len() {
        let length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let c_type = &bytes[offset + 4..offset + 8];

        if c_type == b"IEND" {
            insert_pos = Some(offset);
            break;
        }

        offset += 8 + length + 4; // length + type + data + crc
    }

    let pos = insert_pos.unwrap_or(bytes.len());
    let mut out = Vec::with_capacity(bytes.len() + chunk_bytes.len());
    out.extend_from_slice(&bytes[..pos]);
    out.extend_from_slice(&chunk_bytes);
    out.extend_from_slice(&bytes[pos..]);
    Ok(out)
}

fn extract_png_metadata(bytes: &[u8], target_keyword: &str) -> Result<Option<Value>, String> {
    if !bytes.starts_with(PNG_MAGIC) {
        return Ok(None);
    }

    let mut offset = 8;
    while offset + 8 <= bytes.len() {
        let length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let c_type = &bytes[offset + 4..offset + 8];
        let data_start = offset + 8;
        let data_end = data_start + length;

        if data_end > bytes.len() {
            break;
        }

        if c_type == b"tEXt" || c_type == b"iTXt" {
            let data = &bytes[data_start..data_end];
            if let Some(null_pos) = data.iter().position(|&b| b == 0)
                && let Ok(kw) = std::str::from_utf8(&data[..null_pos])
                && kw.eq_ignore_ascii_case(target_keyword)
            {
                let text_slice = if c_type == b"tEXt" {
                    &data[null_pos + 1..]
                } else {
                    // iTXt has compression flag, method, language tag, translated keyword before text
                    // find text starting after prefix null bytes
                    let rest = &data[null_pos + 1..];
                    if rest.len() >= 2 {
                        let mut rest_pos = 2; // skip compression flag and method
                        while rest_pos < rest.len() && rest[rest_pos] != 0 {
                            rest_pos += 1;
                        }
                        rest_pos += 1; // skip language tag null
                        while rest_pos < rest.len() && rest[rest_pos] != 0 {
                            rest_pos += 1;
                        }
                        rest_pos += 1; // skip translated keyword null
                        if rest_pos <= rest.len() {
                            &rest[rest_pos..]
                        } else {
                            &[]
                        }
                    } else {
                        &[]
                    }
                };

                if let Ok(text) = std::str::from_utf8(text_slice)
                    && let Ok(val) = serde_json::from_str::<Value>(text)
                {
                    return Ok(Some(val));
                }
            }
        }

        if c_type == b"IEND" {
            break;
        }

        offset += 8 + length + 4;
    }

    Ok(None)
}

// ---------------------------------------------------------------------------
// JPEG Implementation (COM Marker Injection & Extraction)
// ---------------------------------------------------------------------------

fn inject_jpeg_metadata(bytes: &[u8], json_str: &str) -> Result<Vec<u8>, String> {
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return Err("Invalid JPEG magic header".to_string());
    }

    let payload = format!("{JPEG_COM_PREFIX}{json_str}");
    let payload_bytes = payload.as_bytes();
    let marker_len = (payload_bytes.len() + 2) as u16; // length includes the 2 length bytes

    let mut marker = Vec::with_capacity(4 + payload_bytes.len());
    marker.push(0xFF);
    marker.push(0xFE); // COM marker
    marker.extend_from_slice(&marker_len.to_be_bytes());
    marker.extend_from_slice(payload_bytes);

    let mut out = Vec::with_capacity(bytes.len() + marker.len());
    out.extend_from_slice(&bytes[..2]); // SOI (0xFF, 0xD8)
    out.extend_from_slice(&marker);
    out.extend_from_slice(&bytes[2..]);
    Ok(out)
}

fn extract_jpeg_metadata(bytes: &[u8]) -> Result<Option<Value>, String> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return Ok(None);
    }

    let mut offset = 2;
    while offset + 4 <= bytes.len() {
        if bytes[offset] != 0xFF {
            offset += 1;
            continue;
        }

        let marker = bytes[offset + 1];
        if marker == 0xD9 {
            // EOI (End of Image)
            break;
        }
        if marker == 0x00 || (0xD0..=0xD7).contains(&marker) {
            // Restart markers or byte stuffing without length
            offset += 2;
            continue;
        }

        let length = u16::from_be_bytes([bytes[offset + 2], bytes[offset + 3]]) as usize;
        if length < 2 || offset + 2 + length > bytes.len() {
            break;
        }

        if marker == 0xFE {
            // COM Marker
            let payload = &bytes[offset + 4..offset + 2 + length];
            if let Ok(text) = std::str::from_utf8(payload) {
                let trimmed = text.strip_prefix(JPEG_COM_PREFIX).unwrap_or(text);
                if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
                    return Ok(Some(val));
                }
            }
        }

        offset += 2 + length;
    }

    Ok(None)
}
