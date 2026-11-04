//! Data access. [`Backend`] is what the UI calls. [`MockBackend`] serves the
//! loaded model plus a JSON record file. A live IDMC implementation can
//! replace it later without changing views.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use idmc_model::{
    compose_label, load_records, validate_values, value_at, Cardinality, FieldSide, FieldValue,
    IdmcExportSource, JsonModelSource, ModelDefinition, ModelError, ModelSource, Record,
};

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Message(String),
}

pub trait Backend {
    fn model(&self) -> &ModelDefinition;
    fn records(&self, entity: &str) -> Vec<Record>;
    fn record(&self, entity: &str, id: &str) -> Option<Record>;
    fn label(&self, entity: &str, id: &str) -> String;
    fn related(&self, relationship: &str, source_id: &str) -> Result<Vec<Record>, BackendError>;
    fn hierarchy_roots(&self, entity: &str) -> Vec<Record>;
    fn hierarchy_children(&self, entity: &str, id: &str) -> Vec<Record>;
}

#[derive(Clone, Debug)]
pub struct MockBackend {
    model: ModelDefinition,
    records: Vec<Record>,
}

impl MockBackend {
    pub fn from_parts(model: ModelDefinition, records: Vec<Record>) -> Result<Self, BackendError> {
        let backend = Self { model, records };
        backend.validate_dataset()?;
        Ok(backend)
    }

    pub fn open_default() -> Result<Self, BackendError> {
        let mock = find_mock_dir().ok_or_else(|| {
            BackendError::Message(
                "could not find mock/ (run from the repo, or set IDMC_MOCK)".into(),
            )
        })?;
        let export = mock.join("idmc-export/person");
        let values = mock.join("reference-values.json");
        let data = mock.join("data.json");
        Self::open_export(&export, Some(&values), Some(&data))
    }

    pub fn open_export(
        export: &Path,
        values: Option<&Path>,
        data: Option<&Path>,
    ) -> Result<Self, BackendError> {
        let mut model = IdmcExportSource::open(export)?.load()?;
        if let Some(path) = values {
            let text = std::fs::read_to_string(path)?;
            model.apply_lookup_values(&text)?;
        }
        let records = match data {
            Some(path) => load_records(&std::fs::read_to_string(path)?)?,
            None => Vec::new(),
        };
        Self::from_parts(model, records)
    }

    pub fn open_json(model_path: &Path, data_path: &Path) -> Result<Self, BackendError> {
        let model = JsonModelSource::open(model_path)?.load()?;
        let records = load_records(&std::fs::read_to_string(data_path)?)?;
        Self::from_parts(model, records)
    }

    pub fn reload_data(&mut self, json: &str) -> Result<(), BackendError> {
        let records = load_records(json)?;
        let trial = Self {
            model: self.model.clone(),
            records,
        };
        trial.validate_dataset()?;
        self.records = trial.records;
        Ok(())
    }

    pub fn validate_dataset(&self) -> Result<(), BackendError> {
        let mut ids: BTreeMap<(&str, &str), ()> = BTreeMap::new();
        for record in &self.records {
            let entity = self.model.entity(&record.entity).ok_or_else(|| {
                BackendError::Message(format!("record {} has unknown entity {}", record.id, record.entity))
            })?;
            if entity.embedded_in.is_some() {
                return Err(BackendError::Message(format!(
                    "record {} is a child row; nest it inside its parent",
                    record.id
                )));
            }
            if ids.insert((record.entity.as_str(), record.id.as_str()), ()).is_some() {
                return Err(BackendError::Message(format!(
                    "duplicate record {} of {}",
                    record.id, record.entity
                )));
            }
            validate_values(&self.model, &record.entity, &record.values)?;
        }
        for record in &self.records {
            let entity = self.model.entity(&record.entity).unwrap();
            check_refs(&self.model, &self.records, entity, &record.values, &record.id)?;
        }
        Ok(())
    }

    fn sort_by_label(&self, mut rows: Vec<Record>) -> Vec<Record> {
        rows.sort_by(|a, b| {
            self.label_of(a)
                .cmp(&self.label_of(b))
                .then_with(|| a.id.cmp(&b.id))
        });
        rows
    }

    fn label_of(&self, record: &Record) -> String {
        let Some(entity) = self.model.entity(&record.entity) else {
            return record.id.clone();
        };
        compose_label(entity, &record.values, &self.model).unwrap_or_else(|| record.id.clone())
    }

