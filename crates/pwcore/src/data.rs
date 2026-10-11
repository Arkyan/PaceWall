//! Ensemble de valeurs lues dans le jeu : `fuel.level`, `tyre.fl.wear`, `grid.003.driver`…

use crate::flatten::Field;
use serde_json::Value;

/// Empreinte d'une clé (FNV-1a) : rapide, et suffisante pour ranger quelques centaines de clés.
fn hash(key: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.as_bytes() {
        h = (h ^ *b as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[derive(Default)]
pub struct Data {
    pub fields: Vec<Field>,
    /// (empreinte de la clé, position du champ), trié par empreinte : une recherche se fait par dichotomie sur des
    /// nombres, sans copier ni comparer de texte (les données sont reconstruites des dizaines de fois par seconde).
    order: Vec<(u64, u32)>,
}

impl Data {
    pub fn new(fields: Vec<Field>) -> Self {
        let mut order: Vec<(u64, u32)> = fields.iter().enumerate().map(|(i, f)| (hash(&f.key), i as u32)).collect();
        // Trié par empreinte puis par position : pour une clé présente deux fois, la dernière reste la dernière.
        order.sort_unstable();
        Self { fields, order }
    }

    /// Position du champ `key` (le dernier s'il est présent plusieurs fois).
    fn find(&self, key: &str) -> Option<usize> {
        let h = hash(key);
        let end = self.order.partition_point(|(k, _)| *k <= h);
        // Deux clés différentes peuvent avoir la même empreinte : on vérifie le texte.
        self.order[..end].iter().rev().take_while(|(k, _)| *k == h).map(|(_, i)| *i as usize).find(|i| self.fields[*i].key == key)
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.find(key).map(|i| &self.fields[i].value)
    }

    pub fn field(&self, key: &str) -> Option<&Field> {
        self.find(key).map(|i| &self.fields[i])
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_fields_by_key_and_keeps_the_last_duplicate() {
        let f = |k: &str, v: i64| Field { key: k.into(), unit: "", label: "", value: json!(v) };
        let d = Data::new(vec![f("b", 2), f("a", 1), f("c", 3), f("a", 9), f("ab", 5)]);
        assert_eq!(d.num("a"), Some(9.0));
        assert_eq!(d.num("ab"), Some(5.0));
        assert_eq!(d.num("b"), Some(2.0));
        assert_eq!(d.num("c"), Some(3.0));
        assert_eq!(d.num("d"), None);
        assert_eq!(d.num(""), None);
        assert!(Data::default().get("a").is_none());
    }

    /// `cargo test --release -p pwcore data::tests::cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn cost() {
        let make = || -> Vec<Field> { (0..800).map(|i| Field { key: format!("grid.{:03}.champ{}", i / 20, i % 20), unit: "", label: "", value: json!(i) }).collect() };
        let sets: Vec<Vec<Field>> = (0..200).map(|_| make()).collect();
        let t = std::time::Instant::now();
        let all: Vec<Data> = sets.into_iter().map(Data::new).collect();
        println!("construction : {} µs", t.elapsed().as_micros() / 200);
        let t = std::time::Instant::now();
        let mut n = 0.0;
        for d in &all {
            for i in 0..800 {
                n += d.f(&format!("grid.{:03}.champ{}", i / 20, i % 20), 0.0);
            }
        }
        println!("800 lectures : {} µs ({n})", t.elapsed().as_micros() / 200);
    }
}
