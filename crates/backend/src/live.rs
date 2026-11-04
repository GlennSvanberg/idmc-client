//! Live Business 360 reads.
//!
//! Login, search, and master-record shapes follow the pages in `docs/idmc-api/`.
//! The model still comes from an export package. Credentials stay in memory and
//! are never written to the debug dump.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::io::{Write, stdout};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use idmc_model::{
    api_body, expand_key, patch_ops, FieldDef, FieldType, FieldValue, GraphEdge, IdmcExportSource,
    JsonModelSource, ModelDefinition, ModelSource, NodeId, Record, Scene,
};
use serde_json::{Map, Value};

use crate::config::env_path;
use crate::{
    apply_records, create_path, delete_path, hierarchy_children_of, hierarchy_roots_of, label_text,
    patch_path, records_of, related_of, Backend, BackendError, SearchHit, WriteKind, WriteNotice, WriteOp,
};

const PAGE: u64 = 500;
const MAX_FLIGHT: usize = 4;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(60);
const PUBLISH_EVERY: Duration = Duration::from_millis(400);

pub struct LiveConfig {
    pub login_url: String,
    pub username: String,
    pub password: String,
    pub export: Option<PathBuf>,
    pub model_json: Option<PathBuf>,
    pub values: Option<PathBuf>,
    pub debug_dump: Option<PathBuf>,
}

pub struct LiveBackend {
    model: Arc<ModelDefinition>,
    roots: Vec<String>,
    shared: Arc<Shared>,
}

struct Shared {
    http: IdmcHttp,
    cache: Mutex<Cache>,
    lists: Mutex<Lists>,
    epoch: AtomicU64,
    generation: AtomicU64,
    seen: AtomicU64,
    last_publish_ms: AtomicU64,
    last_error: Mutex<Option<String>>,
    hub: Mutex<Hub>,
    notices: Mutex<Vec<WriteNotice>>,
}

struct Hub {
    views: HashMap<String, EntityWindow>,
    started: Instant,
    logged_first: bool,
    logged_3000: bool,
    timing: Vec<String>,
    search_gen: u64,
    search_hits: Option<Vec<SearchHit>>,
    tenant: String,
    expand_inflight: BTreeSet<String>,
    expand_ready: Vec<Scene>,
}

#[derive(Clone)]
struct EntityWindow {
    query: String,
    sort_field: String,
    descending: bool,
    hits: Option<usize>,
    pages: BTreeMap<u64, Vec<Record>>,
    inflight: BTreeSet<u64>,
    revalidate: bool,
}

impl EntityWindow {
    fn new(sort_field: String) -> Self {
        Self {
            query: String::new(),
            sort_field,
            descending: false,
            hits: None,
            pages: BTreeMap::new(),
            inflight: BTreeSet::new(),
            revalidate: false,
        }
    }
}

struct Lists {
    generation: u64,
    values: HashMap<String, Arc<Vec<Record>>>,
}

struct Cache {
    records: Vec<Record>,
    index: HashMap<String, usize>,
    lookups: BTreeMap<(String, String), String>,
    hydrated: BTreeSet<String>,
    inflight: BTreeSet<String>,
    sources: HashMap<(String, String), (String, String)>,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            records: Vec::new(),
            index: HashMap::new(),
            lookups: BTreeMap::new(),
            hydrated: BTreeSet::new(),
            inflight: BTreeSet::new(),
            sources: HashMap::new(),
        }
    }
}

struct IdmcHttp {
    http: reqwest::blocking::Client,
    login_url: String,
    username: String,
    password: String,
    dump: Mutex<Option<File>>,
    session: Mutex<Option<SessionInfo>>,
    login_gate: Mutex<()>,
    trace: bool,
}

struct SessionInfo {
    session_id: String,
    mdm_base: String,
    model_version: String,
    api: String,
}

struct SearchPage {
    hits: usize,
    records: Vec<ParsedRecord>,
}

struct ParsedRecord {
    record: Record,
    lookups: Vec<(String, String, String)>,
    hydrated: bool,
}

pub fn check_login(config: &LiveConfig) -> Result<String, BackendError> {
    step("check-login: loading model");
    let model = load_model(config)?;
    let entity = model
        .roots()
        .next()
        .map(|entity| entity.id.clone())
        .unwrap_or_else(|| "c360.person".into());
    step(&format!("check-login: model {}, entity {entity}", model.name));
    let http = IdmcHttp::connect(config, true)?;
    http.login()?;
    let started = Instant::now();
    let body = http.search(&entity, 0, 1)?;
    let page = parse_search_page(&model, &entity, &body)?;
    let first = page.records.first().map(|row| row.record.id.clone());
    step(&format!(
        "search parsed in {}ms: {} hits, {} records{}",
        started.elapsed().as_millis(),
        page.hits,
        page.records.len(),
        first.as_ref().map(|id| format!(", first record {id}")).unwrap_or_default()
    ));
    let id = first
        .as_ref()
        .map(|id| format!(", first record {id}"))
        .unwrap_or_default();
    let api = http.session.lock().ok().and_then(|g| g.as_ref().map(|s| s.api.clone())).unwrap_or_else(|| "?".into());
    let mdm = http.session.lock().ok().and_then(|g| g.as_ref().map(|s| s.mdm_base.clone())).unwrap_or_default();
    Ok(format!("login ok ({api})\nmdm {mdm}\nsearch {entity}: {} hits{id}", page.hits))
}

fn step(line: &str) {
    println!("{line}");
    let _ = stdout().flush();
}

impl LiveBackend {
    pub fn open(config: LiveConfig) -> Result<Self, BackendError> {
        let model = Arc::new(load_model(&config)?);
        let roots: Vec<String> = model.roots().map(|entity| entity.id.clone()).collect();
        if roots.is_empty() {
            return Err(BackendError::Message("the model has no master entities to search".into()));
        }
        let shared = Arc::new(Shared {
            http: IdmcHttp::connect(&config, false)?,
            cache: Mutex::new(Cache::default()),
            lists: Mutex::new(Lists { generation: u64::MAX, values: HashMap::new() }),
            epoch: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            seen: AtomicU64::new(0),
            last_publish_ms: AtomicU64::new(0),
            last_error: Mutex::new(None),
            notices: Mutex::new(Vec::new()),
            hub: Mutex::new(Hub {
                views: HashMap::new(),
                started: Instant::now(),
                logged_first: false,
                logged_3000: false,
                timing: Vec::new(),
                search_gen: 0,
                search_hits: None,
                tenant: String::new(),
                expand_inflight: BTreeSet::new(),
                expand_ready: Vec::new(),
            }),
        });
        let backend = Self { model, roots, shared };
        backend.begin_load()?;
        Ok(backend)
    }

    pub fn restart(&self) -> Result<(), BackendError> {
        self.begin_load()
    }

    fn begin_load(&self) -> Result<(), BackendError> {
        let epoch = self.shared.epoch.fetch_add(1, Ordering::Relaxed) + 1;
        {
            let mut cache = lock(&self.shared.cache);
            *cache = Cache::default();
        }
        {
            let mut lists = lock(&self.shared.lists);
            lists.values.clear();
            lists.generation = u64::MAX;
        }
        if let Ok(mut err) = self.shared.last_error.lock() {
            *err = None;
        }
        self.shared.http.login()?;
        if let Some(base) = self.shared.http.mdm_base() {
            lock(&self.shared.hub).tenant = base;
        }
        self.restore_disk();
        for entity in self.roots.clone() {
            if self.shared.epoch.load(Ordering::Relaxed) != epoch {
                return Ok(());
            }
            self.master_touch(&entity, 0, 1);
        }
        let _ = epoch;
        Ok(())
    }

    /// GET one master off the UI thread and keep the body on that record.
    fn schedule_hydrate(&self, entity: &str, id: &str) {
        if self.model.embedded_children(entity).next().is_none() {
            return;
        }
        let key = format!("{entity}\0{id}");
        {
            let mut cache = lock(&self.shared.cache);
            if cache.hydrated.contains(&key) || cache.inflight.contains(&key) {
                return;
            }
            if !cache.index.contains_key(&key) {
                return;
            }
            cache.inflight.insert(key.clone());
        }
        let shared = Arc::clone(&self.shared);
        let model = Arc::clone(&self.model);
        let entity = entity.to_string();
        let id = id.to_string();
        std::thread::Builder::new()
            .name("idmc-record".into())
            .spawn(move || hydrate_one(shared, model, entity, id))
            .ok();
    }

    fn snapshot(&self) -> Vec<Record> {
        lock(&self.shared.cache).records.clone()
    }

    fn list_hit(&self, key: &str) -> Option<Arc<Vec<Record>>> {
        let generation = self.shared.generation.load(Ordering::Acquire);
        let lists = lock(&self.shared.lists);
        if lists.generation != generation {
            return None;
        }
        lists.values.get(key).cloned()
    }

    fn store_list(&self, key: &str, rows: Arc<Vec<Record>>) {
        let generation = self.shared.generation.load(Ordering::Acquire);
        let mut lists = lock(&self.shared.lists);
        if lists.generation != generation {
            lists.values.clear();
            lists.generation = generation;
        }
        lists.values.insert(key.to_string(), rows);
    }

    fn sort_field_for(&self, entity: &str) -> String {
        let field = self
            .model
            .entity(entity)
            .and_then(|entry| entry.label_fields.first().cloned())
            .or_else(|| {
                self.model
                    .entity(entity)
                    .and_then(|entry| entry.default_columns().into_iter().next())
            })
            .unwrap_or_default();
        if field.is_empty() {
            return String::new();
        }
        search_field_name(entity, &field)
    }

    pub fn master_total(&self, entity: &str) -> usize {
        let hub = lock(&self.shared.hub);
        hub.views.get(entity).and_then(|view| view.hits).unwrap_or(0)
    }

    pub fn master_known(&self, entity: &str) -> bool {
        let hub = lock(&self.shared.hub);
        hub.views.get(entity).and_then(|view| view.hits).is_some()
    }

