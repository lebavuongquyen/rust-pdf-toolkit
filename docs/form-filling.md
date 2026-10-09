---
layout: default
title: Form Filling & Metadata
nav_order: 4
description: "AcroForm discovery, intelligent field filling, flattening, and ISO 32000-1 PieceInfo cryptographic lock"
---

# Form Filling & Metadata Management

## Form Field Discovery

The field discovery engine inspects every AcroForm node, resolving widget annotations, page locations, inherited values, types, and existing signature metadata.

```rust
use pdftoolkit_core::{get_form_fields, FormFieldType};

let template = std::fs::read("template.pdf")?;
let fields = get_form_fields(&template)?;

for field in fields {
    println!("{}: {:?} (value: {:?})", field.name, field.field_type, field.value);
    
    // Extracted date formats (e.g. "dd/mm/yyyy")
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
```

---

## Field Types & Supported Value Formats

| Field Type | Enum Variant | Description | `value` / Output |
|---|---|---|---|
| **Text** | `FormFieldType::Text` | Standard text field | String or `null` |
| **Date** | `FormFieldType::Date` | Text field formatted with Date scripts | String or `null`, plus `date_format` (e.g. `"dd/mm/yyyy"`) |
| **Image** | `FormFieldType::Image` | Pushbutton or widget with image appearance | Base64 Data URL (`"data:image/jpeg;base64,..."`) |
| **Checkbox** | `FormFieldType::Checkbox` | Checkbox toggle | Selected state (e.g. `"Yes"`), `"Off"`, or boolean string |
| **Radio** | `FormFieldType::Radio` | Radio button group | Selected export value or `"Off"` |
| **Choice** | `FormFieldType::ComboBox`, `ListBox` | Dropdown or list selector | String or array of strings (multi-select) |
| **Signature** | `FormFieldType::Signature` | Digital signature `/Sig` field | Visual signature image Data URL (or signer name), plus structured `signature` |
| **Button** | `FormFieldType::Button` | Action button | Pushbutton export state |
| **Barcode** | `FormFieldType::Barcode` | 2D/Paper form barcode | Barcode raw value |

---

## Field Value Resolution Mechanics

The discovery engine resolves field values across complex PDF AcroForm structures:
1. **Direct Field `/V`**: Value defined directly on the field dictionary.
2. **Parent Inheritance**: If `/V` is missing on a child node, parent hierarchy dictionaries are traversed upwards.
3. **Widget State Resolution**: For fields where values reside on individual widget annotations, widget `/V` and active appearance state `/AS` (e.g. checkbox on-state vs `"Off"`) are inspected.
4. **Visual Appearance Streams**: For Image fields and signed Signature fields, appearance streams (`/AP` &rarr; `/N`) are traversed to locate the embedded `XObject` image stream, returning a Base64 data URL (`data:image/jpeg;base64,...`).

---

## Structured `FormField` Model

```rust
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
```

---

## Form Filling

### Basic Filling

```rust
use pdftoolkit_core::fill_pdf;

let template = std::fs::read("template.pdf")?;
let json = r#"{
    "full_name": "Nguyen Van A",
    "agree": true,
    "country": "Vietnam",
    "avatar": "data:image/jpeg;base64,..."
}"#;

let (pdf_bytes, report) = fill_pdf(&template, json, None)?;
std::fs::write("filled.pdf", pdf_bytes)?;
```

The fill report (`FillReport`) contains field-by-field outcomes:
- `filled`
- `missing`
- `invalid`
- `unsupported`
- `failed`

### Form Validation

Validation checks JSON payload validity against field types without modifying the document:

```rust
use pdftoolkit_core::validate_pdf;

let template = std::fs::read("template.pdf")?;
let json = std::fs::read_to_string("data.json")?;

let result = validate_pdf(&template, &json)?;
println!("Validation: {}", result);
```

---

## ISO 32000-1 `/PieceInfo` Metadata

You can attach application-specific metadata into the PDF Document Catalog (`/Root /PieceInfo` compliant with ISO 32000-1 §14.5):

