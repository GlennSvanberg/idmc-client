//! Live Business 360 reads.
//!
//! Login, search, and master-record shapes follow the pages in `docs/idmc-api/`.
//! The model still comes from an export package. Credentials stay in memory and
//! are never written to the debug dump.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use idmc_model::{
    FieldDef, FieldType, FieldValue, IdmcExportSource, JsonModelSource, ModelDefinition, ModelSource,
    Record,
};
use serde_json::{Map, Value};

use crate::config::env_path;
use crate::{
    hierarchy_children_of, hierarchy_roots_of, label_of, record_of, records_of,
    related_of, Backend, BackendError,
};

const PAGE: u64 = 25;
const MAX_RECORDS: usize = 5000;

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
    epoch: AtomicU64,
    generation: AtomicU64,
    seen: AtomicU64,
    last_error: Mutex<Option<String>>,
}

struct Cache {
    records: Vec<Record>,
    lookups: BTreeMap<(String, String), String>,
    hydrated: BTreeSet<String>,
    progress: BTreeMap<String, Progress>,
}

struct Progress {
    loaded: usize,
    hits: usize,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            records: Vec::new(),
            lookups: BTreeMap::new(),
            hydrated: BTreeSet::new(),
            progress: BTreeMap::new(),
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
    let model = load_model(config)?;
    let entity = model
        .roots()
        .next()
        .map(|entity| entity.id.clone())
        .unwrap_or_else(|| "c360.person".into());
    let http = IdmcHttp::connect(config)?;
    http.login()?;
    let body = http.search(&entity, 0, 1)?;
    let page = parse_search_page(&model, &entity, &body)?;
    let first = page.records.first().map(|row| row.record.id.clone());
    let id = first
        .as_ref()
        .map(|id| format!(", first record {id}"))
        .unwrap_or_default();
    let api = http.session.lock().ok().and_then(|g| g.as_ref().map(|s| s.api.clone())).unwrap_or_else(|| "?".into());
    let mdm = http.session.lock().ok().and_then(|g| g.as_ref().map(|s| s.mdm_base.clone())).unwrap_or_default();
    Ok(format!("login ok ({api})\nmdm {mdm}\nsearch {entity}: {} hits{id}", page.hits))
}

