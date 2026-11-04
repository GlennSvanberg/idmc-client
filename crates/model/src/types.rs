//! Metadata the UI is allowed to know. Entity names, fields, and relationships
//! all come from a loaded definition — nothing in the view layer hardcodes them.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::value::{value_at, FieldValue};

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("{0}")]
    Message(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    Text,
    Integer,
    Decimal,
    Boolean,
    Date,
    DateTime,
    Url,
    Lookup,
    Reference,
    Group,
    Unknown,
}

impl FieldType {
    pub fn is_scalar(self) -> bool {
        !matches!(self, FieldType::Group)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    Tree,
    Table,
    Detail,
    Graph,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToOne,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldSide {
    From,
    To,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnumValue {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub parent: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lookup {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub guid: Option<String>,
    #[serde(default)]
    pub list_label: Option<String>,
    #[serde(default)]
    pub depends_on: Option<String>,
    #[serde(default)]
    pub alternates: Vec<String>,
    #[serde(default)]
    pub values: Vec<EnumValue>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDef {
    pub id: String,
    pub name: String,
    pub label: String,
    #[serde(rename = "type")]
    pub ty: FieldType,
    #[serde(default)]
    pub mandatory: bool,
    #[serde(default)]
    pub system: bool,
    #[serde(default)]
    pub many: bool,
    #[serde(default)]
    pub length: Option<u32>,
    #[serde(default)]
    pub precision: Option<u32>,
    #[serde(default)]
    pub scale: Option<u32>,
    #[serde(default)]
    pub lookup: Option<String>,
    #[serde(default)]
    pub target_entity: Option<String>,
    #[serde(default)]
    pub children: Vec<FieldDef>,
    #[serde(default)]
    pub default_column: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hierarchy {
    pub parent_field: String,
    pub label: String,
}

/// A field group stored inside its parent record (addresses, phones, …).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedIn {
    pub parent: String,
    pub field: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityType {
    pub id: String,
    pub name: String,
    pub label: String,
    #[serde(default)]
    pub label_plural: String,
    /// Field paths joined with a space to form the record label.
    /// Filled from the IDMC description template when the export has one.
    #[serde(default)]
    pub label_fields: Vec<String>,
    /// Preferred table columns, when the loader chose an order other than field order.
    #[serde(default)]
    pub column_order: Vec<String>,
    #[serde(default)]
    pub fields: Vec<FieldDef>,
    #[serde(default)]
    pub hierarchy: Option<Hierarchy>,
    #[serde(default)]
    pub embedded_in: Option<EmbeddedIn>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relationship {
    pub id: String,
    pub label: String,
    pub from: String,
    pub to: String,
    pub cardinality: Cardinality,
    /// Field path that holds the link. For an embedded collection this is the
    /// field group on the parent. For a reference it is the reference field.
    pub field: String,
    pub field_on: FieldSide,
    /// Child rows live inside the parent record instead of as their own master records.
    #[serde(default)]
    pub embedded: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDefinition {
    pub name: String,
    #[serde(default)]
    pub origin: String,
    #[serde(default)]
    pub entities: Vec<EntityType>,
    #[serde(default)]
    pub lookups: Vec<Lookup>,
    #[serde(default)]
    pub relationships: Vec<Relationship>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinkOption {
    pub relationship: String,
    pub label: String,
}

impl ModelDefinition {
    pub fn entity(&self, id: &str) -> Option<&EntityType> {
        self.entities.iter().find(|e| e.id == id)
    }

    pub fn lookup(&self, id: &str) -> Option<&Lookup> {
        self.lookups.iter().find(|l| l.id == id)
    }

    pub fn relationship(&self, id: &str) -> Option<&Relationship> {
        self.relationships.iter().find(|r| r.id == id)
    }

    pub fn roots(&self) -> impl Iterator<Item = &EntityType> {
        self.entities.iter().filter(|e| e.embedded_in.is_none())
    }

    pub fn embedded_children<'a>(&'a self, parent: &'a str) -> impl Iterator<Item = &'a EntityType> + 'a {
        self.entities
            .iter()
            .filter(move |e| e.embedded_in.as_ref().is_some_and(|emb| emb.parent == parent))
    }

    pub fn summary(&self) -> String {
        let roots = self.roots().count();
        let children = self.entities.len().saturating_sub(roots);
        format!(
            "{} · {roots} entities · {children} child collections · {} code lists",
            self.name,
            self.lookups.len()
        )
    }

    /// Fill plural labels, infer record-label fields, and mark default columns.
    pub fn normalize(&mut self) {
        for entity in &mut self.entities {
            if entity.label_plural.trim().is_empty() {
                entity.label_plural = pluralize(&entity.label);
            }
            if entity.label_fields.is_empty() {
                entity.label_fields = infer_label_fields(entity);
            }
            if !entity.leaves().iter().any(|f| f.default_column) {
                let ids = default_column_ids(entity);
                mark_default_columns(&mut entity.fields, &ids);
                entity.column_order = ids;
            }
        }
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        let mut entity_ids = BTreeSet::new();
        for entity in &self.entities {
            if !entity_ids.insert(entity.id.as_str()) {
                return Err(ModelError::Message(format!("duplicate entity {}", entity.id)));
            }
            let mut paths = BTreeSet::new();
            check_fields(&entity.id, &entity.fields, &mut paths)?;
            if let Some(hier) = &entity.hierarchy {
                if entity.field(&hier.parent_field).is_none() {
                    return Err(ModelError::Message(format!(
                        "{} hierarchy parent field {} is missing",
                        entity.id, hier.parent_field
                    )));
                }
            }
            for path in &entity.label_fields {
                if entity.field(path).is_none() {
                    return Err(ModelError::Message(format!(
                        "{} label field {path} is missing",
                        entity.id
                    )));
                }
            }
            if let Some(emb) = &entity.embedded_in {
                if !entity_ids.contains(emb.parent.as_str()) && self.entity(&emb.parent).is_none()
                {
                    return Err(ModelError::Message(format!(
                        "{} is embedded in unknown parent {}",
                        entity.id, emb.parent
                    )));
                }
            }
        }
        for lookup_id in self
            .entities
            .iter()
            .flat_map(|e| e.leaves())
            .filter_map(|f| f.lookup.as_deref())
        {
            if self.lookup(lookup_id).is_none() {
                return Err(ModelError::Message(format!("unknown lookup {lookup_id}")));
            }
        }
        for entity in &self.entities {
            for field in entity.leaves() {
                if let Some(target) = field.target_entity.as_deref() {
                    if self.entity(target).is_none() {
                        return Err(ModelError::Message(format!(
                            "{}.{} references unknown entity {target}",
                            entity.id, field.id
                        )));
                    }
                }
            }
        }
        for rel in &self.relationships {
            if self.entity(&rel.from).is_none() || self.entity(&rel.to).is_none() {
                return Err(ModelError::Message(format!(
                    "relationship {} has an unknown endpoint",
                    rel.id
                )));
            }
            let holder = match rel.field_on {
                FieldSide::From => &rel.from,
                FieldSide::To => &rel.to,
            };
            // Embedded collections store the group on the parent (`from`).
            let holder = if rel.embedded { &rel.from } else { holder };
            let Some(ent) = self.entity(holder) else { continue };
            if ent.field(&rel.field).is_none() {
                return Err(ModelError::Message(format!(
                    "relationship {} field {} is not on {holder}",
                    rel.id, rel.field
                )));
            }
        }
        for lookup in &self.lookups {
            if let Some(dep) = &lookup.depends_on {
                if self.lookup(dep).is_none() {
                    return Err(ModelError::Message(format!(
                        "lookup {} depends on unknown lookup {dep}",
                        lookup.id
                    )));
                }
            }
        }
        Ok(())
    }

    /// Ways a view of `source_entity` may drive a view of `target_entity`.
    /// Combinations the model cannot satisfy are omitted.
    pub fn link_options(
        &self,
        source_entity: &str,
        target_entity: &str,
        target_kind: ViewKind,
    ) -> Vec<LinkOption> {
        let mut out = Vec::new();
        if source_entity == target_entity && target_kind == ViewKind::Detail {
            let label = self
                .entity(target_entity)
                .map(|e| e.label.as_str())
                .unwrap_or("record");
            out.push(LinkOption {
                relationship: "identity".into(),
                label: format!("Selected {label}"),
            });
        }
        for rel in &self.relationships {
            if rel.from != source_entity || rel.to != target_entity {
                continue;
            }
            let ok = match target_kind {
                ViewKind::Detail => matches!(
                    rel.cardinality,
                    Cardinality::ManyToOne | Cardinality::OneToOne
                ),
                ViewKind::Table => matches!(
                    rel.cardinality,
                    Cardinality::OneToMany | Cardinality::OneToOne
                ),
                // A tree shows a whole hierarchy. Filtering it through an
                // arbitrary relationship is not offered until a model describes that.
                ViewKind::Tree | ViewKind::Graph => false,
            };
            if ok {
                out.push(LinkOption {
                    relationship: rel.id.clone(),
                    label: rel.label.clone(),
                });
            }
        }
        out
    }

    pub fn lookup_label(&self, lookup_id: Option<&str>, code: &str) -> String {
        let Some(id) = lookup_id else { return code.to_string() };
        let Some(lookup) = self.lookup(id) else { return code.to_string() };
        lookup
            .values
            .iter()
            .find(|v| v.id == code)
            .map(|v| v.label.clone())
            .unwrap_or_else(|| code.to_string())
    }

    pub fn format_field(
        &self,
        field: &FieldDef,
        value: &FieldValue,
        ref_label: &dyn Fn(&str, &str) -> String,
    ) -> String {
        match value {
            FieldValue::Null => String::new(),
            FieldValue::List(items) => items
                .iter()
                .map(|item| self.format_field(field, item, ref_label))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", "),
            FieldValue::Group(_) => String::new(),
            FieldValue::Bool(flag) => {
                if *flag {
                    "Yes".into()
                } else {
                    "No".into()
                }
            }
            FieldValue::Number(n) => match field.ty {
                FieldType::Integer => format_integer(*n),
                _ => trim_number(*n),
            },
            FieldValue::String(text) => match field.ty {
                FieldType::Lookup => self.lookup_label(field.lookup.as_deref(), text),
                FieldType::Reference => {
                    let Some(target) = field.target_entity.as_deref() else {
                        return text.clone();
                    };
                    ref_label(target, text)
                }
                FieldType::Boolean => match text.as_str() {
                    "true" | "Y" | "yes" => "Yes".into(),
                    "false" | "N" | "no" => "No".into(),
                    _ => text.clone(),
                },
                _ => text.clone(),
            },
        }
    }

    /// Merge sample code rows into lookups identified by the export.
    /// Unknown names are an error so a typo does not silently drop labels.
    pub fn apply_lookup_values(&mut self, json: &str) -> Result<(), ModelError> {
        let doc: serde_json::Value = serde_json::from_str(json)?;
        let Some(map) = doc.get("lookups").and_then(|v| v.as_object()) else {
            return Err(ModelError::Message(
                "reference values file needs a lookups object".into(),
            ));
        };
        for (name, rows) in map {
            let Some(lookup) = self.lookups.iter_mut().find(|l| l.id == *name) else {
                return Err(ModelError::Message(format!(
                    "reference values mention unknown code list {name}"
                )));
            };
            let Some(rows) = rows.as_array() else {
                return Err(ModelError::Message(format!(
                    "code list {name} values must be an array"
                )));
            };
            lookup.values.clear();
            for row in rows {
                let id = row
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if id.is_empty() {
                    return Err(ModelError::Message(format!(
                        "code list {name} has a value without an id"
                    )));
                }
                let label = row
                    .get("label")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&id)
                    .to_string();
                let parent = row
                    .get("parent")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                lookup.values.push(EnumValue { id, label, parent });
            }
        }
        Ok(())
    }
}

impl EntityType {
    pub fn field(&self, path: &str) -> Option<&FieldDef> {
        fn walk<'a>(fields: &'a [FieldDef], path: &str) -> Option<&'a FieldDef> {
            for field in fields {
                if field.id == path {
                    return Some(field);
                }
                if let Some(found) = walk(&field.children, path) {
                    return Some(found);
                }
            }
            None
        }
        walk(&self.fields, path)
    }

    pub fn leaves(&self) -> Vec<&FieldDef> {
        let mut out = Vec::new();
        fn walk<'a>(fields: &'a [FieldDef], out: &mut Vec<&'a FieldDef>) {
            for field in fields {
                if field.children.is_empty() {
                    out.push(field);
                } else {
                    walk(&field.children, out);
                }
            }
        }
        walk(&self.fields, &mut out);
        out
    }

    pub fn top_level_leaves(&self) -> Vec<&FieldDef> {
        self.fields
            .iter()
            .filter(|f| f.ty != FieldType::Group && !f.system)
            .collect()
    }

    pub fn default_columns(&self) -> Vec<String> {
        if !self.column_order.is_empty() {
            return self.column_order.clone();
        }
        self.leaves()
            .into_iter()
            .filter(|f| f.default_column)
            .map(|f| f.id.clone())
            .collect()
    }
}

fn check_fields<'a>(
    entity: &str,
    fields: &'a [FieldDef],
    seen: &mut BTreeSet<&'a str>,
) -> Result<(), ModelError> {
    for field in fields {
        if !seen.insert(field.id.as_str()) {
            return Err(ModelError::Message(format!(
                "duplicate field {} on {entity}",
                field.id
            )));
        }
        check_fields(entity, &field.children, seen)?;
    }
    Ok(())
}

pub fn pluralize(label: &str) -> String {
    let lower = label.to_ascii_lowercase();
    if lower.ends_with("s")
        || lower.ends_with("x")
        || lower.ends_with("z")
        || lower.ends_with("ch")
        || lower.ends_with("sh")
    {
        format!("{label}es")
    } else if lower.ends_with('y')
        && lower
            .chars()
            .nth_back(1)
            .is_some_and(|c| !"aeiou".contains(c))
    {
        format!("{}ies", &label[..label.len() - 1])
    } else {
        format!("{label}s")
    }
}

fn infer_label_fields(entity: &EntityType) -> Vec<String> {
    let texts: Vec<&FieldDef> = entity
        .top_level_leaves()
        .into_iter()
        .filter(|f| {
            matches!(
                f.ty,
                FieldType::String | FieldType::Text | FieldType::Url
            )
        })
        .collect();
    for preferred in ["fullName", "name", "identifier", "title"] {
        if let Some(field) = texts
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(preferred))
        {
            return vec![field.id.clone()];
        }
    }
    let mandatory: Vec<String> = texts
        .iter()
        .filter(|f| f.mandatory)
        .map(|f| f.id.clone())
        .take(2)
        .collect();
    if !mandatory.is_empty() {
        return mandatory;
    }
    texts.first().map(|f| vec![f.id.clone()]).unwrap_or_default()
}

fn is_text_like(field: &FieldDef) -> bool {
    matches!(
        field.ty,
        FieldType::String
            | FieldType::Text
            | FieldType::Url
            | FieldType::Integer
            | FieldType::Decimal
    )
}

fn is_lookup_like(field: &FieldDef) -> bool {
    matches!(field.ty, FieldType::Lookup | FieldType::Reference)
}

/// Default columns are a small set derived from the field list:
/// label fields, mandatory fields, then either lookups or remaining text
/// depending on whether the label fields already cover the mandatory text.
fn default_column_ids(entity: &EntityType) -> Vec<String> {
    let leaves = entity.top_level_leaves();
    let label_set: BTreeSet<&str> = entity.label_fields.iter().map(String::as_str).collect();
    // A single label field plus several sibling texts (an address line, then
    // city and postal code) should surface those texts. A multi-field label
    // (first and last name) already identifies the row, so lookups come next.
    let extra_text = leaves
        .iter()
        .filter(|f| is_text_like(f) && !label_set.contains(f.id.as_str()))
        .count();
    let prefer_text = entity.label_fields.len() <= 1 && extra_text >= 2;
    let mut out = Vec::new();
    let mut push = |id: &str| {
        if out.len() < 8 && !out.iter().any(|e: &String| e == id) {
            out.push(id.to_string());
        }
    };
    for id in &entity.label_fields {
        if entity.field(id).is_some_and(|f| !f.system) {
            push(id);
        }
    }
    for field in &leaves {
        if field.mandatory {
            push(&field.id);
        }
    }
    if prefer_text {
        for field in &leaves {
            if is_text_like(field) {
                push(&field.id);
            }
        }
        for field in &leaves {
            if is_lookup_like(field) {
                push(&field.id);
            }
        }
    } else {
        for field in &leaves {
            if is_lookup_like(field) {
                push(&field.id);
            }
        }
        for field in &leaves {
            if is_text_like(field) {
                push(&field.id);
            }
        }
    }
    for field in &leaves {
        push(&field.id);
    }
    out
}

fn mark_default_columns(fields: &mut [FieldDef], ids: &[String]) {
    for field in fields {
        field.default_column = ids.iter().any(|id| id == &field.id);
        mark_default_columns(&mut field.children, ids);
    }
}

pub fn format_integer(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        trim_number(n)
    }
}