    fn project_children(&self, parent: &Record, child_id: &str) -> Vec<Record> {
        let Some(child) = self.model.entity(child_id) else { return Vec::new() };
        let Some(embedded) = &child.embedded_in else { return Vec::new() };
        let Some(value) = parent.values.get(&embedded.field) else { return Vec::new() };
        let groups: Vec<&BTreeMap<String, FieldValue>> = match value {
            FieldValue::List(items) => items
                .iter()
                .filter_map(|item| match item {
                    FieldValue::Group(map) => Some(map),
                    _ => None,
                })
                .collect(),
            FieldValue::Group(map) => vec![map],
            _ => Vec::new(),
        };
        groups
            .into_iter()
            .enumerate()
            .map(|(index, values)| Record {
                entity: child.id.clone(),
                id: format!("{}::{}::{index}", parent.id, embedded.field),
                values: values.clone(),
            })
            .collect()
    }
}

impl Backend for MockBackend {
    fn model(&self) -> &ModelDefinition {
        &self.model
    }

    fn records(&self, entity: &str) -> Vec<Record> {
        if self
            .model
            .entity(entity)
            .and_then(|e| e.embedded_in.as_ref())
            .is_some()
        {
            let Some(child) = self.model.entity(entity) else { return Vec::new() };
            let parent_id = child.embedded_in.as_ref().map(|e| e.parent.as_str()).unwrap_or("");
            let mut rows = Vec::new();
            for parent in self.records.iter().filter(|r| r.entity == parent_id) {
                rows.extend(self.project_children(parent, entity));
            }
            return self.sort_by_label(rows);
        }
        let rows = self
            .records
            .iter()
            .filter(|r| r.entity == entity)
            .cloned()
            .collect();
        self.sort_by_label(rows)
    }

    fn record(&self, entity: &str, id: &str) -> Option<Record> {
        if self
            .model
            .entity(entity)
            .and_then(|e| e.embedded_in.as_ref())
            .is_some()
        {
            return self.records(entity).into_iter().find(|r| r.id == id);
        }
        self.records
            .iter()
            .find(|r| r.entity == entity && r.id == id)
            .cloned()
    }

    fn label(&self, entity: &str, id: &str) -> String {
        self.record(entity, id)
            .map(|r| self.label_of(&r))
            .unwrap_or_else(|| id.to_string())
    }

    fn related(&self, relationship: &str, source_id: &str) -> Result<Vec<Record>, BackendError> {
        if relationship == "identity" {
            return Err(BackendError::Message(
                "identity is resolved by the view, not the backend".into(),
            ));
        }
        let rel = self.model.relationship(relationship).ok_or_else(|| {
            BackendError::Message(format!("unknown relationship {relationship}"))
        })?;
        if rel.embedded {
            let parent = self.record(&rel.from, source_id).ok_or_else(|| {
                BackendError::Message(format!("no {} record {source_id}", rel.from))
            })?;
            return Ok(self.sort_by_label(self.project_children(&parent, &rel.to)));
        }
        match rel.field_on {
            FieldSide::To => {
                let rows = self.records(&rel.to)
                    .into_iter()
                    .filter(|row| {
                        value_at(&row.values, &rel.field)
                            .and_then(|v| v.as_id())
                            .is_some_and(|id| id == source_id)
                    })
                    .collect();
                Ok(self.sort_by_label(rows))
            }
            FieldSide::From => {
                let source = self.record(&rel.from, source_id).ok_or_else(|| {
                    BackendError::Message(format!("no {} record {source_id}", rel.from))
                })?;
                let Some(id) = value_at(&source.values, &rel.field).and_then(|v| v.as_id()) else {
                    return Ok(Vec::new());
                };
                match rel.cardinality {
                    Cardinality::OneToMany => Ok(self.records(&rel.to)
                        .into_iter()
                        .filter(|row| row.id == id)
                        .collect()),
                    _ => Ok(self.record(&rel.to, &id).into_iter().collect()),
                }
            }
        }
    }

    fn hierarchy_roots(&self, entity: &str) -> Vec<Record> {
        let Some(parent_field) = self
            .model
            .entity(entity)
            .and_then(|e| e.hierarchy.as_ref())
            .map(|h| h.parent_field.clone())
        else {
            return self.records(entity);
        };
        let rows = self
            .records(entity)
            .into_iter()
            .filter(|row| match value_at(&row.values, &parent_field) {
                None | Some(FieldValue::Null) => true,
                Some(FieldValue::String(text)) => text.is_empty(),
                _ => false,
            })
            .collect();
        self.sort_by_label(rows)
    }

    fn hierarchy_children(&self, entity: &str, id: &str) -> Vec<Record> {
        let Some(parent_field) = self
            .model
            .entity(entity)
            .and_then(|e| e.hierarchy.as_ref())
            .map(|h| h.parent_field.clone())
        else {
            return Vec::new();
        };
        let rows = self
            .records(entity)
            .into_iter()
            .filter(|row| {
                value_at(&row.values, &parent_field)
                    .and_then(|v| v.as_id())
                    .is_some_and(|parent| parent == id)
            })
            .collect();
        self.sort_by_label(rows)
    }
}

