# rust-pdf-toolkit

[![CI](https://github.com/lebavuongquyen/rust-pdf-toolkit/actions/workflows/ci.yml/badge.svg)](https://github.com/lebavuongquyen/rust-pdf-toolkit/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-GitHub%20Pages-blue.svg)](https://lebavuongquyen.github.io/rust-pdf-toolkit/)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![WASM](https://img.shields.io/badge/WASM-ready-green.svg)](https://webassembly.org/)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#license)

High-performance, pure-Rust and WASM-compatible PDF toolkit for form filling, digital signing, document merging, splitting, rasterizing to images, embedded image extraction, Bates numbering, watermarking, and multi-threaded batch folder processing.

📖 **Full Documentation Portal**: [https://lebavuongquyen.github.io/rust-pdf-toolkit/](https://lebavuongquyen.github.io/rust-pdf-toolkit/)  
🚀 **Live Web Playground (WASM)**: [https://lebavuongquyen.github.io/rust-pdf-toolkit/playground.html](https://lebavuongquyen.github.io/rust-pdf-toolkit/playground.html)

---

## ✨ Key Capabilities

- **Form Filling & Validation**: Fill text (`/Tx`), buttons/checkboxes (`/Btn`), choices (`/Ch`), and images with detailed fill reports and selective flattening.
- **Digital Signing**: Incremental PDF signing with RSA and ECDSA P-384, fully compliant with ISO 32000-1, Foxit Reader, and Adobe Acrobat.
- **Foxit-Style Visual Appearance**: Configurable stamp layouts (`Left`, `Right`, `Behind`, `ImageOnly`, `TextOnly`) with typography styling.
- **Stealth Cryptographic Lock**: AES-256-GCM encryption, document binding fingerprint (anti-transplant), and HMAC-SHA256 tamper verification via ISO 32000-1 `/PieceInfo`.
- **Merge & Split**: High-speed merging with outline bookmarks and splitting by flexible page ranges (`all`, `1-3, 4-5`).
- **Render & Extract**: Rasterize PDF pages to PNG/JPEG with Google PDFium or extract raw embedded XObject photos/stamps.
- **Batch Processing**: Multi-core parallel folder processing (`batch fill`, `batch merge`, `batch split`, `batch to-image`, `batch watermark`) powered by Rayon.
- **Dual-target Ready**: High-performance Native CLI & server library, plus WebAssembly (`wasm32-unknown-unknown`) package for direct browser execution.

---

## 🏛️ Architecture Overview

The library decouples PDF form filling, visual signature appearance, and cryptographic signing, offering both granular control and a unified orchestration pipeline:

```text
PDF Template
    │
    ├──> fill_pdf()               ──> Form filling (/Tx, /Btn, /Ch, Image), FillReport
    ├──> PdfAppearance            ──> Foxit-style visual layout (Left, Right, Behind)
    ├──> PdfSigner                ──> Incremental update, ByteRange, PKCS#7 CMS (RSA / ECDSA)
    │
    └──> fill_and_sign_pdf()      ──> UNIFIED PIPELINE (Fill -> Flatten -> Layout -> Sign)
```

---

## 🚀 Quick Start

### 1. Add as Dependency

Add `pdftoolkit-core` to your `Cargo.toml`:

```toml
[dependencies]
pdftoolkit-core = { path = "../pdftoolkit" } # Or from crates.io / git
serde_json = "1.0"
```

### 2. Form Filling Example (Rust)

```rust
use pdftoolkit_core::fill_pdf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let template = std::fs::read("template.pdf")?;
    let json_data = r#"{
        "full_name": "Nguyen Van A",
        "agree": true,
        "country": "Vietnam"
    }"#;

    let (filled_bytes, report) = fill_pdf(&template, json_data, None)?;
    std::fs::write("filled.pdf", filled_bytes)?;
    println!("Filled successfully: {:?}", report);
    Ok(())
}
```

### 3. Digital Signing Example (Rust)

```rust
use pdftoolkit_core::{CertificateSigner, GraphicPosition, PdfSigner, SignatureAppearanceOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pdf_bytes = std::fs::read("document.pdf")?;
    let cert_der = std::fs::read("signer.cert.der")?;
    let key_der = std::fs::read("signer.key.der")?;

    let signer = CertificateSigner::from_pkcs8_der(cert_der, &key_der)?;

    let appearance = SignatureAppearanceOptions {
        position: GraphicPosition::Left,
        signer_name: Some("Nguyen Van A".to_string()),
        show_signer_name: true,
        date: Some("2026-10-09 17:00:00".to_string()),
        show_date: true,
        reason: Some("Contract Execution".to_string()),
        show_reason: true,
        ..Default::default()
    };

    let signed_bytes = PdfSigner::new()
        .field("Signature1")
        .signer(signer)
        .appearance(appearance)
        .flatten(true) // Flattens all other form fields & locks signature
        .sign(&pdf_bytes)?;

    std::fs::write("signed.pdf", signed_bytes)?;
    println!("Document signed!");
    Ok(())
}
```

---

## 💻 CLI Quick Tour

Install the CLI binary locally:

```bash
cargo install --path .
```

Common commands:

```bash
# 1. Fill a form
pdftoolkit fill -t template.pdf -d data.json -o filled.pdf --flatten

# 2. Merge PDFs with bookmarks
pdftoolkit merge -i part1.pdf part2.pdf -o merged.pdf --bookmarks

# 3. Split PDF into single pages
pdftoolkit split -i doc.pdf -o ./output/ --range all

# 4. Add signature field & digitally sign
pdftoolkit add-sig-field -i doc.pdf -o with_sig.pdf --name "Sig1" --position bottom-right
pdftoolkit sign -i with_sig.pdf -o signed.pdf --field "Sig1" --cert cert.der --key key.der

# 5. Cryptographically verify signatures
pdftoolkit verify -i signed.pdf --json

# 6. High-throughput multi-core batch processing
pdftoolkit batch to-image -d ./folder -o ./gallery/ --dpi 150 --format png
```

---

## 🌐 WebAssembly (WASM)

Compile for browser & Node.js:

```bash
rustup target add wasm32-unknown-unknown
cargo build --target wasm32-unknown-unknown --release
```

Browser JavaScript usage:

```javascript
import init, { get_form_fields_result, fill_pdf_bytes } from './wasm_pkg/pdftoolkit_core.js';

await init();
const fields = JSON.parse(get_form_fields_result(templateBytes));
const filledPdf = fill_pdf_bytes(templateBytes, JSON.stringify({ name: "Alice" }));
```

---

## 📚 Complete Documentation Portal

Detailed specifications, architecture deep-dives, and complete API references are available in the [**Documentation Portal**](https://lebavuongquyen.github.io/rust-pdf-toolkit/):

| Guide | Description |
|---|---|
| [**Getting Started**](docs/getting-started.md) | Cargo installation, build instructions, WASM setup, and verification. |
| [**Architecture & Design**](docs/architecture.md) | Shared field model, design principles, memory model, and security guidelines. |
| [**Form Filling & Metadata**](docs/form-filling.md) | Field discovery, field value resolution, `/PieceInfo`, and AES-256-GCM Stealth Lock. |
| [**Digital Signing & Deep-Dive**](docs/digital-signing.md) | PKCS#7 CMS, RSA & ECDSA P-384, Foxit visual templates, ByteRange internals, and verification. |
| [**CLI Handbook**](docs/cli-reference.md) | Full guide for all 13 subcommands with detailed examples. |
| [**Rust API & Options**](docs/api-options.md) | Detailed configuration structs, default options tables, and JSON models. |
| [**WebAssembly (WASM) Guide**](docs/wasm-guide.md) | Complete JS/TS API reference, in-memory workflows, and browser samples. |

---

## 🧪 Testing

```bash
cargo check
cargo test
cargo test --test sign_api -- --nocapture
cargo test --test foxit_reference -- --nocapture
```

---

## 📄 License

Dual-licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