    pub fn master_at(&self, entity: &str, index: usize) -> Option<Record> {
        let hub = lock(&self.shared.hub);
        let view = hub.views.get(entity)?;
        let offset = (index as u64 / PAGE) * PAGE;
        view.pages.get(&offset)?.get(index - offset as usize).cloned()
    }

    pub fn master_touch(&self, entity: &str, start: usize, end: usize) {
        let sort_field = self.sort_field_for(entity);
        let epoch = self.shared.epoch.load(Ordering::Relaxed);
        let mut offsets = Vec::new();
        {
            let mut hub = lock(&self.shared.hub);
            let flight = hub.views.values().map(|row| row.inflight.len()).sum::<usize>();
            let view = hub.views.entry(entity.to_string()).or_insert_with(|| EntityWindow::new(sort_field));
            let hits = view.hits.unwrap_or(usize::MAX);
            if hits == 0 {
                return;
            }
            let mut flight = flight;
            let from = start.saturating_sub(PAGE as usize);
            let to = end.saturating_add(PAGE as usize).min(hits);
            let mut offset = (from as u64 / PAGE) * PAGE;
            let last = to as u64;
            while offset < last && flight < MAX_FLIGHT {
                let cached = view.pages.contains_key(&offset);
                let refresh = offset == 0 && view.revalidate;
                if (!cached || refresh) && !view.inflight.contains(&offset) {
                    view.inflight.insert(offset);
                    offsets.push(offset);
                    flight += 1;
                }
                offset += PAGE;
            }
            if start == 0 && offsets.is_empty() && !view.pages.contains_key(&0) && !view.inflight.contains(&0) && flight < MAX_FLIGHT {
                view.inflight.insert(0);
                offsets.push(0);
            }
        }
        for offset in offsets {
            self.spawn_page(entity, offset, epoch);
        }
    }

    pub fn set_master_view(&self, entity: &str, query: &str, sort_field: Option<&str>, descending: bool) {
        let sort_field = sort_field
            .filter(|field| !field.is_empty())
            .map(|field| search_field_name(entity, field))
            .unwrap_or_else(|| self.sort_field_for(entity));
        let query = query.trim().to_string();
        let mut reset = false;
        {
            let mut hub = lock(&self.shared.hub);
            let view = hub.views.entry(entity.to_string()).or_insert_with(|| EntityWindow::new(sort_field.clone()));
            if view.query != query || view.sort_field != sort_field || view.descending != descending {
                view.query = query;
                view.sort_field = sort_field;
                view.descending = descending;
                view.pages.clear();
                view.inflight.clear();
                view.hits = None;
                view.revalidate = false;
                reset = true;
            }
        }
        if reset {
            self.restore_one(entity);
            self.master_touch(entity, 0, 1);
        }
    }

    pub fn timing_lines(&self) -> Vec<String> {
        let mut hub = lock(&self.shared.hub);
        std::mem::take(&mut hub.timing)
    }

    pub fn schedule_record_search(&self, query: &str) {
        let query = query.trim().to_string();
        let gen = {
            let mut hub = lock(&self.shared.hub);
            hub.search_gen = hub.search_gen.wrapping_add(1);
            if query.len() < 2 {
                hub.search_hits = Some(Vec::new());
                hub.search_gen
            } else {
                hub.search_gen
            }
        };
        if query.len() < 2 {
            publish(&self.shared, true);
            return;
        }
        let shared = Arc::clone(&self.shared);
        let model = Arc::clone(&self.model);
        let roots = self.roots.clone();
        std::thread::Builder::new()
            .name("idmc-find".into())
            .spawn(move || record_search(shared, model, roots, query, gen))
            .ok();
    }

    pub fn take_record_search(&self) -> Option<Vec<SearchHit>> {
        let mut hub = lock(&self.shared.hub);
        hub.search_hits.take()
    }

    pub fn schedule_expand(&self, key: &str) {
        {
            let mut hub = lock(&self.shared.hub);
            if !hub.expand_inflight.insert(key.to_string()) {
                return;
            }
        }
        let shared = Arc::clone(&self.shared);
        let model = Arc::clone(&self.model);
        let key = key.to_string();
        std::thread::Builder::new()
            .name("idmc-graph".into())
            .spawn(move || {
                let scene = expand_live(&shared, &model, &key);
                {
                    let mut hub = lock(&shared.hub);
                    hub.expand_inflight.remove(&key);
                    if !scene.nodes.is_empty() || !scene.edges.is_empty() {
                        hub.expand_ready.push(scene);
                    }
                }
                publish(&shared, true);
            })
            .ok();
    }

    pub fn take_expand(&self) -> Option<Scene> {
        let mut hub = lock(&self.shared.hub);
        if hub.expand_ready.is_empty() {
            None
        } else {
            Some(hub.expand_ready.remove(0))
        }
    }

    pub fn search_records(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        let _ = (query, limit);
        Vec::new()
    }

    fn spawn_page(&self, entity: &str, offset: u64, epoch: u64) {
        let shared = Arc::clone(&self.shared);
        let model = Arc::clone(&self.model);
        let entity = entity.to_string();
        let (query, sort_field, descending) = {
            let hub = lock(&self.shared.hub);
            match hub.views.get(&entity) {
                Some(view) => (view.query.clone(), view.sort_field.clone(), view.descending),
                None => return,
            }
        };
        std::thread::Builder::new()
            .name("idmc-page".into())
            .spawn(move || fetch_page(shared, model, entity, offset, epoch, query, sort_field, descending))
            .ok();
    }

    fn restore_disk(&self) {
        for entity in self.roots.clone() {
            self.restore_one(&entity);
        }
    }

    fn restore_one(&self, entity: &str) {
        let tenant = lock(&self.shared.hub).tenant.clone();
        if tenant.is_empty() {
            return;
        }
        let sort = self.sort_field_for(entity);
        let Some(cached) = read_disk(&tenant, entity, &sort, false, "") else { return };
        let mut hub = lock(&self.shared.hub);
        let view = hub.views.entry(entity.to_string()).or_insert_with(|| EntityWindow::new(sort));
        if view.query.is_empty() && !view.descending {
            view.hits = Some(cached.hits);
            view.pages = cached.pages;
            view.revalidate = true;
        }
    }
}

fn expand_live(shared: &Shared, model: &ModelDefinition, key: &str) -> Scene {
    if let Some(NodeId::Entity { be, id }) = NodeId::parse(key) {
        let missing = {
            let cache = lock(&shared.cache);
            !cache.records.iter().any(|row| row.entity == be && row.id == id)
        };
        if missing {
            let _ = fetch_entity(shared, model, &be, &id);
        }
    }
    let records = lock(&shared.cache).records.clone();
    let mut scene = expand_key(model, &records, key);
    if let Some(NodeId::Type { be }) = NodeId::parse(key) {
        let has_entity = scene.nodes.iter().any(|node| node.kind == idmc_model::NodeKind::Entity);
        if !has_entity {
            if let Ok(body) = shared.http.search_window(&be, "*", 0, 8, "", false) {
                if let Ok(page) = parse_search_page(model, &be, &body) {
                    for parsed in page.records {
                        scene.merge(idmc_model::build_entity_scene(&parsed.record, model));
                    }
                }
            }
        }
        if let Some(base) = shared.http.mdm_base() {
            let url = format!("{base}/metadata/api/v2/objects/{}", encode_segment(&be));
            if let Ok(body) = shared.http.get_json(&url) {
                scene.edges.extend(metadata_edges(&be, &body));
            }
        }
    }
    scene
}

fn fetch_entity(shared: &Shared, model: &ModelDefinition, entity: &str, id: &str) -> Result<(), BackendError> {
    let base = shared.http.mdm_base().ok_or_else(|| BackendError::Message("not logged in".into()))?;
    let url = format!(
        "{base}/business-entity/public/api/v1/entity/{}/{}",
        encode_segment(entity),
        encode_segment(id)
    );
    let body = shared.http.get_json(&url)?;
    let parsed = parse_entity_record(model, entity, &body)?;
    let mut cache = lock(&shared.cache);
    remember(&mut cache, parsed);
    Ok(())
}

fn metadata_edges(entity: &str, body: &Value) -> Vec<GraphEdge> {
    let mut edges = Vec::new();
    for name in ["relationships", "links", "businessEntityRelationships"] {
        let Some(rows) = body.get(name).and_then(|value| value.as_array()) else { continue };
        for row in rows {
            let label = row.get("label").or_else(|| row.get("name")).and_then(|value| value.as_str()).unwrap_or("related");
            let Some(target) = row.get("to").or_else(|| row.get("target")).or_else(|| row.get("businessEntity")).and_then(|value| value.as_str()) else { continue };
            if target.is_empty() {
                continue;
            }
            let source = NodeId::Type { be: entity.to_string() }.key();
            let target_key = NodeId::Type { be: target.to_string() }.key();
            edges.push(GraphEdge {
                id: format!("{source}|{label}|{target_key}"),
                source,
                target: target_key,
                label: label.to_string(),
            });
        }
    }
    edges
}

fn hydrate_one(shared: Arc<Shared>, model: Arc<ModelDefinition>, entity: String, id: String) {
    let key = format!("{entity}\0{id}");
    let outcome = (|| -> Result<ParsedRecord, BackendError> {
        let base = shared.http.mdm_base().ok_or_else(|| BackendError::Message("not logged in".into()))?;
        let url = format!(
            "{base}/business-entity/public/api/v1/entity/{}/{}",
            encode_segment(&entity),
            encode_segment(&id)
        );
        let body = shared.http.get_json(&url)?;
        parse_entity_record(&model, &entity, &body)
    })();
    let mut cache = lock(&shared.cache);
    cache.inflight.remove(&key);
    match outcome {
        Ok(parsed) => {
            remember(&mut cache, parsed);
            cache.hydrated.insert(key);
            drop(cache);
            publish(&shared, true);
        }
        Err(err) => {
            cache.hydrated.insert(key);
            drop(cache);
            if let Ok(mut slot) = shared.last_error.lock() {
                *slot = Some(public_error(&err.to_string()));
            }
            publish(&shared, true);
        }
    }
}