fn check_refs(
    model: &ModelDefinition,
    records: &[Record],
    entity: &idmc_model::EntityType,
    values: &BTreeMap<String, FieldValue>,
    path: &str,
) -> Result<(), BackendError> {
    for field in &entity.fields {
        let Some(value) = values.get(&field.name) else { continue };
        match (&field.ty, value) {
            (idmc_model::FieldType::Reference, FieldValue::String(id)) => {
                let target = field.target_entity.as_deref().unwrap_or("");
                if !id.is_empty()
                    && !records.iter().any(|r| r.entity == target && r.id == *id)
                {
                    return Err(BackendError::Message(format!(
                        "{path}.{} references missing {target} {id}",
                        field.name
                    )));
                }
            }
            (idmc_model::FieldType::Group, FieldValue::List(items)) => {
                for (index, item) in items.iter().enumerate() {
                    if let FieldValue::Group(map) = item {
                        check_refs(model, records, &group_as_entity(field), map, &format!("{path}.{}[{index}]", field.name))?;
                    }
                }
            }
            (idmc_model::FieldType::Group, FieldValue::Group(map)) => {
                check_refs(model, records, &group_as_entity(field), map, &format!("{path}.{}", field.name))?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn group_as_entity(field: &idmc_model::FieldDef) -> idmc_model::EntityType {
    idmc_model::EntityType {
        id: field.id.clone(),
        name: field.name.clone(),
        label: field.label.clone(),
        label_plural: String::new(),
        label_fields: Vec::new(),
        column_order: Vec::new(),
        fields: field.children.clone(),
        hierarchy: None,
        embedded_in: None,
    }
}

pub fn find_mock_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("IDMC_MOCK") {
        let path = PathBuf::from(dir);
        if path.join("idmc-export/person/exportMetadata.v2.json").is_file() {
            return Some(path);
        }
    }
    let mut starts = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            starts.push(parent.to_path_buf());
        }
    }
    starts.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    for start in starts {
        let mut dir = start;
        loop {
            if dir.join("mock/idmc-export/person/exportMetadata.v2.json").is_file() {
                return Some(dir.join("mock"));
            }
            if !dir.pop() {
                break;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person() -> MockBackend {
        let mock = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mock");
        MockBackend::open_export(
            &mock.join("idmc-export/person"),
            Some(&mock.join("reference-values.json")),
            Some(&mock.join("data.json")),
        )
        .unwrap()
    }

    #[test]
    fn persons_project_nested_addresses_and_labels() {
        let backend = person();
        assert_eq!(backend.label("c360.person", "p-anna"), "Anna Lindström");
        let addresses = backend
            .related("c360.person.PostalAddress#items", "p-anna")
            .unwrap();
        assert_eq!(addresses.len(), 2);
        assert_eq!(
            backend.label(&addresses[0].entity, &addresses[0].id),
            "Kungsgatan 12"
        );
        let country = backend.model().entity("c360.person.PostalAddress").unwrap().field("country").unwrap();
        let country_value = addresses[0].values.get("country").unwrap();
        assert_eq!(
            backend.model().format_field(country, country_value, &|_, id| id.to_string()),
            "Sweden"
        );
        assert!(backend
            .related("c360.person.ElectronicAddress#items", "p-erik")
            .unwrap()
            .is_empty());
        assert_eq!(
            backend.related("c360.person.Phone#items", "p-sofia").unwrap().len(),
            2
        );
        let people = backend.records("c360.person");
        assert_eq!(people.first().unwrap().id, "p-anna");
        assert_eq!(people.len(), 5);
    }

    #[test]
    fn sample_catalog_hierarchy_and_articles() {
        let mock = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mock/sample");
        let backend = MockBackend::open_json(&mock.join("model.json"), &mock.join("data.json")).unwrap();
        let roots = backend.hierarchy_roots("Category");
        assert_eq!(roots.len(), 2);
        let children = backend.hierarchy_children("Category", "cat-electronics");
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, "cat-phones");
        let articles = backend.related("product_articles", "prd-nx").unwrap();
        assert_eq!(articles.len(), 2);
        let product = backend.related("article_product", "art-nx-b").unwrap();
        assert_eq!(product.len(), 1);
        assert_eq!(backend.label("Product", "prd-nx"), "Nordic Phone X");
        let status = backend.model().entity("Product").unwrap().field("status").unwrap();
        let value = backend.record("Product", "prd-nx").unwrap().values.get("status").unwrap().clone();
        assert_eq!(
            backend.model().format_field(status, &value, &|e, id| backend.label(e, id)),
            "Active"
        );
    }
}
