---
layout: default
title: CLI Handbook
nav_order: 7
description: "Complete command-line interface handbook for pdftoolkit"
---

# CLI Handbook

The `pdftoolkit` CLI provides commands for single-file operations as well as multi-threaded batch folder processing.

---

## 1. Form Filling (`pdftoolkit fill`)

Populate form fields with JSON data, with optional flattening and metadata injection:

```bash
# Modern subcommand:
pdftoolkit fill -t template.pdf -d data.json -o filled.pdf --flatten --piece-info piece.json

# Backward-compatible positional syntax:
pdftoolkit template.pdf data.json filled.pdf [piece.json]
```

---

## 2. Document Merging (`pdftoolkit merge`)

Combine multiple PDF files in order, with automatic outline bookmark generation:

```bash
# Merge specific files in order:
pdftoolkit merge -i part1.pdf part2.pdf part3.pdf -o merged.pdf

# Merge directory with bookmarks, initial view mode, and PieceInfo injection:
pdftoolkit merge --dir ./contracts -o merged.pdf \
    --bookmarks \
    --page-mode outlines \
    --flatten \
    --piece-info piece.json
```

---

## 3. Document Splitting (`pdftoolkit split`)

Split documents into individual pages or specific ranges:

```bash
# Split every page into individual files:
pdftoolkit split -i document.pdf -o ./output_pages/ --range all

# Custom page ranges with templated naming and PieceInfo inheritance:
pdftoolkit split -i document.pdf -o ./output_pages/ \
    --range "1-3, 4-5" \
    --naming-pattern "sub_{stem}_{label}.pdf" \
    --inherit-piece-info \
    --flatten
```

---

## 4. Convert PDF to Image (`pdftoolkit to-image`)

High-fidelity PDF page rasterization powered by Google PDFium:

```bash
# Render all pages to PNG with transparent background:
pdftoolkit to-image -i document.pdf -o ./images/ --dpi 150 --format png --transparent

# High-resolution JPEG at 300 DPI with quality 90 and pixel constraints:
pdftoolkit to-image -i document.pdf -o ./images/ \
    --dpi 300 --format jpg --quality 90 \
    --pages "1, 3-5" \
    --max-width 2400 \
    --piece-info audit.json
```

> [!NOTE]
> Rendered images automatically inherit `/PieceInfo` metadata from the source PDF (embedded into PNG `tEXt` or JPEG `COM` chunks).

---

## 5. Extract Embedded Images (`pdftoolkit extract-images`)

Extract raw embedded photos, logos, barcode rasters, and signatures directly from PDF XObjects:

```bash
# Extract all embedded images:
pdftoolkit extract-images -i document.pdf -o ./extracted_images/

# Extract only page 1 images with minimum width/height filter and custom naming:
pdftoolkit extract-images -i document.pdf -o ./extracted_images/ \
    --pages "1" \
    --min-width 100 \
    --min-height 100 \
    --naming-pattern "extracted_{id}_{index}.{ext}"
```

---

## 6. Multi-Threaded Batch Folder Processing (`pdftoolkit batch`)

Parallel multi-core processing of entire directories powered by Rayon:

```bash
# Batch fill: Fill template PDF for an array of records in data.json
pdftoolkit batch fill -t template.pdf -d records.json -o ./filled_folder/ --threads 8

# Batch merge: Merge matching files in a folder with dry-run verification
pdftoolkit batch merge -d ./raw_pdfs -o ./dist/all_in_one.pdf \
    --filter "invoice_*" --bookmarks --dry-run

# Batch split: Split all PDFs in a folder with pattern matching
pdftoolkit batch split -d ./folder -o ./split_folder/ -r "1-2" --naming-pattern "{stem}_part_{label}.pdf"

# Batch convert: Render matching PDFs to images concurrently
pdftoolkit batch to-image -d ./folder -o ./gallery/ \
    --dpi 150 --format png --transparent --filter "*report*"

# Batch extract: Extract all embedded images from all PDFs in a folder
pdftoolkit batch extract-images -d ./folder -o ./all_extracted_images/ --min-width 50

# Batch watermark: Apply watermark across all PDFs in a folder
pdftoolkit batch watermark -d ./folder -o ./watermarked/ --text "CONFIDENTIAL" --opacity 0.15

# Batch number: Apply Bates/page numbering across all PDFs in a folder
pdftoolkit batch number -d ./folder -o ./numbered/ --format "Trang {page} / {total}"

# Batch rotate: Rotate or normalize all PDFs in a folder
pdftoolkit batch rotate -d ./folder -o ./rotated/ --orientation portrait
```