fn publish(shared: &Shared, force: bool) {
    let now = now_ms();
    let last = shared.last_publish_ms.load(Ordering::Relaxed);
    if !force && now.saturating_sub(last) < PUBLISH_EVERY.as_millis() as u64 {
        return;
    }
    shared.last_publish_ms.store(now, Ordering::Relaxed);
    shared.generation.fetch_add(1, Ordering::Release);
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

impl Drop for LiveBackend {
    fn drop(&mut self) {
        self.shared.epoch.fetch_add(1, Ordering::Relaxed);
    }
}

impl Backend for LiveBackend {
    fn model(&self) -> &ModelDefinition { &self.model }

    fn records(&self, entity: &str) -> Vec<Record> {
        self.shared_records(entity).as_ref().clone()
    }

    fn shared_records(&self, entity: &str) -> Arc<Vec<Record>> {
        let key = format!("records\0{entity}");
        if let Some(hit) = self.list_hit(&key) {
            return hit;
        }
        let generation = self.shared.generation.load(Ordering::Acquire);
        let snapshot = self.snapshot();
        let rows = Arc::new(records_of(&self.model, &snapshot, entity));
        if self.shared.generation.load(Ordering::Acquire) == generation {
            self.store_list(&key, Arc::clone(&rows));
        }
        rows
    }

    fn record(&self, entity: &str, id: &str) -> Option<Record> {
        if self.model.entity(entity).and_then(|entry| entry.embedded_in.as_ref()).is_some() {
            return self.shared_records(entity).iter().find(|row| row.id == id).cloned();
        }
        let cache = lock(&self.shared.cache);
        let pos = *cache.index.get(&index_key(entity, id))?;
        Some(cache.records[pos].clone())
    }

    fn label(&self, entity: &str, id: &str) -> String {
        self.record(entity, id)
            .map(|record| label_text(&self.model, &record))
            .unwrap_or_else(|| id.to_string())
    }

    fn related(&self, relationship: &str, source_id: &str) -> Result<Vec<Record>, BackendError> {
        let key = format!("rel\0{relationship}\0{source_id}");
        if let Some(hit) = self.list_hit(&key) {
            return Ok(hit.as_ref().clone());
        }
        let rows = if let Some(rel) = self.model.relationship(relationship) {
            if rel.embedded {
                self.schedule_hydrate(&rel.from, source_id);
                let parent = self.record(&rel.from, source_id).ok_or_else(|| {
                    BackendError::Message(format!("no {} record {source_id}", rel.from))
                })?;
                related_of(&self.model, std::slice::from_ref(&parent), relationship, source_id)?
            } else {
                let snapshot = self.snapshot();
                related_of(&self.model, &snapshot, relationship, source_id)?
            }
        } else {
            return Err(BackendError::Message(format!("unknown relationship {relationship}")));
        };
        let shared = Arc::new(rows);
        self.store_list(&key, Arc::clone(&shared));
        Ok(shared.as_ref().clone())
    }

    fn hierarchy_roots(&self, entity: &str) -> Vec<Record> {
        let key = format!("roots\0{entity}");
        if let Some(hit) = self.list_hit(&key) {
            return hit.as_ref().clone();
        }
        let snapshot = self.snapshot();
        let rows = hierarchy_roots_of(&self.model, &snapshot, entity);
        self.store_list(&key, Arc::new(rows.clone()));
        rows
    }

    fn hierarchy_children(&self, entity: &str, id: &str) -> Vec<Record> {
        let key = format!("kids\0{entity}\0{id}");
        if let Some(hit) = self.list_hit(&key) {
            return hit.as_ref().clone();
        }
        let snapshot = self.snapshot();
        let rows = hierarchy_children_of(&self.model, &snapshot, entity, id);
        self.store_list(&key, Arc::new(rows.clone()));
        rows
    }

    fn prepare(&self, entity: &str, id: &str) { self.schedule_hydrate(entity, id); }

    fn generation(&self) -> u64 { self.shared.generation.load(Ordering::Acquire) }

    fn poll(&self) -> bool {
        let generation = self.shared.generation.load(Ordering::Relaxed);
        let seen = self.shared.seen.load(Ordering::Relaxed);
        if generation != seen {
            self.shared.seen.store(generation, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    fn connection(&self) -> String {
        let err = self.shared.last_error.lock().ok().and_then(|guard| guard.clone());
        if let Some(err) = err {
            return format!("Live · {err}");
        }
        let hub = lock(&self.shared.hub);
        let hits: usize = hub.views.values().filter_map(|view| view.hits).sum();
        let known = hub.views.values().any(|view| view.hits.is_some());
        let refreshing = hub.views.values().any(|view| view.revalidate);
        if !known {
            return "Live · loading".into();
        }
        if refreshing {
            format!("Live · {hits} cached")
        } else {
            format!("Live · {hits}")
        }
    }

    fn lookup_name(&self, lookup: Option<&str>, code: &str) -> Option<String> {
        let lookup = lookup?;
        let cache = lock(&self.shared.cache);
        cache.lookups.get(&(lookup.to_string(), code.to_string())).cloned()
    }

    fn submit_write(&self, mut op: WriteOp) -> Result<Record, BackendError> {
        if op.source_pkey.is_empty() {
            let cache = lock(&self.shared.cache);
            if let Some((_, pkey)) = cache.sources.get(&(op.entity.clone(), op.id.clone())) {
                op.source_pkey = pkey.clone();
            }
        }
        let before = {
            let cache = lock(&self.shared.cache);
            cache.records.iter().find(|row| row.entity == op.entity && row.id == op.id).cloned()
        };
        let saved = {
            let mut cache = lock(&self.shared.cache);
            let mut records = cache.records.clone();
            let saved = apply_records(&self.model, &mut records, &op)?;
            if op.kind == WriteKind::Create {
                cache.sources.insert(
                    (saved.entity.clone(), saved.id.clone()),
                    (op.source_system.clone(), op.source_pkey.clone()),
                );
            }
            if op.kind == WriteKind::Delete {
                cache.sources.remove(&(op.entity.clone(), op.id.clone()));
            }
            set_cache_records(&mut cache, records);
            saved
        };
        {
            let mut hub = lock(&self.shared.hub);
            match op.kind {
                WriteKind::Create => insert_in_pages(&mut hub, &saved),
                WriteKind::Update => replace_in_pages(&mut hub, &saved),
                WriteKind::Delete => remove_from_pages(&mut hub, &op.entity, &op.id),
            }
        }
        publish(&self.shared, true);
        let shared = Arc::clone(&self.shared);
        let model = Arc::clone(&self.model);
        let optimistic = saved.clone();
        std::thread::Builder::new()
            .name("idmc-write".into())
            .spawn(move || finish_write(shared, model, op, before, optimistic))
            .map_err(|err| BackendError::Message(format!("write thread: {err}")))?;
        Ok(saved)
    }

    fn take_write_notice(&self) -> Option<WriteNotice> {
        let mut notices = self.shared.notices.lock().unwrap_or_else(|err| err.into_inner());
        if notices.is_empty() { None } else { Some(notices.remove(0)) }
    }
}

fn set_cache_records(cache: &mut Cache, records: Vec<Record>) {
    cache.index.clear();
    for (index, record) in records.iter().enumerate() {
        cache.index.insert(index_key(&record.entity, &record.id), index);
    }
    cache.records = records;
}

fn replace_in_pages(hub: &mut Hub, record: &Record) {
    let Some(view) = hub.views.get_mut(&record.entity) else { return };
    for page in view.pages.values_mut() {
        if let Some(slot) = page.iter_mut().find(|row| row.id == record.id) {
            *slot = record.clone();
        }
    }
}

fn remove_from_pages(hub: &mut Hub, entity: &str, id: &str) {
    let Some(view) = hub.views.get_mut(entity) else { return };
    let mut removed = false;
    for page in view.pages.values_mut() {
        let before = page.len();
        page.retain(|row| row.id != id);
        if page.len() != before {
            removed = true;
        }
    }
    if removed {
        if let Some(hits) = view.hits.as_mut() {
            *hits = hits.saturating_sub(1);
        }
    }
}

fn insert_in_pages(hub: &mut Hub, record: &Record) {
    let Some(view) = hub.views.get_mut(&record.entity) else { return };
    if let Some(page) = view.pages.get_mut(&0) {
        page.insert(0, record.clone());
    }
    if let Some(hits) = view.hits.as_mut() {
        *hits += 1;
    }
}

fn finish_write(shared: Arc<Shared>, model: Arc<ModelDefinition>, op: WriteOp, before: Option<Record>, optimistic: Record) {
    let Some(base) = shared.http.mdm_base() else {
        fail_write(&shared, &op, before.as_ref(), &optimistic, "not logged in");
        return;
    };
    let result = match op.kind {
        WriteKind::Create => {
            let url = format!("{base}{}", create_path(&op.entity, &op.source_system, &op.source_pkey));
            shared.http.note(&format!("create POST {url}"));
            shared.http.post_json(&url, &api_body(&model, &op.entity, &op.values))
        }
        WriteKind::Update => {
            let url = format!("{base}{}", patch_path(&op.entity, &op.id, &op.source_system));
            let previous = before.as_ref().map(|row| row.values.clone()).unwrap_or_default();
            shared.http.note(&format!("update PATCH {url}"));
            shared.http.send("PATCH", &url, Some(&patch_ops(&model, &op.entity, &previous, &op.values)))
        }
        WriteKind::Delete => {
            let pkey = if op.source_pkey.is_empty() { None } else { Some(op.source_pkey.as_str()) };
            let url = format!("{base}{}", delete_path(&op.entity, &op.id, &op.source_system, pkey));
            shared.http.note(&format!("delete DELETE {url}"));
            shared.http.send("DELETE", &url, None)
        }
    };
    match result {
        Ok(value) => {
            let assigned = value
                .get("businessId")
                .or_else(|| value.get("id"))
                .and_then(|item| item.as_str())
                .filter(|id| *id != optimistic.id)
                .map(str::to_string);
            if let Some(id) = &assigned {
                rename_record(&shared, &optimistic, id);
            }
            let message = value.get("approvalRequired").and_then(|flag| flag.as_bool()).unwrap_or(false).then(|| {
                "Saved. A data steward must approve this change.".to_string()
            });
            push_notice(
                &shared,
                WriteNotice {
                    error: None,
                    message,
                    entity: optimistic.entity,
                    from_id: optimistic.id,
                    to_id: assigned,
                    restore: None,
                },
            );
        }
        Err(err) => fail_write(&shared, &op, before.as_ref(), &optimistic, &err.to_string()),
    }
}

fn fail_write(shared: &Shared, op: &WriteOp, before: Option<&Record>, optimistic: &Record, message: &str) {
    let mut rolled = false;
    {
        let mut cache = lock(&shared.cache);
        let pos = cache.records.iter().position(|row| row.entity == optimistic.entity && row.id == optimistic.id);
        let current = pos.and_then(|index| cache.records.get(index));
        if current.is_some_and(|row| row.values == optimistic.values) {
            let mut records = cache.records.clone();
            if let Some(index) = pos {
                if let Some(previous) = before {
                    records[index] = previous.clone();
                } else {
                    records.remove(index);
                }
                set_cache_records(&mut cache, records);
                rolled = true;
            }
        }
    }
    if rolled {
        let mut hub = lock(&shared.hub);
        match op.kind {
            WriteKind::Create => remove_from_pages(&mut hub, &optimistic.entity, &optimistic.id),
            WriteKind::Update => {
                if let Some(previous) = before {
                    replace_in_pages(&mut hub, previous);
                }
            }
            WriteKind::Delete => {
                if let Some(previous) = before {
                    insert_in_pages(&mut hub, previous);
                }
            }
        }
    }
    push_notice(
        shared,
        WriteNotice {
            error: Some(message.to_string()),
            message: None,
            entity: optimistic.entity.clone(),
            from_id: optimistic.id.clone(),
            to_id: None,
            restore: rolled.then(|| optimistic.clone()),
        },
    );
    publish(shared, true);
}

fn rename_record(shared: &Shared, optimistic: &Record, assigned: &str) {
    {
        let mut cache = lock(&shared.cache);
        let pos = cache.records.iter().position(|row| row.entity == optimistic.entity && row.id == optimistic.id);
        let same = pos.and_then(|index| cache.records.get(index)).is_some_and(|row| row.values == optimistic.values);
        if same {
            let mut records = cache.records.clone();
            if let Some(index) = pos {
                records[index].id = assigned.to_string();
                if let Some(source) = cache.sources.remove(&(optimistic.entity.clone(), optimistic.id.clone())) {
                    cache.sources.insert((optimistic.entity.clone(), assigned.to_string()), source);
                }
                set_cache_records(&mut cache, records);
            }
        } else {
            return;
        }
    }
    let mut hub = lock(&shared.hub);
    if let Some(view) = hub.views.get_mut(&optimistic.entity) {
        for page in view.pages.values_mut() {
            if let Some(slot) = page.iter_mut().find(|row| row.id == optimistic.id && row.values == optimistic.values) {
                slot.id = assigned.to_string();
            }
        }
    }
    publish(shared, true);
}

fn push_notice(shared: &Shared, notice: WriteNotice) {
    if let Ok(mut notices) = shared.notices.lock() {
        notices.push(notice);
    }
}

fn fetch_page(
    shared: Arc<Shared>,
    model: Arc<ModelDefinition>,
    entity: String,
    offset: u64,
    epoch: u64,
    query: String,
    sort_field: String,
    descending: bool,
) {
    if shared.epoch.load(Ordering::Relaxed) != epoch {
        let mut hub = lock(&shared.hub);
        if let Some(view) = hub.views.get_mut(&entity) {
            view.inflight.remove(&offset);
        }
        return;
    }
    let result = shared.http.search_window(&entity, &query, offset, PAGE, &sort_field, descending);
    match result {
        Ok(body) => {
            let page = match parse_search_page(&model, &entity, &body) {
                Ok(page) => page,
                Err(err) => {
                    fail_page(&shared, &entity, offset, &err.to_string());
                    return;
                }
            };
            let hits = page.hits;
            let mut rows = Vec::new();
            {
                let mut cache = lock(&shared.cache);
                for parsed in page.records {
                    rows.push(parsed.record.clone());
                    remember(&mut cache, parsed);
                }
            }
            let shown = rows.len();
            let covers_3000 = {
                let mut hub = lock(&shared.hub);
                let Some(view) = hub.views.get_mut(&entity) else { return };
                view.inflight.remove(&offset);
                view.revalidate = false;
                view.hits = Some(hits);
                view.pages.insert(offset, rows);
                hits > 3000
                    && view.pages.iter().any(|(page_offset, page_rows)| {
                        let start = *page_offset as usize;
                        start <= 3000 && 3000 < start + page_rows.len()
                    })
            };
            {
                let mut hub = lock(&shared.hub);
                let elapsed = hub.started.elapsed().as_millis();
                if offset == 0 && !hub.logged_first {
                    hub.logged_first = true;
                    let line = format!("timing: first rows {elapsed}ms ({shown} of {hits})");
                    step(&line);
                    hub.timing.push(line);
                }
                if covers_3000 && !hub.logged_3000 {
                    hub.logged_3000 = true;
                    let line = format!("timing: row 3000 ready {elapsed}ms");
                    step(&line);
                    hub.timing.push(line);
                }
            }
            let tenant = lock(&shared.hub).tenant.clone();
            if !tenant.is_empty() {
                let pages = lock(&shared.hub).views.get(&entity).map(|view| view.pages.clone()).unwrap_or_default();
                write_disk(&tenant, &entity, &sort_field, descending, &query, hits, &pages);
            }
            publish(&shared, true);
        }
        Err(err) => fail_page(&shared, &entity, offset, &err.to_string()),
    }
}

fn fail_page(shared: &Shared, entity: &str, offset: u64, err: &str) {
    if let Ok(mut slot) = shared.last_error.lock() {
        *slot = Some(public_error(err));
    }
    let mut hub = lock(&shared.hub);
    if let Some(view) = hub.views.get_mut(entity) {
        view.inflight.remove(&offset);
    }
    drop(hub);
    publish(shared, true);
}

fn record_search(shared: Arc<Shared>, model: Arc<ModelDefinition>, roots: Vec<String>, query: String, gen: u64) {
    std::thread::sleep(Duration::from_millis(150));
    if lock(&shared.hub).search_gen != gen {
        return;
    }
    let mut hits = Vec::new();
    for entity in roots {
        if lock(&shared.hub).search_gen != gen {
            return;
        }
        let Ok(body) = shared.http.search_window(&entity, &query, 0, 8, "", false) else { continue };
        let Ok(page) = parse_search_page(&model, &entity, &body) else { continue };
        let entity_label = model.entity(&entity).map(|entry| entry.label.clone()).unwrap_or_else(|| entity.clone());
        for parsed in page.records {
            let label = label_text(&model, &parsed.record);
            let detail = parsed
                .record
                .values
                .values()
                .find_map(|value| match value {
                    idmc_model::FieldValue::String(text) if !text.is_empty() && text != &label => Some(text.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| parsed.record.id.clone());
            hits.push(SearchHit {
                entity: entity.clone(),
                entity_label: entity_label.clone(),
                id: parsed.record.id.clone(),
                label,
                detail,
            });
        }
    }
    if lock(&shared.hub).search_gen != gen {
        return;
    }
    lock(&shared.hub).search_hits = Some(hits);
    publish(&shared, true);
}

struct LoadedDisk {
    hits: usize,
    pages: BTreeMap<u64, Vec<Record>>,
}

fn cache_root() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("IDMC_CACHE") {
        return std::path::PathBuf::from(dir);
    }
    if let Ok(dir) = std::env::var("XDG_CACHE_HOME") {
        return std::path::PathBuf::from(dir).join("idmc-client");
    }
    if let Ok(home) = std::env::var("HOME") {
        return std::path::PathBuf::from(home).join(".cache").join("idmc-client");
    }
    if let Ok(home) = std::env::var("USERPROFILE") {
        return std::path::PathBuf::from(home).join(".cache").join("idmc-client");
    }
    std::path::PathBuf::from(".idmc-cache")
}

fn cache_path(tenant: &str, entity: &str, sort: &str, descending: bool, query: &str) -> std::path::PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    tenant.hash(&mut hasher);
    entity.hash(&mut hasher);
    sort.hash(&mut hasher);
    descending.hash(&mut hasher);
    query.hash(&mut hasher);
    let name = entity.replace(['/', '\\', ':'], "_");
    cache_root().join(format!("{:x}", hasher.finish())).join(format!("{name}.json"))
}

fn read_disk(tenant: &str, entity: &str, sort: &str, descending: bool, query: &str) -> Option<LoadedDisk> {
    let text = std::fs::read_to_string(cache_path(tenant, entity, sort, descending, query)).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let hits = value.get("hits")?.as_u64()? as usize;
    let mut pages = BTreeMap::new();
    for page in value.get("pages")?.as_array()? {
        let offset = page.get("offset")?.as_u64()?;
        let rows: Vec<Record> = serde_json::from_value(page.get("rows")?.clone()).ok()?;
        pages.insert(offset, rows);
    }
    Some(LoadedDisk { hits, pages })
}

fn write_disk(tenant: &str, entity: &str, sort: &str, descending: bool, query: &str, hits: usize, pages: &BTreeMap<u64, Vec<Record>>) {
    let path = cache_path(tenant, entity, sort, descending, query);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut stored = Vec::new();
    for (offset, rows) in pages.iter().take(20) {
        stored.push(serde_json::json!({"offset": offset, "rows": rows}));
    }
    let text = serde_json::json!({"hits": hits, "pages": stored}).to_string();
    let _ = std::fs::write(path, text);
}

fn search_field_name(entity: &str, field: &str) -> String {
    format!("{}.{}", entity.replace('.', ""), field)
}

fn search_clause(entity: &str, query: &str) -> (String, Option<Value>) {
    let query = query.trim();
    if query.is_empty() || query == "*" {
        return ("*".into(), None);
    }
    if let Some((field, value)) = query.split_once('=') {
        let field = field.trim();
        let value = value.trim();
        if !field.is_empty() && !field.contains(' ') {
            let comparator = if value.is_empty() { "IS_MISSING" } else { "CONTAINS" };
            let mut filter = serde_json::json!({
                "comparator": comparator,
                "fieldName": search_field_name(entity, field),
            });
            if !value.is_empty() {
                filter["fieldValue"] = Value::String(value.to_string());
            }
            return ("*".into(), Some(serde_json::json!({"filter": [filter]})));
        }
    }
    (query.to_string(), None)
}

fn remember(cache: &mut Cache, parsed: ParsedRecord) {
    let key = index_key(&parsed.record.entity, &parsed.record.id);
    if parsed.hydrated {
        cache.hydrated.insert(key.clone());
    }
    for (lookup, code, name) in parsed.lookups {
        cache.lookups.insert((lookup, code), name);
    }
    if let Some(&pos) = cache.index.get(&key) {
        cache.records[pos] = parsed.record;
    } else {
        cache.index.insert(key, cache.records.len());
        cache.records.push(parsed.record);
    }
}

fn index_key(entity: &str, id: &str) -> String {
    format!("{entity}\0{id}")
}

fn load_model(config: &LiveConfig) -> Result<ModelDefinition, BackendError> {
    let mut model = if let Some(path) = &config.model_json {
        JsonModelSource::open(path)?.load()?
    } else if let Some(path) = &config.export {
        IdmcExportSource::open(path)?.load()?
    } else if let Some(path) = env_path("IDMC_MODEL_EXPORT") {
        IdmcExportSource::open(&path)?.load()?
    } else {
        let mock = crate::find_mock_dir().ok_or_else(|| {
            BackendError::Message("could not find mock/idmc-export/person".into())
        })?;
        IdmcExportSource::open(&mock.join("idmc-export/person"))?.load()?
    };
    if let Some(path) = &config.values {
        let text = std::fs::read_to_string(path)?;
        model.apply_lookup_values(&text)?;
    }
    Ok(model)
}

impl IdmcHttp {
    fn connect(config: &LiveConfig, trace: bool) -> Result<Self, BackendError> {
        let http = reqwest::blocking::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(READ_TIMEOUT)
            .pool_max_idle_per_host(0)
            .build()
            .map_err(|err| BackendError::Message(format!("http client: {err}")))?;
        let dump = match &config.debug_dump {
            Some(path) => Some(File::create(path).map_err(|err| {
                BackendError::Message(format!("debug dump {}: {err}", path.display()))
            })?),
            None => None,
        };
        Ok(Self {
            http,
            login_url: config.login_url.trim().trim_end_matches('/').to_string(),
            username: config.username.clone(),
            password: config.password.clone(),
            dump: Mutex::new(dump),
            session: Mutex::new(None),
            login_gate: Mutex::new(()),
            trace,
        })
    }

    fn note(&self, line: &str) {
        if self.trace {
            step(line);
        }
    }

    fn login(&self) -> Result<(), BackendError> {
        let _gate = self.login_gate.lock().unwrap_or_else(|err| err.into_inner());
        let (v3, v2) = login_endpoints(&self.login_url);
        let body = serde_json::json!({
            "username": self.username,
            "password": self.password,
        });
        self.note(&format!("login v3 POST {v3}"));
        let started = Instant::now();
        let v3_result = self.post_once(&v3, &body, false);
        let session = match v3_result {
            Ok((status, value, raw)) => {
                self.note(&format!("login v3 HTTP {status} in {}ms", started.elapsed().as_millis()));
                if status == 200 {
                    match parse_v3(&value) {
                        Ok((session_id, base)) => {
                            let mdm = mdm_base_from_api_url(&base)?;
                            self.note(&format!("baseApiUrl {base}"));
                            self.note(&format!("mdm {mdm}"));
                            SessionInfo {
                                session_id,
                                mdm_base: mdm,
                                model_version: model_version_now(),
                                api: "v3".into(),
                            }
                        }
                        Err(err) => {
                            self.note(&format!("login v3 response was not a session ({err})"));
                            self.login_v2(&v2, &body)?
                        }
                    }
                } else if status == 401 || status == 403 {
                    let msg = format!("login failed: {}", failure_line(status, &raw));
                    self.note(&msg);
                    return Err(BackendError::Message(msg));
                } else {
                    let line = failure_line(status, &raw);
                    self.note(&format!("login v3 {line}"));
                    match self.login_v2(&v2, &body) {
                        Ok(session) => session,
                        Err(err) => {
                            if matches!(status, 404 | 405 | 501) {
                                return Err(err);
                            }
                            return Err(BackendError::Message(format!("login failed: {line}")));
                        }
                    }
                }
            }
            Err(err) => {
                let msg = format!(
                    "login v3 failed in {}ms: {}",
                    started.elapsed().as_millis(),
                    public_error(&err.to_string())
                );
                self.note(&msg);
                return Err(BackendError::Message(msg));
            }
        };
        *self.session.lock().unwrap_or_else(|err| err.into_inner()) = Some(session);
        Ok(())
    }

    fn login_v2(&self, url: &str, body: &Value) -> Result<SessionInfo, BackendError> {
        self.note(&format!("login v2 POST {url}"));
        let started = Instant::now();
        let (status, value, raw) = self.post_once(url, body, false)?;
        self.note(&format!("login v2 HTTP {status} in {}ms", started.elapsed().as_millis()));
        if status == 401 || status == 403 {
            let msg = format!("login failed: {}", failure_line(status, &raw));
            self.note(&msg);
            return Err(BackendError::Message(msg));
        }
        if status != 200 {
            let msg = format!("login failed: v2 {}", failure_line(status, &raw));
            self.note(&msg);
            return Err(BackendError::Message(msg));
        }
        let (session_id, base) = parse_v2(&value).map_err(|err| BackendError::Message(err))?;
        let mdm = mdm_base_from_api_url(&base)?;
        self.note(&format!("baseApiUrl {base}"));
        self.note(&format!("mdm {mdm}"));
        Ok(SessionInfo {
            session_id,
            mdm_base: mdm,
            model_version: model_version_now(),
            api: "v2".into(),
        })
    }

    fn search(&self, entity: &str, record_offset: u64, page: u64) -> Result<Value, BackendError> {
        let base = self.mdm_base().ok_or_else(|| BackendError::Message("not logged in".into()))?;
        let url = format!("{base}/search/public/api/v1/search");
        self.note(&format!("search POST {url} entity {entity} offset {record_offset}"));
        let body = serde_json::json!({
            "entityType": entity,
            "search": "*",
            "pageSize": page,
            "pageOffset": 0,
            "recordOffset": record_offset,
            "recordsToReturn": page,
        });
        self.post_json(&url, &body)
    }

    fn search_window(
        &self,
        entity: &str,
        query: &str,
        record_offset: u64,
        page: u64,
        sort_field: &str,
        descending: bool,
    ) -> Result<Value, BackendError> {
        let base = self.mdm_base().ok_or_else(|| BackendError::Message("not logged in".into()))?;
        let url = format!("{base}/search/public/api/v1/search");
        let (search, filters) = search_clause(entity, query);
        self.note(&format!("search POST {url} entity {entity} offset {record_offset} page {page}"));
        let mut body = serde_json::json!({
            "entityType": entity,
            "search": search,
            "pageSize": page,
            "pageOffset": 0,
            "recordOffset": record_offset,
            "recordsToReturn": page,
        });
        if !sort_field.is_empty() {
            body["sort"] = serde_json::json!([{
                "fieldName": sort_field,
                "order": if descending { "DESCENDING" } else { "ASCENDING" },
            }]);
        }
        if let Some(filters) = filters {
            body["filters"] = filters;
        }
        self.post_json(&url, &body)
    }

    fn mdm_base(&self) -> Option<String> {
        self.session.lock().ok().and_then(|guard| guard.as_ref().map(|s| s.mdm_base.clone()))
    }

    fn post_json(&self, url: &str, body: &Value) -> Result<Value, BackendError> {
        self.send("POST", url, Some(body))
    }

    fn get_json(&self, url: &str) -> Result<Value, BackendError> {
        self.send("GET", url, None)
    }

    fn send(&self, method: &str, url: &str, body: Option<&Value>) -> Result<Value, BackendError> {
        let started = Instant::now();
        let mut relogin = false;
        let mut attempt = 0u32;
        loop {
            let response = self.dispatch(method, url, body)?;
            let status = response.status().as_u16();
            if status == 401 && !relogin {
                self.note(&format!("{method} {url} HTTP 401 in {}ms, logging in again", started.elapsed().as_millis()));
                self.login()?;
                relogin = true;
                continue;
            }
            if status == 429 && attempt < 5 {
                let delay = retry_delay(attempt, response.headers().get("retry-after").and_then(|v| v.to_str().ok()));
                self.note(&format!("{method} {url} HTTP 429, waiting {}ms", delay.as_millis()));
                attempt += 1;
                std::thread::sleep(delay);
                continue;
            }
            let text = response.text().map_err(|err| BackendError::Message(format!("http: {err}")))?;
            self.dump_response(method, url, status, &text);
            self.note(&format!("{method} {url} HTTP {status} in {}ms", started.elapsed().as_millis()));
            if status == 401 {
                let msg = failure_line(status, &text);
                self.note(&msg);
                return Err(BackendError::Message(msg));
            }
            if !(200..300).contains(&status) {
                let msg = failure_line(status, &text);
                self.note(&msg);
                return Err(BackendError::Message(msg));
            }
            if text.trim().is_empty() {
                return Ok(Value::Null);
            }
            return serde_json::from_str(&text)
                .map_err(|_| BackendError::Message("response was not JSON".into()));
        }
    }

    fn dispatch(
        &self,
        method: &str,
        url: &str,
        body: Option<&Value>,
    ) -> Result<reqwest::blocking::Response, BackendError> {
        let session = self.session.lock().unwrap_or_else(|err| err.into_inner());
        let info = session.as_ref().ok_or_else(|| BackendError::Message("not logged in".into()))?;
        let mut request = match method {
            "POST" => self.http.post(url),
            "PUT" => self.http.put(url),
            "PATCH" => self.http.patch(url),
            "DELETE" => self.http.delete(url),
            _ => self.http.get(url),
        };
        request = request
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("IDS-SESSION-ID", &info.session_id)
            .header("MODEL_VERSION", &info.model_version)
            .header("Accept-Language", "en");
        if let Some(body) = body {
            request = request.json(body);
        }
        request.send().map_err(|err| BackendError::Message(format!("http: {err}")))
    }

    fn post_once(&self, url: &str, body: &Value, authed: bool) -> Result<(u16, Value, String), BackendError> {
        let mut attempt = 0u32;
        loop {
            let mut request = self
                .http
                .post(url)
                .header("Accept", "application/json")
                .header("Content-Type", "application/json")
                .json(body);
            if authed {
                if let Some(info) = self.session.lock().ok().and_then(|g| g.as_ref().map(|s| (s.session_id.clone(), s.model_version.clone()))) {
                    request = request
                        .header("IDS-SESSION-ID", info.0)
                        .header("MODEL_VERSION", info.1);
                }
            }
            let response = request.send().map_err(|err| BackendError::Message(format!("http: {err}")))?;
            let status = response.status().as_u16();
            if status == 429 && attempt < 5 {
                let delay = retry_delay(attempt, response.headers().get("retry-after").and_then(|v| v.to_str().ok()));
                attempt += 1;
                std::thread::sleep(delay);
                continue;
            }
            let text = response.text().unwrap_or_default();
            self.dump_response("POST", url, status, &text);
            let value = serde_json::from_str(&text).unwrap_or(Value::Null);
            return Ok((status, value, text));
        }
    }

    fn dump_response(&self, method: &str, url: &str, status: u16, body: &str) {
        let Ok(mut slot) = self.dump.lock() else { return };
        let Some(file) = slot.as_mut() else { return };
        let rendered = match serde_json::from_str::<Value>(body) {
            Ok(mut value) => {
                redact_json(&mut value);
                serde_json::to_string_pretty(&value).unwrap_or_else(|_| "[unreadable json]".into())
            }
            Err(_) => format!("[non-json body, {} bytes]", body.len()),
        };
        let _ = writeln!(file, "{method} {url}\nstatus {status}\n{rendered}\n");
    }
}

pub fn mdm_base_from_api_url(url: &str) -> Result<String, BackendError> {
    let url = url.trim().trim_end_matches('/');
    let url = url.strip_suffix("/saas").unwrap_or(url);
    let Some((scheme, rest)) = url.split_once("://") else {
        return Err(BackendError::Message(format!("base URL has no scheme: {url}")));
    };
    let hostport = rest.split('/').next().unwrap_or(rest);
    if hostport.is_empty() {
        return Err(BackendError::Message("base URL has no host".into()));
    }
    let (host, port) = split_host_port(hostport);
    let rewritten = rewrite_pod(host);
    let mut out = format!("{scheme}://{rewritten}");
    if let Some(port) = port {
        out.push(':');
        out.push_str(port);
    }
    Ok(out)
}

fn split_host_port(hostport: &str) -> (&str, Option<&str>) {
    if let Some(rest) = hostport.strip_prefix('[') {
        if let Some((host, port)) = rest.split_once("]:") {
            return (host, Some(port));
        }
        return (rest.trim_end_matches(']'), None);
    }
    if let Some((host, port)) = hostport.rsplit_once(':') {
        if !host.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
            return (host, Some(port));
        }
    }
    (hostport, None)
}

fn host_is_ip(host: &str) -> bool {
    host.parse::<std::net::IpAddr>().is_ok()
}

fn rewrite_pod(host: &str) -> String {
    if host_is_ip(host) {
        return host.to_string();
    }
    let first = host.split('.').next().unwrap_or(host);
    if first.ends_with("-mdm") {
        return host.to_string();
    }
    match host.split_once('.') {
        Some((label, rest)) => format!("{label}-mdm.{rest}"),
        None => format!("{host}-mdm"),
    }
}

fn login_endpoints(login_url: &str) -> (String, String) {
    let base = login_url.trim_end_matches('/');
    if let Some(root) = base.strip_suffix("/saas/public/core/v3/login") {
        return (
            base.to_string(),
            format!("{root}/ma/api/v2/user/login"),
        );
    }
    if let Some(root) = base.strip_suffix("/ma/api/v2/user/login") {
        return (
            format!("{root}/saas/public/core/v3/login"),
            base.to_string(),
        );
    }
    (
        format!("{base}/saas/public/core/v3/login"),
        format!("{base}/ma/api/v2/user/login"),
    )
}

fn parse_v3(body: &Value) -> Result<(String, String), String> {
    let session = body
        .pointer("/userInfo/sessionId")
        .and_then(json_string)
        .ok_or("v3 login has no userInfo.sessionId")?;
    let products = body.get("products").and_then(|v| v.as_array()).ok_or("v3 login has no products")?;
    let mut base = None;
    let mut preferred = None;
    for product in products {
        let Some(url) = product.get("baseApiUrl").and_then(|v| v.as_str()) else { continue };
        if base.is_none() {
            base = Some(url.to_string());
        }
        let name = product.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let lowered = name.to_ascii_lowercase();
        if lowered.contains("mdm") || lowered.contains("business") || url.contains("-mdm") {
            preferred = Some(url.to_string());
        }
    }
    let base = preferred.or(base).ok_or("v3 login has no baseApiUrl")?;
    Ok((session, base))
}

fn parse_v2(body: &Value) -> Result<(String, String), String> {
    let session = body.get("icSessionId").and_then(json_string).ok_or("v2 login has no icSessionId")?;
    let server = body.get("serverUrl").and_then(|v| v.as_str()).ok_or("v2 login has no serverUrl")?;
    Ok((session, server.to_string()))
}

fn model_version_now() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "0".into())
}

pub fn retry_delay(attempt: u32, retry_after: Option<&str>) -> Duration {
    if let Some(text) = retry_after {
        if let Ok(seconds) = text.trim().parse::<u64>() {
            return Duration::from_secs(seconds.min(30));
        }
    }
    Duration::from_millis(200 * 2u64.pow(attempt.min(7))).min(Duration::from_secs(30))
}

fn parse_search_page(model: &ModelDefinition, entity: &str, body: &Value) -> Result<SearchPage, BackendError> {
    let result = body.get("searchResult").unwrap_or(body);
    let records = result
        .get("records")
        .and_then(|v| v.as_array())
        .ok_or_else(|| BackendError::Message("search response has no searchResult.records".into()))?;
    let mut hits = result.get("hits").and_then(as_usize).unwrap_or(records.len());
    if hits < records.len() {
        hits = records.len();
    }
    let mut parsed = Vec::new();
    for row in records {
        let Some(obj) = row.as_object() else { continue };
        let Some(parsed_row) = master_from_object(model, entity, obj, true)? else { continue };
        parsed.push(parsed_row);
    }
    Ok(SearchPage { hits, records: parsed })
}

fn parse_entity_record(model: &ModelDefinition, entity: &str, body: &Value) -> Result<ParsedRecord, BackendError> {
    let obj = body.as_object().ok_or_else(|| BackendError::Message("entity response is not an object".into()))?;
    master_from_object(model, entity, obj, false)?
        .ok_or_else(|| BackendError::Message("entity response has no _meta.businessId".into()))
}

fn master_from_object(
    model: &ModelDefinition,
    entity: &str,
    obj: &Map<String, Value>,
    from_search: bool,
) -> Result<Option<ParsedRecord>, BackendError> {
    let Some(id) = business_id(obj) else { return Ok(None) };
    let fields = model.entity(entity).map(|ent| ent.fields.clone()).unwrap_or_default();
    let (values, lookups) = convert_object(model, entity, &fields, obj, from_search);
    let hydrated = model.embedded_children(entity).next().is_none()
        || model.embedded_children(entity).any(|child| {
            child.embedded_in.as_ref().is_some_and(|emb| values.contains_key(&emb.field))
        });
    Ok(Some(ParsedRecord {
        record: Record { entity: entity.to_string(), id, values },
        lookups,
        hydrated: hydrated || !from_search,
    }))
}

fn business_id(obj: &Map<String, Value>) -> Option<String> {
    obj.get("_meta")
        .and_then(|meta| meta.get("businessId"))
        .and_then(json_string)
        .or_else(|| obj.get("businessId").and_then(json_string))
}

fn convert_object(
    model: &ModelDefinition,
    entity: &str,
    fields: &[FieldDef],
    obj: &Map<String, Value>,
    from_search: bool,
) -> (BTreeMap<String, FieldValue>, Vec<(String, String, String)>) {
    let mut values = BTreeMap::new();
    let mut lookups = Vec::new();
    for (raw_key, raw_value) in obj {
        if raw_key.starts_with('_') || raw_key == "highlight" {
            continue;
        }
        let key = if from_search { strip_prefix(entity, raw_key) } else { raw_key.as_str() };
        if key.is_empty() || key.starts_with('_') {
            continue;
        }
        let (name, nested) = match key.split_once('.') {
            Some((head, tail)) if find_field(fields, head).is_none() => (head, Some(tail)),
            _ => (key, None),
        };
        let field = find_field(fields, name);
        let converted = convert_value(model, entity, field, raw_value, &mut lookups);
        if let Some(tail) = nested {
            let entry = values.entry(name.to_string()).or_insert_with(|| FieldValue::Group(BTreeMap::new()));
            if let FieldValue::Group(map) = entry {
                map.insert(tail.to_string(), converted);
            }
        } else {
            values.insert(name.to_string(), converted);
        }
    }
    (values, lookups)
}

fn convert_value(
    model: &ModelDefinition,
    entity: &str,
    field: Option<&FieldDef>,
    value: &Value,
    lookups: &mut Vec<(String, String, String)>,
) -> FieldValue {
    if let Some((code, name)) = as_lookup(value, field.is_some_and(|f| f.ty == FieldType::Lookup)) {
        if let Some(lookup_id) = field.and_then(|f| f.lookup.clone()) {
            if let Some(name) = name.clone() {
                lookups.push((lookup_id, code.clone(), name));
            }
            return FieldValue::String(code);
        }
        return FieldValue::String(name.unwrap_or(code));
    }
    match value {
        Value::Null => FieldValue::Null,
        Value::Bool(flag) => FieldValue::Bool(*flag),
        Value::Number(number) => FieldValue::Number(number.as_f64().unwrap_or(0.0)),
        Value::String(text) => FieldValue::String(text.clone()),
        Value::Array(items) => {
            let (child_entity, child_fields) = group_context(model, entity, field);
            FieldValue::List(
                items
                    .iter()
                    .map(|item| {
                        if let Some(map) = item.as_object() {
                            let grouped = field.is_some_and(|f| f.ty == FieldType::Group || f.many)
                                || map.keys().any(|key| key != "Code" && key != "Name" && key != "code" && key != "name");
                            if grouped {
                                let (group, more) = convert_object(model, &child_entity, &child_fields, map, false);
                                lookups.extend(more);
                                return FieldValue::Group(group);
                            }
                        }
                        convert_value(model, entity, field, item, lookups)
                    })
                    .collect(),
            )
        }
        Value::Object(map) => {
            if field.is_some_and(|candidate| candidate.ty == FieldType::Group) {
                let (child_entity, child_fields) = group_context(model, entity, field);
                let (group, more) = convert_object(model, &child_entity, &child_fields, map, false);
                lookups.extend(more);
                FieldValue::Group(group)
            } else if let Some(id) = map.get("businessId").and_then(json_string) {
                FieldValue::String(id)
            } else {
                let children = field.map(|candidate| candidate.children.clone()).unwrap_or_default();
                let (group, more) = convert_object(model, entity, &children, map, false);
                lookups.extend(more);
                FieldValue::Group(group)
            }
        }
    }
}

fn group_context(model: &ModelDefinition, entity: &str, field: Option<&FieldDef>) -> (String, Vec<FieldDef>) {
    let Some(field) = field else {
        return (entity.to_string(), Vec::new());
    };
    if let Some(child) = model.entity(&format!("{entity}.{}", field.name)) {
        return (child.id.clone(), child.fields.clone());
    }
    (entity.to_string(), field.children.clone())
}

fn as_lookup(value: &Value, force: bool) -> Option<(String, Option<String>)> {
    let obj = value.as_object()?;
    let code = obj.get("Code").or_else(|| obj.get("code")).and_then(json_string);
    let name = obj.get("Name").or_else(|| obj.get("name")).and_then(json_string);
    let only = obj.keys().all(|key| matches!(key.as_str(), "Code" | "code" | "Name" | "name"));
    if !only && !force {
        return None;
    }
    if code.is_none() && name.is_none() {
        return None;
    }
    let stored = code.clone().or(name.clone())?;
    Some((stored, name.or(code)))
}

fn find_field<'a>(fields: &'a [FieldDef], name: &str) -> Option<&'a FieldDef> {
    fields.iter().find(|field| field.name == name || field.id == name)
}

