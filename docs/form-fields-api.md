---
layout: default
title: Form Fields Discovery API
nav_order: 4
description: "Comprehensive guide to AcroForm field inspection, field types, properties, and JSON schema"
---

# Form Fields Discovery API (`get fields`)

The **Form Fields Discovery API** (`get_form_fields`) is the core inspection engine of `rust-pdf-toolkit`. Before filling, flattening, or signing a PDF form, applications must inspect the document to discover interactive fields, understand their types, extract currently populated values, and identify field constraints.

Built entirely in pure Rust with zero external C/C++ dependencies, this engine runs natively on servers and inside web browsers via WebAssembly with sub-millisecond execution times.

---

## Quick Start & API Entry Points

The discovery API is exposed consistently across all supported environments:

| Environment | Function / Command | Signature & Return Type |
|---|---|---|
| **Rust Native** | `pdffiller_core::get_form_fields` | `get_form_fields(template: &[u8]) -> Result<Vec<FormField>, String>` |
| **Rust JSON** | `pdffiller_core::form_fields_json` | `form_fields_json(template: &[u8]) -> Result<String, String>` |
| **WebAssembly (WASM)** | `get_form_fields_result` | `get_form_fields_result(template: Uint8Array): string` *(JSON)* |
| **CLI** | `pdffiller fields` | `pdffiller fields <input.pdf>` |

### Rust Native Example

```rust
use pdftoolkit_core::{get_form_fields, FormFieldType};

let pdf_bytes = std::fs::read("application_form.pdf")?;
let fields = get_form_fields(&pdf_bytes)?;

for field in &fields {
    println!("Found field: {} [{:?}]", field.name, field.field_type);
    println!("  Current Value: {:?}", field.value);
    
    match field.field_type {
        FormFieldType::ComboBox | FormFieldType::ListBox => {
            println!("  Multi-Select: {:?}", field.multi_select);
            println!("  Custom Option Allowed: {:?}", field.custom_option);
            println!("  Options count: {}", field.options.len());
        }
        FormFieldType::Date => {
            println!("  Date Format: {:?}", field.date_format);
        }
        FormFieldType::Signature => {
            println!("  Signed: {:?} (ReadOnly: {})", field.signed, field.read_only);
        }
        _ => {}
    }
}
```

### WebAssembly (JavaScript / TypeScript) Example

```typescript
import init, { get_form_fields_result } from './pdftoolkit_core.js';

await init();
const pdfBytes = new Uint8Array(await (await fetch('/application.pdf')).arrayBuffer());

// Returns structured JSON string
const jsonString = get_form_fields_result(pdfBytes);
const fields = JSON.parse(jsonString);

for (const field of fields) {
  console.log(`${field.name} (${field.field_type}):`, field.value);
  if (field.multi_select) {
    console.log(`  Allows multiple selection`);
  }
  if (field.custom_option) {
    console.log(`  Allows custom options / free text entry`);
  }
}
```

### CLI Example

```bash
pdffiller fields reference/golden_enterprise_form.pdf
```

---

## FormField Model Specification

Every form field in the JSON response represents a resolved AcroForm node:

