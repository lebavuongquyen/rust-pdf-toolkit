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

### Form Flattening with `fill_pdf_with_options`

To convert interactive form fields into static page content (preventing user editing):

~~~rust
use pdffiller_core::{fill_pdf_with_options, FillOptions};

let options = FillOptions {
    flatten: true, // Flattens text, choices, buttons, images into static page graphics
};

let (pdf, report) = fill_pdf_with_options(&template, &json, &options)?;
~~~

> [!NOTE]
> When flattening during fill, **all non-signature fields are flattened**, but **signature fields remain interactive (`/FT /Sig`)** so they can still be signed cryptographically later. Signature fields are never stripped.

## Form field discovery

The field discovery API scans every AcroForm field, resolving widget annotations, page locations, inherited attributes, exact types, and existing values. Signature fields are discovered with full cryptographic and visual appearance metadata.

~~~rust
use pdffiller_core::{get_form_fields, FormFieldType};

let template = std::fs::read("template.pdf")?;
let fields = get_form_fields(&template)?;

for field in fields {
    println!("{}: {:?} (value: {:?})", field.name, field.field_type, field.value);
    
    // Date fields include detected format pattern
    if let Some(format) = &field.date_format {
        println!("  Date format: {}", format);
    }
    
    // Signed signature fields expose both visual appearance image and digital certificate info
    if let Some(sig) = &field.signature {
        println!("  Signer: {:?}", sig.signer_name);
        println!("  Reason: {:?}", sig.reason);
        println!("  Signing Time: {:?}", sig.signing_time);
        println!("  Visual image present: {}", sig.image.is_some());
    }
}
~~~

### Field types and value formats

| Field Type | Enum Variant | Description | `value` / Output |
|---|---|---|---|
| Text | `FormFieldType::Text` | Standard text field | String or null |
| Date | `FormFieldType::Date` | Text field formatted with Date scripts | String or null, plus `date_format` (e.g. `"dd/mm/yyyy"`) |
| Image | `FormFieldType::Image` | Pushbutton or widget with image appearance | Base64 Data URL (e.g. `"data:image/jpeg;base64,..."`) |
| Checkbox | `FormFieldType::Checkbox` | Checkbox toggle | Selected state (e.g. `"Yes"`), `"Off"`, or boolean string |
| Radio | `FormFieldType::Radio` | Radio button group | Selected export value or `"Off"` |
| Choice | `FormFieldType::ComboBox`, `ListBox` | Dropdown or list selector | String or array of strings (multi-select) |
| Signature | `FormFieldType::Signature` | Digital signature /Sig field | Visual signature image Data URL (or signer name), plus structured `signature` |
| Button | `FormFieldType::Button` | Action button | Pushbutton export state |
| Barcode | `FormFieldType::Barcode` | 2D/Paper form barcode | Barcode raw value |

### Field Value Resolution Mechanics

The discovery engine resolves field values across standard PDF AcroForm structures:
1. **Direct Field `/V`**: Value defined directly on the field dictionary.
2. **Parent Inheritance**: If `/V` is missing on a child node, parent hierarchy dictionaries are traversed upwards.
3. **Widget State Resolution**: For fields where values reside on individual widget annotations, widget `/V` and active appearance state `/AS` (e.g. checkbox on-state vs `"Off"`) are inspected.
4. **Visual Appearance Streams**: For Image fields and signed Signature fields, appearance streams (`/AP` &rarr; `/N`) are traversed to locate the embedded `XObject` image stream, returning a Base64 data URL (`data:image/jpeg;base64,...`).

### Structured `FormField` Model