```rust
use pdftoolkit_core::{fill_pdf_with_options, get_piece_info, FillOptions};

let options = FillOptions {
    flatten: true, // Flattens fields into static graphics
    piece_info: Some(serde_json::json!({
        "ERP": {
            "LastModified": "D:20261009120000Z",
            "Private": {
                "invoice_id": "INV-2026-001",
                "department": "Finance"
            }
        }
    })),
    ..Default::default()
};

let (pdf, report) = fill_pdf_with_options(&template, &json, &options)?;

// Extract PieceInfo metadata back from any PDF
let metadata = get_piece_info(&pdf)?;
```

---

## Stealth Cryptographic Lock for `PieceInfo`

When embedding sensitive private application data (e.g. transaction trace, CIF, signer IP, audit history) into `/PieceInfo`, you can cryptographically seal and encrypt it using a **Secret Key**:

```rust
use pdftoolkit_core::{
    insert_locked_piece_info, verify_and_unlock_piece_info, FillOptions, PieceInfoUnlockStatus,
};

let secret_key = "EnterpriseSecretKey@2026";
let app_name = "CoreBanking";
let private_data = serde_json::json!({
    "account_id": "ACC-998877",
    "customer_cif": "CIF-123456",
    "transaction_amount": 50000000,
    "currency": "VND",
    "approved": true
});

// 1. Lock and inject during form filling:
let options = FillOptions::new()
    .flatten(true)
    .locked_piece_info(app_name, private_data, secret_key);

let (pdf_bytes, report) = pdftoolkit_core::fill_pdf_with_options(&template, &json, &options)?;

// 2. Unlock and verify integrity later:
let result = verify_and_unlock_piece_info(&pdf_bytes, app_name, secret_key)?;

match result.status {
    PieceInfoUnlockStatus::Valid => {
        println!("Verified Data: {:?}", result.data.unwrap());
    }
    PieceInfoUnlockStatus::WrongKey => {
        println!("Access Denied: Incorrect secret key.");
    }
    PieceInfoUnlockStatus::Tampered(reason) => {
        println!("ALERT: Data has been tampered with or modified: {}", reason);
    }
    PieceInfoUnlockStatus::DocumentMismatch => {
        println!("ALERT: PieceInfo was copied/transplanted from another PDF document!");
    }
    PieceInfoUnlockStatus::NotFound => {
        println!("No PieceInfo found for application {}", app_name);
    }
}
```

### Security & Compliance Guarantees:
1. **100% Stealth (Invisible on Page)**: Stored strictly in the PDF Document Catalog (`/Root /PieceInfo`) under ISO 32000-1 §14.5. It does not introduce any visual elements to page content streams.
2. **Seamless Viewing (No Password Prompts)**: Does not utilize standard PDF password encryption (`/Encrypt`), so Adobe Acrobat, Foxit Reader, Chrome, Edge, Safari, iOS, and Android open and print the PDF smoothly without any password popups.
3. **Zero Antivirus / Malware Warnings (0% False Positives)**: Stored as pure static PDF hexadecimal string dictionaries (`/Payload <hex>`, `/DocBinding <hex>`, `/HMAC <hex>`). Contains zero JavaScript (`/JS`), zero external actions (`/Launch`), and zero embedded executables.
4. **Confidentiality (AES-256-GCM)**: Plaintext is encrypted with AES-256-GCM using a cryptographically random 96-bit nonce. Outside parties inspecting the PDF structure only see opaque hex strings.
5. **Anti-Transplant Protection (Document Binding)**: Cryptographically binds the metadata to the host PDF's structural fingerprint (trailer and page tree). If an attacker copies the `/PieceInfo` block to another PDF document, verification fails with `DocumentMismatch`.
6. **Tamper-Proof Verification (HMAC-SHA256)**: Any byte modification to the ciphertext, app name, nonce, or binding immediately triggers `WrongKey` or `Tampered`.

> [!NOTE]
> When flattening during fill, **all non-signature fields are flattened**, but **signature fields remain interactive (`/FT /Sig`)** so they can still be signed cryptographically later. Signature fields are never stripped.
