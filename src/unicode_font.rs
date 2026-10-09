use lopdf::{Document, Object, ObjectId, Stream, dictionary};
use std::collections::HashMap;

pub static EMBEDDED_FONT_DATA: &[u8] = include_bytes!("../assets/fonts/DejaVuSans.ttf");

#[derive(Debug, Clone)]
pub struct UnicodeFontContext {
    pub font_id: ObjectId,
    pub font_alias: &'static str,
    char_to_byte: HashMap<char, u8>,
}

impl UnicodeFontContext {
    /// Creates and embeds a TrueType Unicode font into the document
    /// with an explicit Differences array and ToUnicode CMap for the specified non-ASCII characters.
    pub fn new(doc: &mut Document, non_ascii_chars: &[char]) -> Self {
        let mut char_to_byte = HashMap::new();
        let mut differences = Vec::new();

        let mapped_chars: Vec<char> = non_ascii_chars.iter().copied().take(128).collect();

        if !mapped_chars.is_empty() {
            differences.push(Object::Integer(128));
            for (idx, &c) in mapped_chars.iter().enumerate() {
                let byte_val = (128 + idx) as u8;
                char_to_byte.insert(c, byte_val);
                let glyph_name = format!("uni{:04X}", c as u32);
                differences.push(Object::Name(glyph_name.into_bytes()));
            }
        }

        // Compress and embed the font program (FontFile2)
        let mut font_file_stream = Stream::new(
            dictionary! {
                "Length1" => Object::Integer(EMBEDDED_FONT_DATA.len() as i64),
            },
            EMBEDDED_FONT_DATA.to_vec(),
        );
        let _ = font_file_stream.compress();
        let font_file_id = doc.add_object(font_file_stream);

        // Widths array for character codes 32..=255 with accurate typographic metrics
        let mut widths = Vec::with_capacity(224);
        for code in 32..=255 {
            let w = if code < 128 {
                char_width(code as u8 as char)
            } else {
                let mapped_char = mapped_chars.get((code - 128) as usize).copied();
                match mapped_char {
                    Some(c) => char_width(c),
                    None => 550,
                }
            };
            widths.push(Object::Integer(w));
        }

        // FontDescriptor
        let font_descriptor = doc.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "DejaVuSans",
            "Flags" => 32,
            "FontBBox" => vec![
                Object::Integer(-500),
                Object::Integer(-300),
                Object::Integer(1200),
                Object::Integer(1000),
            ],
            "ItalicAngle" => Object::Integer(0),
            "Ascent" => Object::Integer(900),
            "Descent" => Object::Integer(-200),
            "CapHeight" => Object::Integer(700),
            "StemV" => Object::Integer(80),
            "FontFile2" => Object::Reference(font_file_id),
        });

        // ToUnicode CMap stream
        let mut bfchar_lines = String::new();
        for (idx, &c) in mapped_chars.iter().enumerate() {
            let byte_val = (128 + idx) as u8;
            bfchar_lines.push_str(&format!("<{:02X}> <{:04X}>\n", byte_val, c as u32));
        }

        let tounicode_stream_content = format!(
            "/CIDInit /ProcSet findresource begin\n\
             12 dict begin\n\
             begincmap\n\
             /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
             /CMapName /Custom-ToUnicode def\n\
             /CMapType 2 def\n\
             1 begincodespacerange\n\
             <00> <FF>\n\
             endcodespacerange\n\
             1 beginbfrange\n\
             <20> <7E> <0020>\n\
             endbfrange\n\
             {} beginbfchar\n\
             {}\
             endbfchar\n\
             endcmap\n\
             CMapName currentdict /CMap defineresource pop\n\
             end\n\
             end\n",
            mapped_chars.len(),
            bfchar_lines
        );

        let mut tounicode_stream =
            Stream::new(dictionary! {}, tounicode_stream_content.into_bytes());
        let _ = tounicode_stream.compress();
        let tounicode_id = doc.add_object(tounicode_stream);

        let font_dict = dictionary! {
            "Type" => "Font",
            "Subtype" => "TrueType",
            "BaseFont" => "DejaVuSans",
            "FirstChar" => 32,
            "LastChar" => 255,
            "Widths" => widths,
            "FontDescriptor" => Object::Reference(font_descriptor),
            "Encoding" => dictionary! {
                "Type" => "Encoding",
                "BaseEncoding" => "WinAnsiEncoding",
                "Differences" => differences,
            },
            "ToUnicode" => Object::Reference(tounicode_id),
        };
        let font_id = doc.add_object(font_dict);

        Self {
            font_id,
            font_alias: "UniFont",
            char_to_byte,
        }
    }

    /// Encodes a UTF-8 string into a PDF hexadecimal operand for the Tj operator.
    pub fn encode_text(&self, text: &str) -> String {
        let mut encoded = Vec::with_capacity(text.len());
        for c in text.chars() {
            if let Some(&b) = self.char_to_byte.get(&c) {
                encoded.push(b);
            } else if (c as u32) <= 127 {
                encoded.push(c as u8);
            } else {
                encoded.push(b'?');
            }
        }
        let hex_str: String = encoded.iter().map(|b| format!("{b:02X}")).collect();
        format!("<{hex_str}>")
    }
}

/// Helper to render static text appearance stream (Form XObject) for form flattening.
pub fn create_text_appearance_stream(
    doc: &mut Document,
    w: f64,
    h: f64,
    val_str: &str,
    unicode_ctx: Option<&UnicodeFontContext>,
) -> ObjectId {
    let fs = (h * 0.7).clamp(8.0, 12.0);
    let ty = (h - fs) / 2.0;

    let (content, font_resources) = if let Some(ctx) = unicode_ctx {
        let hex_tj = ctx.encode_text(val_str);
        let content_str = format!(
            "BT /{} {fs:.2} Tf 0 0 0 rg 2 {ty:.2} Td {} Tj ET\n",
            ctx.font_alias, hex_tj
        );
        let font_res = dictionary! {
            ctx.font_alias => Object::Reference(ctx.font_id),
        };
        (content_str, font_res)
    } else {
        let escaped = crate::appearance_renderer::escape_pdf_string(val_str);
        let content_str = format!("BT /Helv {fs:.2} Tf 0 0 0 rg 2 {ty:.2} Td ({escaped}) Tj ET\n");
        let font_res = dictionary! {
            "Helv" => dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => "Helvetica",
                "Encoding" => "WinAnsiEncoding",
            }
        };
        (content_str, font_res)
    };

    let stream = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Real(w as f32),
                Object::Real(h as f32),
            ],
            "Resources" => dictionary! {
                "Font" => font_resources,
            },
        },
        content.into_bytes(),
    );

    doc.add_object(stream)
}

fn char_width(c: char) -> i64 {
    match c {
        ' ' | '\u{00A0}' => 280,
        'i' | 'l' | 'j' | '!' | '|' | ':' | ';' | '\'' => 280,
        't' | 'f' | 'r' | '(' | ')' | '[' | ']' | '-' => 360,
        'm' | 'w' => 840,
        'M' | 'W' => 880,
        'I' => 320,
        'J' => 420,
        'A'..='Z' => 680,
        'a'..='z' => 540,
        '0'..='9' => 560,
        '.' | ',' => 280,
        _ => {
            if c.is_uppercase() {
                680
            } else if c.is_lowercase() {
                540
            } else {
                500
            }
        }
    }
}
