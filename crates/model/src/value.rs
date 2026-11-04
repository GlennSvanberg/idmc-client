//! Record values. Nested field groups are objects; repeatable groups are lists.

use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum FieldValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    List(Vec<FieldValue>),
    Group(BTreeMap<String, FieldValue>),
}

impl<'de> Deserialize<'de> for FieldValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        Ok(FieldValue::from_json(&value))
    }
}

impl FieldValue {
    pub fn from_json(value: &Value) -> Self {
        match value {
            Value::Null => FieldValue::Null,
            Value::Bool(flag) => FieldValue::Bool(*flag),
            Value::Number(n) => FieldValue::Number(n.as_f64().unwrap_or(0.0)),
            Value::String(text) => FieldValue::String(text.clone()),
            Value::Array(items) => {
                FieldValue::List(items.iter().map(FieldValue::from_json).collect())
            }
            Value::Object(map) => FieldValue::Group(
                map.iter()
                    .map(|(k, v)| (k.clone(), FieldValue::from_json(v)))
                    .collect(),
            ),
        }
    }

    pub fn as_id(&self) -> Option<String> {
        match self {
            FieldValue::String(text) if !text.is_empty() => Some(text.clone()),
            FieldValue::Number(n) => Some(crate::types::format_integer(*n)),
            _ => None,
        }
    }
}

/// Walk `a.b.c`. A list of groups yields the first group's field, which is
/// what a table cell shows for a repeatable group.
pub fn value_at<'a>(
    values: &'a BTreeMap<String, FieldValue>,
    path: &str,
) -> Option<&'a FieldValue> {
    let mut parts = path.split('.').filter(|p| !p.is_empty());
    let first = parts.next()?;
    let mut current = values.get(first)?;
    for part in parts {
        current = next_field(current, part)?;
    }
    Some(current)
}

fn next_field<'a>(value: &'a FieldValue, name: &str) -> Option<&'a FieldValue> {
    match value {
        FieldValue::Group(map) => map.get(name),
        FieldValue::List(items) => match items.first() {
            Some(FieldValue::Group(map)) => map.get(name),
            _ => None,
        },
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub entity: String,
    pub id: String,
    #[serde(default)]
    pub values: BTreeMap<String, FieldValue>,
}

impl Record {
    pub fn from_json(value: &Value) -> Result<Self, String> {
        let entity = value
            .get("entity")
            .and_then(|v| v.as_str())
            .ok_or("record is missing entity")?
            .to_string();
        let id = value
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or("record is missing id")?
            .to_string();
        let values = match value.get("values") {
            Some(Value::Object(map)) => map
                .iter()
                .map(|(k, v)| (k.clone(), FieldValue::from_json(v)))
                .collect(),
            None => BTreeMap::new(),
            Some(_) => return Err(format!("record {id} values must be an object")),
        };
        Ok(Record { entity, id, values })
    }
}
