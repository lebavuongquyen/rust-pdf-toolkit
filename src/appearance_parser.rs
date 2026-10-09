use crate::appearance::{GraphicPosition, SignatureDesign, SignatureTextLine};
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId};

pub fn parse_signature_appearance(
    doc: &Document,
    field_id: ObjectId,
    field: &Dictionary,
) -> Option<SignatureDesign> {
    let mut streams = Vec::new();
    collect_appearance_streams(doc, field_id, field, &mut streams);

    if streams.is_empty() {
        return None;
    }

    let mut all_lines = Vec::new();
    let mut found_image_bounds = None;

    for stream_bytes in &streams {
        if let Ok(content) = Content::decode(stream_bytes) {
            let (lines, img_bounds) = parse_operations(&content.operations);
            all_lines.extend(lines);
            if found_image_bounds.is_none() && img_bounds.is_some() {
                found_image_bounds = img_bounds;
            }
        }
    }

    let image = crate::extract_image_value(doc, field_id, field).and_then(|v| match v {
        serde_json::Value::String(s) => Some(s),
        _ => None,
    });

    if all_lines.is_empty() && found_image_bounds.is_none() && image.is_none() {
        return None;
    }

    // Infer position
    let has_image = found_image_bounds.is_some() || image.is_some();
    let has_text = !all_lines.is_empty();

    let position = if has_image && !has_text {
        GraphicPosition::ImageOnly
    } else if !has_image && has_text {
        GraphicPosition::TextOnly
    } else if let (Some(ib), true) = (found_image_bounds, has_text) {
        let img_x = ib[0];
        let img_w = ib[2];
        let min_txt_x = all_lines
            .iter()
            .filter_map(|l| l.x)
            .fold(f64::INFINITY, f64::min);

        if img_x + img_w <= min_txt_x + 8.0 {
            GraphicPosition::Left
        } else if img_x >= min_txt_x {
            GraphicPosition::Right
        } else {
            GraphicPosition::Behind
        }
    } else {
        GraphicPosition::Left
    };

    Some(SignatureDesign {
        position,
        image,
        image_bounds: found_image_bounds,
        text_lines: all_lines,
    })
}

fn collect_appearance_streams(
    doc: &Document,
    field_id: ObjectId,
    field: &Dictionary,
    out: &mut Vec<Vec<u8>>,
) {
    let mut visited = std::collections::HashSet::new();

    if let Ok(ap) = field.get(b"AP") {
        collect_from_ap_object(doc, ap, out, &mut visited, 0);
    }

    for widget_id in crate::widget_ids(doc, field_id, field) {
        if let Ok(widget) = doc.get_object(widget_id).and_then(|x| x.as_dict())
            && let Ok(ap) = widget.get(b"AP")
        {
            collect_from_ap_object(doc, ap, out, &mut visited, 0);
        }
    }
}

fn collect_from_ap_object(
    doc: &Document,
    ap_obj: &Object,
    out: &mut Vec<Vec<u8>>,
    visited: &mut std::collections::HashSet<ObjectId>,
    depth: usize,
) {
    if depth > 5 {
        return;
    }
    let ap_dict = match ap_obj {
        Object::Dictionary(d) => d,
        Object::Reference(id) => {
            if !visited.insert(*id) {
                return;
            }
            match doc.get_object(*id) {
                Ok(Object::Dictionary(d)) => d,
                _ => return,
            }
        }
        _ => return,
    };

    if let Ok(n_obj) = ap_dict.get(b"N") {
        collect_from_form_object(doc, n_obj, out, visited, depth + 1);
    }
}

fn collect_from_form_object(
    doc: &Document,
    obj: &Object,
    out: &mut Vec<Vec<u8>>,
    visited: &mut std::collections::HashSet<ObjectId>,
    depth: usize,
) {
    if depth > 6 {
        return;
    }
    match obj {
        Object::Reference(id) => {
            if !visited.insert(*id) {
                return;
            }
            if let Ok(target) = doc.get_object(*id) {
                collect_from_form_object(doc, target, out, visited, depth + 1);
            }
        }
        Object::Stream(stream) => {
            let decompressed = stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone());
            if !decompressed.is_empty() {
                out.push(decompressed);
            }

            // Inspect nested Form XObjects in Resources
            if let Ok(res_obj) = stream.dict.get(b"Resources")
                && let Some(res) = crate::get_dict_from_object(doc, res_obj)
                && let Ok(xobjs_obj) = res.get(b"XObject")
                && let Some(xobjs) = crate::get_dict_from_object(doc, xobjs_obj)
            {
                for (_name, xobj_val) in xobjs.iter() {
                    collect_from_form_object(doc, xobj_val, out, visited, depth + 1);
                }
            }
        }
        Object::Dictionary(dict) => {
            for (k, v) in dict.iter() {
                if k != b"Off" {
                    collect_from_form_object(doc, v, out, visited, depth + 1);
                }
            }
        }
        _ => {}
    }
}

