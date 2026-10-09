#!/usr/bin/env python3
"""
Generate a professional, fully-compliant ISO 32000-1 Interactive PDF Form
containing all standard AcroForm field types:
- Text (/Tx)
- Date (/Tx with JavaScript AFDate format)
- Checkbox (/Btn)
- Radio button group (/Btn)
- ComboBox / Dropdown (/Ch)
- ListBox (/Ch)
- Image Field (/Btn with /MK /IF)
- Digital Signature (/FT /Sig)
"""

import sys
from pathlib import Path

class PdfWriter:
    def __init__(self):
        self.objects = [] # list of (obj_id, bytes_content)
        self.next_id = 1

    def allocate_id(self):
        obj_id = self.next_id
        self.next_id += 1
        return obj_id

    def add_object(self, obj_id, content):
        if isinstance(content, str):
            content = content.encode('utf-8')
        self.objects.append((obj_id, content))
        return obj_id

    def build(self):
        # Sort objects by ID
        self.objects.sort(key=lambda x: x[0])
        
        out = bytearray()
        out.extend(b"%PDF-1.7\r\n%\xe2\xe3\xcf\xd3\r\n")
        
        offsets = {}
        for obj_id, content in self.objects:
            offsets[obj_id] = len(out)
            out.extend(f"{obj_id} 0 obj\r\n".encode('ascii'))
            out.extend(content)
            out.extend(b"\r\nendobj\r\n")

        # xref
        xref_offset = len(out)
        max_id = self.objects[-1][0] if self.objects else 0
        total_objects = max_id + 1

        out.extend(b"xref\r\n")
        out.extend(f"0 {total_objects}\r\n".encode('ascii'))
        out.extend(b"0000000000 65535 f \r\n")
        for i in range(1, total_objects):
            if i in offsets:
                out.extend(f"{offsets[i]:010d} 00000 n \r\n".encode('ascii'))
            else:
                out.extend(b"0000000000 65535 f \r\n")

        # trailer
        out.extend(b"trailer\r\n<<\r\n")
        out.extend(f"  /Size {total_objects}\r\n".encode('ascii'))
        out.extend(b"  /Root 1 0 R\r\n")
        out.extend(b">>\r\nstartxref\r\n")
        out.extend(f"{xref_offset}\r\n%%EOF\r\n".encode('ascii'))
        return bytes(out)

