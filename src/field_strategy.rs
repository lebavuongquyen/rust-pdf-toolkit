use lopdf::{Dictionary, Document, ObjectId};
use serde_json::Value;

pub trait FieldStrategy {
    fn field_type(&self) -> &'static [u8];
    fn supports(
        &self,
        value: &Value,
        doc: &Document,
        field_id: ObjectId,
        field: &Dictionary,
    ) -> bool;
    fn fill(
        &self,
        doc: &mut Document,
        field_id: ObjectId,
        field: &Dictionary,
        value: &Value,
    ) -> Result<(), String>;
    fn validate(
        &self,
        doc: &Document,
        field_id: ObjectId,
        field: &Dictionary,
        value: &Value,
    ) -> Result<(), String>;
}

pub struct TextFieldStrategy;
pub struct ButtonFieldStrategy;
pub struct ChoiceFieldStrategy;
pub struct ImageFieldStrategy;

impl FieldStrategy for TextFieldStrategy {
    fn field_type(&self) -> &'static [u8] {
        b"Tx"
    }
    fn supports(&self, value: &Value, _: &Document, _: ObjectId, _: &Dictionary) -> bool {
        value.is_string()
    }
    fn fill(
        &self,
        doc: &mut Document,
        field_id: ObjectId,
        _: &Dictionary,
        value: &Value,
    ) -> Result<(), String> {
        super::set_text(doc, field_id, value)
    }
    fn validate(
        &self,
        _: &Document,
        _: ObjectId,
        _: &Dictionary,
        value: &Value,
    ) -> Result<(), String> {
        if value.is_string() {
            Ok(())
        } else {
            Err("Text field value must be a string".into())
        }
    }
}

impl FieldStrategy for ButtonFieldStrategy {
    fn field_type(&self) -> &'static [u8] {
        b"Btn"
    }
    fn supports(
        &self,
        value: &Value,
        doc: &Document,
        field_id: ObjectId,
        field: &Dictionary,
    ) -> bool {
        value.is_boolean() || (value.is_string() && super::has_button_states(doc, field_id, field))
    }
    fn fill(
        &self,
        doc: &mut Document,
        field_id: ObjectId,
        field: &Dictionary,
        value: &Value,
    ) -> Result<(), String> {
        if !self.supports(value, doc, field_id, field) {
            return Err("Button value is not supported by this field".into());
        }
        super::set_button(doc, field_id, field, value)
    }
    fn validate(
        &self,
        doc: &Document,
        field_id: ObjectId,
        field: &Dictionary,
        value: &Value,
    ) -> Result<(), String> {
        if value.is_boolean() {
            return Ok(());
        }
        if let Some(selected) = value.as_str() {
            if super::has_button_states(doc, field_id, field) {
                return if super::all_button_states(doc, field_id, field)
                    .iter()
                    .any(|x| String::from_utf8_lossy(x) == selected)
                {
                    Ok(())
                } else {
                    Err("Button option not found".into())
                };
            }
            return super::parse_image(selected)
                .and_then(|bytes| super::jpeg_size(&bytes).map(|_| ()));
        }
        Err("Button value must be boolean, option name, or image Base64".into())
    }
}

impl FieldStrategy for ChoiceFieldStrategy {
    fn field_type(&self) -> &'static [u8] {
        b"Ch"
    }
    fn supports(&self, value: &Value, _: &Document, _: ObjectId, _: &Dictionary) -> bool {
        value.is_string() || value.is_array()
    }
    fn fill(
        &self,
        doc: &mut Document,
        field_id: ObjectId,
        field: &Dictionary,
        value: &Value,
    ) -> Result<(), String> {
        super::set_choice(doc, field_id, field, value)
    }
    fn validate(
        &self,
        _: &Document,
        _: ObjectId,
        field: &Dictionary,
        value: &Value,
    ) -> Result<(), String> {
        let options = super::choice_options(field);
        let values: Vec<String> = if let Some(s) = value.as_str() {
            vec![s.to_string()]
        } else if let Some(a) = value.as_array() {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        } else {
            return Err("Choice value must be a string or array of strings".into());
        };
        if values.is_empty() {
            return Err("Choice value cannot be empty".into());
        }
        if options.is_empty()
            || values.iter().all(|v| {
                options
                    .iter()
                    .any(|pair| pair.iter().any(|option| option == v))
            })
        {
            Ok(())
        } else {
            Err("Choice option not found".into())
        }
    }
}

impl FieldStrategy for ImageFieldStrategy {
    fn field_type(&self) -> &'static [u8] {
        b"Btn"
    }
    fn supports(
        &self,
        value: &Value,
        doc: &Document,
        field_id: ObjectId,
        field: &Dictionary,
    ) -> bool {
        value.is_string() && !super::has_button_states(doc, field_id, field)
    }
    fn fill(
        &self,
        doc: &mut Document,
        field_id: ObjectId,
        field: &Dictionary,
        value: &Value,
    ) -> Result<(), String> {
        super::set_image(doc, field_id, field, value)
    }
    fn validate(
        &self,
        _: &Document,
        _: ObjectId,
        _: &Dictionary,
        value: &Value,
    ) -> Result<(), String> {
        let encoded = value
            .as_str()
            .ok_or("Image field value must be a Base64 string")?;
        let bytes = super::parse_image(encoded)?;
        super::jpeg_size(&bytes).map(|_| ())
    }
}

pub struct FieldStrategyRegistry {
    strategies: Vec<Box<dyn FieldStrategy>>,
}

impl FieldStrategyRegistry {
    pub fn new() -> Self {
        Self {
            strategies: vec![
                Box::new(TextFieldStrategy),
                Box::new(ImageFieldStrategy),
                Box::new(ButtonFieldStrategy),
                Box::new(ChoiceFieldStrategy),
            ],
        }
    }

    pub fn find(
        &self,
        field_type: &[u8],
        value: &Value,
        doc: &Document,
        field_id: ObjectId,
        field: &Dictionary,
    ) -> Option<&dyn FieldStrategy> {
        self.strategies
            .iter()
            .filter(|strategy| strategy.field_type() == field_type)
            .find(|strategy| strategy.supports(value, doc, field_id, field))
            .map(|strategy| strategy.as_ref())
    }
}
