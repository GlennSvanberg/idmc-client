//! Data access. [`Backend`] is what the UI calls. [`MockBackend`] serves the
//! loaded model plus a JSON record file. A live IDMC implementation can
//! replace it later without changing views.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use idmc_model::{
    compose_label, load_records, validate_changed, validate_edit, validate_values, value_at, Cardinality, FieldSide,
    FieldValue, IdmcExportSource, JsonModelSource, ModelDefinition, ModelError, ModelSource, Record,
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
    /// Fetch a fuller master record when a search hit is missing nested groups.
    fn prepare(&self, _entity: &str, _id: &str) {}
    /// True when background loading has appended records since the last call.
    fn poll(&self) -> bool { false }
    /// Short connection line for the window chrome. No secrets.
    fn connection(&self) -> String { "Mock".into() }
    /// Display name captured from a live lookup `{Code, Name}` object.
    fn lookup_name(&self, _lookup: Option<&str>, _code: &str) -> Option<String> { None }
    /// Code/name pairs seen on live records for one code list. Empty when the
    /// tenant catalog has not been observed. The sample file is not this list.
    fn lookup_catalog(&self, _lookup: &str) -> Vec<(String, String)> { Vec::new() }
    /// Code/name pairs from a Reference 360 code list, when that call succeeds.
    fn tenant_codes(&self, _lookup: &str) -> Vec<(String, String)> { Vec::new() }
    /// Bumps when the record set changes. Views cache sorts against this.
    fn generation(&self) -> u64 { 0 }
    /// Sorted records for one entity, shared until `generation` changes.
    fn shared_records(&self, entity: &str) -> Arc<Vec<Record>> {
        Arc::new(self.records(entity))
    }
    /// Total rows for a master table. Live lists report the server hit count
    /// before every row has been fetched.
    fn master_total(&self, entity: &str) -> usize {
        self.shared_records(entity).len()
    }
    fn master_known(&self, _entity: &str) -> bool { true }
    fn master_at(&self, entity: &str, index: usize) -> Option<Record> {
        self.shared_records(entity).get(index).cloned()
    }
    /// Ask a live list to fetch the pages covering this range. Mock data is local.
    fn master_touch(&self, _entity: &str, _start: usize, _end: usize) {}
    /// Field ids present on search hits for this entity. `None` means search
    /// has not reported a field set (mock data, or a live list that has not
    /// returned a page). `Some` may be empty when search omitted the entity.
    fn search_fields(&self, _entity: &str) -> Option<Vec<String>> { None }
    fn set_master_view(&self, _entity: &str, _query: &str, _sort_field: Option<&str>, _descending: bool) {}
    fn search_records(&self, _query: &str, _limit: usize) -> Vec<SearchHit> { Vec::new() }
    fn schedule_record_search(&self, _query: &str) {}
    fn take_record_search(&self) -> Option<Vec<SearchHit>> { None }
    fn timing_lines(&self) -> Vec<String> { Vec::new() }
    /// Fetch the neighborhood of a graph node. In-flight calls for the same key are dropped.
    fn schedule_expand(&self, _key: &str) {}
    fn take_expand(&self) -> Option<idmc_model::Scene> { None }
    /// Create, update, or delete one master record. Child rows are saved as part of the parent.
    fn submit_write(&self, _op: WriteOp) -> Result<Record, BackendError> {
        Err(BackendError::Message("this backend is read-only".into()))
    }
    /// A live write that failed after the optimistic update, or a server-assigned id.
    fn take_write_notice(&self) -> Option<WriteNotice> { None }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteKind {
    Create,
    Update,
    Delete,
}

#[derive(Clone, Debug)]
pub struct WriteOp {
    pub kind: WriteKind,
    pub entity: String,
    pub id: String,
    pub source_system: String,
    pub source_pkey: String,
    pub values: BTreeMap<String, FieldValue>,
    pub before: Option<Record>,
}

#[derive(Clone, Debug)]
pub struct WriteNotice {
    pub error: Option<String>,
    pub message: Option<String>,
    pub entity: String,
    pub from_id: String,
    pub to_id: Option<String>,
    pub restore: Option<Record>,
}