fn obj_to_f64(obj: &Object) -> Option<f64> {
    match obj {
        Object::Real(r) => Some(*r as f64),
        Object::Integer(i) => Some(*i as f64),
        _ => None,
    }
}

fn obj_to_string(obj: &Object) -> Option<String> {
    match obj {
        Object::String(bytes, _) => {
            if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
                let u16s: Vec<u16> = bytes[2..]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect();
                if let Ok(s) = String::from_utf16(&u16s) {
                    return Some(s);
                }
            }
            if let Ok(s) = std::str::from_utf8(bytes) {
                return Some(s.to_string());
            }
            Some(bytes.iter().map(|&b| b as char).collect())
        }
        _ => None,
    }
}

fn parse_operations(ops: &[Operation]) -> (Vec<SignatureTextLine>, Option<[f64; 4]>) {
    let mut matrix_stack: Vec<[f64; 6]> = Vec::new();
    let mut current_matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut text_pos = (0.0, 0.0);
    let mut current_font = None;
    let mut current_font_size = None;
    let mut current_color = None;

    let mut text_lines = Vec::new();
    let mut image_bounds = None;

    for op in ops {
        match op.operator.as_str() {
            "q" => {
                matrix_stack.push(current_matrix);
            }
            "Q" => {
                if let Some(m) = matrix_stack.pop() {
                    current_matrix = m;
                }
            }
            "cm" => {
                if op.operands.len() >= 6 {
                    let a = obj_to_f64(&op.operands[0]).unwrap_or(1.0);
                    let b = obj_to_f64(&op.operands[1]).unwrap_or(0.0);
                    let c = obj_to_f64(&op.operands[2]).unwrap_or(0.0);
                    let d = obj_to_f64(&op.operands[3]).unwrap_or(1.0);
                    let e = obj_to_f64(&op.operands[4]).unwrap_or(0.0);
                    let f = obj_to_f64(&op.operands[5]).unwrap_or(0.0);
                    current_matrix = [a, b, c, d, e, f];
                }
            }
            "Do" => {
                let [a, _b, _c, d, e, f] = current_matrix;
                let w = a.abs();
                let h = d.abs();
                if w > 1.0 && h > 1.0 && image_bounds.is_none() {
                    image_bounds = Some([e, f, w, h]);
                }
            }
            "BT" => {
                text_pos = (0.0, 0.0);
            }
            "ET" => {}
            "Tf" => {
                if op.operands.len() >= 2 {
                    if let Object::Name(name) = &op.operands[0] {
                        current_font = Some(String::from_utf8_lossy(name).into_owned());
                    }
                    current_font_size = obj_to_f64(&op.operands[1]);
                }
            }
            "rg" => {
                if op.operands.len() >= 3 {
                    let r = obj_to_f64(&op.operands[0]).unwrap_or(0.0);
                    let g = obj_to_f64(&op.operands[1]).unwrap_or(0.0);
                    let b = obj_to_f64(&op.operands[2]).unwrap_or(0.0);
                    current_color = Some([
                        (r * 255.0).round().clamp(0.0, 255.0) as u8,
                        (g * 255.0).round().clamp(0.0, 255.0) as u8,
                        (b * 255.0).round().clamp(0.0, 255.0) as u8,
                    ]);
                }
            }
            "g" => {
                if let Some(gray) = op.operands.first().and_then(obj_to_f64) {
                    let v = (gray * 255.0).round().clamp(0.0, 255.0) as u8;
                    current_color = Some([v, v, v]);
                }
            }
            "Tm" => {
                if op.operands.len() >= 6 {
                    let e = obj_to_f64(&op.operands[4]).unwrap_or(0.0);
                    let f = obj_to_f64(&op.operands[5]).unwrap_or(0.0);
                    text_pos = (e, f);
                }
            }
            "Td" | "TD" => {
                if op.operands.len() >= 2 {
                    let tx = obj_to_f64(&op.operands[0]).unwrap_or(0.0);
                    let ty = obj_to_f64(&op.operands[1]).unwrap_or(0.0);
                    text_pos.0 += tx;
                    text_pos.1 += ty;
                }
            }
            "Tj" => {
                if let Some(s) = op.operands.first().and_then(obj_to_string)
                    && !s.trim().is_empty()
                {
                    text_lines.push(SignatureTextLine {
                        text: s,
                        x: Some(text_pos.0),
                        y: Some(text_pos.1),
                        font_size: current_font_size,
                        font_name: current_font.clone(),
                        color_rgb: current_color,
                    });
                }
            }
            "TJ" => {
                if let Some(Object::Array(items)) = op.operands.first() {
                    let mut full_text = String::new();
                    for item in items {
                        if let Some(s) = obj_to_string(item) {
                            full_text.push_str(&s);
                        }
                    }
                    if !full_text.trim().is_empty() {
                        text_lines.push(SignatureTextLine {
                            text: full_text,
                            x: Some(text_pos.0),
                            y: Some(text_pos.1),
                            font_size: current_font_size,
                            font_name: current_font.clone(),
                            color_rgb: current_color,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    (text_lines, image_bounds)
}