| Property | Type | Description |
|---|---|---|
| `id` | `string` | PDF indirect object identifier (e.g. `"14 0 R"`). |
| `name` | `string` | Fully-qualified hierarchical field name (e.g. `"full_name"`, `"department"`). |
| `field_type` | `string` | Precise field type enum: `"Text"`, `"Date"`, `"Image"`, `"Checkbox"`, `"Radio"`, `"ComboBox"`, `"ListBox"`, `"Signature"`, `"Button"`, `"Barcode"`. |
| `page` | `number \| null` | 1-based page number where the primary field widget appears. |
| `rect` | `[number, number, number, number] \| null` | Coordinates `[llx, lly, urx, ury]` in PDF points (bottom-left to top-right). |
| `value` | `string \| string[] \| null` | Current resolved field value. Single string, array of strings (multi-select), Base64 data URI, or `null`. |
| `default_value` | `any \| null` | Initial default value (`/DV`), if defined by the template author. |
| `required` | `boolean` | `true` if completion is required (ISO 32000-1 bit 2). |
| `read_only` | `boolean` | `true` if field is locked against user editing (ISO 32000-1 bit 1). |
| `visible` | `boolean` | `true` if the field widget annotation is visible on page. |
| `enabled` | `boolean` | `true` if the field is active (interactive and not read-only). |
| `tooltip` | `string \| null` | Alternate user-friendly description or tooltip (`/TU`). |
| `options` | `Array<{ value: string, label: string }>` | Available selectable choices for ComboBox, ListBox, and Button fields. |
| `flags` | `number` | Raw integer bitmask representing ISO 32000-1 field flags (`/Ff`). |
| `locations` | `Array<FieldLocation>` | Bounding boxes and page numbers for all widget instances of this field. |
| `signed` | `boolean \| null` | For Signature fields: `true` if signed, `false` if unsigned (signable). `null` for other types. |
| `date_format` | `string \| null` | Detected date format pattern (e.g. `"dd/mm/yyyy"` or `"yyyy-mm-dd"`) for Date fields. |
| `signature` | `SignatureInfo \| null` | Detailed cryptographic certificate and signing metadata if signed. |
| `multi_select` | `boolean \| null` | **Choice fields only (`ComboBox`, `ListBox`)**: `true` if multiple options can be chosen simultaneously. |
| `editable` / `custom_option` | `boolean \| null` | **Choice fields only (`ComboBox`, `ListBox`)**: `true` if users can enter custom free-text options outside predefined `options`. |

---

## Detailed Breakdown by Field Type

Below is the complete architectural specification and JSON schema for each individual field type.

---

### 1. Text Field (`FormFieldType::Text`)

- **ISO 32000-1 Spec**: §12.7.4.3 Text Fields (`/FT /Tx`)
- **Description**: Standard single-line or multi-line text input fields. Supports full UTF-8 Unicode (e.g. Vietnamese `"Nguyễn Văn A"`, Japanese, Arabic, accented Latin).
- **Value Format**: `string` or `null`

#### Text Field Schema Example

```json
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
  "tooltip": "Enter your full legal name",
  "options": [],
  "flags": 2,
  "locations": [
    { "page": 1, "rect": [45.0, 665.0, 290.0, 689.0], "visible": true, "enabled": true }
  ]
}
```

---

### 2. Date Field (`FormFieldType::Date`)

- **ISO 32000-1 Spec**: §12.7.4.3 with Adobe Acrobat Date Format JavaScript actions (`AFDate_FormatEx`)
- **Description**: Specialized text fields that enforce date formatting rules. The discovery engine parses embedded format scripts and extracts the date mask into `date_format`.
- **Value Format**: Formatted date `string` (e.g. `"16/08/1995"`) or `null`
- **Key Properties**:
  - `date_format`: Formatted string mask, such as `"dd/mm/yyyy"`, `"yyyy-mm-dd"`, `"mm/dd/yyyy"`, or `"d-mmm-yyyy"`.

#### Date Field Schema Example

```json
{
  "id": "17 0 R",
  "name": "birth_date",
  "field_type": "Date",
  "page": 1,
  "rect": [305.0, 613.0, 435.0, 637.0],
  "value": "16/08/1995",
  "default_value": null,
  "required": true,
  "read_only": false,
  "visible": true,
  "enabled": true,
  "tooltip": "Date of Birth",
  "options": [],
  "flags": 0,
  "locations": [
    { "page": 1, "rect": [305.0, 613.0, 435.0, 637.0], "visible": true, "enabled": true }
  ],
  "date_format": "dd/mm/yyyy"
}
```

---

### 3. ComboBox / Dropdown (`FormFieldType::ComboBox`)

- **ISO 32000-1 Spec**: §12.7.4.4 Choice Fields (`/FT /Ch`) with bit 18 `Combo` (131,072) set.
- **Description**: Drop-down choice list. Allows selecting an item from predefined options.
- **Key Properties**:
  - `multi_select`: `false` (ComboBox is single-selection).
  - `editable` / `custom_option`: `true` if bit 19 `Edit` (262,144) is set. When `true`, users can type custom free-text options not in the predefined list (e.g. `"Cái gì vậy"`).
  - `options`: List of `{ value, label }` pairs.
- **Value Format**: Single `string` or `null`.

#### ComboBox Schema Example (Editable with Custom Value)

```json
{
  "id": "19 0 R",
  "name": "department",
  "field_type": "ComboBox",
  "page": 1,
  "rect": [305.0, 520.0, 550.0, 544.0],
  "value": "Cái gì vậy",
  "default_value": "Engineering",
  "required": false,
  "read_only": false,
  "visible": true,
  "enabled": true,
  "tooltip": "Select or enter your department",
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
}
```

