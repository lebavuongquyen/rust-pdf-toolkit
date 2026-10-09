---
layout: default
title: Architecture & Design
nav_order: 3
description: "Core architectural foundations, design principles, memory model, and security guidelines"
---

# Architecture & Design Principles

## Core Design Principles

### 1. Filling is Tolerant
Form filling operates under the premise of graceful degradation. If a template contains dozens of fields and one field is missing, has an incompatible type, or has invalid data, the filling operation does **not** abort. Instead, it completes valid fields and compiles a detailed `FillReport` indicating the exact status of each field:
- `filled`: Successfully populated
- `missing`: Key not found in input data
- `invalid`: Data format mismatch
- `unsupported`: Unsupported field annotation
- `failed`: Low-level render or stream failure

### 2. Signing is Strict
Unlike filling, cryptographic digital signing is strictly fail-safe. If any prerequisite fails—such as certificate validation, byte range calculation, hash digest mismatch, or CMS encoding—the operation aborts immediately with a `SignError`. A damaged or partially signed document is never emitted.

### 3. Appearance is Not Signing
A visual image or graphical stamp on a PDF page is **not** a digital signature. 
- `PdfAppearance` modifies visual form XObjects (`/AP /N`).
- `PdfSigner` generates cryptographic PKCS#7 / CMS byte streams and computes `/ByteRange` digests.
Separating visual layout from cryptographic signing ensures clean architecture and prevents pseudo-signing security bugs.

### 4. Incremental Updates (ISO 32000-1 §7.5.6)
Digital signatures **must not** resave the document via a standard full serialization (`lopdf::Document::save()`), because reordering object numbers or recompressing existing streams breaks previously computed cryptographic hashes. 
`pdftoolkit` appends a clean incremental revision trailer to the existing byte buffer, preserving previous revisions and multi-signature audit trails.

### 5. Pluggable Cryptography (`Signer` Trait)
The PDF manipulation engine does not assume private keys live on disk in local memory:
```rust
pub trait Signer: Send + Sync {
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, SignError>;

    fn certificate_chain(&self) -> &[Vec<u8>] {
        &[]
    }

    fn cms_signature_mode(&self) -> CmsSignatureMode;
}
```
Implementations can easily delegate to Hardware Security Modules (HSM), Cloud KMS (AWS KMS, Azure Key Vault, GCP Cloud KMS), or remote signing microservices without changing the PDF incremental revision engine.

### 6. WASM-Safe Core
The core form-filling, splitting, merging, watermarking, cropping, rotating, and metadata extraction logic does not rely on OS-specific C libraries or filesystem calls, allowing clean compilation to WebAssembly (`wasm32-unknown-unknown`).

---

## Shared Field Model

All field-aware operations share a unified discovery and resolution engine:

```text
PDF Document Buffer
        │
        ▼
collect_field_definitions()
        │
        ├──> get_form_fields()      (Inspection & discovery)
        ├──> fill()                 (AcroForm filling)
        ├──> validate_pdf()         (Field validation)
        └──> PdfSigner::validate()  (Signature field verification)
```

This guarantees consistent resolution across:
- Fully qualified hierarchical field names (`Parent.Child`)
- Inherited field attributes (such as `/V` and `/Ff`)
- Widget annotations across multiple pages
- Detection of interactive vs. signed signature fields

---

## Output Model & Memory Safety

The core API processes documents strictly as byte slices in memory (`&[u8] -> Vec<u8>`):
- **Zero Temporary Disk Files**: Ideal for high-throughput server environments, microservices, and serverless lambdas.
- **Pure Rust Primitives**: High performance and memory safety without unsafe C pointers in the core pipeline.
- **Streaming Ready**: Final bytes can be returned directly over HTTP or stored to cloud object storage (S3/GCS/Azure Blob).

---

## Repository Layout

```text
src/
├── lib.rs                   # Public API surface & re-exports
├── main.rs                  # CLI entry point (Clap subcommands)
├── field_strategy.rs        # AcroForm field classification & population
├── appearance.rs            # Foxit/Adobe visual appearance layout engine
├── appearance_parser.rs     # Signature appearance reverse-engineering parser
├── appearance_renderer.rs   # Low-level PDF stream appearance synthesis
├── piece_info_security.rs   # AES-256-GCM / HMAC-SHA256 stealth metadata lock
├── sign.rs                  # Generic PDF signing pipeline & ByteRange math
├── sign_native.rs           # Pure-Rust CMS/PKCS#7, RSA & ECDSA signers
└── wasm.rs                  # WebAssembly wasm-bindgen bindings

reference/
├── template.pdf             # Base AcroForm test fixture
├── template_8field.pdf      # Multi-type AcroForm test fixture
└── template_signed.pdf      # Foxit golden reference digital signature

tests/
├── form_fields.rs           # Discovery & type resolution tests
├── foxit_reference.rs       # Foxit interoperability regression tests
├── piece_info_security_test.rs # Stealth lock tamper & transplant tests
├── sign_api.rs              # End-to-end digital signing integration tests
└── validation.rs            # Form validation rule tests
```

---

## Security Guidelines

When integrating `pdftoolkit` into production systems:
1. **Never commit private keys** or sensitive credentials to source repositories.
2. **Never expose private keys** in browser JavaScript or client-side WASM binaries.
3. **Use HSM / KMS** for high-assurance signing keys in automated pipelines.
4. **Always preserve incremental updates**: Do not run whole-document reformatting or optimization tools on digitally signed PDF documents.
