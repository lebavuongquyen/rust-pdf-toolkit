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
- Unified fill-and-sign API (`fill_and_sign_pdf`) for one-shot form filling and digital signing.
- Native signing uses incremental PDF updates.
- Pluggable signer abstraction for future software, remote, HSM, KMS, and cloud signing backends.
- WASM build remains available for the filling pipeline; digital signing is native-only at this stage.

## Architecture

The library separates PDF filling, visual signature appearance, and cryptographic signing, while offering a unified orchestration pipeline.

~~~text
PDF Template
    |
    +--> fill_pdf()
    |       +--> /Tx, /Btn, /Ch, Image
    |       +--> FillReport
    |       +--> Optional non-signature field flattening
    |
    +--> PdfAppearance
    |       +--> Foxit-style visual layout (Left, Right, Behind)
    |       +--> Typography (Helvetica, Times, Courier, bold, italic)
    |
    +--> PdfSigner
    |       +--> Target signature field validation
    |       +--> Visual appearance embedding
    |       +--> Non-signature field flattening & signature locking
    |       +--> Incremental PDF revision
    |       +--> Exact ByteRange calculation
    |       +--> CMS / PKCS#7 SignedData (RSA / P-384 ECDSA)
    |       +--> Pluggable Signer abstraction (Local, HSM, Cloud KMS)
    |
    +--> fill_and_sign_pdf()  <=== UNIFIED PIPELINE
            +--> 1. In-memory form filling with FillReport
            +--> 2. Document flattening (optional)
            +--> 3. Visual signature appearance layout
            +--> 4. ReadOnly & /Lock field protection
            +--> 5. Incremental cryptographic signing
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
    pub design: Option<SignatureDesign>,    // Extracted visual design, text lines & image bounds
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureDesign {
    pub position: GraphicPosition,          // Left, Right, Behind, ImageOnly, TextOnly
    pub image: Option<String>,              // Base64 Data URL of stamp/seal
    pub image_bounds: Option<[f64; 4]>,     // [x, y, width, height] within widget box
    pub text_lines: Vec<SignatureTextLine>, // Detailed lines with exact coordinates
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureTextLine {
    pub text: String,                       // Line text content
    pub x: Option<f64>,                     // X coordinate (points)
    pub y: Option<f64>,                     // Y coordinate (points)
    pub font_size: Option<f64>,             // Font size (pt)
    pub font_name: Option<String>,          // Font name, e.g. "F1", "Helvetica"
    pub color_rgb: Option<[u8; 3]>,         // Text color RGB
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
    "serial_number": "00:be:55:a1:06:10:1c:ca:5f",
    "design": {
      "position": "Left",
      "image": "data:image/jpeg;base64,/9j/4AAQSkZJRg...",
      "image_bounds": [3.0, 2.5, 48.0, 34.0],
      "text_lines": [
        {
          "text": "NGƯỜI KÝ: NGUYỄN VĂN A",
          "x": 58.5,
          "y": 26.0,
          "font_size": 8.5,
          "font_name": "F1",
          "color_rgb": [20, 30, 80]
        },
        {
          "text": "NGÀY: 2026-10-08 23:00",
          "x": 58.5,
          "y": 15.0,
          "font_size": 8.0,
          "font_name": "F1",
          "color_rgb": [40, 40, 40]
        }
      ]
    }
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

## Unified Fill & Sign API (`fill_and_sign_pdf`)

In many automated document workflows, an application needs to populate dynamic customer data into form fields, optionally flatten non-signature form fields into static page content, render a visual signature appearance, and cryptographically seal the document with a digital certificate in a single operation.

`rust-pdffiller` provides a unified entry point:
- `fill_and_sign_pdf(template: &[u8], json_data: &str, signer: &PdfSigner) -> Result<(Vec<u8>, FillReport), SignError>`
- `PdfSigner::fill_and_sign(&self, template: &[u8], json_data: &str) -> Result<(Vec<u8>, FillReport), SignError>`

### Example Usage

```rust
use pdffiller_core::{
    fill_and_sign_pdf, CertificateSigner, GraphicPosition, PdfSigner,
    SignatureAppearanceOptions, SignatureFont, TextAlign,
};

// 1. Load PDF template, form data, and certificate credentials
let template = std::fs::read("contract_template.pdf")?;
let json_data = r#"{
    "customer_name": "Nguyen Van B",
    "contract_date": "2026-10-08",
    "service_package": "Enterprise VIP"
}"#;

let cert_bytes = std::fs::read("company_signer.cert.der")?;
let key_bytes = std::fs::read("company_signer.key.der")?;
let signer = CertificateSigner::from_pkcs8_der(cert_bytes, &key_bytes)?;

// 2. Configure Foxit-style visual appearance
let appearance = SignatureAppearanceOptions {
    image: Some("data:image/jpeg;base64,...".into()),
    position: GraphicPosition::Left,
    font: SignatureFont::Times,
    bold: true,
    signer_name: Some("Nguyen Van B".into()),
    show_signer_name: true,
    date: Some("2026-10-08 15:30:00".into()),
    show_date: true,
    reason: Some("I approve this agreement".into()),
    show_reason: true,
    location: Some("Hanoi, Vietnam".into()),
    show_location: true,
    ..Default::default()
};

// 3. Configure PdfSigner with flattening enabled
let pdf_signer = PdfSigner::new()
    .field("Signature_0")
    .signer(signer)
    .reason("Contract Execution")
    .location("Hanoi, Vietnam")
    .flatten(true)              // Flattens text/choice/button fields, locks signature
    .appearance(appearance);

// 4. Execute unified fill and sign
let (signed_pdf, report) = pdf_signer.fill_and_sign(&template, json_data)?;

// 5. Inspect structured report and save output
println!("Filled fields: {}", report.filled_count());
std::fs::write("contract_executed.pdf", signed_pdf)?;
```

### Unified Execution Workflow

1. **In-Memory Form Fill**: The template is populated with form values using `fill_pdf_with_options` with `flatten: false`. Form values (`/V`) and widget appearances (`/AP`) are created. The operation yields a detailed `FillReport` indicating the status of every submitted field (`Filled`, `Missing`, `Invalid`, etc.).
2. **Selective Form Flattening**: If `pdf_signer.flatten(true)` is set:
   - All standard non-signature fields (`/Tx`, `/Btn`, `/Ch`, Image widgets) are transformed into permanent vector/text graphics embedded in the page's `/Contents` stream and removed from `/AcroForm /Fields`.
   - The target signature field is **not removed**; its flags are updated with `ReadOnly` (`/Ff 1`), annotation flags `Locked` (`/F 65`), and a standard `/Lock << /Type /SigFieldLock /Action /All >>` dictionary is attached.
   - Any secondary unsigned signature fields (`/FT /Sig` without `/V`) remain fully interactive for subsequent signers in multi-party workflows.
3. **Visual Appearance Rendering**: Foxit/Adobe-compatible Form XObjects (`/FRM`, `/n0`, `/n2`) are synthesized according to `SignatureAppearanceOptions` and bound to the widget's `/AP /N` stream.
4. **Incremental Cryptographic Sealing**: An incremental update section is synthesized appending the new `/Sig` dictionary, `/ByteRange` placeholders, xref table, and trailer. SHA-256 digests are computed over the ByteRange portions, detached CMS/PKCS#7 SignedData is produced, and the cryptographic DER is injected into `/Contents <hex>`.

---

### Signature Design Extraction & Round-Trip Re-Injection

`rust-pdffiller` supports **two-way round-trip signature design processing**:
1. **Extraction**: When reading a signed PDF, `get_form_fields()` parses the `/AP /N` Form XObject content stream to extract the exact `SignatureDesign` containing every text line's position `(x, y)`, font size, color, and `image_bounds`.
2. **Re-Injection when Signing**: You can pass this `SignatureDesign` directly into `PdfSigner::new().design(design)` to replicate the exact visual layout on a new document, or customize individual text lines and coordinates freely.

```rust
use pdffiller_core::{get_form_fields, PdfSigner, CertificateSigner, SignatureDesign, SignatureTextLine, GraphicPosition};

// 1. Extract design from a signed reference PDF
let sample_pdf = std::fs::read("reference/template_signed.pdf")?;
let fields = get_form_fields(&sample_pdf)?;
let mut design = fields[0].signature.as_ref().unwrap().design.clone().unwrap();

// 2. Modify or update text lines while preserving layout coordinates
design.text_lines[0].text = "NGƯỜI KÝ: TRẦN VĂN B".into();

// 3. Sign a new document with the exact extracted design
let signed = PdfSigner::new()
    .field("Signature_0")
    .signer(signer)
    .design(design)  // Re-injects exact coordinates, image bounds & font sizes
    .sign(&new_pdf)?;
```

## Deep-Dive: PDF Digital Signature Design & Architecture (Chi Tiết Thiết Kế Chữ Ký Số PDF)

Digital signatures in PDF are fundamentally different from ordinary graphical stamps or form field modifications. A valid PDF digital signature adheres to ISO 32000-1 (PDF 1.7) and Adobe Acrobat / Foxit Reader interoperability specifications.

### 1. AcroForm Signature Structure

In the PDF object model, an interactive signature is represented by an AcroForm field coupled with a Widget annotation and a Signature Dictionary:

```text
Catalog (/Root)
   │
   ├── /AcroForm
   │      ├── /Fields [ 10 0 R, ... ]
   │      └── /SigFlags 3   (SignaturesExist | AppendOnly)
   │
   └── /Pages ──> Page
                    └── /Annots [ 10 0 R, ... ]
                           │
                           ▼
                 Field / Widget (10 0 R)
                 ├── /Type /Annot
                 ├── /Subtype /Widget
                 ├── /FT /Sig
                 ├── /T (Signature_0)
                 ├── /Rect [ 100 200 300 250 ]
                 ├── /P (Page Object Ref)
                 ├── /F 65                (Print | Locked)
                 ├── /Ff 1                (ReadOnly)
                 ├── /AP << /N 12 0 R >>  (Visual Appearance)
                 ├── /Lock <<             (Field Lock Dictionary)
                 │     /Type /SigFieldLock
                 │     /Action /All
                 │   >>
                 └── /V 11 0 R            (Signature Value Dictionary)
                           │
                           ▼
                 Signature Dictionary (11 0 R)
                 ├── /Type /Sig
                 ├── /Filter /Adobe.PPKLite
                 ├── /SubFilter /adbe.pkcs7.detached
                 ├── /ByteRange [ 0, 10540, 15660, 4200 ]
                 ├── /Contents < 30820...0000 >
                 ├── /Name (Signer Common Name)
                 ├── /Reason (Approval Reason)
                 ├── /Location (Signer Location)
                 └── /M (D:20261008153000+07'00')
```

- **/FT /Sig**: Designates the AcroForm field type as a Digital Signature.
- **/V (Value)**: References an indirect Signature Dictionary containing cryptographic parameters.
- **/AP (Appearance)**: The normal appearance stream (`/N`) displayed by PDF viewers on the page canvas.
- **/Lock**: Prevents subsequent form modifications from invalidating document semantics.

---

### 2. Incremental Update Mechanics (Revision Appending)

PDF digital signatures **require** Incremental Updates (ISO 32000-1 §7.5.6).

```text
┌──────────────────────────────────────────────┐
│  Original Document Bytes (0 .. N)            │  <-- Byte Range Part 1 (Never Modified)
│  (Existing pages, objects, previous xref)    │
├──────────────────────────────────────────────┤
│  Incremental Revision (Appended Bytes)       │
│  - Modified Page /Annots                     │
│  - New Signature Widget & Value Dict         │
│  - Appearance Streams (/AP)                  │
│  - /ByteRange [ 0, N+x, N+x+len, y ]         │
│  - /Contents < ... CMS DER in Hex ... >      │
│  - New xref Table / Stream                   │
│  - New trailer << /Size .. /Prev .. >>       │
│  - startxref / %%EOF                         │
└──────────────────────────────────────────────┘
```

#### Why Incremental Updates are Mandatory:
1. **Cryptographic Immutability**: If a signed PDF is resaved using standard full serialization (`lopdf::Document::save()`), object IDs, stream compressions, dictionary ordering, and byte offsets are rearranged. This instantly corrupts byte digests and breaks any existing digital signatures.
2. **Audit Trail & Multi-Signatures**: By appending new revisions (`xref` pointing back to `trailer /Prev`), PDF viewers like Adobe Acrobat and Foxit can reconstruct every historical version of the document and verify each signer's chronological approval.

---

### 3. ByteRange & Detached CMS Architecture

A digital signature must sign the file, but the signature itself lives inside the file. To resolve this chicken-and-egg paradox, PDF defines the `/ByteRange` 4-tuple:

```text
/ByteRange [ Offset_1, Length_1, Offset_2, Length_2 ]
```

```text
0                                      Offset_2
│◄──────── Length_1 ────────►│         │◄────── Length_2 ──────►│
┌────────────────────────────┬─────────┬────────────────────────┐
│  Covered Bytes (Prefix)    │/Contents│  Covered Bytes (Suffix)│
│                            │<  ...  >│                        │
└────────────────────────────┴─────────┴────────────────────────┘
                             ▲         ▲
                          Offset_1+  Offset_2
                          Length_1   (Offset_1 + Length_1 + Hex_Len)
```

1. **Placeholder Allocation**: The PDF generator allocates a fixed-size hex string placeholder for `/Contents` (e.g. 8,192 hex chars for RSA, 4,096 hex chars for ECDSA) and calculates exact byte offsets for `/ByteRange`.
2. **Digest Computation**: The byte ranges `[Offset_1 .. Offset_1 + Length_1]` and `[Offset_2 .. Offset_2 + Length_2]` are concatenated and hashed with SHA-256. The placeholder `<...>` itself is excluded from hashing.
3. **Detached Injection**: The cryptographic signer signs the SHA-256 hash, generates a detached CMS `SignedData` container, encodes it into uppercase hex, and overwrites the allocated placeholder without altering any file offsets.

---

### 4. CMS / PKCS#7 Cryptographic SignedData Structure

`rust-pdffiller` constructs standard ASN.1 DER CMS structures (`adbe.pkcs7.detached` / RFC 5652):

```text
ContentInfo (1.2.840.113549.1.7.2 - signedData)
└── SignedData
    ├── version: 1
    ├── digestAlgorithms: [ id-sha256 (2.16.840.1.101.3.4.2.1) ]
    ├── encapContentInfo: id-data (1.2.840.113549.1.7.1) [eContent OMITTED]
    ├── certificates: [ X.509 Certificate Chain ]
    └── signerInfos:
        └── SignerInfo
            ├── version: 1
            ├── sid: issuerAndSerialNumber
            ├── digestAlgorithm: id-sha256
            ├── signedAttrs:
            │   ├── 1.2.840.113549.1.9.3 (contentType: id-data)
            │   ├── 1.2.840.113549.1.9.4 (messageDigest: SHA-256 of ByteRange bytes)
            │   └── 1.2.840.113549.1.9.5 (signingTime: UTCTime)
            ├── signatureAlgorithm:
            │   ├── rsaEncryption (1.2.840.113549.1.1.1) [CertificateSigner]
            │   └── id-ecPublicKey (1.2.840.10045.2.1) [EcdsaSigner - Foxit Golden Reference]
            └── signature: <Raw or DER signature bytes>
```

#### Supported Cryptographic Modes:
- **RSA PKCS#1 v1.5 with SHA-256 (`CertificateSigner`)**:
  - Full authenticated CMS signed attributes (`contentType`, `messageDigest`, `signingTime`).
  - Compatible with Adobe Acrobat, Foxit Reader, and signers using RSA-2048 / RSA-4096 keys.
- **P-384 ECDSA with SHA-256 (`EcdsaSigner`)**:
  - Direct CMS signing format adhering to Foxit PDF Editor's golden reference structure (`tests/foxit_reference.rs`).
  - Compact signature payload and high cryptographic security.

---

### 5. Foxit/Adobe Visual Appearance Layout Engine

When displaying a digital signature, modern PDF editors render a composite visual stamp combining an authorized graphical image (e.g., wet signature scan or company seal) and formatted text metadata:

```text
┌────────────────────────────────────────────────────────┐
│ ┌────────────────┐  Digitally signed by Nguyen Van B    │
│ │                │  DN: C=VN, CN=Nguyen Van B          │
│ │   [Graphic /   │  Date: 2026.10.08 15:30:00 +07'00'   │
│ │     Seal]      │  Reason: Contract Execution         │
│ │                │  Location: Hanoi, Vietnam           │
│ └────────────────┘                                     │
└────────────────────────────────────────────────────────┘
```

#### Appearance Configuration Options (`SignatureAppearanceOptions`):
- **Graphic Positioning (`GraphicPosition`)**:
  - `Left`: Image on the left (configurable ratio, default 40%), text on the right.
  - `Right`: Text on the left, image on the right.
  - `Behind`: Watermark style — graphic scaled in background with text overlaid.
  - `ImageOnly`: Graphical stamp only, without metadata text.
  - `TextOnly`: Pure text layout with clean typography.
- **Typography & Font Fallback**:
  - Standard Type1 fonts: `Helvetica`, `Times`, `Courier`.
  - Font styling: `bold: bool`, `italic: bool`, custom font size, text alignment (`Left`, `Center`, `Right`).
  - Text color: Custom RGB triple `[r, g, b]`.
- **Label Localization (`SignatureLabels`)**:
  - Configurable metadata prefixes (e.g., Vietnamese localization: `"Ký bởi"`, `"Ngày"`, `"Lý do"`, `"Địa điểm"`).
- **Auto-Wrap & Scaling**:
  - Automatically wraps long reason strings and subject distinguished names to prevent clipping beyond the widget rectangle.

---

### 6. Form Flattening & Field Protection Security Model

```text
              Document Protection Strategy
                          │
         ┌────────────────┴────────────────┐
         ▼                                 ▼
Non-Signature Fields              Signature Fields
         │                                 │
Render to /Contents page stream   ┌────────┴────────┐
Delete from /AcroForm /Fields     ▼                 ▼
(Static non-editable text/vector) Target Field     Other Sig Fields
                                  │                 │
                                  ├─ ReadOnly (Ff 1)└─ Kept Interactive
                                  ├─ Locked (F 65)     (For 2nd/3rd signers)
                                  └─ /SigFieldLock
```

1. **Non-Signature Field Rasterization**: Text inputs, checkboxes, dates, choices, and images are written directly as PDF content operators into the page's `/Contents` stream. Their AcroForm field dictionaries and widget annotations are purged from the document tree. They become permanent, uneditable graphics.
2. **Signed Field Protection**:
   - `Ff 1`: Sets the field flag to **ReadOnly**, instructing all conforming viewers not to allow modifying the field value.
   - `F 65`: Sets annotation flags to **Print** (bit 1) and **Locked** (bit 7), preventing viewers from moving or deleting the widget.
   - `/Lock << /Type /SigFieldLock /Action /All >>`: Implements the ISO 32000-1 signature field lock dictionary, cryptographically signaling to Adobe Acrobat and Foxit Reader that the entire form is sealed upon signing.
3. **Preservation of Subsequent Signatures**: Unlike naive PDF flatteners that destroy all form fields, `rust-pdffiller` inspects every field. Any unsigned signature field (`/FT /Sig` without `/V`) is preserved in `/AcroForm /Fields` so that subsequent parties can sign the document.

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
- Unified fill-and-sign API (`fill_and_sign_pdf` & `PdfSigner::fill_and_sign`)
- Comprehensive PDF digital signature architecture specification (ISO 32000-1 AcroForm, Incremental Update, ByteRange, CMS SignedData, Appearance templates, and Field Locking)

## License

This repository does not currently declare a project-level license. Review dependency license requirements before publishing or distributing binaries that include native signing support.