pub fn trim_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        return format!("{}", n as i64);
    }
    let rendered = format!("{n:.4}");
    rendered
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

/// Structural check of one master record against its entity.
pub fn validate_values(
    model: &ModelDefinition,
    entity_id: &str,
    values: &BTreeMap<String, FieldValue>,
) -> Result<(), ModelError> {
    let Some(entity) = model.entity(entity_id) else {
        return Err(ModelError::Message(format!("unknown entity {entity_id}")));
    };
    check_value_map(model, &entity.id, &entity.fields, values, &entity.id)
}

fn check_value_map(
    model: &ModelDefinition,
    entity_id: &str,
    fields: &[FieldDef],
    values: &BTreeMap<String, FieldValue>,
    path: &str,
) -> Result<(), ModelError> {
    let known: BTreeSet<&str> = fields.iter().map(|f| f.name.as_str()).collect();
    for key in values.keys() {
        if key.starts_with('_') || known.contains(key.as_str()) {
            continue;
        }
        return Err(ModelError::Message(format!(
            "{path} has unknown field {key}"
        )));
    }
    for field in fields {
        if field.system {
            continue;
        }
        let value = values.get(&field.name);
        let here = if path == entity_id {
            format!("{entity_id}.{}", field.name)
        } else {
            format!("{path}.{}", field.name)
        };
        if field.ty == FieldType::Group {
            match value {
                None | Some(FieldValue::Null) => {}
                Some(FieldValue::List(items)) if field.many => {
                    for (index, item) in items.iter().enumerate() {
                        let FieldValue::Group(map) = item else {
                            return Err(ModelError::Message(format!(
                                "{here}[{index}] must be an object"
                            )));
                        };
                        check_value_map(model, entity_id, &field.children, map, &format!("{here}[{index}]"))?;
                    }
                }
                Some(FieldValue::Group(map)) if !field.many => {
                    check_value_map(model, entity_id, &field.children, map, &here)?;
                }
                Some(_) => {
                    return Err(ModelError::Message(format!(
                        "{here} must be {}a field group",
                        if field.many { "a list of " } else { "" }
                    )));
                }
            }
            continue;
        }
        if field.mandatory && is_empty(value) && !field.many {
            return Err(ModelError::Message(format!("{here} is required")));
        }
        if let Some(value) = value {
            check_scalar(model, field, value, &here)?;
        }
    }
    Ok(())
}