def create_standard_form(output_path):
    pdf = PdfWriter()

    # Pre-allocate key object IDs
    catalog_id = pdf.allocate_id()   # 1
    pages_id = pdf.allocate_id()     # 2
    page_id = pdf.allocate_id()      # 3
    acroform_id = pdf.allocate_id()  # 4
    font_helv_id = pdf.allocate_id() # 5
    font_bold_id = pdf.allocate_id() # 6
    content_id = pdf.allocate_id()   # 7

    # Field IDs
    f_fullname = pdf.allocate_id()
    f_email = pdf.allocate_id()
    f_phone = pdf.allocate_id()
    f_birthdate = pdf.allocate_id()
    f_jobtitle = pdf.allocate_id()
    f_department = pdf.allocate_id()
    f_skills = pdf.allocate_id()
    f_photo = pdf.allocate_id()
    f_gender_parent = pdf.allocate_id()
    f_gender_male = pdf.allocate_id()
    f_gender_female = pdf.allocate_id()
    f_agree = pdf.allocate_id()
    f_newsletter = pdf.allocate_id()
    f_sig_applicant = pdf.allocate_id()
    f_sig_manager = pdf.allocate_id()

    # Checkbox appearances
    cb_on_agree = pdf.allocate_id()
    cb_off_agree = pdf.allocate_id()
    cb_on_news = pdf.allocate_id()
    cb_off_news = pdf.allocate_id()

    # Radio appearances
    rad_on_male = pdf.allocate_id()
    rad_off_male = pdf.allocate_id()
    rad_on_female = pdf.allocate_id()
    rad_off_female = pdf.allocate_id()

    all_fields = [
        f_fullname, f_email, f_phone, f_birthdate, f_jobtitle,
        f_department, f_skills, f_photo, f_gender_parent,
        f_agree, f_newsletter, f_sig_applicant, f_sig_manager
    ]

    all_annots = [
        f_fullname, f_email, f_phone, f_birthdate, f_jobtitle,
        f_department, f_skills, f_photo, f_gender_male, f_gender_female,
        f_agree, f_newsletter, f_sig_applicant, f_sig_manager
    ]

    # 1. Catalog
    pdf.add_object(catalog_id, f"""<<
  /Type /Catalog
  /Pages {pages_id} 0 R
  /AcroForm {acroform_id} 0 R
>>""")

    # 2. Pages
    pdf.add_object(pages_id, f"""<<
  /Type /Pages
  /Kids [ {page_id} 0 R ]
  /Count 1
>>""")

    # 3. Page (A4: 595.28 x 841.89)
    annots_str = " ".join(f"{x} 0 R" for x in all_annots)
    pdf.add_object(page_id, f"""<<
  /Type /Page
  /Parent {pages_id} 0 R
  /MediaBox [ 0 0 595.28 841.89 ]
  /Contents {content_id} 0 R
  /Resources <<
    /Font <<
      /Helv {font_helv_id} 0 R
      /HelvB {font_bold_id} 0 R
    >>
    /ProcSet [ /PDF /Text /ImageB /ImageC /ImageI ]
  >>
  /Annots [ {annots_str} ]
>>""")

    # 4. AcroForm
    fields_str = " ".join(f"{x} 0 R" for x in all_fields)
    pdf.add_object(acroform_id, f"""<<
  /Fields [ {fields_str} ]
  /NeedAppearances true
  /DR <<
    /Font <<
      /Helv {font_helv_id} 0 R
      /HelvB {font_bold_id} 0 R
    >>
  >>
  /DA (/Helv 10 Tf 0 0 0 rg)
  /SigFlags 3
>>""")

    # 5. Standard Type1 Fonts
    pdf.add_object(font_helv_id, """<<
  /Type /Font
  /Subtype /Type1
  /BaseFont /Helvetica
  /Encoding /WinAnsiEncoding
>>""")

    pdf.add_object(font_bold_id, """<<
  /Type /Font
  /Subtype /Type1
  /BaseFont /Helvetica-Bold
  /Encoding /WinAnsiEncoding
>>""")

    # Page Visual Background Stream
    # Generates a clean, modern, enterprise layout matching the interactive widgets
    stream_content = """
q
% Background canvas
1 1 1 rg
0 0 595.28 841.89 re f

% Top Banner
0.08 0.14 0.25 rg
0 760 595.28 82 re f

% Gold accent line
0.85 0.65 0.13 rg
0 758 595.28 2 re f

% Header Text
BT
/HelvB 16 Tf
1 1 1 rg
45 802 Td
(ENTERPRISE EMPLOYMENT APPLICATION FORM) Tj
ET

BT
/Helv 9.5 Tf
0.75 0.82 0.92 rg
45 778 Td
(Official ISO 32000-1 Interactive PDF Specification Template | Pure Rust & WASM Ready) Tj
ET

% SECTION 1: PERSONAL INFORMATION
0.94 0.96 0.98 rg
40 722 515 22 re f
0.2 0.35 0.6 rg
40 722 4 22 re f

BT
/HelvB 10.5 Tf
0.1 0.18 0.35 rg
52 729 Td
(1. PERSONAL INFORMATION) Tj
ET

% Field Labels: Section 1
BT
/Helv 8.5 Tf
0.3 0.3 0.3 rg
45 700 Td (Full Legal Name *) Tj
305 700 Td (Email Address *) Tj
45 648 Td (Phone Number *) Tj
305 648 Td (Date of Birth (DD/MM/YYYY) *) Tj
445 648 Td (Gender) Tj
ET

% Radio Labels
BT
/Helv 8.5 Tf
0.25 0.25 0.25 rg
465 621 Td (Male) Tj
518 621 Td (Female) Tj
ET

% Field visual bounding box outlines (Section 1)
0.82 0.85 0.9 rg
0.75 w
45 665 245 24 re S
305 665 245 24 re S
45 615 245 24 re S
305 615 125 24 re S

% SECTION 2: PROFESSIONAL & POSITION DETAILS
0.94 0.96 0.98 rg
40 575 515 22 re f
0.2 0.35 0.6 rg
40 575 4 22 re f

BT
/HelvB 10.5 Tf
0.1 0.18 0.35 rg
52 582 Td
(2. PROFESSIONAL & POSITION DETAILS) Tj
ET

% Field Labels: Section 2
BT
/Helv 8.5 Tf
0.3 0.3 0.3 rg
45 553 Td (Applied Position / Job Title *) Tj
305 553 Td (Department *) Tj
45 498 Td (Core Competencies & Skills (ListBox)) Tj
330 498 Td (Applicant Photo (Image Field)) Tj
ET

% Field visual bounding box outlines (Section 2)
45 520 245 24 re S
305 520 245 24 re S
45 425 245 65 re S

% Photo frame (dashed line)
[3 3] 0 d
0.7 0.7 0.7 rg
330 415 140 75 re S
[] 0 d

BT
/Helv 8 Tf
0.6 0.6 0.6 rg
355 450 Td ([ Click or fill photo ]) Tj
ET

% SECTION 3: DECLARATIONS & TERMS
0.94 0.96 0.98 rg
40 375 515 22 re f
0.2 0.35 0.6 rg
40 375 4 22 re f

BT
/HelvB 10.5 Tf
0.1 0.18 0.35 rg
52 382 Td
(3. DECLARATION & TERMS) Tj
ET

BT
/Helv 8.5 Tf
0.2 0.2 0.2 rg
68 348 Td (I certify that all statements given in this application are true, accurate and complete.) Tj
68 322 Td (I agree to receive electronic employment notifications, verification alerts, and updates.) Tj
ET

% SECTION 4: DIGITAL SIGNATURES & VERIFICATION
0.94 0.96 0.98 rg
40 280 515 22 re f
0.2 0.35 0.6 rg
40 280 4 22 re f

BT
/HelvB 10.5 Tf
0.1 0.18 0.35 rg
52 287 Td
(4. DIGITAL SIGNATURES & VERIFICATION (PKCS#7 / CMS)) Tj
ET

% Signature Boxes (Dashed)
[4 3] 0 d
0.65 0.68 0.75 rg
45 155 240 105 re S
310 155 240 105 re S
[] 0 d

% Signature Box Labels
BT
/HelvB 8.5 Tf
0.2 0.25 0.35 rg
55 245 Td (Applicant Digital Signature (/Sig)) Tj
320 245 Td (Authorized Approver / Manager (/Sig)) Tj
ET

BT
/Helv 8 Tf
0.5 0.5 0.5 rg
55 200 Td ([ Digitally sign with Certificate ]) Tj
55 185 Td (Supports RSA PKCS#1 & ECDSA P-384) Tj
320 200 Td ([ Authorized Seal & Signature ]) Tj
320 185 Td (Locks document upon execution) Tj
ET

% Footer
0.88 0.88 0.9 rg
40 45 515 1 re f

BT
/Helv 7.5 Tf
0.45 0.45 0.45 rg
40 32 Td (Document ID: REF-APP-2026-STD  |  ISO 32000-1 AcroForm  |  pdftoolkit-core Engine) Tj
495 32 Td (Page 1 of 1) Tj
ET
Q
""".strip()

    pdf.add_object(content_id, f"""<<
  /Length {len(stream_content.encode('utf-8'))}
>>
stream
{stream_content}
endstream""")

    # ------------------ ACROFORM FIELD DEFINITIONS ------------------

    # 1. full_name (Text Field)
    pdf.add_object(f_fullname, f"""<<
  /Type /Annot
  /Subtype /Widget
  /FT /Tx
  /T (full_name)
  /V ()
  /Rect [ 45 665 290 689 ]
  /P {page_id} 0 R
  /F 4
  /BS << /W 1 /S /S >>
  /MK << /BC [ 0.8 0.85 0.9 ] /BG [ 0.98 0.99 1.0 ] >>
  /DA (/Helv 9.5 Tf 0 0 0 rg)
>>""")

    # 2. email (Text Field)
    pdf.add_object(f_email, f"""<<
  /Type /Annot
  /Subtype /Widget
  /FT /Tx
  /T (email)
  /V ()
  /Rect [ 305 665 550 689 ]
  /P {page_id} 0 R
  /F 4
  /BS << /W 1 /S /S >>
  /MK << /BC [ 0.8 0.85 0.9 ] /BG [ 0.98 0.99 1.0 ] >>
  /DA (/Helv 9.5 Tf 0 0 0 rg)
>>""")

    # 3. phone (Text Field)
    pdf.add_object(f_phone, f"""<<
  /Type /Annot
  /Subtype /Widget
  /FT /Tx
  /T (phone)
  /V ()
  /Rect [ 45 615 290 639 ]
  /P {page_id} 0 R
  /F 4
  /BS << /W 1 /S /S >>
  /MK << /BC [ 0.8 0.85 0.9 ] /BG [ 0.98 0.99 1.0 ] >>
  /DA (/Helv 9.5 Tf 0 0 0 rg)
>>""")

    # 4. birth_date (Date Field with AFDate JavaScript)
    pdf.add_object(f_birthdate, f"""<<
  /Type /Annot
  /Subtype /Widget
  /FT /Tx
  /T (birth_date)
  /V ()
  /Rect [ 305 615 430 639 ]
  /P {page_id} 0 R
  /F 4
  /BS << /W 1 /S /S >>
  /MK << /BC [ 0.8 0.85 0.9 ] /BG [ 0.98 0.99 1.0 ] >>
  /DA (/Helv 9.5 Tf 0 0 0 rg)
  /AA <<
    /F << /S /JavaScript /JS (AFDate_FormatEx("dd/mm/yyyy");) >>
    /K << /S /JavaScript /JS (AFDate_KeystrokeEx("dd/mm/yyyy");) >>
  >>
>>""")

    # 5. job_title (Text Field)
    pdf.add_object(f_jobtitle, f"""<<
  /Type /Annot
  /Subtype /Widget
  /FT /Tx
  /T (job_title)
  /V ()
  /Rect [ 45 520 290 544 ]
  /P {page_id} 0 R
  /F 4
  /BS << /W 1 /S /S >>
  /MK << /BC [ 0.8 0.85 0.9 ] /BG [ 0.98 0.99 1.0 ] >>
  /DA (/Helv 9.5 Tf 0 0 0 rg)
>>""")

    # 6. department (ComboBox / Dropdown: /Ff 131072)
    pdf.add_object(f_department, f"""<<
  /Type /Annot
  /Subtype /Widget
  /FT /Ch
  /Ff 131072
  /T (department)
  /Opt [ (Engineering) (Finance) (Human Resources) (Legal) (Marketing) (Operations) ]
  /V (Engineering)
  /Rect [ 305 520 550 544 ]
  /P {page_id} 0 R
  /F 4
  /BS << /W 1 /S /S >>
  /MK << /BC [ 0.8 0.85 0.9 ] /BG [ 0.98 0.99 1.0 ] >>
  /DA (/Helv 9.5 Tf 0 0 0 rg)
>>""")

    # 7. skills (ListBox: /Ff 0)
    pdf.add_object(f_skills, f"""<<
  /Type /Annot
  /Subtype /Widget
  /FT /Ch
  /Ff 0
  /T (skills)
  /Opt [ (Rust) (WebAssembly) (TypeScript) (Python) (Cloud Architecture) (DevOps & CI) ]
  /V (Rust)
  /Rect [ 45 425 290 490 ]
  /P {page_id} 0 R
  /F 4
  /BS << /W 1 /S /S >>
  /MK << /BC [ 0.8 0.85 0.9 ] /BG [ 0.98 0.99 1.0 ] >>
  /DA (/Helv 9 Tf 0 0 0 rg)
>>""")

    # 8. photo (Image Pushbutton Field: /Ff 65536 with /MK /IF)
    pdf.add_object(f_photo, f"""<<
  /Type /Annot
  /Subtype /Widget
  /FT /Btn
  /Ff 65536
  /T (photo)
  /Rect [ 330 415 470 490 ]
  /P {page_id} 0 R
  /F 4
  /BS << /W 1 /S /D >>
  /MK <<
    /BC [ 0.7 0.7 0.7 ]
    /BG [ 0.96 0.96 0.96 ]
    /IF << /SW /B /S /P >>
  >>
>>""")

    # 9. Radio Buttons (gender: Male / Female)
    # Parent Field
    pdf.add_object(f_gender_parent, f"""<<
  /FT /Btn
  /Ff 49152
  /T (gender)
  /V /Off
  /Kids [ {f_gender_male} 0 R {f_gender_female} 0 R ]
>>""")

    # Male Radio Widget
    pdf.add_object(f_gender_male, f"""<<
  /Type /Annot
  /Subtype /Widget
  /Parent {f_gender_parent} 0 R
  /Rect [ 448 619 460 631 ]
  /P {page_id} 0 R
  /F 4
  /AS /Off
  /AP <<
    /N << /Off {rad_off_male} 0 R /Male {rad_on_male} 0 R >>
  >>
>>""")

    # Female Radio Widget
    pdf.add_object(f_gender_female, f"""<<
  /Type /Annot
  /Subtype /Widget
  /Parent {f_gender_parent} 0 R
  /Rect [ 502 619 514 631 ]
  /P {page_id} 0 R
  /F 4
  /AS /Off
  /AP <<
    /N << /Off {rad_off_female} 0 R /Female {rad_on_female} 0 R >>
  >>
>>""")

    # Radio Appearances
    radio_off_stream = "q 0.8 0.8 0.8 rg 0.4 0.4 0.4 RG 0.5 w 6 6 5.5 0 360 arc B Q"
    radio_on_stream = "q 0.8 0.8 0.8 rg 0.2 0.4 0.8 RG 0.5 w 6 6 5.5 0 360 arc B 0.1 0.3 0.7 rg 6 6 3 0 360 arc f Q"
    
    for rid in (rad_off_male, rad_off_female):
        pdf.add_object(rid, f"""<<
  /Type /XObject /Subtype /Form /BBox [ 0 0 12 12 ]
  /Length {len(radio_off_stream)}
>>
stream
{radio_off_stream}
endstream""")

    for rid in (rad_on_male, rad_on_female):
        pdf.add_object(rid, f"""<<
  /Type /XObject /Subtype /Form /BBox [ 0 0 12 12 ]
  /Length {len(radio_on_stream)}
>>
stream
{radio_on_stream}
endstream""")

    # 10. Checkboxes: agree_terms & newsletter
    pdf.add_object(f_agree, f"""<<
  /Type /Annot
  /Subtype /Widget
  /FT /Btn
  /T (agree_terms)
  /V /Off
  /AS /Off
  /Rect [ 45 344 60 359 ]
  /P {page_id} 0 R
  /F 4
  /AP <<
    /N << /Off {cb_off_agree} 0 R /Yes {cb_on_agree} 0 R >>
  >>
>>""")

    pdf.add_object(f_newsletter, f"""<<
  /Type /Annot
  /Subtype /Widget
  /FT /Btn
  /T (newsletter)
  /V /Off
  /AS /Off
  /Rect [ 45 318 60 333 ]
  /P {page_id} 0 R
  /F 4
  /AP <<
    /N << /Off {cb_off_news} 0 R /Yes {cb_on_news} 0 R >>
  >>
>>""")

    # Checkbox Appearances
    cb_off_stream = "q 1 1 1 rg 0.4 0.4 0.4 RG 0.75 w 0.5 0.5 14 14 re B Q"
    cb_on_stream = "q 1 1 1 rg 0.1 0.4 0.8 RG 0.75 w 0.5 0.5 14 14 re B 0.1 0.3 0.7 rg 1.5 w 3 7.5 m 6 3.5 l 12 12 l S Q"

    for cid in (cb_off_agree, cb_off_news):
        pdf.add_object(cid, f"""<<
  /Type /XObject /Subtype /Form /BBox [ 0 0 15 15 ]
  /Length {len(cb_off_stream)}
>>
stream
{cb_off_stream}
endstream""")

    for cid in (cb_on_agree, cb_on_news):
        pdf.add_object(cid, f"""<<
  /Type /XObject /Subtype /Form /BBox [ 0 0 15 15 ]
  /Length {len(cb_on_stream)}
>>
stream
{cb_on_stream}
endstream""")

    # 11. Digital Signature: Signature_Applicant
    pdf.add_object(f_sig_applicant, f"""<<
  /Type /Annot
  /Subtype /Widget
  /FT /Sig
  /T (Signature_Applicant)
  /Rect [ 45 155 285 260 ]
  /P {page_id} 0 R
  /F 4
  /Lock <<
    /Type /SigFieldLock
    /Action /Include
    /Fields [ (full_name) (email) (phone) (birth_date) (job_title) (agree_terms) ]
  >>
>>""")

    # 12. Digital Signature: Signature_Manager
    pdf.add_object(f_sig_manager, f"""<<
  /Type /Annot
  /Subtype /Widget
  /FT /Sig
  /T (Signature_Manager)
  /Rect [ 310 155 550 260 ]
  /P {page_id} 0 R
  /F 4
  /Lock <<
    /Type /SigFieldLock
    /Action /All
  >>
>>""")

    # Output file
    pdf_bytes = pdf.build()
    Path(output_path).write_bytes(pdf_bytes)
    print(f"Generated standard PDF form: {output_path} ({len(pdf_bytes)} bytes)")

if __name__ == "__main__":
    out_file = sys.argv[1] if len(sys.argv) > 1 else "reference/standard_form.pdf"
    create_standard_form(out_file)