---

## 7. Watermarking & Bates Page Numbering

```bash
# Text watermark with smart diagonal scaling and dynamic placeholders:
pdftoolkit watermark -i document.pdf -o watermarked.pdf \
    --text "BẢN SAO LƯU - {date} - Trang {page}/{total}" \
    --position diagonal --opacity 0.18 --color "#C00000"

# Image watermark (PNG/JPEG logo/seal):
pdftoolkit watermark -i document.pdf -o stamped.pdf \
    --image logo.png --position bottom-right --opacity 0.85

# Bates / Page Numbering header & footer:
pdftoolkit number -i document.pdf -o numbered.pdf \
    --format "DOC-NO-{bates:06d} (Trang {page}/{total})" \
    --position bottom-center --font-size 10 --color "#333333" --start-page 2
```

---

## 8. Page Rotation & Orientation Normalization (`rotate`)

```bash
# Rotate by explicit angle (+90 degrees relative):
pdftoolkit rotate -i document.pdf -o rotated.pdf --angle 90

# Set absolute rotation angle to 180 degrees:
pdftoolkit rotate -i document.pdf -o rotated.pdf --angle 180 --absolute

# Normalize pages to Landscape orientation:
pdftoolkit rotate -i document.pdf -o normalized.pdf --orientation landscape

# Normalize pages to Portrait orientation:
pdftoolkit rotate -i document.pdf -o normalized.pdf --orientation portrait

# Smart text-based orientation auto-detection (analyzes content stream Tm/cm matrices):
pdftoolkit rotate -i scanned_doc.pdf -o upright.pdf --auto-text --fallback-angle 90

# Target specific pages only (e.g. pages 2 and 4):
pdftoolkit rotate -i document.pdf -o rotated.pdf --angle 90 --pages "2, 4" --json
```

---

## 9. Signature Field Management (`add-sig-field` & `remove-sig-field`)

```bash
# Add an invisible cryptographic signature field:
pdftoolkit add-sig-field -i document.pdf -o with_sig.pdf --name "ApprovalSig" --invisible

# Add a visible signature field using preset placement (bottom-right):
pdftoolkit add-sig-field -i document.pdf -o with_sig.pdf --name "Signature1" \
    --position bottom-right --width 160 --height 55 --page 1

# Remove a specific signature field by name:
pdftoolkit remove-sig-field -i with_sig.pdf -o cleaned.pdf --name "ApprovalSig"

# Remove all unsigned signature fields:
pdftoolkit remove-sig-field -i with_sig.pdf -o cleaned.pdf --all-unsigned
```

---

## 10. Digital Signing & Cryptographic Verification (`sign` & `verify`)

