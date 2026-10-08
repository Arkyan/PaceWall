//! Ensemble de valeurs lues dans le jeu : `fuel.level`, `tyre.fl.wear`, `grid.003.driver`…

use crate::flatten::Field;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Default)]
pub struct Data {
    pub fields: Vec<Field>,
    index: HashMap<String, usize>,
}

impl Data {
    pub fn new(fields: Vec<Field>) -> Self {
        let index = fields.iter().enumerate().map(|(i, f)| (f.key.clone(), i)).collect();
        Self { fields, index }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.index.get(key).map(|&i| &self.fields[i].value)
    }

    pub fn field(&self, key: &str) -> Option<&Field> {
        self.index.get(key).map(|&i| &self.fields[i])
    }

    pub fn num(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(Value::as_f64)
    }

    pub fn f(&self, key: &str, default: f64) -> f64 {
        self.num(key).unwrap_or(default)
    }

    pub fn text(&self, key: &str) -> &str {
        self.get(key).and_then(Value::as_str).unwrap_or("")
    }

    pub fn flag(&self, key: &str) -> bool {
        self.get(key).and_then(Value::as_bool).unwrap_or(false)
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}
