//! Record and schema graphs. Nothing here names a Customer 360 or Product 360
//! field. Groups, child rows, and reference stubs come from the loaded model.
//!
//! Node marks, label priority, and the 400-node cap follow the exploration
//! graph in https://github.com/braian-io/braian (Apache-2.0). The parameters
//! are reimplemented here; the TypeScript is not copied.

use std::collections::{BTreeMap, HashMap};

use crate::types::{compose_label, Cardinality, EntityType, FieldSide, FieldType, ModelDefinition};
use crate::value::{value_at, FieldValue, Record};

pub const SCENE_CAP: usize = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeKind {
    Entity,
    ChildGroup,
    ChildRecord,
    Field,
    Type,
}

impl NodeKind {
    pub fn base_radius(self) -> f32 {
        match self {
            NodeKind::Entity => 22.0,
            NodeKind::Type => 24.0,
            NodeKind::ChildGroup => 18.0,
            NodeKind::ChildRecord => 15.0,
            NodeKind::Field => 14.0,
        }
    }

    /// Ring scale for the phyllotaxis seed. Hubs sit inside, leaves outside.
    pub fn seed_ring(self) -> f32 {
        match self {
            NodeKind::Entity | NodeKind::Type => 1.0,
            NodeKind::Field => 2.1,
            NodeKind::ChildRecord => 3.0,
            NodeKind::ChildGroup => 3.6,
        }
    }
}

pub fn radius_for(kind: NodeKind, degree: usize) -> f32 {
    let extra = ((degree.saturating_sub(1) as f32) * 2.0).min(14.0);
    kind.base_radius() + extra
}