impl LiveBackend {
    pub fn open(config: LiveConfig) -> Result<Self, BackendError> {
        let model = Arc::new(load_model(&config)?);
        let roots: Vec<String> = model.roots().map(|entity| entity.id.clone()).collect();
        if roots.is_empty() {
            return Err(BackendError::Message("the model has no master entities to search".into()));
        }
        let shared = Arc::new(Shared {
            http: IdmcHttp::connect(&config)?,
            cache: Mutex::new(Cache::default()),
            epoch: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            seen: AtomicU64::new(0),
            last_error: Mutex::new(None),
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
        if let Ok(mut err) = self.shared.last_error.lock() {
            *err = None;
        }
        self.shared.http.login()?;
        for entity in &self.roots {
            if self.shared.epoch.load(Ordering::Relaxed) != epoch {
                return Ok(());
            }
            let body = self.shared.http.search(entity, 0, PAGE)?;
            let page = parse_search_page(&self.model, entity, &body)?;
            self.ingest(entity, page);
        }
        self.spawn(epoch);
        Ok(())
    }

    fn spawn(&self, epoch: u64) {
        let shared = Arc::clone(&self.shared);
        let model = Arc::clone(&self.model);
        let roots = self.roots.clone();
        std::thread::Builder::new()
            .name("idmc-search".into())
            .spawn(move || load_rest(shared, model, roots, epoch))
            .ok();
    }

    fn ingest(&self, entity: &str, page: SearchPage) {
        let mut cache = lock(&self.shared.cache);
        for parsed in page.records {
            remember(&mut cache, parsed);
        }
        let loaded = cache.records.iter().filter(|row| row.entity == entity).count();
        cache.progress.insert(
            entity.to_string(),
            Progress { loaded, hits: page.hits.max(loaded) },
        );
        self.shared.generation.fetch_add(1, Ordering::Relaxed);
    }

    fn ensure(&self, entity: &str, id: &str) {
        if self.model.embedded_children(entity).next().is_none() {
            return;
        }
        let key = format!("{entity}\0{id}");
        let present = {
            let cache = lock(&self.shared.cache);
            if cache.hydrated.contains(&key) {
                return;
            }
            cache.records.iter().any(|row| row.entity == entity && row.id == id)
        };
        if !present {
            return;
        }
        let base = match self.shared.http.mdm_base() {
            Some(base) => base,
            None => return,
        };
        let url = format!(
            "{base}/business-entity/public/api/v1/entity/{}/{}",
            encode_segment(entity),
            encode_segment(id)
        );
        let body = match self.shared.http.get_json(&url) {
            Ok(body) => body,
            Err(err) => {
                if let Ok(mut slot) = self.shared.last_error.lock() {
                    *slot = Some(public_error(&err.to_string()));
                }
                return;
            }
        };
        let parsed = match parse_entity_record(&self.model, entity, &body) {
            Ok(parsed) => parsed,
            Err(err) => {
                if let Ok(mut slot) = self.shared.last_error.lock() {
                    *slot = Some(public_error(&err.to_string()));
                }
                return;
            }
        };
        let mut cache = lock(&self.shared.cache);
        if let Some(slot) = cache.records.iter_mut().find(|row| row.entity == entity && row.id == id) {
            *slot = parsed.record;
        }
        for (lookup, code, name) in parsed.lookups {
            cache.lookups.insert((lookup, code), name);
        }
        cache.hydrated.insert(key);
        self.shared.generation.fetch_add(1, Ordering::Relaxed);
    }
}

impl Drop for LiveBackend {
    fn drop(&mut self) {
        self.shared.epoch.fetch_add(1, Ordering::Relaxed);
    }
}

impl Backend for LiveBackend {
    fn model(&self) -> &ModelDefinition { &self.model }

    fn records(&self, entity: &str) -> Vec<Record> {
        let cache = lock(&self.shared.cache);
        records_of(&self.model, &cache.records, entity)
    }

    fn record(&self, entity: &str, id: &str) -> Option<Record> {
        let cache = lock(&self.shared.cache);
        record_of(&self.model, &cache.records, entity, id)
    }

    fn label(&self, entity: &str, id: &str) -> String {
        let cache = lock(&self.shared.cache);
        label_of(&self.model, &cache.records, entity, id)
    }

    fn related(&self, relationship: &str, source_id: &str) -> Result<Vec<Record>, BackendError> {
        if let Some(rel) = self.model.relationship(relationship) {
            if rel.embedded {
                self.ensure(&rel.from, source_id);
            }
        }
        let cache = lock(&self.shared.cache);
        related_of(&self.model, &cache.records, relationship, source_id)
    }

    fn hierarchy_roots(&self, entity: &str) -> Vec<Record> {
        let cache = lock(&self.shared.cache);
        hierarchy_roots_of(&self.model, &cache.records, entity)
    }

    fn hierarchy_children(&self, entity: &str, id: &str) -> Vec<Record> {
        let cache = lock(&self.shared.cache);
        hierarchy_children_of(&self.model, &cache.records, entity, id)
    }

    fn prepare(&self, entity: &str, id: &str) { self.ensure(entity, id); }

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
        let cache = lock(&self.shared.cache);
        let loaded: usize = cache.progress.values().map(|row| row.loaded).sum();
        let hits: usize = cache.progress.values().map(|row| row.hits).sum();
        if hits > loaded {
            format!("Live · {loaded} of {hits}")
        } else {
            format!("Live · {loaded} records")
        }
    }

    fn lookup_name(&self, lookup: Option<&str>, code: &str) -> Option<String> {
        let lookup = lookup?;
        let cache = lock(&self.shared.cache);
        cache.lookups.get(&(lookup.to_string(), code.to_string())).cloned()
    }
}

fn load_rest(shared: Arc<Shared>, model: Arc<ModelDefinition>, roots: Vec<String>, epoch: u64) {
    for entity in roots {
        loop {
            if shared.epoch.load(Ordering::Relaxed) != epoch {
                return;
            }
            let (loaded, hits) = {
                let cache = lock(&shared.cache);
                match cache.progress.get(&entity) {
                    Some(progress) => (progress.loaded, progress.hits),
                    None => (0, 0),
                }
            };
            if loaded >= hits || loaded >= MAX_RECORDS {
                break;
            }
            let body = match shared.http.search(&entity, loaded as u64, PAGE) {
                Ok(body) => body,
                Err(err) => {
                    if let Ok(mut slot) = shared.last_error.lock() {
                        *slot = Some(public_error(&err.to_string()));
                    }
                    shared.generation.fetch_add(1, Ordering::Relaxed);
                    return;
                }
            };
            let page = match parse_search_page(&model, &entity, &body) {
                Ok(page) => page,
                Err(err) => {
                    if let Ok(mut slot) = shared.last_error.lock() {
                        *slot = Some(public_error(&err.to_string()));
                    }
                    return;
                }
            };
            if page.records.is_empty() {
                break;
            }
            let mut cache = lock(&shared.cache);
            let before = cache.records.len();
            for parsed in page.records {
                remember(&mut cache, parsed);
            }
            if cache.records.len() == before {
                break;
            }
            let count = cache.records.iter().filter(|row| row.entity == entity).count();
            cache.progress.insert(entity.clone(), Progress { loaded: count, hits: page.hits.max(count) });
            drop(cache);
            shared.generation.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn remember(cache: &mut Cache, parsed: ParsedRecord) {
    let key = format!("{}\0{}", parsed.record.entity, parsed.record.id);
    if parsed.hydrated {
        cache.hydrated.insert(key);
    }
    for (lookup, code, name) in parsed.lookups {
        cache.lookups.insert((lookup, code), name);
    }
    if let Some(slot) = cache
        .records
        .iter_mut()
        .find(|row| row.entity == parsed.record.entity && row.id == parsed.record.id)
    {
        *slot = parsed.record;
    } else {
        cache.records.push(parsed.record);
    }
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
    fn connect(config: &LiveConfig) -> Result<Self, BackendError> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(45))
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
        })
    }

