//! Turn edited text into record values and Business 360 request bodies.
//! Lookup codes stay strings on the record. The wire payload wraps them as
//! `{Code, Name}` objects, which is what the entity APIs expect.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::types::{FieldDef, FieldType, ModelDefinition, ModelError};
use crate::value::{value_at, FieldValue, Record};

/// Source system used when the user does not pick one. This is the system id
/// in the Business 360 entity API examples.
pub const DEFAULT_SOURCE_SYSTEM: &str = "c360.default.system";

pub fn parse_field(field: &FieldDef, text: &str, model: &ModelDefinition) -> Result<FieldValue, String> {
    let text = text.trim();
    if text.is_empty() {
        if field.mandatory {
            return Err(format!("{} is required", field.label));
        }
        return Ok(FieldValue::Null);
    }
    match field.ty {
        FieldType::Integer => text
            .parse::<i64>()
            .map(|n| FieldValue::Number(n as f64))
            .map_err(|_| format!("{} must be a whole number", field.label)),
        FieldType::Decimal => text
            .parse::<f64>()
            .map(FieldValue::Number)
            .map_err(|_| format!("{} must be a number", field.label)),
        FieldType::Boolean => match text.to_ascii_lowercase().as_str() {
            "true" | "yes" | "y" | "1" => Ok(FieldValue::Bool(true)),
            "false" | "no" | "n" | "0" => Ok(FieldValue::Bool(false)),
            _ => Err(format!("{} must be true or false", field.label)),
        },
        FieldType::Date => {
            if is_date(text) {
                Ok(FieldValue::String(text.to_string()))
            } else {
                Err(format!("{} must be a date, YYYY-MM-DD", field.label))
            }
        }
        FieldType::DateTime => {
            if is_date(text) || (text.len() >= 19 && text.as_bytes().get(10) == Some(&b'T')) {
                Ok(FieldValue::String(text.to_string()))
            } else {
                Err(format!("{} must be a date and time", field.label))
            }
        }
        FieldType::Lookup => parse_lookup(field, text, model),
        FieldType::Group => Err(format!("{} is edited as its own rows", field.label)),
        FieldType::String | FieldType::Text | FieldType::Url | FieldType::Reference | FieldType::Unknown => {
            check_length(field, text)?;
            Ok(FieldValue::String(text.to_string()))
        }
    }
}

fn parse_lookup(field: &FieldDef, text: &str, model: &ModelDefinition) -> Result<FieldValue, String> {
    let Some(id) = field.lookup.as_deref() else {
        check_length(field, text)?;
        return Ok(FieldValue::String(text.to_string()));
    };
    let Some(lookup) = model.lookup(id) else {
        check_length(field, text)?;
        return Ok(FieldValue::String(text.to_string()));
    };
    if lookup.values.is_empty() {
        check_length(field, text)?;
        return Ok(FieldValue::String(text.to_string()));
    }
    let found = lookup.values.iter().find(|value| {
        value.id.eq_ignore_ascii_case(text) || value.label.eq_ignore_ascii_case(text)
    });
    match found {
        Some(value) => Ok(FieldValue::String(value.id.clone())),
        None if lookup.closed && !lookup.values.is_empty() => Err(format!("{} is not in the list", field.label)),
        None => Ok(FieldValue::String(text.to_string())),
    }
}

fn check_length(field: &FieldDef, text: &str) -> Result<(), String> {
    if let Some(limit) = field.length {
        if text.chars().count() > limit as usize {
            return Err(format!("{} is longer than {limit}", field.label));
        }
    }
    Ok(())
}

fn is_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let year = text[0..4].parse::<u32>().ok();
    let month = text[5..7].parse::<u32>().ok();
    let day = text[8..10].parse::<u32>().ok();
    matches!((year, month, day), (Some(y), Some(m), Some(d)) if y >= 1 && (1..=12).contains(&m) && (1..=31).contains(&d))
}