---

### 4. ListBox (`FormFieldType::ListBox`)

- **ISO 32000-1 Spec**: §12.7.4.4 Choice Fields (`/FT /Ch`) with bit 18 `Combo` clear.
- **Description**: Scrollable list box showing multiple choices simultaneously.
- **Key Properties**:
  - `multi_select`: `true` if bit 22 `MultiSelect` (2,097,152) is set; `false` if only a single item can be selected.
  - `editable` / `custom_option`: `true` if bit 19 `Edit` (262,144) is set.
  - `options`: List of `{ value, label }` choices.
- **Value Format**:
  - When `multi_select == true`: Array of strings `["TypeScript", "Python"]`.
  - When `multi_select == false`: Single string `"TypeScript"`.

#### ListBox Schema Example (Multi-Select Enabled)

```json
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
  "tooltip": "Select your core technical competencies",
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
}
```

---

### 5. Checkbox (`FormFieldType::Checkbox`)

- **ISO 32000-1 Spec**: §12.7.4.2 Button Fields (`/FT /Btn`) without Pushbutton or Radio flags.
- **Description**: Two-state toggle switch.
- **Value Format**: On-state string (e.g. `"Yes"`), `"Off"`, or boolean `true`/`false`.
- **Options**: Contains the active export state name (e.g. `[{"value": "Yes", "label": "Yes"}]`).

#### Checkbox Schema Example

```json
{
  "id": "22 0 R",
  "name": "agree_terms",
  "field_type": "Checkbox",
  "page": 1,
  "rect": [45.0, 375.0, 57.0, 387.0],
  "value": "Yes",
  "default_value": "Off",
  "required": true,
  "read_only": false,
  "visible": true,
  "enabled": true,
  "tooltip": "Accept terms and conditions",
  "options": [
    { "value": "Yes", "label": "Yes" }
  ],
  "flags": 0,
  "locations": [
    { "page": 1, "rect": [45.0, 375.0, 57.0, 387.0], "visible": true, "enabled": true }
  ]
}
```

---

### 6. Radio Button Group (`FormFieldType::Radio`)

- **ISO 32000-1 Spec**: §12.7.4.2 Button Fields (`/FT /Btn`) with bit 16 `Radio` (32,768) set.
- **Description**: Mutually exclusive choice group consisting of a parent field and multiple child widget annotations.
- **Value Format**: Currently selected option export value (e.g. `"Male"` or `"Female"`), or `"Off"`.
- **Locations**: Lists the bounding rects for every radio option button across pages.

#### Radio Button Schema Example

```json
{
  "id": "21 0 R",
  "name": "gender",
  "field_type": "Radio",
  "page": 1,
  "rect": [445.0, 613.0, 457.0, 625.0],
  "value": "Male",
  "default_value": "Off",
  "required": false,
  "read_only": false,
  "visible": true,
  "enabled": true,
  "tooltip": "Select gender",
  "options": [
    { "value": "Male", "label": "Male" },
    { "value": "Female", "label": "Female" }
  ],
  "flags": 32768,
  "locations": [
    { "page": 1, "rect": [445.0, 613.0, 457.0, 625.0], "visible": true, "enabled": true },
    { "page": 1, "rect": [498.0, 613.0, 510.0, 625.0], "visible": true, "enabled": true }
  ]
}
```

---

### 7. Image Field (`FormFieldType::Image`)

- **ISO 32000-1 Spec**: Pushbutton field (`/Btn`) with appearance characteristics (`/MK /I` or `/MK /IF`) designed to render graphics, photos, or stamps.
- **Description**: Used for ID photos, applicant pictures, stamps, and visual logos.
- **Value Format**: Base64 Data URL string (`"data:image/jpeg;base64,..."` or `"data:image/png;base64,..."`), or `null` if empty.

#### Image Field Schema Example

```json
{
  "id": "18 0 R",
  "name": "photo",
  "field_type": "Image",
  "page": 1,
  "rect": [305.0, 425.0, 435.0, 500.0],
  "value": "data:image/jpeg;base64,/9j/4AAQSkZJRg...",
  "default_value": null,
  "required": false,
  "read_only": false,
  "visible": true,
  "enabled": true,
  "tooltip": "Upload headshot photo",
  "options": [],
  "flags": 65536,
  "locations": [
    { "page": 1, "rect": [305.0, 425.0, 435.0, 500.0], "visible": true, "enabled": true }
  ]
}
```