fn strip_prefix<'a>(entity: &str, key: &'a str) -> &'a str {
    let mut prefix = String::new();
    for ch in entity.chars() {
        if ch != '.' {
            prefix.push(ch);
        }
    }
    prefix.push('.');
    key.strip_prefix(&prefix).unwrap_or(key)
}

fn json_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn as_usize(value: &Value) -> Option<usize> {
    value
        .as_u64()
        .or_else(|| value.as_i64().map(|n| n.max(0) as u64))
        .map(|n| n as usize)
}

fn encode_segment(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub fn redact_json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                let lower = key.to_ascii_lowercase();
                if lower == "sessionid" || lower == "icsessionid" || lower == "password" || lower == "ids-session-id" {
                    *child = Value::String("***".into());
                } else {
                    redact_json(child);
                }
            }
        }
        Value::Array(items) => {
            for child in items {
                redact_json(child);
            }
        }
        _ => {}
    }
}

fn public_error(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.contains("eyJ") || trimmed.to_ascii_lowercase().contains("password") {
        return "request failed".into();
    }
    trimmed.chars().take(180).collect()
}

fn failure_line(status: u16, body: &str) -> String {
    format!("HTTP {status}: {}", preview_body(body))
}

fn preview_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "(empty body)".into();
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(mut value) => {
            redact_json(&mut value);
            let rendered = value.to_string();
            if rendered.contains("eyJ") {
                return "request failed".into();
            }
            rendered.chars().take(180).collect()
        }
        Err(_) => public_error(trimmed),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

    #[test]
    fn mdm_host_drops_saas_and_marks_the_pod() {
        assert_eq!(
            mdm_base_from_api_url("https://use4.dm-us.informaticacloud.com/saas").unwrap().to_string(),
            "https://use4-mdm.dm-us.informaticacloud.com"
        );
        assert_eq!(
            mdm_base_from_api_url("https://use4-mdm.dm-us.informaticacloud.com/saas").unwrap().to_string(),
            "https://use4-mdm.dm-us.informaticacloud.com"
        );
        assert_eq!(
            mdm_base_from_api_url("https://usw1.dmp-us.informaticacloud.com/saas").unwrap(),
            "https://usw1-mdm.dmp-us.informaticacloud.com"
        );
        assert_eq!(
            mdm_base_from_api_url("https://usw1-mdm.dmp-us.informaticacloud.com").unwrap(),
            "https://usw1-mdm.dmp-us.informaticacloud.com"
        );
        assert_eq!(
            mdm_base_from_api_url("https://emw1.dm-eu.informaticacloud.com/saas").unwrap(),
            "https://emw1-mdm.dm-eu.informaticacloud.com"
        );
        assert_eq!(
            mdm_base_from_api_url("http://127.0.0.1:9/saas").unwrap().to_string(),
            "http://127.0.0.1:9"
        );
        assert_eq!(retry_delay(0, Some("0")), Duration::from_secs(0));
        assert_eq!(retry_delay(0, Some("90")), Duration::from_secs(30));
        assert!(retry_delay(2, None) >= Duration::from_millis(200));
    }

    #[test]
    fn redacts_session_and_password() {
        let mut value = serde_json::json!({
            "userInfo": {"sessionId": "abc", "name": "kept"},
            "icSessionId": "xyz",
            "password": "nope"
        });
        redact_json(&mut value);
        assert_eq!(value["userInfo"]["sessionId"], "***");
        assert_eq!(value["userInfo"]["name"], "kept");
        assert_eq!(value["icSessionId"], "***");
        assert_eq!(value["password"], "***");
        let line = failure_line(
            400,
            r#"{"password":"s3cret-password","userInfo":{"sessionId":"sess-old"},"error":"nope"}"#,
        );
        assert!(line.starts_with("HTTP 400:"), "{line}");
        assert!(!line.contains("s3cret-password"), "{line}");
        assert!(!line.contains("sess-old"), "{line}");
        assert!(line.contains("nope"), "{line}");
    }

    #[test]
    fn search_and_get_shapes_match_the_docs() {
        let model = person_model();
        let search = serde_json::json!({
            "searchResult": {
                "hits": 1,
                "records": [{
                    "_meta": {
                        "businessId": "29924818400438832441125634728",
                        "id": "60b13ac80cabf75604b602a9",
                        "status": "ACTIVE"
                    },
                    "highlight": {"c360person.firstName": ["Arny"]},
                    "c360person.firstName": "Arny",
                    "c360person.lastName": "Eidelburg",
                    "c360person.gender": {"Code": "M", "Name": "Male"},
                    "c360person.PostalAddress": [{
                        "addressLine1": "17 W Jackson Blvd",
                        "city": "San Jose",
                        "country": {"Code": "US", "Name": "United States"}
                    }],
                    "c360person.Phone": [{"phoneNumber": "016-7658"}]
                }]
            }
        });
        let page = parse_search_page(&model, "c360.person", &search).unwrap();
        assert_eq!(page.hits, 1);
        let record = &page.records[0].record;
        assert_eq!(record.id, "29924818400438832441125634728");
        assert!(record.values.get("_meta").is_none());
        assert!(record.values.get("highlight").is_none());
        assert_eq!(record.values.get("firstName"), Some(&FieldValue::String("Arny".into())));
        assert_eq!(record.values.get("gender"), Some(&FieldValue::String("M".into())));
        assert!(page.records[0].lookups.iter().any(|(_, code, name)| code == "M" && name == "Male"));
        let addresses = crate::project_children(&model, record, "c360.person.PostalAddress");
        assert_eq!(addresses.len(), 1);
        assert_eq!(addresses[0].id, "29924818400438832441125634728::PostalAddress::0");
        assert_eq!(addresses[0].values.get("addressLine1"), Some(&FieldValue::String("17 W Jackson Blvd".into())));
        assert_eq!(addresses[0].values.get("country"), Some(&FieldValue::String("US".into())));
        let phones = crate::project_children(&model, record, "c360.person.Phone");
        assert_eq!(phones[0].values.get("phoneNumber"), Some(&FieldValue::String("016-7658".into())));

        let entity = serde_json::json!({
            "firstName": "Andrew",
            "lastName": "Plata",
            "gender": {"Code": "M", "Name": "Male"},
            "_meta": {"id": "do-not-use", "businessId": "MDM0000000GXKU", "state": "ACTIVE"},
            "_contentMeta": {"trust": {"fieldData": []}}
        });
        let read = parse_entity_record(&model, "c360.person", &entity).unwrap();
        assert_eq!(read.record.id, "MDM0000000GXKU");
        assert_eq!(read.record.values.get("firstName"), Some(&FieldValue::String("Andrew".into())));
        assert!(read.record.values.get("_contentMeta").is_none());
        assert!(read.hydrated);
    }

    #[test]
    fn login_retries_401_and_backs_off_on_429() {
        let logins = Arc::new(AtomicU32::new(0));
        let searches = Arc::new(AtomicU32::new(0));
        let login_count = Arc::clone(&logins);
        let search_count = Arc::clone(&searches);
        let url = stub(move |line, body| {
            if line.contains("/saas/public/core/v3/login") {
                let n = login_count.fetch_add(1, AtomicOrdering::SeqCst);
                let session = if n == 0 { "sess-old" } else { "sess-new" };
                (200, format!(r#"{{"products":[{{"name":"Business 360","baseApiUrl":"BASE/saas"}}],"userInfo":{{"sessionId":"{session}"}}}}"#), "")
            } else if line.contains("/search/") {
                let n = search_count.fetch_add(1, AtomicOrdering::SeqCst);
                assert!(body.contains("\"search\":\"*\""), "{body}");
                assert!(body.contains("\"entityType\":\"c360.person\""));
                assert!(!body.contains("\"fields\""));
                if n == 0 {
                    (401, r#"{"error":"expired"}"#.into(), "")
                } else if n == 1 {
                    (429, r#"{"error":"slow"}"#.into(), "Retry-After: 0\r\n")
                } else {
                    (200, r#"{"searchResult":{"hits":3,"records":[{"_meta":{"businessId":"MDM1"},"c360person.firstName":"A"}]}}"#.into(), "")
                }
            } else {
                (404, "{}".into(), "")
            }
        });
        let config = LiveConfig {
            login_url: url.clone(),
            username: "user".into(),
            password: "s3cret-password".into(),
            export: Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mock/idmc-export/person")),
            model_json: None,
            values: None,
            debug_dump: None,
        };
        let message = check_login(&config).unwrap();
        assert!(message.contains("login ok (v3)"), "{message}");
        assert!(message.contains("search c360.person: 3 hits"), "{message}");
        assert!(message.contains("first record MDM1"), "{message}");
        assert!(!message.contains("s3cret-password"));
        assert!(!message.contains("sess-old") && !message.contains("sess-new"), "{message}");
        assert_eq!(logins.load(AtomicOrdering::SeqCst), 2);
        assert!(searches.load(AtomicOrdering::SeqCst) >= 3);
    }

    #[test]
    fn window_fetches_the_visible_page_not_the_entity() {
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        let offsets = Arc::new(std::sync::Mutex::new(Vec::<u64>::new()));
        let seen = Arc::clone(&offsets);
        let base = stub(move |line, body| {
            if line.contains("/login") {
                (
                    200,
                    r#"{"products":[{"name":"Business 360","baseApiUrl":"BASE/saas"}],"userInfo":{"sessionId":"sess-win"}}"#.into(),
                    "",
                )
            } else if line.contains("/search/") {
                assert!(!body.contains("\"fields\""), "{body}");
                assert!(body.contains("\"pageSize\":500") || body.contains("\"recordsToReturn\":8"), "{body}");
                let offset = record_offset(body);
                seen.lock().unwrap().push(offset);
                let id = if offset == 3000 { "R3000" } else { "R0" };
                let name = if offset == 3000 { "Late" } else { "Early" };
                (
                    200,
                    format!(
                        r#"{{"searchResult":{{"hits":4000,"records":[{{"_meta":{{"businessId":"{id}"}},"c360person.firstName":"{name}"}}]}}}}"#
                    ),
                    "",
                )
            } else {
                (404, "{}".into(), "")
            }
        });
        let dir = std::env::temp_dir().join(format!("idmc-cache-{}", base.replace([':', '/'], "_")));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("IDMC_CACHE", &dir);
        let config = LiveConfig {
            login_url: base.clone(),
            username: "user".into(),
            password: "pw".into(),
            export: Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mock/idmc-export/person")),
            model_json: None,
            values: None,
            debug_dump: None,
        };
        let backend = LiveBackend::open(config).expect("open");
        let started = Instant::now();
        while !backend.master_known("c360.person") && started.elapsed() < Duration::from_secs(4) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(backend.master_known("c360.person"), "first window");
        assert_eq!(backend.master_total("c360.person"), 4000);
        assert!(backend.master_at("c360.person", 0).is_some());
        backend.master_touch("c360.person", 3000, 3010);
        let started = Instant::now();
        while backend.master_at("c360.person", 3000).is_none() && started.elapsed() < Duration::from_secs(4) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(backend.master_at("c360.person", 3000).is_some(), "row 3000");
        let got = offsets.lock().unwrap().clone();
        assert!(got.contains(&0), "{got:?}");
        assert!(got.contains(&3000), "{got:?}");
        assert!(
            got.iter().all(|offset| matches!(*offset, 0 | 500 | 2500 | 3000 | 3500)),
            "unexpected page {got:?}"
        );
        let lines = backend.timing_lines();
        assert!(lines.iter().any(|line| line.starts_with("timing: first rows")), "{lines:?}");
        assert!(lines.iter().any(|line| line.starts_with("timing: row 3000")), "{lines:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn four_thousand_rows_sort_once_then_reuse() {
        use std::sync::Arc;
        use std::time::Instant;
        use idmc_model::FieldValue;
        let model = person_model();
        let records: Vec<Record> = (0..4000)
            .map(|index| {
                let mut values = BTreeMap::new();
                values.insert("firstName".into(), FieldValue::String(format!("N{index:04}")));
                values.insert(
                    "lastName".into(),
                    FieldValue::String(format!("L{:04}", (index * 17) % 4000)),
                );
                Record {
                    entity: "c360.person".into(),
                    id: format!("id{index:04}"),
                    values,
                }
            })
            .collect();
        let started = Instant::now();
        let mut walked = 0usize;
        for _ in 0..20 {
            let rows = crate::records_of(&model, &records, "c360.person");
            walked += rows.len();
            for row in &rows {
                walked += crate::label_text(&model, row).len();
            }
        }
        let before = started.elapsed();
        let backend = crate::MockBackend::from_records_unchecked(model.clone(), records);
        let shared = backend.shared_records("c360.person");
        let started = Instant::now();
        let mut visible = 0usize;
        for _ in 0..20 {
            let page = backend.shared_records("c360.person");
            assert!(Arc::ptr_eq(&shared, &page));
            for row in page.iter().take(40) {
                visible += crate::label_text(&model, row).len();
            }
        }
        let after = started.elapsed();
        let painted = painted_rows(4000, 700.0, 28.0);
        eprintln!(
            "4000 persons, 20 frames: full sort+walk every row {before:?} ({walked} touches, 4000 elements); cached list + 40 visible rows {after:?} ({visible} touches, {painted} elements at 700px/28px)"
        );
        let clock = Instant::now();
        let total = backend.master_total("c360.person");
        let _ = backend.master_at("c360.person", 0);
        let first_rows = clock.elapsed();
        let clock = Instant::now();
        let row = backend.master_at("c360.person", 3000).expect("row 3000");
        let scroll = clock.elapsed();
        eprintln!(
            "timing: first rows {}us (1 of {total})",
            first_rows.as_micros()
        );
        eprintln!("timing: row 3000 ready {}us ({})", scroll.as_micros(), row.id);
        assert_eq!(shared.len(), 4000);
        assert!(painted < 40, "visible window {painted}");
        assert!(
            after.as_micros() * 8 < before.as_micros().max(1) || after.as_millis() < 50,
            "before {before:?} after {after:?}"
        );
    }

    fn painted_rows(total: usize, viewport_px: f32, row_px: f32) -> usize {
        let last = (viewport_px / row_px).ceil() as usize;
        last.min(total)
    }

    fn person_model() -> ModelDefinition {
        IdmcExportSource::open(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mock/idmc-export/person"))
            .unwrap()
            .load()
            .unwrap()
    }

    fn record_offset(body: &str) -> u64 {
        let key = "\"recordOffset\":";
        let Some(start) = body.find(key) else { return 0 };
        body[start + key.len()..]
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or(0)
    }

    fn stub(handler: impl Fn(&str, &str) -> (u16, String, &'static str) + Send + Sync + 'static) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");
        let handler = Arc::new(handler);
        let advertised = base.clone();
        std::thread::spawn(move || {
            for _ in 0..8 {
                let Ok((mut sock, _)) = listener.accept() else { break };
                sock.set_read_timeout(Some(Duration::from_secs(2))).ok();
                let Ok((line, body)) = read_http(&mut sock) else { break };
                let (status, mut payload, extra) = handler(&line, &body);
                payload = payload.replace("BASE", &advertised);
                let bytes = payload.into_bytes();
                let header = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n",
                    bytes.len()
                );
                let _ = sock.write_all(header.as_bytes());
                let _ = sock.write_all(&bytes);
            }
        });
        base
    }

    fn read_http(sock: &mut TcpStream) -> std::io::Result<(String, String)> {
        let mut buf = Vec::new();
        let mut tmp = [0u8; 2048];
        let mut header_end = None;
        while header_end.is_none() {
            let n = sock.read(&mut tmp)?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
            header_end = buf.windows(4).position(|w| w == b"\r\n\r\n").map(|pos| pos + 4);
            if buf.len() > 100_000 {
                break;
            }
        }
        let header_end = header_end.unwrap_or(buf.len());
        let header = String::from_utf8_lossy(&buf[..header_end]).to_string();
        let mut length = 0usize;
        for line in header.lines() {
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
        let mut body = buf.get(header_end..).unwrap_or(&[]).to_vec();
        while body.len() < length {
            let n = sock.read(&mut tmp)?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&tmp[..n]);
        }
        let line = header.lines().next().unwrap_or("").to_string();
        Ok((line, String::from_utf8_lossy(&body).into_owned()))
    }
}