/// Text placed in the editor. Lookups show the label when one is known.
pub fn edit_text(model: &ModelDefinition, field: &FieldDef, value: Option<&FieldValue>) -> String {
    let Some(value) = value else { return String::new() };
    match value {
        FieldValue::Null => String::new(),
        FieldValue::Bool(flag) => if *flag { "true".into() } else { "false".into() },
        FieldValue::Number(n) => match field.ty {
            FieldType::Integer => crate::types::format_integer(*n),
            _ => crate::types::trim_number(*n),
        },
        FieldValue::String(text) => {
            if field.ty == FieldType::Lookup {
                let label = model.lookup_label(field.lookup.as_deref(), text);
                if !label.is_empty() && label != *text {
                    return label;
                }
            }
            text.clone()
        }
        FieldValue::Group(map) => map
            .get("Code")
            .and_then(|code| match code {
                FieldValue::String(text) => Some(edit_text(model, field, Some(&FieldValue::String(text.clone())))),
                _ => None,
            })
            .unwrap_or_default(),
        FieldValue::List(_) => String::new(),
    }
}

pub fn lookup_choices(model: &ModelDefinition, lookup: Option<&str>) -> Vec<(String, String)> {
    let Some(id) = lookup else { return Vec::new() };
    model
        .lookup(id)
        .map(|lookup| {
            lookup
                .values
                .iter()
                .map(|value| (value.id.clone(), value.label.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// `validate_values` plus length limits. Load-time checks stay looser so an
/// export that predates a length change still opens.
pub fn validate_edit(
    model: &ModelDefinition,
    entity: &str,
    values: &BTreeMap<String, FieldValue>,
) -> Result<(), ModelError> {
    crate::types::validate_values(model, entity, values)?;
    let Some(definition) = model.entity(entity) else {
        return Err(ModelError::Message(format!("unknown entity {entity}")));
    };
    check_lengths(model, &definition.fields, values)
}

/// Labels of fields that differ, for the save result.
pub fn changed_labels(
    model: &ModelDefinition,
    entity: &str,
    before: &BTreeMap<String, FieldValue>,
    after: &BTreeMap<String, FieldValue>,
) -> Vec<String> {
    let Some(definition) = model.entity(entity) else {
        return Vec::new();
    };
    let mut labels = Vec::new();
    for field in &definition.fields {
        if field.system {
            continue;
        }
        if before.get(&field.name) != after.get(&field.name) {
            labels.push(field.label.clone());
        }
    }
    labels
}

fn check_lengths(
    model: &ModelDefinition,
    fields: &[FieldDef],
    values: &BTreeMap<String, FieldValue>,
) -> Result<(), ModelError> {
    for field in fields {
        let Some(value) = values.get(&field.name) else { continue };
        match value {
            FieldValue::String(text) => {
                if let Some(limit) = field.length {
                    if text.chars().count() > limit as usize {
                        return Err(ModelError::Message(format!("{} is longer than {limit}", field.label)));
                    }
                }
            }
            FieldValue::List(items) if field.ty == FieldType::Group => {
                for item in items {
                    if let FieldValue::Group(map) = item {
                        check_lengths(model, &field.children, map)?;
                    }
                }
            }
            FieldValue::Group(map) if field.ty == FieldType::Group => {
                check_lengths(model, &field.children, map)?;
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn api_body(model: &ModelDefinition, entity: &str, values: &BTreeMap<String, FieldValue>) -> Value {
    let Some(definition) = model.entity(entity) else {
        return Value::Object(Map::new());
    };
    Value::Object(encode_map(model, &definition.fields, values))
}

pub fn patch_ops(
    model: &ModelDefinition,
    entity: &str,
    before: &BTreeMap<String, FieldValue>,
    after: &BTreeMap<String, FieldValue>,
) -> Value {
    let Some(definition) = model.entity(entity) else {
        return Value::Array(Vec::new());
    };
    let mut ops = Vec::new();
    for field in &definition.fields {
        if field.system {
            continue;
        }
        let left = before.get(&field.name);
        let right = after.get(&field.name);
        if field.ty == FieldType::Group {
            diff_group(&mut ops, model, field, left, right);
            continue;
        }
        if left == right {
            continue;
        }
        match right {
            None | Some(FieldValue::Null) => {
                ops.push(json!({ "op": "remove", "path": field.name }));
            }
            Some(value) => {
                ops.push(json!({
                    "op": "replace",
                    "path": field.name,
                    "value": encode_value(model, field, value),
                }));
            }
        }
    }
    Value::Array(ops)
}

fn diff_group(
    ops: &mut Vec<Value>,
    model: &ModelDefinition,
    field: &FieldDef,
    left: Option<&FieldValue>,
    right: Option<&FieldValue>,
) {
    let before = groups_of(left);
    let after = groups_of(right);
    if before == after {
        return;
    }
    let granular = before.iter().chain(after.iter()).all(|group| !group_id(group).is_empty());
    if !granular {
        ops.push(json!({
            "op": "replace",
            "path": field.name,
            "value": encode_groups(model, field, &after),
        }));
        return;
    }
    let before_ids: Vec<String> = before.iter().map(group_id).collect();
    let after_ids: Vec<String> = after.iter().map(group_id).collect();
    for group in &after {
        let id = group_id(group);
        if let Some(previous) = before.iter().find(|item| group_id(item) == id) {
            if previous != group {
                ops.push(json!({
                    "op": "replace",
                    "path": format!("{}[{id}]", field.name),
                    "value": encode_map(model, &field.children, group),
                }));
            }
        } else {
            ops.push(json!({
                "op": "add",
                "path": field.name,
                "value": encode_map(model, &field.children, group),
            }));
        }
    }
    for id in before_ids {
        if !after_ids.iter().any(|other| other == &id) {
            ops.push(json!({ "op": "remove", "path": format!("{}[{id}]", field.name) }));
        }
    }
}

fn groups_of(value: Option<&FieldValue>) -> Vec<BTreeMap<String, FieldValue>> {
    match value {
        Some(FieldValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                FieldValue::Group(map) => Some(map.clone()),
                _ => None,
            })
            .collect(),
        Some(FieldValue::Group(map)) => vec![map.clone()],
        _ => Vec::new(),
    }
}

fn group_id(map: &BTreeMap<String, FieldValue>) -> String {
    match map.get("_id") {
        Some(FieldValue::String(text)) if !text.is_empty() => text.clone(),
        _ => String::new(),
    }
}

fn encode_groups(model: &ModelDefinition, field: &FieldDef, groups: &[BTreeMap<String, FieldValue>]) -> Value {
    Value::Array(
        groups
            .iter()
            .map(|group| Value::Object(encode_map(model, &field.children, group)))
            .collect(),
    )
}

fn encode_map(model: &ModelDefinition, fields: &[FieldDef], values: &BTreeMap<String, FieldValue>) -> Map<String, Value> {
    let mut out = Map::new();
    if let Some(id) = values.get("_id") {
        if let FieldValue::String(text) = id {
            if !text.is_empty() {
                out.insert("_id".into(), Value::String(text.clone()));
            }
        }
    }
    for field in fields {
        if field.system {
            continue;
        }
        let Some(value) = values.get(&field.name) else { continue };
        if matches!(value, FieldValue::Null) {
            continue;
        }
        out.insert(field.name.clone(), encode_value(model, field, value));
    }
    out
}

fn encode_value(model: &ModelDefinition, field: &FieldDef, value: &FieldValue) -> Value {
    if field.ty == FieldType::Lookup {
        let code = match value {
            FieldValue::String(text) => text.clone(),
            FieldValue::Group(map) => match map.get("Code") {
                Some(FieldValue::String(text)) => text.clone(),
                _ => String::new(),
            },
            _ => String::new(),
        };
        if code.is_empty() {
            return Value::Null;
        }
        let name = model.lookup_label(field.lookup.as_deref(), &code);
        return json!({ "Code": code, "Name": name });
    }
    match value {
        FieldValue::Null => Value::Null,
        FieldValue::Bool(flag) => Value::Bool(*flag),
        FieldValue::Number(n) => json!(n),
        FieldValue::String(text) => Value::String(text.clone()),
        FieldValue::List(items) if field.ty == FieldType::Group => encode_groups(
            model,
            field,
            &items
                .iter()
                .filter_map(|item| match item {
                    FieldValue::Group(map) => Some(map.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
        ),
        FieldValue::Group(map) if field.ty == FieldType::Group => Value::Object(encode_map(model, &field.children, map)),
        FieldValue::List(_) | FieldValue::Group(_) => Value::Null,
    }
}

pub fn split_child_id(id: &str) -> Option<(String, String, usize)> {
    let (rest, index) = id.rsplit_once("::")?;
    let index = index.parse().ok()?;
    let (parent, field) = rest.rsplit_once("::")?;
    if parent.is_empty() || field.is_empty() {
        return None;
    }
    Some((parent.to_string(), field.to_string(), index))
}

pub fn child_record_id(parent: &str, field: &str, index: usize) -> String {
    format!("{parent}::{field}::{index}")
}

pub fn push_child(parent: &mut Record, field: &str, mut values: BTreeMap<String, FieldValue>) -> usize {
    if group_id(&values).is_empty() {
        values.insert("_id".into(), FieldValue::String(local_token()));
    }
    let list = parent
        .values
        .entry(field.to_string())
        .or_insert_with(|| FieldValue::List(Vec::new()));
    let FieldValue::List(items) = list else {
        *list = FieldValue::List(vec![FieldValue::Group(values)]);
        return 0;
    };
    let index = items.len();
    items.push(FieldValue::Group(values));
    index
}

pub fn set_child_field(parent: &mut Record, field: &str, index: usize, name: &str, value: FieldValue) -> bool {
    let Some(FieldValue::List(items)) = parent.values.get_mut(field) else {
        return false;
    };
    let Some(FieldValue::Group(map)) = items.get_mut(index) else {
        return false;
    };
    if matches!(value, FieldValue::Null) {
        map.remove(name);
    } else {
        map.insert(name.to_string(), value);
    }
    true
}

pub fn remove_child(parent: &mut Record, field: &str, index: usize) -> bool {
    let Some(FieldValue::List(items)) = parent.values.get_mut(field) else {
        return false;
    };
    if index >= items.len() {
        return false;
    }
    items.remove(index);
    true
}

pub fn read_field<'a>(record: &'a Record, field: &FieldDef) -> Option<&'a FieldValue> {
    value_at(&record.values, &field.id).or_else(|| record.values.get(&field.name))
}

fn local_token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!("idmc{:x}", NEXT.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EnumValue, EntityType, FieldDef, FieldType, Lookup};

    fn field(name: &str, ty: FieldType) -> FieldDef {
        FieldDef {
            id: name.into(),
            name: name.into(),
            label: name.into(),
            ty,
            mandatory: false,
            system: false,
            many: false,
            length: None,
            precision: None,
            scale: None,
            lookup: None,
            target_entity: None,
            children: Vec::new(),
            default_column: true,
        }
    }

    fn person_model() -> ModelDefinition {
        let mut name = field("firstName", FieldType::String);
        name.mandatory = true;
        name.length = Some(8);
        name.label = "First name".into();
        let mut country = field("country", FieldType::Lookup);
        country.lookup = Some("Country".into());
        country.label = "Country".into();
        let mut line = field("addressLine1", FieldType::String);
        line.label = "Address line".into();
        let mut address = field("PostalAddress", FieldType::Group);
        address.many = true;
        address.children = vec![line];
        ModelDefinition {
            name: "test".into(),
            origin: String::new(),
            entities: vec![EntityType {
                id: "person".into(),
                name: "person".into(),
                label: "Person".into(),
                label_plural: "Persons".into(),
                label_fields: vec!["firstName".into()],
                column_order: Vec::new(),
                fields: vec![name, country, address],
                hierarchy: None,
                embedded_in: None,
            }],
            lookups: vec![Lookup {
                id: "Country".into(),
                label: "Country".into(),
                guid: None,
                code_list_id: None,
                list_label: None,
                depends_on: None,
                alternates: Vec::new(),
                values: vec![
                    EnumValue { id: "SE".into(), label: "Sweden".into(), parent: None },
                    EnumValue { id: "NO".into(), label: "Norway".into(), parent: None },
                ],
                closed: true,
            }],
            relationships: Vec::new(),
        }
    }

    #[test]
    fn parse_enforces_required_length_lookup_and_date() {
        let model = person_model();
        let entity = model.entity("person").unwrap();
        let name = entity.field("firstName").unwrap();
        assert!(parse_field(name, "  ", &model).unwrap_err().contains("required"));
        assert!(parse_field(name, "Alexander", &model).unwrap_err().contains("longer"));
        assert_eq!(parse_field(name, "Ada", &model).unwrap(), FieldValue::String("Ada".into()));
        let country = entity.field("country").unwrap();
        assert_eq!(parse_field(country, "sweden", &model).unwrap(), FieldValue::String("SE".into()));
        assert!(parse_field(country, "Mars", &model).is_err());
        let mut born = field("born", FieldType::Date);
        born.label = "Born".into();
        assert!(parse_field(&born, "2024-13-01", &model).is_err());
        assert!(parse_field(&born, "2024-03-01", &model).is_ok());
        let mut count = field("count", FieldType::Integer);
        count.label = "Count".into();
        assert!(parse_field(&count, "12.5", &model).is_err());
    }

    #[test]
    fn payloads_wrap_lookups_and_patch_changed_fields() {
        let model = person_model();
        let mut before = BTreeMap::new();
        before.insert("firstName".into(), FieldValue::String("Ada".into()));
        before.insert("country".into(), FieldValue::String("SE".into()));
        let mut after = before.clone();
        after.insert("firstName".into(), FieldValue::String("Ann".into()));
        let body = api_body(&model, "person", &after);
        assert_eq!(body["firstName"], "Ann");
        assert_eq!(body["country"]["Code"], "SE");
        assert_eq!(body["country"]["Name"], "Sweden");
        let ops = patch_ops(&model, "person", &before, &after);
        let ops = ops.as_array().unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0]["op"], "replace");
        assert_eq!(ops[0]["path"], "firstName");
        assert_eq!(ops[0]["value"], "Ann");
        let mut values = BTreeMap::new();
        values.insert("firstName".into(), FieldValue::String("TooLongName".into()));
        assert!(validate_edit(&model, "person", &values).is_err());
    }

    #[test]
    fn an_untouched_unknown_code_does_not_block_a_save() {
        let mut model = person_model();
        model.lookups[0].closed = false;
        let mut address = BTreeMap::new();
        address.insert("_id".into(), FieldValue::String("a1".into()));
        address.insert("state".into(), FieldValue::String("NE".into()));
        let mut before = BTreeMap::new();
        before.insert("firstName".into(), FieldValue::String("Acacia".into()));
        before.insert("country".into(), FieldValue::String("ZZ".into()));
        before.insert("PostalAddress".into(), FieldValue::List(vec![FieldValue::Group(address)]));
        let mut after = before.clone();
        after.insert("firstName".into(), FieldValue::String("Ada".into()));
        assert!(crate::types::validate_changed(&model, "person", &before, &after).is_ok());
        assert_eq!(changed_labels(&model, "person", &before, &after), vec!["First name".to_string()]);
        after.insert("country".into(), FieldValue::String("Mars".into()));
        model.lookups[0].closed = true;
        let err = crate::types::validate_changed(&model, "person", &before, &after).unwrap_err().to_string();
        assert!(err.contains("Mars"), "{err}");
        assert!(err.contains("country"), "{err}");
    }

    #[test]
    fn child_rows_round_trip_through_the_parent_group() {
        let mut parent = Record {
            entity: "person".into(),
            id: "p1".into(),
            values: BTreeMap::new(),
        };
        let index = push_child(&mut parent, "PostalAddress", BTreeMap::new());
        assert_eq!(index, 0);
        assert!(set_child_field(
            &mut parent,
            "PostalAddress",
            0,
            "addressLine1",
            FieldValue::String("Main street".into()),
        ));
        let (parent_id, field, index) = split_child_id("p1::PostalAddress::0").unwrap();
        assert_eq!((parent_id, field, index), ("p1".into(), "PostalAddress".into(), 0));
        assert!(remove_child(&mut parent, "PostalAddress", 0));
        assert!(matches!(parent.values.get("PostalAddress"), Some(FieldValue::List(items)) if items.is_empty()));
    }
}