fn check_scalar(
    model: &ModelDefinition,
    field: &FieldDef,
    value: &FieldValue,
    path: &str,
) -> Result<(), ModelError> {
    match value {
        FieldValue::Null => Ok(()),
        FieldValue::List(items) => {
            for (index, item) in items.iter().enumerate() {
                check_scalar(model, field, item, &format!("{path}[{index}]"))?;
            }
            Ok(())
        }
        FieldValue::Group(_) => Err(ModelError::Message(format!(
            "{path} must be a single value"
        ))),
        FieldValue::Bool(_) => {
            if field.ty != FieldType::Boolean && field.ty != FieldType::Unknown {
                return Err(ModelError::Message(format!("{path} must not be a boolean")));
            }
            Ok(())
        }
        FieldValue::Number(_) => {
            if !matches!(
                field.ty,
                FieldType::Integer | FieldType::Decimal | FieldType::Unknown
            ) {
                return Err(ModelError::Message(format!("{path} must not be a number")));
            }
            Ok(())
        }
        FieldValue::String(text) => {
            if field.ty == FieldType::Lookup {
                if let Some(id) = field.lookup.as_deref() {
                    if let Some(lookup) = model.lookup(id) {
                        if !lookup.values.is_empty()
                            && !text.is_empty()
                            && !lookup.values.iter().any(|v| v.id == *text)
                        {
                            return Err(ModelError::Message(format!(
                                "{path} code {text} is not in code list {id}"
                            )));
                        }
                    }
                }
            }
            Ok(())
        }
    }
}

fn is_empty(value: Option<&FieldValue>) -> bool {
    match value {
        None | Some(FieldValue::Null) => true,
        Some(FieldValue::String(text)) => text.is_empty(),
        Some(FieldValue::List(items)) => items.is_empty(),
        _ => false,
    }
}

pub fn compose_label(entity: &EntityType, values: &BTreeMap<String, FieldValue>, model: &ModelDefinition) -> Option<String> {
    let mut parts = Vec::new();
    for path in &entity.label_fields {
        let Some(field) = entity.field(path) else { continue };
        let Some(value) = value_at(values, path) else { continue };
        let text = model.format_field(field, value, &|_, id| id.to_string());
        if !text.is_empty() {
            parts.push(text);
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}