---

### 8. Digital Signature Field (`FormFieldType::Signature`)

- **ISO 32000-1 Spec**: §12.7.4.5 Signature Fields (`/FT /Sig`)
- **Description**: Cryptographic digital signature placeholder or executed signature.
- **Key Properties**:
  - `signed`: `false` when unsigned (ready to sign); `true` when digitally signed.
  - `read_only`: Unsigned fields have `read_only: false`. Once signed, the field is permanently locked with `read_only: true`.
  - `signature`: Contains detailed X.509 certificate and cryptographic metadata when signed.

#### State A: Unsigned Signature Field (Ready for Signing)

```json
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
  "tooltip": "Sign here using digital certificate",
  "options": [],
  "flags": 0,
  "locations": [
    { "page": 1, "rect": [45.0, 185.0, 280.0, 310.0], "visible": true, "enabled": true }
  ],
  "signed": false,
  "signature": null
}
```

#### State B: Executed Digital Signature Field

```json
{
  "id": "25 0 R",
  "name": "Signature_Applicant",
  "field_type": "Signature",
  "page": 1,
  "rect": [45.0, 185.0, 280.0, 310.0],
  "value": "Nguyễn Văn An",
  "default_value": null,
  "required": false,
  "read_only": true,
  "visible": true,
  "enabled": false,
  "tooltip": null,
  "options": [],
  "flags": 1,
  "locations": [
    { "page": 1, "rect": [45.0, 185.0, 280.0, 310.0], "visible": true, "enabled": false }
  ],
  "signed": true,
  "signature": {
    "name": "Nguyen Van An",
    "reason": "Enterprise Employment Application Sign-off",
    "location": "Ho Chi Minh City, VN",
    "contact_info": "nguyen.vana@enterprise.vn",
    "signing_time": "D:20261009192800+07'00'",
    "filter": "Adobe.PPKLite",
    "sub_filter": "adbe.pkcs7.detached",
    "byte_range": [0, 4820, 15060, 890],
    "image": "data:image/png;base64,iVBORw0KGgo...",
    "signer_name": "Nguyen Van An",
    "signer_organization": "Enterprise Corp",
    "issuer": "Global Enterprise Root CA",
    "not_before": "2026-01-01 00:00:00 UTC",
    "not_after": "2028-01-01 00:00:00 UTC",
    "serial_number": "1A:2B:3C:4D:5E:6F"
  }
}
```

---

### 9. Push Button (`FormFieldType::Button`)

- **ISO 32000-1 Spec**: §12.7.4.2 with bit 17 `Pushbutton` (65,536) set.
- **Description**: Action triggers such as "Submit Form", "Reset", or "Print".

```json
{
  "id": "26 0 R",
  "name": "btn_submit",
  "field_type": "Button",
  "page": 1,
  "rect": [440.0, 50.0, 550.0, 75.0],
  "value": null,
  "default_value": null,
  "required": false,
  "read_only": false,
  "visible": true,
  "enabled": true,
  "tooltip": "Submit Application",
  "options": [],
  "flags": 65536,
  "locations": [
    { "page": 1, "rect": [440.0, 50.0, 550.0, 75.0], "visible": true, "enabled": true }
  ]
}
```

---

### 10. Barcode Field (`FormFieldType::Barcode`)

- **ISO 32000-1 Spec**: §12.7.4.3 Text field containing 2D/Paper form `/DataPrep` dictionary.
- **Description**: Encodes form values into QR Code, PDF417, or DataMatrix barcodes for physical processing.

```json
{
  "id": "27 0 R",
  "name": "form_barcode",
  "field_type": "Barcode",
  "page": 1,
  "rect": [45.0, 50.0, 150.0, 100.0],
  "value": "REF-APP-2026-STD|Nguyen Van A|Engineering",
  "default_value": null,
  "required": false,
  "read_only": true,
  "visible": true,
  "enabled": false,
  "tooltip": null,
  "options": [],
  "flags": 1,
  "locations": [
    { "page": 1, "rect": [45.0, 50.0, 150.0, 100.0], "visible": true, "enabled": false }
  ]
}
```

