---
layout: default
title: Digital Signing & Verification
nav_order: 6
description: "CMS PKCS#7 digital signing, Foxit-compatible appearance engine, unified pipeline, and cryptographic internals"
---

# Digital Signing & Verification

## Overview

Digital signing in PDF produces cryptographic PKCS#7 / CMS signatures adhering to ISO 32000-1 (PDF 1.7) and Adobe Acrobat / Foxit Reader interoperability specifications.

---

## Native Certificate Signing

`PdfSigner` provides native, dependency-light digital signing for both RSA and ECDSA certificates:

```rust
use pdftoolkit_core::{
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
    date: Some("2026-10-09 17:00:00".into()),
    show_date: true,
    reason: Some("Contract Approval".into()),
    show_reason: true,
    location: Some("Hanoi, Vietnam".into()),
    show_location: true,
    ..Default::default()
};

let signed = PdfSigner::new()
    .field("Signature_0")
    .signer(signer)
    .reason("Contract Approval")
    .location("Hanoi, Vietnam")
    .contact("signer@example.com")
    .flatten(true)          // Flattens other fields & locks this signature
    .appearance(appearance) // Renders visual appearance
    .sign(&pdf)?;

std::fs::write("signed.pdf", signed)?;
```

---

## Foxit-Style Visual Appearance Engine

`PdfAppearance` allows rendering a standalone signature appearance with flexible layout options matching standard PDF editors (Foxit, Adobe Acrobat):

```rust
use pdftoolkit_core::{
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
    date: Some("2026-10-09 17:00:00".into()),
    show_reason: true,
    reason: Some("Approval of contract".into()),
    show_location: true,
    location: Some("Hanoi, VN".into()),
    extra_lines: vec!["Ref: #987654".into()],
    ..Default::default()
};

let output = PdfAppearance::set_signature_appearance(&template, "Signature_0", &options)?;
std::fs::write("signature-appearance.pdf", output)?;
```

> [!NOTE]
> `PdfAppearance` modifies visual form XObjects and does not create cryptographic signatures. To cryptographically sign with visual appearance, pass `SignatureAppearanceOptions` directly into `PdfSigner`.

---

## Unified Fill & Sign Pipeline (`fill_and_sign_pdf`)

In automated workflows, applications typically need to fill form fields, selectively flatten static content, render signature stamps, and digitally sign in a single atomic pass:

```rust
use pdftoolkit_core::{
    fill_and_sign_pdf, CertificateSigner, GraphicPosition, PdfSigner,
    SignatureAppearanceOptions, SignatureFont,
};

let template = std::fs::read("contract_template.pdf")?;
let json_data = r#"{
    "customer_name": "Nguyen Van B",
    "contract_date": "2026-10-09",
    "service_package": "Enterprise VIP"
}"#;

let cert_bytes = std::fs::read("signer.cert.der")?;
let key_bytes = std::fs::read("signer.key.der")?;
let signer = CertificateSigner::from_pkcs8_der(cert_bytes, &key_bytes)?;

let appearance = SignatureAppearanceOptions {
    image: Some("data:image/jpeg;base64,...".into()),
    position: GraphicPosition::Left,
    signer_name: Some("Nguyen Van B".into()),
    show_signer_name: true,
    date: Some("2026-10-09 17:00:00".into()),
    show_date: true,
    reason: Some("I approve this agreement".into()),
    show_reason: true,
    ..Default::default()
};

let pdf_signer = PdfSigner::new()
    .field("Signature_0")
    .signer(signer)
    .reason("Contract Execution")
    .flatten(true)
    .appearance(appearance)
    .piece_info_json(r#"{"AuditPortal":{"status":"sealed"}}"#)?;

let (signed_pdf, report) = pdf_signer.fill_and_sign(&template, json_data)?;
std::fs::write("contract_executed.pdf", signed_pdf)?;
```

---

## Signature Design Extraction & Round-Trip Re-Injection

`pdftoolkit` supports **two-way round-trip signature design processing**:
1. **Extraction**: Parse the `/AP /N` Form XObject content stream to extract the exact `SignatureDesign` containing every text line's position `(x, y)`, font size, color, and `image_bounds`.
2. **Re-Injection**: Replicate the exact visual layout on a new document, or customize individual text lines and coordinates freely.

```rust
use pdftoolkit_core::{get_form_fields, PdfSigner};

// 1. Extract design from a signed reference PDF
let sample_pdf = std::fs::read("reference/template_signed.pdf")?;
let fields = get_form_fields(&sample_pdf)?;
let mut design = fields[0].signature.as_ref().unwrap().design.clone().unwrap();

// 2. Modify text lines while preserving layout coordinates
design.text_lines[0].text = "NGƯỜI KÝ: TRẦN VĂN B".into();

// 3. Sign a new document with the exact extracted design
let signed = PdfSigner::new()
    .field("Signature_0")
    .signer(signer)
    .design(design)  // Re-injects exact coordinates, image bounds & font sizes
    .sign(&new_pdf)?;
```

---

## Deep-Dive: PDF Digital Signature Design & Architecture

### 1. AcroForm Signature Structure

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
                 └── /M (D:20261009170000+07'00')
```

### 2. Incremental Update Mechanics (Revision Appending)

PDF digital signatures **require** Incremental Updates (ISO 32000-1 §7.5.6):

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
1. **Cryptographic Immutability**: If a signed PDF is resaved using standard full serialization (`lopdf::Document::save()`), object IDs, stream compressions, dictionary ordering, and byte offsets are rearranged, instantly breaking any existing digital signatures.
2. **Audit Trail & Multi-Signatures**: By appending new revisions (`xref` pointing back to `trailer /Prev`), PDF viewers like Adobe Acrobat and Foxit can reconstruct every historical version and verify each signer's chronological approval.

### 3. ByteRange & Detached CMS Architecture

To sign the file while storing the signature inside the file, PDF defines `/ByteRange`:

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
2. **Digest Computation**: The byte ranges `[Offset_1 .. Offset_1 + Length_1]` and `[Offset_2 .. Offset_2 + Length_2]` are concatenated and hashed with SHA-256. The placeholder `<...>` itself is excluded.
3. **Detached Injection**: The cryptographic signer signs the SHA-256 hash, generates a detached CMS `SignedData` container, encodes it into uppercase hex, and overwrites the allocated placeholder without altering any file offsets.

### 4. CMS / PKCS#7 Cryptographic SignedData Structure

Standard ASN.1 DER CMS structure (`adbe.pkcs7.detached` / RFC 5652):

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
            │   └── id-ecPublicKey (1.2.840.10045.2.1) [EcdsaSigner]
            └── signature: <Raw or DER signature bytes>
```

### 5. Form Flattening & Field Protection Security Model

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

1. **Non-Signature Field Rasterization**: Text inputs, checkboxes, dates, choices, and images are written directly as PDF content operators into `/Contents`. Their AcroForm dictionaries are purged. They become permanent, uneditable graphics.
2. **Signed Field Protection**:
   - `Ff 1`: ReadOnly flag.
   - `F 65`: Print & Locked flags.
   - `/Lock << /Type /SigFieldLock /Action /All >>`: Signals conforming viewers to lock the form upon signature.
3. **Preservation of Subsequent Signatures**: Any unsigned signature field (`/FT /Sig` without `/V`) remains interactive for secondary signers.
