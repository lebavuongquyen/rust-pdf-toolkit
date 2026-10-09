---
layout: default
title: Rust API & Options
nav_order: 7
description: "Comprehensive Rust options structs, default values, and data models"
---

# Rust API & Options Reference

This section documents all configuration structs used across `pdftoolkit-core`.

---

## `MergeOptions`

| Option | Type | Default | Description |
|---|---|---|---|
| `create_bookmarks` | `bool` | `true` | Generate PDF bookmarks pointing to the first page of each merged document |
| `page_mode` | `PageMode` | `UseOutlines` | Initial view mode (`UseOutlines`, `UseThumbs`, `FullScreen`, `UseNone`) |
| `flatten` | `bool` | `false` | Flatten form fields before merging to prevent field name collision |
| `piece_info` | `Option<Value>` | `None` | Custom ISO 32000-1 `/PieceInfo` metadata JSON to inject |
| `locked_piece_info` | `Option<LockedPieceInfoConfig>` | `None` | Stealth cryptographic AES-256-GCM locked metadata |

---

## `SplitOptions`

| Option | Type | Default | Description |
|---|---|---|---|
| `ranges` | `String` | `"all"` | Page range specification (`"all"`, `"1-3, 4-5"`, `"1, 3, 5"`, `"2-"`, `"-4"`) |
| `naming_pattern` | `String` | `"{stem}_{label}.pdf"` | Filename format template supporting `{stem}`, `{label}`, `{index:03}`, `{page}` |
| `inherit_piece_info`| `bool` | `true` | Automatically copy `/PieceInfo` metadata from source PDF into all split parts |
| `piece_info` | `Option<Value>` | `None` | Override `/PieceInfo` metadata to inject into each split part |
| `flatten` | `bool` | `false` | Flatten form fields on each split page |

---

## `RenderOptions` (PDF to Image)

| Option | Type | Default | Description |
|---|---|---|---|
| `dpi` | `f32` | `150.0` | Target rendering resolution |
| `format` | `OutputImageFormat`| `Png` | Image format: `Png` or `Jpeg` |
| `jpeg_quality` | `u8` | `85` | JPEG compression quality (1-100) |
| `pages` | `Option<String>` | `None` (all) | Page selection string (e.g. `"1, 3-5"`) |
| `max_width` | `Option<u32>` | `None` | Maximum pixel width constraint |
| `max_height` | `Option<u32>` | `None` | Maximum pixel height constraint |
| `transparent_background`| `bool`| `false` | Keep transparent background for PNG |
| `render_annotations`| `bool` | `true` | Render signatures, stamps, and annotations |
| `inherit_pdf_piece_info`| `bool`| `true` | Automatically extract `/PieceInfo` from PDF and embed into the image |
| `piece_info` | `Option<Value>` | `None` | Custom metadata to embed into image (`tEXt` chunk for PNG, `COM` marker for JPEG) |

---

## `ExtractImageOptions` (Extract Embedded Images)

| Option | Type | Default | Description |
|---|---|---|---|
| `pages` | `Option<Vec<u32>>` | `None` (all) | Target 1-based page numbers to extract images from |
| `min_width` | `u32` | `0` | Minimum image width in pixels (filters out tiny icons and bullets) |
| `min_height` | `u32` | `0` | Minimum image height in pixels (filters out tiny icons and bullets) |
| `naming_pattern` | `Option<String>` | `None` | Custom filename pattern (`"img_p{page}_{index}.{ext}"`, `{id}`) |
| `deduplicate` | `bool` | `true` | Skip duplicate embedded images sharing the same PDF ObjectId |

---

## `WatermarkOptions` (Text & Image Watermark)

| Option | Type | Default | Description |
|---|---|---|---|
| `text` | `Option<String>` | `None` | Text string supporting placeholders `{page}`, `{total}`, `{date}`, `{time}`, `{filename}` |
| `image_bytes` | `Option<Vec<u8>>` | `None` | Raw image bytes (PNG or JPEG) for logo/stamp watermark |
| `pages` | `PageSelection` | `All` | Target pages: `All`, `Odd`, `Even`, `First`, `Last`, `Range(Vec<u32>)` |
| `position` | `WatermarkPosition` | `Diagonal` | `Diagonal`, `Center`, `TopLeft`, `BottomRight`, `Tiled { step_x, step_y }`, `Custom { x, y }` |
| `rotation` | `Option<f64>` | `None` | Rotation angle in degrees (auto-calculated from page diagonal if omitted) |
| `opacity` | `f64` | `0.15` | Alpha transparency (0.0 -> 1.0) via PDF `ExtGState /ca` |
| `font_name` | `String` | `"Helvetica-Bold"` | Standard BaseFont (`Helvetica`, `Times-Bold`, `Courier`, etc.) |
| `font_size` | `Option<f64>` | `None` | Font size in points (auto-scaled to fit ~65% of page diagonal if omitted) |
| `color` | `ColorRgb` | `Gray` | RGB color parsed from hex (`"#FF0000"`) or name (`"red"`, `"gray"`, `"black"`) |
| `layer` | `LayerMode` | `Over` | `Over` (draw on top of page content) or `Under` (draw behind page content) |
| `flatten` | `bool` | `false` | Flatten existing form fields after watermarking |