~~~rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    pub id: String,                         // e.g. "340 0 R"
    pub name: String,                       // e.g. "Signature_0"
    pub field_type: FormFieldType,
    pub page: Option<usize>,                // 1-indexed page
    pub rect: Option<[f64; 4]>,             // [x1, y1, x2, y2]
    pub value: Option<serde_json::Value>,   // Current value (or Base64 image data URL)
    pub default_value: Option<serde_json::Value>,
    pub required: bool,
    pub read_only: bool,
    pub visible: bool,
    pub enabled: bool,
    pub tooltip: Option<String>,
    pub options: Vec<FormFieldOption>,
    pub flags: u32,
    pub locations: Vec<FieldLocation>,
    pub signed: Option<bool>,               // Some(true) if signed, Some(false) if unsigned
    pub date_format: Option<String>,        // e.g. "dd/mm/yyyy" or "yyyy-mm-dd"
    pub signature: Option<SignatureInfo>,   // Detailed signature & cert metadata
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureInfo {
    pub name: Option<String>,               // Signer name from PDF dictionary
    pub reason: Option<String>,             // Signing reason
    pub location: Option<String>,           // Signing location
    pub contact_info: Option<String>,       // Contact info
    pub signing_time: Option<String>,       // PDF timestamp, e.g. "D:20261008162001+07'00'"
    pub filter: Option<String>,             // e.g. "Adobe.PPKLite"
    pub sub_filter: Option<String>,         // e.g. "adbe.pkcs7.detached"
    pub byte_range: Option<Vec<i64>>,       // [offset1, len1, offset2, len2]
    pub image: Option<String>,              // Visual appearance Base64 Data URL
    pub signer_name: Option<String>,        // Subject CN from X.509 certificate
    pub signer_organization: Option<String>,// Subject O from X.509 certificate
    pub issuer: Option<String>,             // Issuer CN from X.509 certificate
    pub not_before: Option<String>,         // Certificate validity start
    pub not_after: Option<String>,          // Certificate validity expiry
    pub serial_number: Option<String>,      // Certificate serial number
}
~~~

### Example JSON output for a signed Signature field

~~~json
{
  "id": "340 0 R",
  "name": "Signature_0",
  "field_type": "Signature",
  "page": 1,
  "rect": [435.5, 680.2, 522.67, 719.6],
  "value": "data:image/jpeg;base64,/9j/4AAQSkZJRg...",
  "signed": true,
  "signature": {
    "reason": "I am the author of this document",
    "signing_time": "D:20261008162001+07'00'",
    "filter": "Adobe.PPKLite",
    "sub_filter": "adbe.pkcs7.detached",
    "byte_range": [0, 52115, 54905, 4136],
    "image": "data:image/jpeg;base64,/9j/4AAQSkZJRg...",
    "signer_name": "e8f4f4a3-fcad-44aa-af38-715f6ae7f8af",
    "issuer": "e8f4f4a3-fcad-44aa-af38-715f6ae7f8af",
    "not_before": "Apr 19 15:53:21 2026 +00:00",
    "not_after": "Apr 20 03:53:21 2027 +00:00",
    "serial_number": "00:be:55:a1:06:10:1c:ca:5f"
  }
}
~~~

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

## Visual signature appearance (Foxit-style Templates)

`PdfAppearance` allows rendering a standalone signature appearance with flexible layout options matching standard PDF editors (Foxit, Adobe Acrobat):

~~~rust
use pdffiller_core::{
    GraphicPosition, PdfAppearance, SignatureAppearanceOptions, SignatureFont, TextAlign,
};

let template = std::fs::read("template.pdf")?;

let options = SignatureAppearanceOptions {
    image: Some("data:image/jpeg;base64,...".to_string()),
    position: GraphicPosition::Left,      // Left, Right, Behind, ImageOnly, TextOnly
    image_width_ratio: Some(0.40),       // 40% image width, remainder for text
    font: SignatureFont::Helvetica,       // Helvetica, Times, Courier
    font_size: Some(8.5),                // Auto-scaled if None
    bold: true,
    italic: false,
    align: TextAlign::Left,              // Left, Center, Right
    text_color: Some([30, 30, 30]),       // RGB
    show_signer_name: true,
    signer_name: Some("Nguyen Van A".into()),
    show_date: true,
    date: Some("2026-10-08 22:30:00".into()),
    show_reason: true,
    reason: Some("Approval of contract".into()),
    show_location: true,
    location: Some("Hanoi, VN".into()),
    extra_lines: vec!["Ref: #987654".into()],
    ..Default::default()
};