pub fn create_path(entity: &str, source_system: &str, source_pkey: &str) -> String {
    format!(
        "/business-entity/public/api/v1/entity/{entity}?sourceSystem={}&sourcePKey={}",
        query_escape(source_system),
        query_escape(source_pkey)
    )
}

pub fn patch_path(entity: &str, id: &str, source_system: &str) -> String {
    format!(
        "/business-entity/public/api/v1/entity/{entity}/{id}?sourceSystem={}",
        query_escape(source_system)
    )
}

pub fn delete_path(entity: &str, id: &str, source_system: &str, source_pkey: Option<&str>) -> String {
    let mut path = format!(
        "/business-entity/public/api/v1/entity/{entity}/{id}?sourceSystem={}",
        query_escape(source_system)
    );
    if let Some(pkey) = source_pkey.filter(|text| !text.is_empty()) {
        path.push_str("&sourcePKey=");
        path.push_str(&query_escape(pkey));
    }
    path
}

fn query_escape(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub fn apply_records(model: &ModelDefinition, records: &mut Vec<Record>, op: &WriteOp) -> Result<Record, BackendError> {
    if op.source_system.is_empty() {
        return Err(BackendError::Message("sourceSystem is required".into()));
    }
    if op.kind == WriteKind::Create && op.source_pkey.is_empty() {
        return Err(BackendError::Message("sourcePKey is required".into()));
    }
    let entity = model
        .entity(&op.entity)
        .ok_or_else(|| BackendError::Message(format!("unknown entity {}", op.entity)))?;
    if entity.embedded_in.is_some() {
        return Err(BackendError::Message(
            "save the parent record; child rows are part of it".into(),
        ));
    }
    match op.kind {
        WriteKind::Create => {
            validate_edit(model, &op.entity, &op.values).map_err(|err| BackendError::Message(err.to_string()))?;
            let id = if op.id.is_empty() || op.id.starts_with("new:") {
                alloc_id(records, &op.entity)
            } else if records.iter().any(|row| row.entity == op.entity && row.id == op.id) {
                return Err(BackendError::Message(format!("{} {} already exists", op.entity, op.id)));
            } else {
                op.id.clone()
            };
            let record = Record { entity: op.entity.clone(), id, values: op.values.clone() };
            records.push(record.clone());
            Ok(record)
        }
        WriteKind::Update => {
            let current = records
                .iter()
                .find(|row| row.entity == op.entity && row.id == op.id)
                .ok_or_else(|| BackendError::Message(format!("no {} record {}", op.entity, op.id)))?;
            validate_changed(model, &op.entity, &current.values, &op.values)
                .map_err(|err| BackendError::Message(err.to_string()))?;
            let row = records
                .iter_mut()
                .find(|row| row.entity == op.entity && row.id == op.id)
                .expect("record");
            row.values = op.values.clone();
            Ok(row.clone())
        }
        WriteKind::Delete => {
            let pos = records
                .iter()
                .position(|row| row.entity == op.entity && row.id == op.id)
                .ok_or_else(|| BackendError::Message(format!("no {} record {}", op.entity, op.id)))?;
            Ok(records.remove(pos))
        }
    }
}

fn alloc_id(records: &[Record], entity: &str) -> String {
    let mut n = records.iter().filter(|row| row.entity == entity).count() + 1;
    loop {
        let id = format!("local-{n}");
        if !records.iter().any(|row| row.entity == entity && row.id == id) {
            return id;
        }
        n += 1;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchHit {
    pub entity: String,
    pub entity_label: String,
    pub id: String,
    pub label: String,
    pub detail: String,
}

mod config;
mod live;

pub use config::{load_dotenv, read_env, BackendMode, EnvConfig};
pub use live::{
    check_login, classify_login, mdm_base_from_api_url, LiveBackend, LiveConfig, LoginCause, LoginFailure,
};

#[derive(Clone, Debug)]
struct ListCache {
    generation: u64,
    lists: HashMap<String, Arc<Vec<Record>>>,
}

impl Default for ListCache {
    fn default() -> Self {
        Self { generation: u64::MAX, lists: HashMap::new() }
    }
}

#[derive(Clone)]
struct MasterView {
    query: String,
    sort_field: Option<String>,
    descending: bool,
}

pub struct MockBackend {
    model: ModelDefinition,
    records: RefCell<Vec<Record>>,
    generation: Cell<u64>,
    lists: RefCell<ListCache>,
    view_state: RefCell<HashMap<String, MasterView>>,
    expand_inflight: RefCell<HashSet<String>>,
    expand_ready: RefCell<Vec<(String, idmc_model::Scene)>>,
    source_keys: RefCell<HashMap<(String, String), (String, String)>>,
}

impl MockBackend {
    pub fn from_parts(model: ModelDefinition, records: Vec<Record>) -> Result<Self, BackendError> {
        let backend = Self::with_records(model, records, 1);
        backend.validate_dataset()?;
        Ok(backend)
    }

    /// Records that skip dataset validation. Used to time large lists.
    pub fn from_records_unchecked(model: ModelDefinition, records: Vec<Record>) -> Self {
        Self::with_records(model, records, 1)
    }

    fn with_records(model: ModelDefinition, records: Vec<Record>, generation: u64) -> Self {
        Self {
            model,
            records: RefCell::new(records),
            generation: Cell::new(generation),
            lists: RefCell::new(ListCache::default()),
            view_state: RefCell::new(HashMap::new()),
            expand_inflight: RefCell::new(HashSet::new()),
            expand_ready: RefCell::new(Vec::new()),
            source_keys: RefCell::new(HashMap::new()),
        }
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
        let trial = Self::with_records(self.model.clone(), records, self.generation.get());
        trial.validate_dataset()?;
        self.records = trial.records;
        self.generation.set(self.generation.get().wrapping_add(1));
        self.lists.borrow_mut().lists.clear();
        Ok(())
    }

    pub fn validate_dataset(&self) -> Result<(), BackendError> {
        let mut ids: BTreeMap<(&str, &str), ()> = BTreeMap::new();
        let records = self.records.borrow();
        for record in records.iter() {
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
        for record in records.iter() {
            let entity = self.model.entity(&record.entity).unwrap();
            check_refs(&self.model, &records, entity, &record.values, &record.id)?;
        }
        Ok(())
    }

}

impl MockBackend {
    fn cached(&self, key: &str, build: impl FnOnce() -> Vec<Record>) -> Arc<Vec<Record>> {
        let generation = self.generation.get();
        {
            let mut slot = self.lists.borrow_mut();
            if slot.generation != generation {
                slot.lists.clear();
                slot.generation = generation;
            }
            if let Some(hit) = slot.lists.get(key) {
                return Arc::clone(hit);
            }
        }
        let rows = Arc::new(build());
        let mut slot = self.lists.borrow_mut();
        if slot.generation != generation {
            slot.lists.clear();
            slot.generation = generation;
        }
        slot.lists.insert(key.to_string(), Arc::clone(&rows));
        rows
    }
}

impl Backend for MockBackend {
    fn model(&self) -> &ModelDefinition { &self.model }
    fn records(&self, entity: &str) -> Vec<Record> { self.shared_records(entity).as_ref().clone() }
    fn shared_records(&self, entity: &str) -> Arc<Vec<Record>> {
        let view = self.view_state.borrow().get(entity).cloned();
        let key = match &view {
            Some(view) => format!(
                "records\0{entity}\0{}\0{}\0{}",
                view.query,
                view.sort_field.clone().unwrap_or_default(),
                view.descending
            ),
            None => format!("records\0{entity}"),
        };
        let model = &self.model;
        let records = self.records.borrow();
        self.cached(&key, || {
            let mut rows = records_of(model, &records, entity);
            if let Some(view) = &view {
                if let Some(field) = &view.sort_field {
                    let field = field.clone();
                    let descending = view.descending;
                    rows.sort_by(|left, right| {
                        let order = text_at(left, &field).cmp(&text_at(right, &field)).then_with(|| left.id.cmp(&right.id));
                        if descending { order.reverse() } else { order }
                    });
                }
                if !view.query.trim().is_empty() {
                    let query = view.query.to_ascii_lowercase();
                    rows.retain(|row| record_blob(row).contains(&query));
                }
            }
            rows
        })
    }
    fn record(&self, entity: &str, id: &str) -> Option<Record> { record_of(&self.model, &self.records.borrow(), entity, id) }
    fn label(&self, entity: &str, id: &str) -> String { label_of(&self.model, &self.records.borrow(), entity, id) }
    fn related(&self, relationship: &str, source_id: &str) -> Result<Vec<Record>, BackendError> {
        let key = format!("rel\0{relationship}\0{source_id}");
        {
            let slot = self.lists.borrow();
            if slot.generation == self.generation.get() {
                if let Some(hit) = slot.lists.get(&key) {
                    return Ok(hit.as_ref().clone());
                }
            }
        }
        let rows = related_of(&self.model, &self.records.borrow(), relationship, source_id)?;
        self.cached(&key, || rows.clone());
        Ok(rows)
    }
    fn hierarchy_roots(&self, entity: &str) -> Vec<Record> {
        let records = self.records.borrow();
        self.cached(&format!("roots\0{entity}"), || hierarchy_roots_of(&self.model, &*records, entity))
            .as_ref()
            .clone()
    }
    fn hierarchy_children(&self, entity: &str, id: &str) -> Vec<Record> {
        self.cached(&format!("kids\0{entity}\0{id}"), || {
            let held = self.records.borrow();
            hierarchy_children_of(&self.model, &*held, entity, id)
        })
        .as_ref()
        .clone()
    }
    fn generation(&self) -> u64 { self.generation.get() }
    fn master_total(&self, entity: &str) -> usize { self.shared_records(entity).len() }
    fn master_at(&self, entity: &str, index: usize) -> Option<Record> {
        self.shared_records(entity).get(index).cloned()
    }
    fn set_master_view(&self, entity: &str, query: &str, sort_field: Option<&str>, descending: bool) {
        self.view_state.borrow_mut().insert(
            entity.to_string(),
            MasterView {
                query: query.trim().to_string(),
                sort_field: sort_field.map(str::to_string),
                descending,
            },
        );
        self.lists.borrow_mut().lists.clear();
    }
    fn search_records(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        search_local(&self.model, &self.records.borrow(), query, limit)
    }
    fn schedule_expand(&self, key: &str) {
        if !self.expand_inflight.borrow_mut().insert(key.to_string()) {
            return;
        }
        let scene = idmc_model::expand_key(&self.model, &self.records.borrow(), key);
        if scene.nodes.is_empty() && scene.edges.is_empty() {
            self.expand_inflight.borrow_mut().remove(key);
            return;
        }
        self.expand_ready.borrow_mut().push((key.to_string(), scene));
    }
    fn take_expand(&self) -> Option<idmc_model::Scene> {
        let mut ready = self.expand_ready.borrow_mut();
        if ready.is_empty() {
            None
        } else {
            let (key, scene) = ready.remove(0);
            self.expand_inflight.borrow_mut().remove(&key);
            Some(scene)
        }
    }
    fn submit_write(&self, mut op: WriteOp) -> Result<Record, BackendError> {
        if op.source_pkey.is_empty() {
            if let Some((_, pkey)) = self.source_keys.borrow().get(&(op.entity.clone(), op.id.clone())) {
                op.source_pkey = pkey.clone();
            }
        }
        let saved = {
            let mut records = self.records.borrow_mut();
            apply_records(&self.model, &mut records, &op)?
        };
        if op.kind == WriteKind::Create {
            self.source_keys.borrow_mut().insert(
                (saved.entity.clone(), saved.id.clone()),
                (op.source_system, op.source_pkey),
            );
        }
        if op.kind == WriteKind::Delete {
            self.source_keys.borrow_mut().remove(&(op.entity, op.id));
        }
        self.generation.set(self.generation.get().wrapping_add(1));
        Ok(saved)
    }
}

pub(crate) fn records_of(model: &ModelDefinition, records: &[Record], entity: &str) -> Vec<Record> {
    if model.entity(entity).and_then(|e| e.embedded_in.as_ref()).is_some() {
        let Some(child) = model.entity(entity) else { return Vec::new() };
        let parent_id = child.embedded_in.as_ref().map(|e| e.parent.as_str()).unwrap_or("");
        let mut rows = Vec::new();
        for parent in records.iter().filter(|r| r.entity == parent_id) {
            rows.extend(project_children(model, parent, entity));
        }
        return sort_by_label(model, rows);
    }
    let rows = records.iter().filter(|r| r.entity == entity).cloned().collect();
    sort_by_label(model, rows)
}

pub(crate) fn record_of(model: &ModelDefinition, records: &[Record], entity: &str, id: &str) -> Option<Record> {
    if model.entity(entity).and_then(|e| e.embedded_in.as_ref()).is_some() {
        return records_of(model, records, entity).into_iter().find(|r| r.id == id);
    }
    records.iter().find(|r| r.entity == entity && r.id == id).cloned()
}

pub(crate) fn label_of(model: &ModelDefinition, records: &[Record], entity: &str, id: &str) -> String {
    record_of(model, records, entity, id)
        .map(|record| label_text(model, &record))
        .unwrap_or_else(|| id.to_string())
}

pub(crate) fn related_of(
    model: &ModelDefinition,
    records: &[Record],
    relationship: &str,
    source_id: &str,
) -> Result<Vec<Record>, BackendError> {
    if relationship == "identity" {
        return Err(BackendError::Message(
            "identity is resolved by the view, not the backend".into(),
        ));
    }
    let rel = model.relationship(relationship).ok_or_else(|| {
        BackendError::Message(format!("unknown relationship {relationship}"))
    })?;
    if rel.embedded {
        let parent = record_of(model, records, &rel.from, source_id).ok_or_else(|| {
            BackendError::Message(format!("no {} record {source_id}", rel.from))
        })?;
        return Ok(sort_by_label(model, project_children(model, &parent, &rel.to)));
    }
    match rel.field_on {
        FieldSide::To => {
            let rows = records_of(model, records, &rel.to)
                .into_iter()
                .filter(|row| {
                    value_at(&row.values, &rel.field)
                        .and_then(|v| v.as_id())
                        .is_some_and(|id| id == source_id)
                })
                .collect();
            Ok(sort_by_label(model, rows))
        }
        FieldSide::From => {
            let source = record_of(model, records, &rel.from, source_id).ok_or_else(|| {
                BackendError::Message(format!("no {} record {source_id}", rel.from))
            })?;
            let Some(id) = value_at(&source.values, &rel.field).and_then(|v| v.as_id()) else {
                return Ok(Vec::new());
            };
            match rel.cardinality {
                Cardinality::OneToMany => Ok(records_of(model, records, &rel.to)
                    .into_iter()
                    .filter(|row| row.id == id)
                    .collect()),
                _ => Ok(record_of(model, records, &rel.to, &id).into_iter().collect()),
            }
        }
    }
}

pub(crate) fn hierarchy_roots_of(model: &ModelDefinition, records: &[Record], entity: &str) -> Vec<Record> {
    let Some(parent_field) = model
        .entity(entity)
        .and_then(|e| e.hierarchy.as_ref())
        .map(|h| h.parent_field.clone())
    else {
        return records_of(model, records, entity);
    };
    let rows = records_of(model, records, entity)
        .into_iter()
        .filter(|row| match value_at(&row.values, &parent_field) {
            None | Some(FieldValue::Null) => true,
            Some(FieldValue::String(text)) => text.is_empty(),
            _ => false,
        })
        .collect();
    sort_by_label(model, rows)
}

pub(crate) fn hierarchy_children_of(
    model: &ModelDefinition,
    records: &[Record],
    entity: &str,
    id: &str,
) -> Vec<Record> {
    let Some(parent_field) = model
        .entity(entity)
        .and_then(|e| e.hierarchy.as_ref())
        .map(|h| h.parent_field.clone())
    else {
        return Vec::new();
    };
    let rows = records_of(model, records, entity)
        .into_iter()
        .filter(|row| {
            value_at(&row.values, &parent_field)
                .and_then(|v| v.as_id())
                .is_some_and(|parent| parent == id)
        })
        .collect();
    sort_by_label(model, rows)
}

fn sort_by_label(model: &ModelDefinition, rows: Vec<Record>) -> Vec<Record> {
    let mut keyed: Vec<(String, Record)> = rows
        .into_iter()
        .map(|row| {
            let label = label_text(model, &row);
            (label, row)
        })
        .collect();
    keyed.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.id.cmp(&right.1.id)));
    keyed.into_iter().map(|(_, row)| row).collect()
}

pub(crate) fn label_text(model: &ModelDefinition, record: &Record) -> String {
    let Some(entity) = model.entity(&record.entity) else {
        return record.id.clone();
    };
    compose_label(entity, &record.values, model).unwrap_or_else(|| record.id.clone())
}

pub fn project_children(model: &ModelDefinition, parent: &Record, child_id: &str) -> Vec<Record> {
    let Some(child) = model.entity(child_id) else { return Vec::new() };
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

/// Mock file data or a live Business 360 session. The UI holds one of these.
pub enum AppBackend {
    Mock(MockBackend),
    Live(live::LiveBackend),
}

impl AppBackend {
    pub fn is_live(&self) -> bool {
        matches!(self, AppBackend::Live(_))
    }

    pub fn reload_file(&mut self, json: &str) -> Result<(), BackendError> {
        match self {
            AppBackend::Mock(backend) => backend.reload_data(json),
            AppBackend::Live(_) => Err(BackendError::Message(
                "live records reload from IDMC, not a file".into(),
            )),
        }
    }

    pub fn restart(&mut self) -> Result<(), BackendError> {
        match self {
            AppBackend::Mock(_) => Ok(()),
            AppBackend::Live(backend) => backend.restart(),
        }
    }

    pub fn login_failure(&self) -> Option<LoginFailure> {
        match self {
            AppBackend::Mock(_) => None,
            AppBackend::Live(backend) => backend.login_failure(),
        }
    }

    pub fn login_busy(&self) -> bool {
        match self {
            AppBackend::Mock(_) => false,
            AppBackend::Live(backend) => backend.login_busy(),
        }
    }

    pub fn retry_login(&self) {
        if let AppBackend::Live(backend) = self {
            backend.retry_login();
        }
    }
}

impl Backend for AppBackend {
    fn model(&self) -> &ModelDefinition {
        match self {
            AppBackend::Mock(backend) => backend.model(),
            AppBackend::Live(backend) => backend.model(),
        }
    }
    fn records(&self, entity: &str) -> Vec<Record> {
        match self {
            AppBackend::Mock(backend) => backend.records(entity),
            AppBackend::Live(backend) => backend.records(entity),
        }
    }
    fn record(&self, entity: &str, id: &str) -> Option<Record> {
        match self {
            AppBackend::Mock(backend) => backend.record(entity, id),
            AppBackend::Live(backend) => backend.record(entity, id),
        }
    }
    fn label(&self, entity: &str, id: &str) -> String {
        match self {
            AppBackend::Mock(backend) => backend.label(entity, id),
            AppBackend::Live(backend) => backend.label(entity, id),
        }
    }
    fn related(&self, relationship: &str, source_id: &str) -> Result<Vec<Record>, BackendError> {
        match self {
            AppBackend::Mock(backend) => backend.related(relationship, source_id),
            AppBackend::Live(backend) => backend.related(relationship, source_id),
        }
    }
    fn hierarchy_roots(&self, entity: &str) -> Vec<Record> {
        match self {
            AppBackend::Mock(backend) => backend.hierarchy_roots(entity),
            AppBackend::Live(backend) => backend.hierarchy_roots(entity),
        }
    }
    fn hierarchy_children(&self, entity: &str, id: &str) -> Vec<Record> {
        match self {
            AppBackend::Mock(backend) => backend.hierarchy_children(entity, id),
            AppBackend::Live(backend) => backend.hierarchy_children(entity, id),
        }
    }
    fn prepare(&self, entity: &str, id: &str) {
        if let AppBackend::Live(backend) = self {
            backend.prepare(entity, id);
        }
    }
    fn poll(&self) -> bool {
        match self {
            AppBackend::Mock(_) => false,
            AppBackend::Live(backend) => backend.poll(),
        }
    }
    fn connection(&self) -> String {
        match self {
            AppBackend::Mock(_) => "Mock".into(),
            AppBackend::Live(backend) => backend.connection(),
        }
    }
    fn lookup_name(&self, lookup: Option<&str>, code: &str) -> Option<String> {
        match self {
            AppBackend::Mock(_) => None,
            AppBackend::Live(backend) => backend.lookup_name(lookup, code),
        }
    }
    fn lookup_catalog(&self, lookup: &str) -> Vec<(String, String)> {
        match self {
            AppBackend::Mock(_) => Vec::new(),
            AppBackend::Live(backend) => backend.lookup_catalog(lookup),
        }
    }
    fn tenant_codes(&self, lookup: &str) -> Vec<(String, String)> {
        match self {
            AppBackend::Mock(_) => Vec::new(),
            AppBackend::Live(backend) => backend.tenant_codes(lookup),
        }
    }
    fn generation(&self) -> u64 {
        match self {
            AppBackend::Mock(backend) => backend.generation(),
            AppBackend::Live(backend) => backend.generation(),
        }
    }
    fn shared_records(&self, entity: &str) -> Arc<Vec<Record>> {
        match self {
            AppBackend::Mock(backend) => backend.shared_records(entity),
            AppBackend::Live(backend) => backend.shared_records(entity),
        }
    }
    fn master_total(&self, entity: &str) -> usize {
        match self {
            AppBackend::Mock(backend) => backend.master_total(entity),
            AppBackend::Live(backend) => backend.master_total(entity),
        }
    }
    fn master_known(&self, entity: &str) -> bool {
        match self {
            AppBackend::Mock(backend) => backend.master_known(entity),
            AppBackend::Live(backend) => backend.master_known(entity),
        }
    }
    fn master_at(&self, entity: &str, index: usize) -> Option<Record> {
        match self {
            AppBackend::Mock(backend) => backend.master_at(entity, index),
            AppBackend::Live(backend) => backend.master_at(entity, index),
        }
    }
    fn master_touch(&self, entity: &str, start: usize, end: usize) {
        if let AppBackend::Live(backend) = self {
            backend.master_touch(entity, start, end);
        }
    }
    fn search_fields(&self, entity: &str) -> Option<Vec<String>> {
        match self {
            AppBackend::Mock(_) => None,
            AppBackend::Live(backend) => backend.search_fields(entity),
        }
    }
    fn set_master_view(&self, entity: &str, query: &str, sort_field: Option<&str>, descending: bool) {
        match self {
            AppBackend::Mock(backend) => backend.set_master_view(entity, query, sort_field, descending),
            AppBackend::Live(backend) => backend.set_master_view(entity, query, sort_field, descending),
        }
    }
    fn search_records(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        match self {
            AppBackend::Mock(backend) => backend.search_records(query, limit),
            AppBackend::Live(backend) => backend.search_records(query, limit),
        }
    }
    fn schedule_record_search(&self, query: &str) {
        if let AppBackend::Live(backend) = self {
            backend.schedule_record_search(query);
        }
    }
    fn take_record_search(&self) -> Option<Vec<SearchHit>> {
        match self {
            AppBackend::Mock(_) => None,
            AppBackend::Live(backend) => backend.take_record_search(),
        }
    }
    fn timing_lines(&self) -> Vec<String> {
        match self {
            AppBackend::Mock(_) => Vec::new(),
            AppBackend::Live(backend) => backend.timing_lines(),
        }
    }
    fn schedule_expand(&self, key: &str) {
        match self {
            AppBackend::Mock(backend) => backend.schedule_expand(key),
            AppBackend::Live(backend) => backend.schedule_expand(key),
        }
    }
    fn take_expand(&self) -> Option<idmc_model::Scene> {
        match self {
            AppBackend::Mock(backend) => backend.take_expand(),
            AppBackend::Live(backend) => backend.take_expand(),
        }
    }
    fn submit_write(&self, op: WriteOp) -> Result<Record, BackendError> {
        match self {
            AppBackend::Mock(backend) => backend.submit_write(op),
            AppBackend::Live(backend) => backend.submit_write(op),
        }
    }
    fn take_write_notice(&self) -> Option<WriteNotice> {
        match self {
            AppBackend::Mock(_) => None,
            AppBackend::Live(backend) => backend.take_write_notice(),
        }
    }
}

fn text_at(record: &Record, field: &str) -> String {
    match value_at(&record.values, field) {
        Some(FieldValue::String(text)) => text.clone(),
        Some(FieldValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                FieldValue::String(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    }
}

fn record_blob(record: &Record) -> String {
    let mut parts = vec![record.id.to_ascii_lowercase()];
    for (key, value) in &record.values {
        if let FieldValue::String(text) = value {
            parts.push(key.to_ascii_lowercase());
            parts.push(text.to_ascii_lowercase());
        }
    }
    parts.join(" ")
}

fn search_local(model: &ModelDefinition, records: &[Record], query: &str, limit: usize) -> Vec<SearchHit> {
    let query = query.trim().to_ascii_lowercase();
    if query.len() < 2 {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for entity in model.entities.iter().filter(|entity| entity.embedded_in.is_none()) {
        let mut found = 0usize;
        let rows = records_of(model, records, &entity.id);
        for row in rows {
            let label = label_text(model, &row);
            let detail = key_detail(entity, &row, &label);
            let blob = format!("{} {} {}", label, detail, record_blob(&row)).to_ascii_lowercase();
            if !blob.contains(&query) {
                continue;
            }
            hits.push(SearchHit {
                entity: entity.id.clone(),
                entity_label: entity.label.clone(),
                id: row.id.clone(),
                label,
                detail,
            });
            found += 1;
            if found >= limit {
                break;
            }
        }
    }
    hits
}

fn key_detail(entity: &idmc_model::EntityType, record: &Record, label: &str) -> String {
    for field in entity.leaves() {
        if field.system || entity.label_fields.iter().any(|path| path == &field.id) {
            continue;
        }
        let text = text_at(record, &field.id);
        if !text.is_empty() && text != label {
            return text;
        }
    }
    record.id.clone()
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

    #[test]
    fn mock_creates_updates_and_deletes_in_memory() {
        let backend = person();
        let mut values = backend.record("c360.person", "p-anna").unwrap().values;
        values.insert("firstName".into(), FieldValue::String("Ann".into()));
        let updated = backend
            .submit_write(WriteOp {
                kind: WriteKind::Update,
                entity: "c360.person".into(),
                id: "p-anna".into(),
                source_system: idmc_model::DEFAULT_SOURCE_SYSTEM.into(),
                source_pkey: String::new(),
                values,
                before: None,
            })
            .unwrap();
        assert_eq!(updated.values.get("firstName"), Some(&FieldValue::String("Ann".into())));
        assert_eq!(backend.label("c360.person", "p-anna"), "Ann Lindström");
        let created = backend
            .submit_write(WriteOp {
                kind: WriteKind::Create,
                entity: "c360.person".into(),
                id: "new:1".into(),
                source_system: idmc_model::DEFAULT_SOURCE_SYSTEM.into(),
                source_pkey: "pkey-1".into(),
                values: backend.record("c360.person", "p-anna").unwrap().values,
                before: None,
            })
            .unwrap();
        assert!(created.id.starts_with("local-"));
        assert!(backend.record("c360.person", &created.id).is_some());
        backend
            .submit_write(WriteOp {
                kind: WriteKind::Delete,
                entity: "c360.person".into(),
                id: created.id.clone(),
                source_system: idmc_model::DEFAULT_SOURCE_SYSTEM.into(),
                source_pkey: "pkey-1".into(),
                values: BTreeMap::new(),
                before: None,
            })
            .unwrap();
        assert!(backend.record("c360.person", &created.id).is_none());
        let missing = backend.submit_write(WriteOp {
            kind: WriteKind::Create,
            entity: "c360.person".into(),
            id: "new:2".into(),
            source_system: String::new(),
            source_pkey: "pkey-2".into(),
            values: BTreeMap::new(),
            before: None,
        });
        assert!(missing.unwrap_err().to_string().contains("sourceSystem"));
        let path = create_path("c360.person", "c360.default.system", "pkey 1");
        assert!(path.contains("sourceSystem=c360.default.system"));
        assert!(path.contains("sourcePKey=pkey%201"));
        let patch = patch_path("c360.person", "MDM1", "c360.default.system");
        assert_eq!(
            patch,
            "/business-entity/public/api/v1/entity/c360.person/MDM1?sourceSystem=c360.default.system"
        );
        let delete = delete_path("c360.person", "MDM1", "c360.default.system", Some("SFDC_566"));
        assert!(delete.contains("sourceSystem=c360.default.system"));
        assert!(delete.contains("sourcePKey=SFDC_566"));
    }
}