/// One or two letters drawn inside the circle.
pub fn node_mark(caption: &str) -> String {
    let words: Vec<&str> = caption.split(|ch: char| ch.is_whitespace() || ch == '/').filter(|word| !word.is_empty()).collect();
    let Some(first) = words.first() else { return String::new() };
    if words.len() == 1 {
        let chars: Vec<char> = first.chars().collect();
        if chars.len() <= 2 {
            return (*first).to_string();
        }
        return chars.iter().take(2).collect::<String>().to_uppercase();
    }
    let a = first.chars().next().unwrap_or(' ');
    let b = words.get(1).and_then(|word| word.chars().next()).unwrap_or(' ');
    format!("{a}{b}").to_uppercase()
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum NodeId {
    Entity { be: String, id: String },
    ChildGroup { be: String, record: String, field: String },
    ChildRecord { be: String, id: String },
    Field { be: String, path: String },
    Type { be: String },
}

impl NodeId {
    pub fn key(&self) -> String {
        match self {
            NodeId::Entity { be, id } => format!("entity\t{be}\t{id}"),
            NodeId::ChildGroup { be, record, field } => format!("group\t{be}\t{record}\t{field}"),
            NodeId::ChildRecord { be, id } => format!("child\t{be}\t{id}"),
            NodeId::Field { be, path } => format!("field\t{be}\t{path}"),
            NodeId::Type { be } => format!("type\t{be}"),
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        let mut parts = key.split('\t');
        match parts.next()? {
            "entity" => Some(NodeId::Entity { be: parts.next()?.to_string(), id: parts.next()?.to_string() }),
            "group" => Some(NodeId::ChildGroup {
                be: parts.next()?.to_string(),
                record: parts.next()?.to_string(),
                field: parts.next()?.to_string(),
            }),
            "child" => Some(NodeId::ChildRecord { be: parts.next()?.to_string(), id: parts.next()?.to_string() }),
            "field" => Some(NodeId::Field { be: parts.next()?.to_string(), path: parts.next()?.to_string() }),
            "type" => Some(NodeId::Type { be: parts.next()?.to_string() }),
            _ => None,
        }
    }

    pub fn kind(&self) -> NodeKind {
        match self {
            NodeId::Entity { .. } => NodeKind::Entity,
            NodeId::ChildGroup { .. } => NodeKind::ChildGroup,
            NodeId::ChildRecord { .. } => NodeKind::ChildRecord,
            NodeId::Field { .. } => NodeKind::Field,
            NodeId::Type { .. } => NodeKind::Type,
        }
    }

    pub fn be_type(&self) -> &str {
        match self {
            NodeId::Entity { be, .. }
            | NodeId::ChildGroup { be, .. }
            | NodeId::ChildRecord { be, .. }
            | NodeId::Field { be, .. }
            | NodeId::Type { be } => be,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphProperty {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphNode {
    pub id: NodeId,
    pub kind: NodeKind,
    pub caption: String,
    pub subtitle: String,
    pub properties: Vec<GraphProperty>,
    pub be_type: String,
    pub raw_caption: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub capped: bool,
}

impl Scene {
    pub fn empty() -> Self {
        Self { nodes: Vec::new(), edges: Vec::new(), capped: false }
    }

    pub fn merge(&mut self, extra: Scene) {
        let mut index: HashMap<String, usize> = self.nodes.iter().enumerate().map(|(i, node)| (node.id.key(), i)).collect();
        for node in extra.nodes {
            let key = node.id.key();
            if let Some(&slot) = index.get(&key) {
                let existing = &mut self.nodes[slot];
                if prefers_incoming(existing, &node) {
                    existing.caption = node.caption;
                    if !node.subtitle.is_empty() {
                        existing.subtitle = node.subtitle;
                    }
                    existing.raw_caption = node.raw_caption;
                    existing.be_type = node.be_type;
                }
                if node.properties.len() > existing.properties.len() {
                    existing.properties = node.properties;
                }
            } else {
                index.insert(key, self.nodes.len());
                self.nodes.push(node);
            }
        }
        let mut seen: HashMap<String, ()> = self.edges.iter().map(|edge| (edge.id.clone(), ())).collect();
        for edge in extra.edges {
            let id = undirected_id(&edge.source, &edge.label, &edge.target);
            if seen.contains_key(&id) {
                continue;
            }
            seen.insert(id.clone(), ());
            self.edges.push(GraphEdge { id, ..edge });
        }
        self.apply_cap(SCENE_CAP);
    }

    pub fn apply_cap(&mut self, limit: usize) {
        if self.nodes.len() <= limit {
            self.capped = false;
            return;
        }
        self.nodes.truncate(limit);
        let ids: HashMap<String, ()> = self.nodes.iter().map(|node| (node.id.key(), ())).collect();
        self.edges.retain(|edge| ids.contains_key(&edge.source) && ids.contains_key(&edge.target));
        self.capped = true;
    }
}

fn prefers_incoming(existing: &GraphNode, incoming: &GraphNode) -> bool {
    existing.raw_caption && !incoming.raw_caption
}

fn undirected_id(source: &str, label: &str, target: &str) -> String {
    if source <= target {
        format!("{source}|{label}|{target}")
    } else {
        format!("{target}|{label}|{source}")
    }
}

fn edge(source: &str, label: &str, target: &str) -> GraphEdge {
    GraphEdge {
        id: undirected_id(source, label, target),
        source: source.to_string(),
        target: target.to_string(),
        label: label.to_string(),
    }
}

fn prop(label: &str, value: impl Into<String>) -> GraphProperty {
    GraphProperty { label: label.to_string(), value: value.into() }
}

/// Person (or any master) with its child collections and reference stubs.
pub fn build_entity_scene(record: &Record, model: &ModelDefinition) -> Scene {
    let mut scene = Scene::empty();
    let Some(entity) = model.entity(&record.entity) else { return scene };
    let caption = compose_label(entity, &record.values, model).unwrap_or_else(|| entity.label.clone());
    let self_id = NodeId::Entity { be: record.entity.clone(), id: record.id.clone() };
    let self_key = self_id.key();
    scene.nodes.push(GraphNode {
        id: self_id,
        kind: NodeKind::Entity,
        caption,
        subtitle: entity.label.clone(),
        properties: record_properties(entity, record, model),
        be_type: record.entity.clone(),
        raw_caption: false,
    });
    for child in model.embedded_children(&record.entity) {
        let Some(embedded) = &child.embedded_in else { continue };
        let group = NodeId::ChildGroup {
            be: record.entity.clone(),
            record: record.id.clone(),
            field: embedded.field.clone(),
        };
        let group_key = group.key();
        scene.nodes.push(GraphNode {
            id: group,
            kind: NodeKind::ChildGroup,
            caption: child.label.clone(),
            subtitle: child.label_plural.clone(),
            properties: vec![prop("Collection", &child.label), prop("Field", &embedded.field), prop("Parent", &entity.label)],
            be_type: child.id.clone(),
            raw_caption: false,
        });
        scene.edges.push(edge(&self_key, &child.label, &group_key));
        for row in child_rows(record, &embedded.field, &child.id) {
            let label = compose_label(child, &row.values, model).unwrap_or_else(|| child.label.clone());
            let node = NodeId::ChildRecord { be: child.id.clone(), id: row.id.clone() };
            let key = node.key();
            scene.nodes.push(GraphNode {
                id: node,
                kind: NodeKind::ChildRecord,
                caption: label,
                subtitle: child.label.clone(),
                properties: record_properties(child, &row, model),
                be_type: child.id.clone(),
                raw_caption: false,
            });
            scene.edges.push(edge(&group_key, &child.label, &key));
        }
    }
    for rel in model.relationships.iter().filter(|rel| {
        rel.from == record.entity && !rel.embedded && rel.field_on == FieldSide::From
    }) {
        let Some(value) = value_at(&record.values, &rel.field) else { continue };
        for target_id in reference_ids(value) {
            let target_label = model.entity(&rel.to).map(|item| item.label.clone()).unwrap_or_else(|| rel.to.clone());
            let node = NodeId::Entity { be: rel.to.clone(), id: target_id.clone() };
            let key = node.key();
            scene.nodes.push(GraphNode {
                id: node,
                kind: NodeKind::Entity,
                caption: target_id.clone(),
                subtitle: target_label.clone(),
                properties: vec![prop("Type", &target_label), prop("Id", &target_id)],
                be_type: rel.to.clone(),
                raw_caption: true,
            });
            scene.edges.push(edge(&self_key, &rel.label, &key));
        }
    }
    scene.apply_cap(SCENE_CAP);
    scene
}

/// Schema mode: one type per master entity, its fields, its child collections, and its references.
pub fn build_model_scene(model: &ModelDefinition) -> Scene {
    let mut scene = Scene::empty();
    for entity in model.roots() {
        let id = NodeId::Type { be: entity.id.clone() };
        let key = id.key();
        scene.nodes.push(GraphNode {
            id,
            kind: NodeKind::Type,
            caption: entity.label.clone(),
            subtitle: entity.label_plural.clone(),
            properties: vec![
                prop("Entity", &entity.label),
                prop("Id", &entity.id),
                prop("Fields", entity.leaves().len().to_string()),
            ],
            be_type: entity.id.clone(),
            raw_caption: false,
        });
        for field in entity.fields.iter().filter(|field| !field.system && field.ty != FieldType::Group) {
            let node = NodeId::Field { be: entity.id.clone(), path: field.id.clone() };
            let field_key = node.key();
            scene.nodes.push(GraphNode {
                id: node,
                kind: NodeKind::Field,
                caption: field.label.clone(),
                subtitle: field_kind_label(field.ty),
                properties: vec![prop("Field", &field.label), prop("Path", &field.id), prop("Entity", &entity.label)],
                be_type: entity.id.clone(),
                raw_caption: false,
            });
            scene.edges.push(edge(&key, &field.label, &field_key));
        }
        for child in model.embedded_children(&entity.id) {
            let Some(embedded) = &child.embedded_in else { continue };
            let node = NodeId::ChildGroup {
                be: entity.id.clone(),
                record: String::new(),
                field: embedded.field.clone(),
            };
            let group_key = node.key();
            scene.nodes.push(GraphNode {
                id: node,
                kind: NodeKind::ChildGroup,
                caption: child.label.clone(),
                subtitle: cardinality_label(model, &entity.id, &child.id),
                properties: vec![prop("Collection", &child.label), prop("Field", &embedded.field), prop("Parent", &entity.label)],
                be_type: child.id.clone(),
                raw_caption: false,
            });
            scene.edges.push(edge(&key, &child.label, &group_key));
        }
    }
    for rel in model.relationships.iter().filter(|rel| !rel.embedded && rel.field_on == FieldSide::From) {
        if model.entity(&rel.from).and_then(|entity| entity.embedded_in.as_ref()).is_some() {
            continue;
        }
        if model.entity(&rel.to).and_then(|entity| entity.embedded_in.as_ref()).is_some() {
            continue;
        }
        let source = NodeId::Type { be: rel.from.clone() }.key();
        let target = NodeId::Type { be: rel.to.clone() }.key();
        scene.edges.push(edge(&source, &rel.label, &target));
    }
    scene.apply_cap(SCENE_CAP);
    scene
}

/// Fields of one record, used when a child row or a fetched stub is expanded.
pub fn build_record_fields(record: &Record, model: &ModelDefinition) -> Scene {
    let mut scene = Scene::empty();
    let Some(entity) = model.entity(&record.entity) else { return scene };
    let owner = if entity.embedded_in.is_some() {
        NodeId::ChildRecord { be: record.entity.clone(), id: record.id.clone() }
    } else {
        NodeId::Entity { be: record.entity.clone(), id: record.id.clone() }
    };
    let owner_key = owner.key();
    let caption = compose_label(entity, &record.values, model).unwrap_or_else(|| entity.label.clone());
    scene.nodes.push(GraphNode {
        id: owner,
        kind: if entity.embedded_in.is_some() { NodeKind::ChildRecord } else { NodeKind::Entity },
        caption,
        subtitle: entity.label.clone(),
        properties: record_properties(entity, record, model),
        be_type: record.entity.clone(),
        raw_caption: false,
    });
    for field in entity.leaves().into_iter().filter(|field| !field.system && field.ty != FieldType::Group) {
        let shown = value_at(&record.values, &field.id)
            .map(|value| scalar_text(value))
            .unwrap_or_default();
        if shown.is_empty() {
            continue;
        }
        let node = NodeId::Field { be: record.entity.clone(), path: format!("{}:{}", record.id, field.id) };
        let key = node.key();
        scene.nodes.push(GraphNode {
            id: node,
            kind: NodeKind::Field,
            caption: field.label.clone(),
            subtitle: shorten(&shown, 32),
            properties: vec![prop("Field", &field.label), prop("Value", shorten(&shown, 80))],
            be_type: record.entity.clone(),
            raw_caption: false,
        });
        scene.edges.push(edge(&owner_key, &field.label, &key));
    }
    scene.apply_cap(SCENE_CAP);
    scene
}

fn cardinality_label(model: &ModelDefinition, parent: &str, child: &str) -> String {
    model
        .relationships
        .iter()
        .find(|rel| rel.from == parent && rel.to == child)
        .map(|rel| match rel.cardinality {
            Cardinality::OneToMany => "one to many".to_string(),
            Cardinality::OneToOne => "one to one".to_string(),
            Cardinality::ManyToOne => "many to one".to_string(),
        })
        .unwrap_or_else(|| "collection".into())
}

fn field_kind_label(ty: FieldType) -> String {
    match ty {
        FieldType::String => "text",
        FieldType::Text => "text",
        FieldType::Integer => "integer",
        FieldType::Decimal => "decimal",
        FieldType::Boolean => "boolean",
        FieldType::Date => "date",
        FieldType::DateTime => "date",
        FieldType::Url => "url",
        FieldType::Lookup => "lookup",
        FieldType::Reference => "reference",
        FieldType::Group => "group",
        FieldType::Unknown => "value",
    }
    .into()
}

fn record_properties(entity: &EntityType, record: &Record, model: &ModelDefinition) -> Vec<GraphProperty> {
    let mut props = vec![prop("Type", &entity.label), prop("Id", &record.id)];
    for field in entity.leaves().into_iter().filter(|field| !field.system).take(6) {
        let Some(value) = value_at(&record.values, &field.id) else { continue };
        let text = scalar_text(value);
        if text.is_empty() {
            continue;
        }
        let _ = model;
        props.push(prop(&field.label, shorten(&text, 80)));
        if props.len() >= 8 {
            break;
        }
    }
    props
}

fn scalar_text(value: &FieldValue) -> String {
    match value {
        FieldValue::String(text) => text.clone(),
        FieldValue::Number(number) => {
            if number.fract() == 0.0 {
                format!("{}", *number as i64)
            } else {
                format!("{number}")
            }
        }
        FieldValue::Bool(flag) => flag.to_string(),
        FieldValue::List(items) => items.iter().filter_map(|item| match item {
            FieldValue::String(text) => Some(text.clone()),
            _ => None,
        }).collect::<Vec<_>>().join(", "),
        _ => String::new(),
    }
}

fn shorten(text: &str, max: usize) -> String {
    let clean = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() <= max {
        return clean;
    }
    let truncated: String = clean.chars().take(max.saturating_sub(1)).collect();
    format!("{truncated}…")
}

fn child_rows(parent: &Record, field: &str, child_entity: &str) -> Vec<Record> {
    let Some(value) = parent.values.get(field) else { return Vec::new() };
    let groups: Vec<&BTreeMap<String, FieldValue>> = match value {
        FieldValue::List(items) => items.iter().filter_map(|item| match item {
            FieldValue::Group(map) => Some(map),
            _ => None,
        }).collect(),
        FieldValue::Group(map) => vec![map],
        _ => Vec::new(),
    };
    groups
        .into_iter()
        .enumerate()
        .map(|(index, values)| Record {
            entity: child_entity.to_string(),
            id: format!("{}::{field}::{index}", parent.id),
            values: values.clone(),
        })
        .collect()
}

fn reference_ids(value: &FieldValue) -> Vec<String> {
    match value {
        FieldValue::String(text) if !text.is_empty() => vec![text.clone()],
        FieldValue::Number(number) => vec![format!("{}", *number as i64)],
        FieldValue::Group(map) => map
            .get("businessId")
            .and_then(FieldValue::as_id)
            .into_iter()
            .collect(),
        FieldValue::List(items) => items.iter().flat_map(reference_ids).collect(),
        _ => Vec::new(),
    }
}

/// Expand one node from records the caller already has. A missing master record
/// yields an empty scene so a live backend can fetch and call again.
pub fn expand_key(model: &ModelDefinition, records: &[Record], key: &str) -> Scene {
    let Some(id) = NodeId::parse(key) else { return Scene::empty() };
    match id {
        NodeId::Entity { be, id } => records
            .iter()
            .find(|row| row.entity == be && row.id == id)
            .map(|row| build_entity_scene(row, model))
            .unwrap_or_default(),
        NodeId::ChildRecord { id, .. } => {
            let parent_id = id.split("::").next().unwrap_or("");
            let Some(parent) = records.iter().find(|row| row.id == parent_id) else { return Scene::empty() };
            let Some(child) = child_rows_all(parent, model).into_iter().find(|row| row.id == id) else { return Scene::empty() };
            build_record_fields(&child, model)
        }
        NodeId::ChildGroup { be, record, .. } => records
            .iter()
            .find(|row| row.entity == be && (record.is_empty() || row.id == record))
            .map(|row| build_entity_scene(row, model))
            .unwrap_or_default(),
        NodeId::Field { .. } => Scene::empty(),
        NodeId::Type { be } => {
            let mut scene = Scene::empty();
            let mut count = 0usize;
            for row in records.iter().filter(|row| row.entity == be) {
                scene.merge(build_entity_scene(row, model));
                count += 1;
                if count >= 8 {
                    break;
                }
            }
            scene
        }
    }
}

fn child_rows_all(parent: &Record, model: &ModelDefinition) -> Vec<Record> {
    let mut rows = Vec::new();
    for child in model.embedded_children(&parent.entity) {
        let Some(embedded) = &child.embedded_in else { continue };
        rows.extend(child_rows(parent, &embedded.field, &child.id));
    }
    rows
}

impl Default for Scene {
    fn default() -> Self { Self::empty() }
}

/// A search hit that has not been expanded yet. The caption is the display name.
pub fn stub_entity(be: &str, id: &str, caption: &str, type_label: &str) -> GraphNode {
    let raw = caption.is_empty() || caption == id;
    GraphNode {
        id: NodeId::Entity { be: be.to_string(), id: id.to_string() },
        kind: NodeKind::Entity,
        caption: if raw { id.to_string() } else { caption.to_string() },
        subtitle: type_label.to_string(),
        properties: vec![prop("Type", type_label), prop("Id", id)],
        be_type: be.to_string(),
        raw_caption: raw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IdmcExportSource, ModelSource};
    use std::path::PathBuf;

    fn person_model() -> ModelDefinition {
        IdmcExportSource::open(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mock/idmc-export/person"))
            .unwrap()
            .load()
            .unwrap()
    }

    fn anna() -> Record {
        let json = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mock/data.json")).unwrap();
        crate::load_records(&json).unwrap().into_iter().find(|row| row.id.contains("anna") || row.values.get("firstName").and_then(|v| v.as_id()).as_deref() == Some("Anna")).expect("anna")
    }

    #[test]
    fn person_scene_has_groups_then_child_records() {
        let model = person_model();
        let record = anna();
        let scene = build_entity_scene(&record, &model);
        let groups: Vec<_> = model.embedded_children(&record.entity).map(|child| child.label.clone()).collect();
        assert!(groups.len() >= 3, "{groups:?}");
        for label in &groups {
            assert!(scene.nodes.iter().any(|node| node.kind == NodeKind::ChildGroup && node.caption == *label), "missing group {label}");
        }
        let children = scene.nodes.iter().filter(|node| node.kind == NodeKind::ChildRecord).count();
        assert!(children >= 1, "child records");
        let person = scene.nodes.iter().find(|node| node.kind == NodeKind::Entity && !node.raw_caption).unwrap();
        assert!(person.caption.contains("Anna"), "{}", person.caption);
        for edge in &scene.edges {
            assert!(scene.nodes.iter().any(|node| node.id.key() == edge.source));
            assert!(scene.nodes.iter().any(|node| node.id.key() == edge.target));
        }
    }

    #[test]
    fn reference_stub_is_replaced_when_the_record_arrives() {
        let model = IdmcExportSource::open(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mock/idmc-export/item"))
            .unwrap()
            .load()
            .unwrap();
        let rel = model.relationships.iter().find(|rel| !rel.embedded && rel.field_on == FieldSide::From).expect("reference");
        let mut values = BTreeMap::new();
        values.insert(rel.field.clone(), FieldValue::String("sup-1".into()));
        let record = Record { entity: rel.from.clone(), id: "item-1".into(), values };
        let mut scene = build_entity_scene(&record, &model);
        let stub = scene.nodes.iter().find(|node| node.raw_caption).expect("stub");
        assert_eq!(stub.caption, "sup-1");
        assert_eq!(stub.be_type, rel.to);
        let fetched = GraphNode {
            id: stub.id.clone(),
            kind: NodeKind::Entity,
            caption: "Acme Supply".into(),
            subtitle: stub.subtitle.clone(),
            properties: vec![prop("Type", &stub.subtitle), prop("Id", "sup-1"), prop("Name", "Acme Supply")],
            be_type: stub.be_type.clone(),
            raw_caption: false,
        };
        let mut extra = Scene::empty();
        extra.nodes.push(fetched);
        scene.merge(extra);
        let node = scene.nodes.iter().find(|node| matches!(&node.id, NodeId::Entity { id, .. } if id == "sup-1")).unwrap();
        assert_eq!(node.caption, "Acme Supply");
        assert!(!node.raw_caption);
    }

    #[test]
    fn schema_scene_is_model_driven_and_capped() {
        let model = person_model();
        let scene = build_model_scene(&model);
        assert!(scene.nodes.iter().any(|node| node.kind == NodeKind::Type));
        assert!(scene.nodes.iter().any(|node| node.kind == NodeKind::ChildGroup));
        assert!(scene.nodes.iter().any(|node| node.kind == NodeKind::Field));
        assert!(!scene.capped);
        let mut huge = Scene::empty();
        for index in 0..SCENE_CAP + 5 {
            huge.nodes.push(stub_entity("c360.person", &format!("id{index}"), &format!("N{index}"), "Person"));
        }
        huge.apply_cap(SCENE_CAP);
        assert!(huge.capped);
        assert_eq!(huge.nodes.len(), SCENE_CAP);
    }

    #[test]
    fn node_ids_roundtrip_including_child_colons() {
        let id = NodeId::ChildRecord { be: "c360.person.PostalAddress".into(), id: "p-anna::PostalAddress::0".into() };
        assert_eq!(NodeId::parse(&id.key()).unwrap(), id);
        assert_eq!(node_mark("Postal Address"), "PA");
        assert_eq!(node_mark("Phone"), "PH");
    }
}
