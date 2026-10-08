# rust-pdffiller

Rust PDF form filling and digital signing library with a WASM-compatible filling core.

## Features

- Fill AcroForm text fields (/Tx).
- Fill checkbox and radio button fields (/Btn).
- Fill choice fields (/Ch).
- Fill JPEG image fields.
- Return a structured fill report for missing, invalid, unsupported, and failed fields.
- Validate input without modifying the PDF.
- Render a visual signature image separately from cryptographic signing.
- Digital signing through a separate PdfSigner API.
- Native signing uses incremental PDF updates.
- Pluggable signer abstraction for future software, remote, HSM, KMS, and cloud signing backends.
- WASM build remains available for the filling pipeline; digital signing is native-only at this stage.

## Architecture

The library separates PDF filling, visual signature appearance, and cryptographic signing.

~~~text
PDF Template
    |
    +--> fill_pdf()
    |       +--> /Tx
    |       +--> /Btn
    |       +--> /Ch
    |       +--> Image
    |       +--> report
    |
    +--> PdfAppearance
    |       +--> visual signature image
    |
    +--> PdfSigner
            +--> signature field
            +--> incremental PDF revision
            +--> ByteRange
            +--> CMS / PKCS#7
            +--> certificate chain
            +--> Signer abstraction
~~~

A visual signature image is not treated as a digital signature. PdfAppearance changes only the visible appearance. PdfSigner creates the cryptographic signature.

## Filling

~~~rust
use pdffiller_core::fill_pdf;

let template = std::fs::read("template.pdf")?;
let json = std::fs::read_to_string("data.json")?;

let (pdf, report) = fill_pdf(&template, &json)?;

std::fs::write("filled.pdf", pdf)?;
println!("{}", pdffiller_core::report_json(&report));
~~~

Example input:

~~~json
{
  "full_name": "Nguyen Van A",
  "agree": true,
  "country": "Vietnam",
  "photo": "data:image/jpeg;base64,..."
}
~~~

The fill report uses these statuses:

- filled
- missing
- invalid
- unsupported
- failed

A bad individual field does not have to abort the entire fill operation.

## Validation

Validation does not modify the PDF.

~~~rust
use pdffiller_core::validate_pdf;

let template = std::fs::read("template.pdf")?;
let json = std::fs::read_to_string("data.json")?;

let result = validate_pdf(&template, &json)?;
println!("{result}");
~~~

Digital signature fields are deliberately excluded from normal filling. Use PdfSigner for cryptographic signing or PdfAppearance for a visual signature image.

## Visual signature appearance

~~~rust
use pdffiller_core::PdfAppearance;

let template = std::fs::read("template.pdf")?;

let output = PdfAppearance::set_signature_image(
    &template,
    "signature",
    "data:image/jpeg;base64,...",
)?;

std::fs::write("signature-appearance.pdf", output)?;
~~~

This does not create a CMS/PKCS#7 signature.

## Digital signing

Digital signing is a separate API.

### PKCS#12

~~~rust
use pdffiller_core::{PdfSigner, Pkcs12Signer};

let pdf = std::fs::read("filled.pdf")?;

let signer = Pkcs12Signer::from_pkcs12_file(
    "identity.p12",
    "password",
)?;

let signed = PdfSigner::new()
    .field("Signature1")
    .signer(signer)
    .reason("Approved")
    .location("Ho Chi Minh City")
    .contact("document@example.com")
    .sign(&pdf)?;

std::fs::write("signed.pdf", signed)?;
~~~

The signer can also be loaded from bytes:

~~~rust
let p12 = std::fs::read("identity.p12")?;
let signer = Pkcs12Signer::from_pkcs12_bytes(&p12, "password")?;
~~~

If the requested field already exists, it must be a /Sig field. If it does not exist, the native signing backend can create the signature field during the signing revision.

## Signer abstraction

The PDF layer is separated from the cryptographic signer.

~~~rust
pub trait Signer: Send + Sync {
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, SignError>;

    fn certificate_chain(&self) -> &[Vec<u8>] {
        &[]
    }
}
~~~

The intended extension points are:

~~~text
Signer
  |
  +-- Pkcs12Signer
  +-- CertificateSigner
  +-- ExternalSigner
  +-- RemoteSigner
  +-- HsmSigner
  +-- AzureKeyVaultSigner
  +-- AwsKmsSigner
~~~

A remote or HSM implementation can keep the private key outside the PDF process.

## Signing pipeline

~~~text
Input PDF
   |
   v
Resolve signature field
   |
   v
Create incremental revision
   |
   v
Create /Sig dictionary
   |
   +--> /ByteRange placeholder
   |
   +--> /Contents placeholder
   |
   v
Calculate bytes covered by ByteRange
   |
   v
Signer::sign(bytes)
   |
   v