---

## `NumberingOptions` (Bates / Page Numbering)

| Option | Type | Default | Description |
|---|---|---|---|
| `format` | `String` | `"Trang {page} / {total}"` | Format template with `{page}`, `{total}`, `{bates:06d}`, `{date}`, `{filename}` |
| `position` | `NumberingPosition` | `BottomCenter` | `BottomCenter`, `BottomRight`, `BottomLeft`, `TopCenter`, `TopRight`, `TopLeft` |
| `margin_x` | `f64` | `36.0` | Horizontal margin from page edge in points (0.5 inch) |
| `margin_y` | `f64` | `24.0` | Vertical margin from page edge in points |
| `pages` | `PageSelection` | `All` | Target pages: `"all"`, `"odd"`, `"even"`, `"2-end"` |
| `start_page` | `u32` | `1` | Physical page index to begin numbering (e.g. 2 to skip cover page) |
| `start_number` | `u32` | `1` | Starting counter sequence number |
| `font_name` | `String` | `"Helvetica"` | Standard BaseFont |
| `font_size` | `f64` | `10.0` | Font size in points |
| `color` | `ColorRgb` | `#404040` | Text color |
| `opacity` | `f64` | `1.0` | Opacity in range 0.0 -> 1.0 |

---

## `RotateOptions` (Page Rotation & Orientation)

| Option | Type | Default | Description |
|---|---|---|---|
| `mode` | `RotationMode` | `Angle { degrees: 90, relative: true }` | `Angle { degrees, relative }`, `ToOrientation { target, direction }`, or `AutoDetectText { fallback_angle }` |
| `pages` | `PageSelection` | `All` | Target pages: `"all"`, `"odd"`, `"even"`, `"first"`, `"last"`, `"1,3-5"` |
| `flatten` | `bool` | `false` | Flatten form fields after rotation |
| `piece_info` | `Option<Value>` | `None` | Custom ISO 32000-1 `/PieceInfo` metadata JSON to inject |
| `locked_piece_info` | `Option<LockedPieceInfoConfig>` | `None` | Stealth cryptographic AES-256-GCM locked metadata |

---

## `BatchOptions` (Folder Processing)

| Option | Type | Default | Description |
|---|---|---|---|
| `recursive` | `bool` | `false` | Recursively scan subdirectories for `.pdf` files |
| `filter_pattern` | `Option<String>` | `None` | Filter filenames by glob pattern (e.g. `"*invoice*.pdf"`) |
| `continue_on_error`| `bool` | `true` | Continue processing remaining files if one fails |
| `overwrite` | `bool` | `true` | Overwrite existing output files |
| `dry_run` | `bool` | `false` | Simulate operations and return count without writing to disk |
| `max_threads` | `Option<usize>` | `None` | Limit Rayon multi-threading concurrency |

---

## `AddSignatureFieldOptions` & `RemoveSignatureFieldOptions`

### Creation: `AddSignatureFieldOptions`
| Option | Type | Default | Description |
|---|---|---|---|
| `field_name` | `String` | `"Signature1"` | Name of the new AcroForm `/FT /Sig` field |
| `page` | `Option<u32>` | `None` (last page) | 1-based target page to place widget annotation |
| `placement` | `SignaturePlacement` | `visible_preset(BottomRight, 150, 50)` | `Invisible` (`[0,0,0,0]`), `Rect([x,y,w,h])`, or `Preset` |
| `lock_all_fields` | `bool` | `false` | Attach `/Lock << /Type /SigFieldLock /Action /All >>` to seal document upon signing |

### Cleanup: `RemoveSignatureFieldOptions`
| Option | Type | Default | Description |
|---|---|---|---|
| `field_name` | `Option<String>` | `None` | Name of specific signature field to remove |
| `only_unsigned` | `bool` | `true` | Only remove unsigned fields, preserving signed certificates |
| `all` | `bool` | `false` | Remove all signature fields regardless of signing state |

---

## `SignatureVerification` Model

