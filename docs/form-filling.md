---
layout: default
title: Form Filling & Metadata
nav_order: 5
description: "AcroForm discovery, intelligent field filling, flattening, and ISO 32000-1 PieceInfo cryptographic lock"
---

# Form Filling & Metadata Management

## Form Field Discovery API (`get fields`)

> For comprehensive specifications, field flag tables, and detailed JSON schemas for all 10 individual field types (Text, Date, ComboBox, ListBox, Checkbox, Radio, Image, Signature, Button, Barcode), see the dedicated [**Form Fields Discovery API Guide**](form-fields-api.md).

`pdffiller` provides a zero-dependency, ultra-fast field inspection engine across Rust, WebAssembly, and the CLI. It parses complex ISO 32000-1 AcroForm trees, resolving inherited attributes, widget bounding boxes, field formats, choice configurations (multi-select, custom options), and digital signatures.

### API Entry Points

| Environment | Function / Command | Signature & Return Type |
|---|---|---|
| **Rust Native** | [`get_form_fields`](file:///e:/10_Learning/Rust/PDFFiller/src/lib.rs) | `get_form_fields(template: &[u8]) -> Result<Vec<FormField>, String>` |
| **Rust JSON** | [`form_fields_json`](file:///e:/10_Learning/Rust/PDFFiller/src/lib.rs) | `form_fields_json(template: &[u8]) -> Result<String, String>` |
| **WebAssembly (WASM)** | `get_form_fields_result` | `get_form_fields_result(template: Uint8Array): string` *(JSON string)* |
| **CLI** | `pdffiller fields` | `pdffiller fields <input.pdf>` |

### Rust Example

```rust
use pdftoolkit_core::{get_form_fields, FormFieldType};

let template = std::fs::read("enterprise_application.pdf")?;
let fields = get_form_fields(&template)?;

for field in &fields {
    println!("{}: {:?} (value: {:?})", field.name, field.field_type, field.value);

    // Choice field capabilities (ComboBox & ListBox)
    if let Some(multi) = field.multi_select {
        println!("  Multi-selection allowed: {multi}");
    }
    if let Some(custom) = field.custom_option {
        println!("  Custom options / editable: {custom}");
    }
    
    // Extracted date formats (e.g. "dd/mm/yyyy")
    if let Some(format) = &field.date_format {
        println!("  Date format: {}", format);
    }
    
    // Digital signatures
    if field.field_type == FormFieldType::Signature {
        println!("  Signed status: {:?}", field.signed);
    }
}
```

### WebAssembly (JavaScript/TypeScript) Example

```javascript
import init, { get_form_fields_result } from './pdftoolkit_core.js';

await init();
const pdfBuffer = new Uint8Array(await (await fetch('/template.pdf')).arrayBuffer());

// Returns structured JSON string
const jsonString = get_form_fields_result(pdfBuffer);
const fields = JSON.parse(jsonString);

fields.forEach(field => {
  console.log(`Field ${field.name} (${field.field_type}):`, field.value);
  if (field.multi_select) {
    console.log(`  Allows multiple selection`);
  }
  if (field.custom_option) {
    console.log(`  Allows custom options / free text`);
  }
});
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
| **ComboBox** | `FormFieldType::ComboBox` | Dropdown choice field | String or `null`. Supports `multi_select` and `custom_option` |
| **ListBox** | `FormFieldType::ListBox` | Scrollable list choice field | String (single) or Array of Strings (multi-select). Supports `multi_select` and `custom_option` |
| **Signature** | `FormFieldType::Signature` | Digital signature `/Sig` field | Visual signature image Data URL (or signer name), plus structured `signature` |
| **Button** | `FormFieldType::Button` | Action button | Pushbutton export state |
| **Barcode** | `FormFieldType::Barcode` | 2D/Paper form barcode | Barcode raw value |

---

## Form Field Return Schema (`FormField`)

Every field returned by `get_form_fields` or `get_form_fields_result` conforms to the following schema:

| Property | Type | Description |
|---|---|---|
| `id` | `string` | PDF indirect object ID (e.g. `"14 0 R"`). |
| `name` | `string` | Fully-qualified form field name hierarchy (e.g. `"department"`, `"full_name"`). |
| `field_type` | `string` | Form field type enum (`"Text"`, `"Date"`, `"Image"`, `"Checkbox"`, `"Radio"`, `"ComboBox"`, `"ListBox"`, `"Signature"`, `"Button"`, `"Barcode"`). |
| `page` | `number \| null` | 1-based page number where the field appears. |
| `rect` | `[number, number, number, number] \| null` | Bounding box coordinates `[x1, y1, x2, y2]` in standard PDF points. |
| `value` | `string \| string[] \| null` | Current resolved field value. Single string for Text/Date/ComboBox, array of strings for multi-select ListBox, Base64 data URL for Image/Signature, or `null` if empty. |
| `default_value` | `any \| null` | Field default value (`/DV`), if defined. |
| `required` | `boolean` | `true` if input is mandatory (ISO 32000-1 bit 2). |
| `read_only` | `boolean` | `true` if field is locked against modification (ISO 32000-1 bit 1). |
| `visible` | `boolean` | `true` if the widget annotation is visible on page. |
| `enabled` | `boolean` | `true` if the field is active (interactive and not read-only). |
| `tooltip` | `string \| null` | Tooltip or alternate descriptive text (`/TU`). |
| `options` | `Array<{ value: string, label: string }>` | Selectable options for ComboBox, ListBox, and Button fields. |
| `flags` | `number` | Raw integer field flags (`/Ff`). |
| `locations` | `Array<FieldLocation>` | Bounding boxes and pages for all widget instances of this field. |
| `signed` | `boolean \| null` | `true` if digitally signed, `false` if unsigned (ready to sign), omitted for non-signature fields. |
| `date_format` | `string \| null` | Detected date format pattern (e.g. `"dd/mm/yyyy"` or `"yyyy-mm-dd"`) for Date fields. |
| `signature` | `SignatureInfo \| null` | Cryptographic signature & X.509 certificate metadata if signed. |
| `multi_select` | `boolean \| null` | **Choice fields only (`ComboBox`, `ListBox`)**: `true` if multiple items may be selected simultaneously (ISO 32000-1 Table 230 bit 22 / MultiSelect). |
| `editable` / `custom_option` | `boolean \| null` | **Choice fields only (`ComboBox`, `ListBox`)**: `true` if custom options / free text entries not present in `options` are permitted (ISO 32000-1 Table 230 bit 19 / Edit). |

### Choice Field Capabilities: ComboBox vs ListBox

Choice fields (`/Ch`) in PDF define two critical flags:

1. **`multi_select` (Bit 22 / MultiSelect: 2,097,152)**:
   - When `true`, multiple items can be selected at the same time.
   - The returned `value` will be an array of strings: `["TypeScript", "Python"]`.
2. **`editable` / `custom_option` (Bit 19 / Edit: 262,144)**:
   - When `true`, the user is not restricted to the predefined `options` array and can enter arbitrary custom text.
   - For example, `department` with `editable: true` accepts `"Cái gì vậy"` even if it was not in the predefined list of departments.

---

### Example JSON Output

```json
[
  {
    "id": "14 0 R",
    "name": "full_name",
    "field_type": "Text",
    "page": 1,
    "rect": [45.0, 665.0, 290.0, 689.0],
    "value": "Nguyễn Văn A",
    "default_value": null,
    "required": true,
    "read_only": false,
    "visible": true,
    "enabled": true,
    "tooltip": null,
    "options": [],
    "flags": 2,
    "locations": [
      { "page": 1, "rect": [45.0, 665.0, 290.0, 689.0], "visible": true, "enabled": true }
    ]
  },
  {
    "id": "19 0 R",
    "name": "department",
    "field_type": "ComboBox",
    "page": 1,
    "rect": [305.0, 520.0, 550.0, 544.0],
    "value": "Cái gì vậy",
    "default_value": null,
    "required": false,
    "read_only": false,
    "visible": true,
    "enabled": true,
    "tooltip": null,
    "options": [
      { "value": "Engineering", "label": "Engineering" },
      { "value": "Finance", "label": "Finance" },
      { "value": "Human Resources", "label": "Human Resources" },
      { "value": "Legal", "label": "Legal" },
      { "value": "Marketing", "label": "Marketing" },
      { "value": "Operations", "label": "Operations" }
    ],
    "flags": 393216,
    "locations": [
      { "page": 1, "rect": [305.0, 520.0, 550.0, 544.0], "visible": true, "enabled": true }
    ],
    "multi_select": false,
    "editable": true,
    "custom_option": true
  },
  {
    "id": "20 0 R",
    "name": "skills",
    "field_type": "ListBox",
    "page": 1,
    "rect": [45.0, 425.0, 290.0, 490.0],
    "value": ["TypeScript", "Python"],
    "default_value": null,
    "required": false,
    "read_only": false,
    "visible": true,
    "enabled": true,
    "tooltip": null,
    "options": [
      { "value": "Rust", "label": "Rust" },
      { "value": "WebAssembly", "label": "WebAssembly" },
      { "value": "TypeScript", "label": "TypeScript" },
      { "value": "Python", "label": "Python" },
      { "value": "Cloud Architecture", "label": "Cloud Architecture" },
      { "value": "DevOps & CI", "label": "DevOps & CI" }
    ],
    "flags": 2097152,
    "locations": [
      { "page": 1, "rect": [45.0, 425.0, 290.0, 490.0], "visible": true, "enabled": true }
    ],
    "multi_select": true,
    "editable": false,
    "custom_option": false
  },
  {
    "id": "25 0 R",
    "name": "Signature_Applicant",
    "field_type": "Signature",
    "page": 1,
    "rect": [45.0, 185.0, 280.0, 310.0],
    "value": null,
    "default_value": null,
    "required": false,
    "read_only": false,
    "visible": true,
    "enabled": true,
    "tooltip": null,
    "options": [],
    "flags": 0,
    "locations": [
      { "page": 1, "rect": [45.0, 185.0, 280.0, 310.0], "visible": true, "enabled": true }
    ],
    "signed": false
  }
]
```

---

## Field Value Resolution Mechanics

The discovery engine resolves field values across complex PDF AcroForm structures:
1. **Direct Field `/V`**: Value defined directly on the field dictionary.
2. **Parent Inheritance**: If `/V` is missing on a child node, parent hierarchy dictionaries are traversed upwards.
3. **Widget State Resolution**: For fields where values reside on individual widget annotations, widget `/V` and active appearance state `/AS` (e.g. checkbox on-state vs `"Off"`) are inspected.
4. **Visual Appearance Streams**: For Image fields and signed Signature fields, appearance streams (`/AP` &rarr; `/N`) are traversed to locate the embedded `XObject` image stream, returning a Base64 data URL (`data:image/jpeg;base64,...`).

---

## Structured Rust `FormField` Model

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    pub id: String,                         // e.g. "14 0 R"
    pub name: String,                       // e.g. "department"
    pub field_type: FormFieldType,
    pub page: Option<usize>,                // 1-indexed page
    pub rect: Option<[f64; 4]>,             // [x1, y1, x2, y2]
    pub value: Option<serde_json::Value>,   // Current value (string, array of strings, or Base64 data URL)
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
    pub multi_select: Option<bool>,         // Choice fields: allows multiple selection
    pub editable: Option<bool>,             // Choice fields: allows entering custom options
    pub custom_option: Option<bool>,        // Alias for editable
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