```bash
# Digitally sign an existing signature field using an RSA or ECDSA certificate:
pdftoolkit sign -i document.pdf -o signed.pdf \
    --field "Signature1" \
    --cert signer.cert.der \
    --key signer.key.der \
    --reason "Phê duyệt hợp đồng kinh tế" \
    --location "Hà Nội, Việt Nam"

# Auto-create signature field with company stamp/seal behind certificate text:
pdftoolkit sign -i document.pdf -o signed.pdf \
    --field "CompanySealSig" \
    --auto-create-field \
    --position bottom-right \
    --cert company.cert.der \
    --key company.key.der \
    --image "./assets/company_seal.png" \
    --graphic-position behind \
    --reason "Phê duyệt hợp đồng kinh tế" \
    --location "Hà Nội, Việt Nam"

# Auto-create signature field on the fly with handwritten signature image on left:
pdftoolkit sign -i document.pdf -o signed.pdf \
    --field "AutoSig1" \
    --auto-create-field \
    --position bottom-right \
    --cert signer.cert.der \
    --key signer.key.der \
    --image "./assets/handwritten_sig.png" \
    --graphic-position left \
    --reason "Đã duyệt và ký số điện tử"

# Image-only signature appearance (stamp without text overlay):
pdftoolkit sign -i document.pdf -o signed.pdf \
    --field "StampOnly" \
    --cert signer.cert.der \
    --key signer.key.der \
    --image "./assets/stamp.png" \
    --graphic-position image-only

# Cryptographically verify all digital signatures in a PDF:
pdftoolkit verify -i signed.pdf --json
```

### Sign CLI Flags Reference
| Flag | Description | Values / Examples |
|---|---|---|
| `-i, --input <PATH>` | Input PDF document path | `contract.pdf` |
| `-o, --output <PATH>` | Output signed PDF path | `signed.pdf` |
| `--field <NAME>` | Target signature field name | `"Signature1"`, `"AutoSig1"` |
| `--cert <PATH>` | Signer X.509 certificate (DER / PEM) | `signer.cert.der` |
| `--key <PATH>` | Signer private key (DER / PKCS#8) | `signer.key.der` |
| `--image <PATH_OR_BASE64>` | Visual signature image or company stamp | File path or Base64 string |
| `--graphic-position <POS>` | Visual appearance layout position | `left` (default), `right`, `behind`, `image-only`, `text-only` |
| `--auto-create-field` | Create AcroForm field automatically if missing | Flag |
| `--position <POS>` | Auto-created field position preset | `bottom-right`, `bottom-left`, `top-right`, `top-left`, `center` |
| `--page <NUM>` | Target page for auto-created field | 1-based page number |
| `--reason <TEXT>` | Signing reason annotation | `"Phê duyệt hợp đồng"` |
| `--location <TEXT>` | Geographic signing location | `"Hà Nội, Việt Nam"` |
| `--contact-info <TEXT>` | Signer contact information | `"signer@company.vn"` |


---

## 11. Smart Page Removal (`remove-pages`)

```bash
# Remove cover page (page 1):
pdftoolkit remove-pages -i document.pdf -o without_cover.pdf --cover

# Remove back cover (last page):
pdftoolkit remove-pages -i document.pdf -o without_back.pdf --back-cover

# Automatically detect and remove blank/empty pages:
pdftoolkit remove-pages -i document.pdf -o cleaned.pdf --blank

# Remove specific page ranges with keep-pages whitelist override:
pdftoolkit remove-pages -i document.pdf -o cleaned.pdf --pages "2-5" --keep-pages "3" --json
```

---

## 12. Smart Page Cropping (`crop`)

```bash
# Crop margins in PDF points (top, bottom, left, right):
pdftoolkit crop -i document.pdf -o cropped.pdf --margins "36, 36, 40, 40"

# Crop uniform margin on all 4 sides (0.5 inch / 36 points):
pdftoolkit crop -i document.pdf -o cropped.pdf --margins "36"

# Smart content auto-cropping (automatically calculates tight content bounding box):
pdftoolkit crop -i document.pdf -o auto_cropped.pdf --auto-content --padding 18 --json
```

---

## 13. Inspection Commands (`inspect` & `inspect-image`)

```bash
# Inspect PDF AcroForm fields:
pdftoolkit inspect -i template.pdf

# Inspect embedded PieceInfo metadata from PNG or JPEG image:
pdftoolkit inspect-image -i rendered_page.png
```