let output = PdfAppearance::set_signature_appearance(&template, "Signature_0", &options)?;
std::fs::write("signature-appearance.pdf", output)?;
~~~

> [!NOTE]
> `PdfAppearance` modifies visual form XObjects and does not create cryptographic signatures. To cryptographically sign with visual appearance, pass `SignatureAppearanceOptions` directly into `PdfSigner`.

## Digital signing

Digital signing is a separate API producing standard cryptographic PKCS#7 / CMS signatures.

### Native certificate signer with Flattening & Appearance

`PdfSigner` supports automatic document flattening and visual appearance injection:

~~~rust
use pdffiller_core::{
    CertificateSigner, GraphicPosition, PdfSigner, SignatureAppearanceOptions, SignatureFont,
};

let pdf = std::fs::read("filled.pdf")?;
let certificate = std::fs::read("identity.cert.der")?;
let private_key = std::fs::read("identity.key.der")?;

let signer = CertificateSigner::from_pkcs8_der(certificate, &private_key)?;

let appearance = SignatureAppearanceOptions {
    image: Some("data:image/jpeg;base64,...".into()),
    position: GraphicPosition::Left,
    font: SignatureFont::Times,
    bold: true,
    signer_name: Some("Nguyen Van A".into()),
    show_signer_name: true,
    date: Some("2026-10-08 22:30:00".into()),
    show_date: true,
    reason: Some("Contract Approval".into()),
    show_reason: true,
    location: Some("Vietnam".into()),
    show_location: true,
    ..Default::default()
};

let signed = PdfSigner::new()
    .field("Signature_0")
    .signer(signer)
    .reason("Contract Approval")
    .location("Vietnam")
    .contact("signer@example.com")
    .flatten(true)              // Flattens other form fields before signing & locks this signature
    .appearance(appearance)     // Renders custom visual appearance
    .sign(&pdf)?;

std::fs::write("signed.pdf", signed)?;
~~~

### Flattening & Signature Protection Mechanics

When `.flatten(true)` is passed to `PdfSigner`:
1. **Non-signature fields**: Text, Date, Choice, Button, and Image fields are rendered into permanent page content streams and removed from `/AcroForm /Fields` and page `/Annots`.
2. **Signed signature field**: The cryptographic CMS/PKCS#7 signature is preserved (`/FT /Sig`, `/V`), field flags are marked **ReadOnly** (`/Ff 1`), widget annotation flags are **Locked** (`/F 65`), and a standard `/Lock << /Type /SigFieldLock /Action /All >>` dictionary is attached.
3. **Unsigned signature fields**: Remain untouched and fully interactive so that subsequent signers can still sign the document!

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
- Exact field type classification: Text, Date, Image, Checkbox, Radio, ListBox, ComboBox, Signature, Button, Barcode
- Date format extraction for Date fields (`date_format`)
- Image field value extraction (Base64 data URL from existing appearances)
- Signature metadata, visual image, and X.509 certificate extraction (`signature`)
- Robust field value extraction across all field types with parent inheritance and widget state resolution
- Buttons
- Choice fields
- JPEG image fields
- Foxit template_8field.pdf discovery regression fixture
- Field strategy registry
- Fill report
- Validation API
- Visual signature appearance API
- Separation of visual appearance and digital signing
- Native incremental digital signing API
- Native RSA certificate signer
- Native ECDSA certificate signer (Foxit reference shape)
- Signer abstraction
- WASM filling build
- Native signing integration test
- AcroForm form field flattening (`FillOptions { flatten: bool }` & `flatten_form_fields`) with signature field preservation
- Foxit/Adobe style custom signature appearance options (`GraphicPosition`, `SignatureFont`, `TextAlign`, `SignatureAppearanceOptions`)
- `PdfSigner` document flattening with cryptographic signature protection and field locking (`/Lock`, `ReadOnly`, `Locked`)

## License

This repository does not currently declare a project-level license. Review dependency license requirements before publishing or distributing binaries that include native signing support.
