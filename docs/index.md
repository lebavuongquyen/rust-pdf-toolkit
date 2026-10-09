---
layout: default
title: Home
nav_order: 1
description: "High-performance, pure-Rust and WASM-compatible PDF toolkit"
permalink: /
---

# rust-pdf-toolkit

{: .fs-6 .fw-300 }
High-performance, pure-Rust and WASM-compatible PDF toolkit for form filling, digital signing, document merging, splitting, rasterizing, image extraction, Bates numbering, watermarking, and multi-threaded batch folder processing.

[Live Playground](playground.html){: .btn .btn-primary .fs-5 .mb-4 .mb-md-0 .mr-2 }
[Get Started](getting-started.md){: .btn .fs-5 .mb-4 .mb-md-0 .mr-2 }
[View on GitHub](https://github.com/lebavuongquyen/rust-pdf-toolkit){: .btn .fs-5 .mb-4 .mb-md-0 }

---

## Key Capabilities

- **Form Filling & Validation**: Fill text (`/Tx`), buttons/checkboxes (`/Btn`), choices (`/Ch`), and JPEG images with detailed fill reports and selective flattening.
- **Merge PDF**: High-speed merging of multiple PDF files into a single document (pure Rust, available on both Native & WASM).
- **Split PDF**: Split PDF by page ranges (`all`, `1-3, 4-5`, `1, 3`) into clean standalone sub-documents.
- **Convert to Image (Render)**: High-fidelity PDF page rasterization to PNG/JPEG with configurable DPI powered by Google PDFium.
- **Extract Embedded Images**: Extract raw embedded photos, logos, barcode rasters, and signatures directly from PDF XObjects with size filters and deduplication.
- **Batch Folder Processing**: Parallel multi-core scanning and processing of entire folders (`batch fill`, `batch merge`, `batch split`, `batch to-image`, `batch extract-images`) powered by Rayon.
- **Digital Signing**: Incremental PDF signing with RSA and ECDSA P-384, fully compliant with ISO 32000-1, Foxit Reader, and Adobe Acrobat.
- **Stealth Cryptographic Lock**: AES-256-GCM encryption, document binding fingerprint (anti-transplant), and HMAC-SHA256 tamper verification using ISO 32000-1 `/PieceInfo`.
- **Dual-target Ready**: Full native CLI & server performance, plus WebAssembly (`wasm32-unknown-unknown`) package for direct browser execution.

---

## Architecture at a Glance

The library decouples PDF filling, visual signature appearance, and cryptographic signing, while offering a unified orchestration pipeline:

```text
PDF Template
    │
    ├──> fill_pdf()
    │       ├── /Tx, /Btn, /Ch, Image
    │       ├── FillReport
    │       └── Optional non-signature field flattening
    │
    ├──> PdfAppearance
    │       ├── Foxit-style visual layout (Left, Right, Behind)
    │       └── Typography (Helvetica, Times, Courier, bold, italic)
    │
    ├──> PdfSigner
    │       ├── Target signature field validation
    │       ├── Visual appearance embedding
    │       ├── Non-signature field flattening & signature locking
    │       ├── Incremental PDF revision (ISO 32000-1 §7.5.6)
    │       ├── Exact ByteRange calculation
    │       ├── CMS / PKCS#7 SignedData (RSA / P-384 ECDSA)
    │       └── Pluggable Signer abstraction (Local, HSM, Cloud KMS)
    │
    └──> fill_and_sign_pdf()  <=== UNIFIED PIPELINE
            ├── 1. In-memory form filling with FillReport
            ├── 2. Document flattening (optional)
            ├── 3. Visual signature appearance layout
            ├── 4. ReadOnly & /Lock field protection
            └── 5. Incremental cryptographic signing
```

---

## Documentation Guide

| Chapter | Description |
|---|---|
| [**Getting Started**](getting-started.md) | Installation, Cargo setup, WASM compilation, and quick start code snippets. |
| [**Architecture & Design**](architecture.md) | Architectural foundations, shared field model, design principles, and memory model. |
| [**Form Fields Discovery API**](form-fields-api.md) | In-depth AcroForm inspection, field discovery specifications, full JSON schema, and individual field type breakdown (Text, Date, ComboBox, ListBox, Checkbox, Radio, Image, Signature, Button, Barcode). |
| [**Form Filling & Metadata**](form-filling.md) | Intelligent field filling, selective flattening, `/PieceInfo` metadata, and AES-256-GCM Stealth Cryptographic Lock. |
| [**Digital Signing & Verification**](digital-signing.md) | PKCS#7 CMS signing, Foxit visual templates, unified pipeline, and in-depth cryptographic internals. |
| [**CLI Handbook**](cli-reference.md) | Complete CLI manual covering all 13 subcommands with comprehensive examples. |
| [**Rust API & Options**](api-options.md) | Detailed Rust configuration structs, default values, and text/image inspection models. |
| [**WebAssembly (WASM) Guide**](wasm-guide.md) | Browser and Node.js execution, JS/TS API reference, and in-memory workflows. |
