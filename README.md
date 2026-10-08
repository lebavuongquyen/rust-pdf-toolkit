# rust-pdffiller

Rust PDF form filling and digital signing library with a WASM-compatible filling core.

## Features

- Fill AcroForm text fields (/Tx).
- Fill checkbox and radio button fields (/Btn).
- Fill choice fields (/Ch).
- Fill JPEG image fields.
- Discover all AcroForm fields, including signature fields, with page and widget metadata.
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

### Shared field model

All field-aware APIs resolve fields through one internal registry:

~~~text
PDF
 |
 v
collect_field_definitions()
 |
 +--> get_form_fields()
 +--> fill()
 +--> validate_pdf()
 +--> PdfSigner::validate()
~~~

This keeps inherited names, field types, widgets, pages, and signature-field detection consistent across discovery, filling, validation, and signing.

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

## Form field discovery

The field discovery API returns every AcroForm field, including `/Sig` signature fields. Signature fields are metadata only here; cryptographic signing remains the responsibility of `PdfSigner`.

~~~rust
use pdffiller_core::get_form_fields;

let template = std::fs::read("template.pdf")?;
let fields = get_form_fields(&template)?;

for field in fields {
    println!("{} {:?} page={:?} rect={:?}", field.name, field.field_type, field.page, field.rect);
}
~~~

Each field exposes its object id, logical name, type, primary page and rectangle, all widget locations, current/default value, required/read-only flags, visibility/enabled state, tooltip, options, raw field flags, and signature status when the field is `/Sig`.

The WASM API exposes the same metadata through `get_form_fields_result(template)` as JSON.

## Validation

Validation does not modify the PDF.

~~~rust
use pdffiller_core::validate_pdf;

let template = std::fs::read("template.pdf")?;
let json = std::fs::read_to_string("data.json")?;

let result = validate_pdf(&template, &json)?;
println!("{}", result);
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

### Native certificate signer

~~~rust
use pdffiller_core::{CertificateSigner, PdfSigner};

let pdf = std::fs::read("filled.pdf")?;
let certificate = std::fs::read("identity.cert.der")?;
let private_key = std::fs::read("identity.key.der")?;

let signer = CertificateSigner::from_pkcs8_der(certificate, &private_key)?;
let signed = PdfSigner::new()
    .field("Signature_0")
    .signer(signer)
    .reason("Approved")
    .location("Ho Chi Minh City")
    .contact("document@example.com")
    .sign(&pdf)?;

std::fs::write("signed.pdf", signed)?;
~~~

The native signing layer accepts RSA PKCS#8 DER and P-384 ECDSA PKCS#8 DER private keys with X.509 DER certificates. PKCS#12/PFX parsing is intentionally kept outside the core signing engine for now.

EcdsaSigner uses SHA-256 with direct CMS signing and the id-ecPublicKey signature algorithm shape used by the Foxit reference PDF in reference/template_signed.pdf. CertificateSigner keeps the existing RSA PKCS#1 v1.5 flow with signed CMS attributes.

PdfSigner::validate() resolves the requested field through the same shared field-discovery registry used by filling and metadata discovery. The field must exist, must be a /Sig field, and must not already contain a signature.

## Signer abstraction

The PDF layer is separated from the cryptographic signer.

~~~rust
pub trait Signer: Send + Sync {
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, SignError>;

    fn certificate_chain(&self) -> &[Vec<u8>] {
        &[]
    }

    fn cms_signature_mode(&self) -> CmsSignatureMode;
}
~~~

The intended extension points are:

~~~text
Signer
  |
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

The current native signing baseline supports:

- RSA private key in PKCS#8 DER
- P-384 ECDSA private key in PKCS#8 DER
- X.509 certificate in DER
- SHA-256
- RSA PKCS#1 v1.5 CMS signing with signed attributes
- ECDSA SHA-256 direct CMS signing with id-ecPublicKey
- Adobe adbe.pkcs7.detached PDF signatures

The Foxit reference in reference/template_signed.pdf is treated as a golden interoperability fixture. Its ByteRange, detached CMS shape, SHA-256 digest algorithm, P-384 certificate, and direct ECDSA SignerInfo are covered by tests/foxit_reference.rs.

Other algorithms and credential containers should be added through the signer abstraction rather than coupled to the PDF engine.

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
- invalid certificate or private key
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

The test certificate and private key fixtures are test-only credentials.

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

## Standalone signing design

The native signing implementation does not use a third-party PDF signing backend.

The PDF incremental update, ByteRange handling, CMS/PKCS#7 construction, certificate metadata extraction, RSA PKCS#1 v1.5 signing, and SHA-256 digest path are owned by this project. Pure-Rust cryptographic crates are used for cryptographic primitives and certificate parsing.

This keeps the PDF engine independent from a PDF signing vendor and leaves external, remote, HSM, and cloud-KMS signers behind the same Signer abstraction.

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
- Native RSA certificate signer
- Signer abstraction
- WASM filling build
- Native signing integration test

### Next

- Native CMS verification API
- Certificate chain support
- PKCS#12/PFX adapter outside the core engine
- External signer API with digest/signature metadata
- Remote signer
- HSM signer
- RFC 3161 timestamping
- PAdES B-T / B-LT / B-LTA
- DSS / LTV
- Multiple signature support
- Streaming output wrappers
- WASM external-signing bridge

## License

This repository does not currently declare a project-level license. Review dependency license requirements before publishing or distributing binaries that include native signing support.