| Field | Type | Description |
|---|---|---|
| `field_name` | `String` | AcroForm signature field name |
| `is_valid` | `bool` | Overall cryptographic validity |
| `digest_matched` | `bool` | `true` if computed SHA-256 matches CMS `messageDigest` |
| `signer_subject` | `Option<String>` | X.509 certificate subject DN |
| `signer_issuer` | `Option<String>` | X.509 certificate issuer DN |
| `validity_start` / `validity_end` | `Option<String>` | Certificate validity dates |
| `reason` / `location` | `Option<String>` | Signing metadata extracted from `/Sig` dictionary |
| `byte_range` | `[usize; 4]` | Physical `/ByteRange` 4-tuple covered by digital signature |
| `error` | `Option<String>` | Error note if verification fails |

---

## `RemovePagesOptions` & `CropOptions`

### `RemovePagesOptions`
| Option | Type | Default | Description |
|---|---|---|---|
| `remove_cover` | `bool` | `false` | Remove first page (cover) |
| `remove_back_cover` | `bool` | `false` | Remove last page (back cover) |
| `remove_blank_pages` | `bool` | `false` | Detect and remove blank pages via rule-based content stream and annotation parsing |
| `blank_detection` | `BlankDetectionOptions` | `Default` | Configurable heuristics: `max_content_bytes`, `ignore_whitespace_only_text`, `check_annotations` |
| `pages` | `Option<PageSelection>` | `None` | Explicit pages to remove (`"2"`, `"3-5"`, `"odd"`, `"even"`) |
| `keep_pages` | `Option<PageSelection>` | `None` | Whitelist override: protected pages that will never be removed |
| `min_pages_retained` | `usize` | `1` | Safety guard threshold preventing accidental deletion of the entire document |
| `flatten` | `bool` | `false` | Flatten form fields before saving |

### `CropOptions`
| Option | Type | Default | Description |
|---|---|---|---|
| `mode` | `CropMode` | `Margins(0)` | `Margins`, `Box([llx, lly, urx, ury])`, or `AutoDetectContent { padding, fallback_rect }` |
| `target_box` | `TargetBox` | `CropBox` | Box to modify: `CropBox`, `MediaBox`, `TrimBox`, `BleedBox`, or `AllBoxes` |
| `pages` | `PageSelection` | `PageSelection::All` | Target pages to apply crop to |
| `clamp_to_media_box` | `bool` | `true` | Clamp resulting coordinates within physical `MediaBox` bounds |
| `flatten` | `bool` | `false` | Flatten form fields after cropping |

---

## `TextExtractionOptions` & Structured Model

| Option | Type | Default | Description |
|---|---|---|---|
| `pages` | `PageSelection` | `PageSelection::All` | Target pages: `"all"`, `"odd"`, `"even"`, `"1,3-5"` |
| `include_bounding_boxes` | `bool` | `true` | Compute exact 4-point bounding boxes `[llx, lly, urx, ury]` |
| `include_styles` | `bool` | `true` | Extract full typography styling (font family, size, bold, italic, RGB color) |
| `sort_reading_order` | `bool` | `true` | Reconstruct natural human reading order via spatial coordinate clustering |
| `line_break_tolerance` | `f64` | `4.0` | Vertical Y-distance tolerance in points before starting a new line |
| `word_spacing_factor` | `f64` | `0.3` | Factor multiplied by font size to distinguish word boundaries |
| `normalize_whitespace` | `bool` | `true` | Normalize redundant spaces and clean non-printable characters |

### Structured Text JSON Output:
```json
{
  "total_pages": 1,
  "pages": [
    {
      "page_number": 1,
      "width": 595.0,
      "height": 842.0,
      "blocks": [
        {
          "bbox": [50.0, 690.0, 150.0, 715.0],
          "lines": [
            {
              "bbox": [50.0, 700.0, 150.0, 715.0],
              "text": "Hello World",
              "words": [
                {
                  "text": "Hello",
                  "bbox": [50.0, 700.0, 85.0, 715.0],
                  "style": {
                    "font_name": "Helvetica-Bold",
                    "font_size": 14.0,
                    "is_bold": true,
                    "color_hex": "#1A1AE5"
                  }
                }
              ]
            }
          ]
        }
      ]
    }
  ]
}
```

---

## Image Metadata Injection & Extraction (2-Way)

Embed and read back `/PieceInfo` metadata directly to and from raster images:

```rust
use pdftoolkit_core::ops::{inject_image_metadata, get_image_metadata, ImageFormatType};
use serde_json::json;

let meta = json!({ "doc_id": "INV-2026", "audit": true });

// 1. Inject PieceInfo metadata into PNG or JPEG
let tagged_png = inject_image_metadata(&raw_png_bytes, &meta, ImageFormatType::Png)?;

// 2. Read back embedded metadata from image
let extracted_meta = get_image_metadata(&tagged_png)?;
assert_eq!(extracted_meta, Some(meta));
```
