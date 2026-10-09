---
layout: default
title: WebAssembly (WASM) Guide
nav_order: 9
description: "Browser & Node.js usage with WebAssembly bindings, JavaScript/TypeScript API reference"
---

# WebAssembly (WASM) Guide

`rust-pdf-toolkit` compiles to WebAssembly (`wasm32-unknown-unknown`), allowing high-performance PDF manipulation directly inside web browsers or Node.js without backend server dependencies.

---

## Compilation

Build the release WASM package:

```bash
# Add target if not already installed
rustup target add wasm32-unknown-unknown

# Build release WASM binary
cargo build --target wasm32-unknown-unknown --release
```

Or package it with `wasm-pack`:

```bash
wasm-pack build --target web --out-dir wasm_pkg
```

---

## JavaScript / TypeScript API Reference

The WASM module exposes the following entry points:

### Form Inspection & Filling
- `get_form_fields_result(template: Uint8Array): string`  
  Discovers all form fields and returns structured JSON with field types, values, and signatures.
- `fill_pdf_bytes(template: Uint8Array, json: string, piece_info?: string): Uint8Array`  
  Fills the PDF template and returns raw bytes (`Uint8Array`).
- `fill_pdf_base64(template: Uint8Array, json: string, piece_info?: string): string`  
  Fills the PDF template and returns Base64 data string.
- `fill_pdf_result(template: Uint8Array, json: string, piece_info?: string): string`  
  Fills the PDF template and returns the `FillReport` as JSON.
- `validate_pdf_result(template: Uint8Array, json: string): string`  
  Validates form data against field constraints without altering the document.

### ISO 32000-1 `/PieceInfo` Metadata & Stealth Lock
- `get_piece_info_result(template: Uint8Array): string`  
  Extracts `/PieceInfo` JSON metadata from the document catalog.
- `lock_pdf_piece_info(template: Uint8Array, app_name: string, data_json: string, secret_key: string): Uint8Array`  
  Seals and encrypts application data into `/PieceInfo` using AES-256-GCM + HMAC-SHA256.

### Document Manipulation
- `merge_pdfs(pdf_list: Uint8Array[]): Uint8Array` / `merge_pdfs_with_options(pdf_list, options_json)`  
  Merges multiple PDF byte arrays into a single document.
- `split_pdf_with_options(template: Uint8Array, options_json: string): Array<{ label: string, bytes: Uint8Array }>`  
  Splits a PDF into sub-documents according to page ranges.
- `rotate_pdf_pages_wasm(template: Uint8Array, options_json: string): Uint8Array`  
  Rotates pages according to rotation options.
- `rotate_pdf_pages_report_wasm(template: Uint8Array, options_json: string): string`  
  Rotates pages and returns execution details as JSON.
- `remove_pdf_pages_wasm(template: Uint8Array, options_json: string): Uint8Array`  
  Removes pages (cover, back cover, blank pages, custom ranges) returning cleaned PDF bytes.
- `crop_pdf_pages_wasm(template: Uint8Array, options_json: string): Uint8Array`  
  Crops pages (margins, explicit box, auto content) returning cropped PDF bytes.

### Watermark & Bates Numbering
- `apply_watermark_wasm(template: Uint8Array, options_json: string): Uint8Array`  
  Applies customizable text or image watermarks.
- `apply_page_numbering_wasm(template: Uint8Array, options_json: string): Uint8Array`  
  Applies Bates numbering headers and footers.

### Signature Field Management
- `add_signature_field_wasm(template: Uint8Array, options_json: string): Uint8Array`  
  Dynamically adds a new invisible or visible signature field.
- `remove_signature_field_wasm(template: Uint8Array, options_json: string): { bytes: Uint8Array, removedCount: number }`  
  Removes signature fields.

### Embedded Image Extraction & Metadata
- `extract_images_from_pdf_wasm(template: Uint8Array, options_json: string): Array<{ page: number, width: number, height: number, format: string, fileName: string, bytes: Uint8Array }>`  
  Extracts embedded photos and stamps.
- `inject_image_metadata_bytes(image_bytes: Uint8Array, metadata_json: string): Uint8Array`  
  Embeds metadata into PNG (`tEXt`) or JPEG (`COM`).
- `get_image_metadata_json(image_bytes: Uint8Array): string`  
  Reads embedded metadata from PNG or JPEG.

---

## Browser Example

```javascript
import init, { get_form_fields_result, fill_pdf_bytes } from './wasm_pkg/pdftoolkit_core.js';

async function run() {
    await init();

    const response = await fetch('/assets/contract_template.pdf');
    const templateBytes = new Uint8Array(await response.arrayBuffer());

    // 1. Discover fields
    const fieldsJson = get_form_fields_result(templateBytes);
    console.log("Fields found:", JSON.parse(fieldsJson));

    // 2. Fill form
    const formData = JSON.stringify({
        "customer_name": "Nguyen Van C",
        "agree_terms": true
    });

    const filledBytes = fill_pdf_bytes(templateBytes, formData);

    // 3. Trigger browser download or preview
    const blob = new Blob([filledBytes], { type: 'application/pdf' });
    const url = URL.createObjectURL(blob);
    window.open(url);
}

run();
```