    fn login(&self) -> Result<(), BackendError> {
        let _gate = self.login_gate.lock().unwrap_or_else(|err| err.into_inner());
        let (v3, v2) = login_endpoints(&self.login_url);
        let body = serde_json::json!({
            "username": self.username,
            "password": self.password,
        });
        let v3_result = self.post_once(&v3, &body, false);
        let session = match v3_result {
            Ok((status, value)) if status == 200 => match parse_v3(&value) {
                Ok((session_id, base)) => SessionInfo {
                    session_id,
                    mdm_base: mdm_base_from_api_url(&base)?,
                    model_version: model_version_now(),
                    api: "v3".into(),
                },
                Err(_) => self.login_v2(&v2, &body)?,
            },
            Ok((status, _)) if matches!(status, 404 | 405 | 501) => self.login_v2(&v2, &body)?,
            Ok((status, _)) if status == 401 || status == 403 => {
                return Err(BackendError::Message(format!("login failed: HTTP {status}")));
            }
            Ok((status, _)) => match self.login_v2(&v2, &body) {
                Ok(session) => session,
                Err(_) => return Err(BackendError::Message(format!("login failed: HTTP {status}"))),
            },
            Err(err) => return Err(err),
        };
        *self.session.lock().unwrap_or_else(|err| err.into_inner()) = Some(session);
        Ok(())
    }

    fn login_v2(&self, url: &str, body: &Value) -> Result<SessionInfo, BackendError> {
        let (status, value) = self.post_once(url, body, false)?;
        if status == 401 || status == 403 {
            return Err(BackendError::Message(format!("login failed: HTTP {status}")));
        }
        if status != 200 {
            return Err(BackendError::Message(format!("login failed: v2 HTTP {status}")));
        }
        let (session_id, base) = parse_v2(&value).map_err(|err| BackendError::Message(err))?;
        Ok(SessionInfo {
            session_id,
            mdm_base: mdm_base_from_api_url(&base)?,
            model_version: model_version_now(),
            api: "v2".into(),
        })
    }

    fn search(&self, entity: &str, record_offset: u64, page: u64) -> Result<Value, BackendError> {
        let base = self.mdm_base().ok_or_else(|| BackendError::Message("not logged in".into()))?;
        let url = format!("{base}/search/public/api/v1/search");
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
        let mut relogin = false;
        let mut attempt = 0u32;
        loop {
            let response = self.dispatch(method, url, body)?;
            let status = response.status().as_u16();
            if status == 401 && !relogin {
                self.login()?;
                relogin = true;
                continue;
            }
            if status == 429 && attempt < 5 {
                let delay = retry_delay(attempt, response.headers().get("retry-after").and_then(|v| v.to_str().ok()));
                attempt += 1;
                std::thread::sleep(delay);
                continue;
            }
            let text = response.text().map_err(|err| BackendError::Message(format!("http: {err}")))?;
            self.dump_response(method, url, status, &text);
            if status == 401 {
                return Err(BackendError::Message("HTTP 401".into()));
            }
            if !(200..300).contains(&status) {
                return Err(BackendError::Message(format!("HTTP {status}")));
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

    fn post_once(&self, url: &str, body: &Value, authed: bool) -> Result<(u16, Value), BackendError> {
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
            return Ok((status, value));
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

    fn person_model() -> ModelDefinition {
        IdmcExportSource::open(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mock/idmc-export/person"))
            .unwrap()
            .load()
            .unwrap()
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