---

## Complete Golden Reference Example

Below is the complete output from `pdffiller fields reference/golden_enterprise_form.pdf`, demonstrating all 10 field types in a single production document:

```json
[
  {
    "id": "14 0 R",
    "name": "full_name",
    "field_type": "Text",
    "page": 1,
    "rect": [45.0, 665.0, 290.0, 689.0],
    "value": "Nguyễn Văn A",
    "required": true,
    "read_only": false,
    "visible": true,
    "enabled": true,
    "options": []
  },
  {
    "id": "15 0 R",
    "name": "email",
    "field_type": "Text",
    "page": 1,
    "rect": [305.0, 665.0, 550.0, 689.0],
    "value": "nguyen.vana@enterprise.vn",
    "required": true,
    "read_only": false,
    "visible": true,
    "enabled": true,
    "options": []
  },
  {
    "id": "16 0 R",
    "name": "phone",
    "field_type": "Text",
    "page": 1,
    "rect": [45.0, 613.0, 290.0, 637.0],
    "value": "+84 987 654 321",
    "required": true,
    "read_only": false,
    "visible": true,
    "enabled": true,
    "options": []
  },
  {
    "id": "17 0 R",
    "name": "birth_date",
    "field_type": "Date",
    "page": 1,
    "rect": [305.0, 613.0, 435.0, 637.0],
    "value": "16/08/1995",
    "date_format": "dd/mm/yyyy",
    "required": true,
    "read_only": false,
    "visible": true,
    "enabled": true,
    "options": []
  },
  {
    "id": "18 0 R",
    "name": "gender",
    "field_type": "Radio",
    "page": 1,
    "rect": [445.0, 613.0, 457.0, 625.0],
    "value": "Male",
    "options": [
      { "value": "Male", "label": "Male" },
      { "value": "Female", "label": "Female" }
    ]
  },
  {
    "id": "19 0 R",
    "name": "department",
    "field_type": "ComboBox",
    "page": 1,
    "rect": [305.0, 520.0, 550.0, 544.0],
    "value": "Cái gì vậy",
    "multi_select": false,
    "editable": true,
    "custom_option": true,
    "options": [
      { "value": "Engineering", "label": "Engineering" },
      { "value": "Finance", "label": "Finance" },
      { "value": "Human Resources", "label": "Human Resources" },
      { "value": "Legal", "label": "Legal" },
      { "value": "Marketing", "label": "Marketing" },
      { "value": "Operations", "label": "Operations" }
    ]
  },
  {
    "id": "20 0 R",
    "name": "skills",
    "field_type": "ListBox",
    "page": 1,
    "rect": [45.0, 425.0, 290.0, 490.0],
    "value": ["TypeScript", "Python"],
    "multi_select": true,
    "editable": false,
    "custom_option": false,
    "options": [
      { "value": "Rust", "label": "Rust" },
      { "value": "WebAssembly", "label": "WebAssembly" },
      { "value": "TypeScript", "label": "TypeScript" },
      { "value": "Python", "label": "Python" },
      { "value": "Cloud Architecture", "label": "Cloud Architecture" },
      { "value": "DevOps & CI", "label": "DevOps & CI" }
    ]
  },
  {
    "id": "21 0 R",
    "name": "agree_terms",
    "field_type": "Checkbox",
    "page": 1,
    "rect": [45.0, 375.0, 57.0, 387.0],
    "value": "Yes",
    "options": [{ "value": "Yes", "label": "Yes" }]
  },
  {
    "id": "22 0 R",
    "name": "newsletter",
    "field_type": "Checkbox",
    "page": 1,
    "rect": [45.0, 345.0, 57.0, 357.0],
    "value": "Yes",
    "options": [{ "value": "Yes", "label": "Yes" }]
  },
  {
    "id": "23 0 R",
    "name": "Signature_Applicant",
    "field_type": "Signature",
    "page": 1,
    "rect": [45.0, 185.0, 280.0, 310.0],
    "value": null,
    "signed": false,
    "read_only": false
  },
  {
    "id": "24 0 R",
    "name": "Signature_Manager",
    "field_type": "Signature",
    "page": 1,
    "rect": [305.0, 185.0, 550.0, 310.0],
    "value": null,
    "signed": false,
    "read_only": false
  }
]
```