CMS / PKCS#7 SignedData
   |
   v
Inject CMS into /Contents
   |
   v
Signed PDF
~~~

The final signing revision is not produced through the normal full-document save path. Existing PDF bytes and existing signatures are preserved.

## Current native signing support

The current Pkcs12Signer integration supports:

- RSA private key
- SHA-256
- RSA PKCS#1 v1.5
- PKCS#12 / PFX

Other algorithms should be added through the signer abstraction rather than coupled to the PDF engine.

## WASM

Build the filling core with:

~~~bash
cargo build --target wasm32-unknown-unknown --release
~~~

Digital signing is intentionally native-only at this stage. Browser signing should eventually use an external signer or WebCrypto/HSM/remote signing boundary.

## CLI

The current CLI focuses on filling:

~~~bash
pdffiller <template.pdf> <data.json> <output.pdf>
~~~

Example:

~~~bash
pdffiller reference/template.pdf reference/data.json output/filled.pdf
~~~

The library API is preferred for applications that need signing, validation, streaming, or custom signer implementations.

## Output model

The core API works with PDF bytes in memory. Higher-level wrappers can expose:

- output PDF bytes
- output file
- Base64 output
- validation result
- fill report

For server applications, the recommended flow is to keep the PDF in memory and stream the final bytes to the response.

## Error model

Signing uses SignError for:

- missing signing configuration
- invalid PDF
- missing or incompatible signature field
- invalid PKCS#12
- unsupported signing algorithm
- CMS construction failure
- cryptographic signing failure

Filling is deliberately more tolerant and reports field-level problems.

## Testing

Run:

~~~bash
cargo check
cargo test
cargo build --target wasm32-unknown-unknown --release
~~~

Native signing integration test:

~~~bash
cargo test --test sign_api -- --nocapture
~~~

The integration test verifies:

- signing succeeds
- the signed PDF is larger than the source
- the original bytes remain the prefix of the signed revision
- the signed PDF can be parsed
- a /Sig field exists
- /ByteRange exists

The test certificate and PKCS#12 identity are test-only credentials.

## Repository layout

~~~text
src/
├── lib.rs
├── main.rs
├── field_strategy.rs
├── appearance.rs
├── appearance_renderer.rs
├── sign.rs
└── wasm.rs

reference/
├── template.pdf
├── data.json
└── filled.pdf

tests/
└── sign_api.rs
~~~

## Design principles

### Filling is tolerant

A single bad field should not destroy an otherwise useful fill result.

### Signing is strict

A cryptographic signing failure must never silently produce an apparently signed PDF.

### Appearance is not signing

A signature image and a cryptographic PDF signature are different concepts and use different APIs.

### Incremental signing

Signing appends a new PDF revision instead of rewriting the complete document.

### Pluggable cryptography

The PDF engine should not know whether the private key is local, remote, in an HSM, or in a cloud KMS.

### WASM-safe core

PDF filling remains usable in browser/WASM environments without requiring private-key handling in the WASM module.

## Dependency note

The current native signing implementation uses pdfluent-sign as the cryptographic and incremental signing backend.

pdfluent-sign is distributed under the GNU AGPLv3 or a commercial license. Applications distributing native signing support should review that dependency's license requirements and choose an appropriate licensing strategy.

The filling core remains separated from the native signing dependency through the signer abstraction.

## Security

Do not:

- commit production private keys
- put production passwords in source code
- treat a visual signature image as a digital signature
- modify bytes inside an existing signed revision
- reuse a test certificate for production
- expose private keys through browser JavaScript or WASM

Production remote/HSM signing is intended to follow:

~~~text
Application
    |
    v
PdfSigner
    |
    v
ByteRange
    |
    v
ExternalSigner
    |
    +--> HSM
    +--> Remote signing service
    +--> Cloud KMS
    |
    v
CMS
    |
    v
Signed PDF
~~~

## Roadmap

### Completed

- AcroForm field discovery
- Text fields
- Buttons
- Choice fields
- JPEG image fields
- Field strategy registry
- Fill report
- Validation API
- Visual signature appearance API
- Separation of visual appearance and digital signing
- Native incremental digital signing API
- PKCS#12 signer
- Signer abstraction
- WASM filling build
- Native signing integration test

### Next

- Certificate + private-key signer API
- External signer API with digest/signature metadata
- Remote signer
- HSM signer
- RFC 3161 timestamping
- PAdES B-T
- DSS / LTV
- PAdES B-LT
- Document timestamp / B-LTA
- Signature verification API
- Multiple signature support
- Streaming output wrappers
- WASM external-signing bridge

## License

This repository does not currently declare a project-level license. Review dependency license requirements before publishing or distributing binaries that include native signing support.

[executed on device: QuyenLe (dc1d89ef-2452-4cf0-af98-88586f0bd77d)]