---
layout: default
title: Getting Started
nav_order: 2
description: "Installation, build instructions, and quick start guide for rust-pdf-toolkit"
---

# Getting Started

## Installation

### Adding to your Rust Project

Add `pdftoolkit-core` to your `Cargo.toml`:

```toml
[dependencies]
pdftoolkit-core = { path = "../pdftoolkit" } # Or from crates.io / git repository
serde_json = "1.0"
```

### Installing the CLI Tool

To compile and install the CLI binary locally on your system:

```bash
cargo install --path .
```

Verify the installation:

```bash
pdftoolkit --help
```

---

## WebAssembly (WASM) Setup

To compile the library for browser or Node.js runtime environments:

```bash
# Add the wasm32 compilation target
rustup target add wasm32-unknown-unknown

# Build release WASM artifact
cargo build --target wasm32-unknown-unknown --release
```

Or using `wasm-pack`:

```bash
wasm-pack build --target web --out-dir wasm_pkg
```

---

## 5-Minute Quick Start

### 1. Inspect & Discover Form Fields

Before filling or signing, inspect all interactive fields in the template (see full [Form Fields Discovery API](form-fields-api.md)):

```rust
use pdftoolkit_core::get_form_fields;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let template = std::fs::read("template.pdf")?;
    let fields = get_form_fields(&template)?;

    for field in fields {
        println!("{}: {:?} (value: {:?})", field.name, field.field_type, field.value);
    }
    Ok(())
}
```

### 2. Fill a PDF Form in Rust

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

    println!("Fill status: {:?}", report);
    Ok(())
}
```

### 3. Digitally Sign a PDF in Rust

```rust
use pdftoolkit_core::{
    CertificateSigner, GraphicPosition, PdfSigner, SignatureAppearanceOptions, SignatureFont,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pdf_bytes = std::fs::read("filled.pdf")?;
    let cert_der = std::fs::read("signer.cert.der")?;
    let key_der = std::fs::read("signer.key.der")?;

    let signer = CertificateSigner::from_pkcs8_der(cert_der, &key_der)?;

    let appearance = SignatureAppearanceOptions {
        position: GraphicPosition::Left,
        signer_name: Some("Nguyen Van A".to_string()),
        show_signer_name: true,
        date: Some("2026-10-09 17:00:00".to_string()),
        show_date: true,
        reason: Some("Approved & Sealed".to_string()),
        show_reason: true,
        ..Default::default()
    };

    let signed_bytes = PdfSigner::new()
        .field("Signature1")
        .signer(signer)
        .appearance(appearance)
        .flatten(true) // Flattens all other form fields and locks signature
        .sign(&pdf_bytes)?;

    std::fs::write("signed.pdf", signed_bytes)?;
    println!("Document signed successfully!");
    Ok(())
}
```

### 4. Quick CLI Examples

```bash
# Fill a form
pdftoolkit fill -t template.pdf -d data.json -o filled.pdf --flatten

# Merge multiple PDFs
pdftoolkit merge -i part1.pdf part2.pdf -o merged.pdf --bookmarks

# Split into single pages
pdftoolkit split -i merged.pdf -o ./output/ --range all

# Add signature field & sign
pdftoolkit add-sig-field -i doc.pdf -o with_sig.pdf --name "Sig1" --position bottom-right
pdftoolkit sign -i with_sig.pdf -o signed.pdf --field "Sig1" --cert cert.der --key key.der

# Verify signature
pdftoolkit verify -i signed.pdf --json
```

---

## Running the Test Suite

```bash
# Check compiler diagnostics
cargo check

# Run unit tests
cargo test

# Run native digital signing integration tests
cargo test --test sign_api -- --nocapture

# Run Foxit interoperability tests
cargo test --test foxit_reference -- --nocapture
```
