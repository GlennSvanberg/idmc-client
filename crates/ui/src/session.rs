//! Workspace session: which views exist, what is selected, and how links resolve.
//! No GPUI types, so the behavior can be tested without a window.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use idmc_backend::{
    check_login, load_dotenv, read_env, AppBackend, Backend, BackendMode, LiveConfig, MockBackend,
    SearchHit,
};
use idmc_layout::{Node, Zone};
use idmc_model::{value_at, FieldDef, FieldType, FieldValue, Record, ViewKind};

use crate::actions::{palette_actions, Action};
use crate::edit::TextEdit;
use crate::workspace::{
    default_workspace, sanitize, template_names, template_workspace, ViewLink, ViewSpec, WorkspaceFile,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    Quit,
}

#[derive(Clone, Debug)]
pub struct PaletteState {
    pub open: bool,
    pub edit: TextEdit,
    pub cursor: usize,
    pub mode: PaletteMode,
}

#[derive(Clone, Debug)]
pub enum PaletteMode {
    Commands,
    PickEntity(ViewKind),
    PickLink(String),
    OpenView,
    PickTemplate,
    RenameWorkspace,
}

impl PaletteState {
    fn closed() -> Self {
        Self {
            open: false,
            edit: TextEdit::new(),
            cursor: 0,
            mode: PaletteMode::Commands,
        }
    }

    pub fn open_commands(&mut self) {
        *self = Self {
            open: true,
            edit: TextEdit::new(),
            cursor: 0,
            mode: PaletteMode::Commands,
        };
    }

    pub fn query(&self) -> &str {
        self.edit.text.as_str()
    }
}

#[derive(Clone, Debug)]
pub struct MenuState {
    pub x: f32,
    pub y: f32,
    pub items: Vec<MenuItem>,
}

#[derive(Clone, Debug)]
pub struct MenuItem {
    pub label: String,
    pub detail: String,
    pub command: MenuCommand,
}

#[derive(Clone, Debug)]
pub enum MenuCommand {
    Run(&'static str),
    Open { kind: ViewKind, entity: String, target: String },
    Link { view: String, source: String, relationship: String },
    Dismiss,
}

struct CachedRows {
    generation: u64,
    source: Option<String>,
    rows: Arc<Vec<Record>>,
}

struct CellCache {
    generation: u64,
    values: HashMap<(String, String, String), String>,
}

pub struct Session {
    pub backend: AppBackend,
    pub file: WorkspaceFile,
    pub workspace_path: PathBuf,
    pub data_path: Option<PathBuf>,
    pub status: String,
    pub palette: PaletteState,
    pub menu: Option<MenuState>,
    pub chooser_for: Option<String>,
    pub chooser_filter: TextEdit,
    pub dock_hover: Option<(String, Zone)>,
    pub pending_dock: Option<String>,
    row_cache: RefCell<HashMap<String, CachedRows>>,
    cell_cache: RefCell<CellCache>,
    seen_generation: u64,
    seen_sig: u64,
    search_hits: Vec<SearchHit>,
    master_sync: RefCell<HashMap<String, String>>,
    pub filter_for: Option<String>,
    pub filter_edit: TextEdit,
    pub graph: Option<crate::graph::GraphBoard>,
    pub graph_view: Option<String>,
    pub graph_query: bool,
    pub graph_edit: TextEdit,
    pub graph_origin: RefCell<(f32, f32, f32, f32)>,
}

pub enum Launch {
    Run(Session),
    Done { code: i32, message: String },
}

impl Session {
    pub fn boot(
        backend: AppBackend,
        file: WorkspaceFile,
        workspace_path: PathBuf,
        data_path: Option<PathBuf>,
    ) -> Self {
        Self {
            backend,
            file,
            workspace_path,
            data_path,
            status: String::new(),
            palette: PaletteState::closed(),
            menu: None,
            chooser_for: None,
            chooser_filter: TextEdit::new(),
            dock_hover: None,
            pending_dock: None,
            row_cache: RefCell::new(HashMap::new()),
            cell_cache: RefCell::new(CellCache {
                generation: u64::MAX,
                values: HashMap::new(),
            }),
            seen_generation: u64::MAX,
            seen_sig: 0,
            search_hits: Vec::new(),
            master_sync: RefCell::new(HashMap::new()),
            filter_for: None,
            filter_edit: TextEdit::new(),
            graph: None,
            graph_view: None,
            graph_query: false,
            graph_edit: TextEdit::new(),
            graph_origin: RefCell::new((0.0, 0.0, 800.0, 600.0)),
        }
    }

    pub fn spec(&self, id: &str) -> Option<&ViewSpec> {
        self.file.views.iter().find(|view| view.id == id)
    }

    pub fn view_title(&self, id: &str) -> String {
        let Some(spec) = self.spec(id) else { return id.to_string() };
        if spec.kind == ViewKind::Graph {
            return "Graph".into();
        }
        let Some(entity) = self.backend.model().entity(&spec.entity) else { return spec.entity.clone() };
        match spec.kind {
            ViewKind::Tree => entity
                .hierarchy
                .as_ref()
                .map(|h| h.label.clone())
                .unwrap_or_else(|| entity.label_plural.clone()),
            ViewKind::Table => entity.label_plural.clone(),
            ViewKind::Detail => entity.label.clone(),
            ViewKind::Graph => "Graph".into(),
        }
    }

    pub fn link_caption(&self, id: &str) -> Option<String> {
        let link = self.spec(id)?.link.as_ref()?;
        let source = self.view_title(&link.source);
        if link.relationship == "identity" {
            Some(format!("Selected record from {source}"))
        } else {
            let label = self
                .backend
                .model()
                .relationship(&link.relationship)
                .map(|rel| rel.label.clone())
                .unwrap_or_else(|| link.relationship.clone());
            Some(format!("{label} from {source}"))
        }
    }

    pub fn rows(&self, view_id: &str) -> Vec<Record> {
        self.rows_cached(view_id).as_ref().clone()
    }

    /// Sorted rows for a view. The same allocation is reused until the data
    /// generation or the link source changes.
    pub fn rows_cached(&self, view_id: &str) -> Arc<Vec<Record>> {
        let generation = self.backend.generation();
        let source = self.row_source(view_id);
        if let Some(hit) = self.row_cache.borrow().get(view_id) {
            if hit.generation == generation && hit.source == source {
                return Arc::clone(&hit.rows);
            }
        }
        let rows = self.fresh_rows(view_id);
        self.row_cache.borrow_mut().insert(
            view_id.to_string(),
            CachedRows {
                generation,
                source,
                rows: Arc::clone(&rows),
            },
        );
        rows
    }

    fn row_source(&self, view_id: &str) -> Option<String> {
        let link = self.spec(view_id)?.link.as_ref()?;
        self.file.selections.get(&link.source).cloned()
    }

    fn fresh_rows(&self, view_id: &str) -> Arc<Vec<Record>> {
        let Some(spec) = self.spec(view_id) else { return Arc::new(Vec::new()) };
        if spec.kind == ViewKind::Graph {
            return Arc::new(Vec::new());
        }
        if spec.link.is_none() && spec.kind != ViewKind::Tree {
            return self.backend.shared_records(&spec.entity);
        }
        Arc::new(self.compute_rows(view_id))
    }

    fn compute_rows(&self, view_id: &str) -> Vec<Record> {
        let Some(spec) = self.spec(view_id) else { return Vec::new() };
        if let Some(link) = &spec.link {
            let Some(selected) = self.file.selections.get(&link.source) else { return Vec::new() };
            if link.relationship == "identity" {
                return self.backend.record(&spec.entity, selected).into_iter().collect();
            }
            return self.backend.related(&link.relationship, selected).unwrap_or_default();
        }
        if spec.kind == ViewKind::Tree {
            return Vec::new();
        }
        self.backend.shared_records(&spec.entity).as_ref().clone()
    }

    pub fn visible_ids(&self, view_id: &str) -> Vec<String> {
        let Some(spec) = self.spec(view_id) else { return Vec::new() };
        if spec.kind == ViewKind::Tree && spec.link.is_none() {
            let mut out = Vec::new();
            self.walk_tree(&spec.entity, None, &mut out);
            return out;
        }
        self.rows_cached(view_id).iter().map(|row| row.id.clone()).collect()
    }

    fn workspace_sig(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for view in &self.file.views {
            view.id.hash(&mut hasher);
            view.entity.hash(&mut hasher);
            match view.kind {
                ViewKind::Tree => 0u8,
                ViewKind::Table => 1,
                ViewKind::Detail => 2,
                ViewKind::Graph => 3,
            }
            .hash(&mut hasher);
            if let Some(link) = &view.link {
                link.source.hash(&mut hasher);
                link.relationship.hash(&mut hasher);
            }
        }
        for (key, value) in &self.file.selections {
            key.hash(&mut hasher);
            value.hash(&mut hasher);
        }
        hasher.finish()
    }

    fn walk_tree(&self, entity: &str, parent: Option<&str>, out: &mut Vec<String>) {
        let rows = match parent {
            None => self.backend.hierarchy_roots(entity),
            Some(id) => self.backend.hierarchy_children(entity, id),
        };
        for row in rows {
            let id = row.id.clone();
            out.push(id.clone());
            self.walk_tree(entity, Some(&id), out);
        }
    }

    pub fn reconcile(&mut self) {
        let generation = self.backend.generation();
        let sig = self.workspace_sig();
        if self.seen_generation == generation && self.seen_sig == sig {
            return;
        }
        for _ in 0..8 {
            let mut changed = false;
            let ids: Vec<String> = self.file.views.iter().map(|view| view.id.clone()).collect();
            for id in ids {
                let visible = self.visible_ids(&id);
                let current = self.file.selections.get(&id).cloned();
                let next = match &current {
                    Some(cur) if visible.iter().any(|id| id == cur) => current.clone(),
                    _ => visible.first().cloned(),
                };
                if next != current {
                    changed = true;
                    match next {
                        Some(value) => {
                            self.file.selections.insert(id, value);
                        }
                        None => {
                            self.file.selections.remove(&id);
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        self.seen_generation = self.backend.generation();
        self.seen_sig = self.workspace_sig();
    }

    pub fn display(&self, field: &FieldDef, value: &FieldValue) -> String {
        if field.ty == FieldType::Reference {
            return match value {
                FieldValue::String(id) => {
                    let target = field.target_entity.as_deref().unwrap_or("");
                    if target.is_empty() {
                        id.clone()
                    } else {
                        self.backend.label(target, id)
                    }
                }
                FieldValue::List(items) => items
                    .iter()
                    .map(|item| self.display(field, item))
                    .filter(|text| !text.is_empty())
                    .collect::<Vec<_>>()
                    .join(", "),
                _ => String::new(),
            };
        }
        if field.ty == FieldType::Lookup {
            if let FieldValue::String(code) = value {
                if let Some(name) = self.backend.lookup_name(field.lookup.as_deref(), code) {
                    return name;
                }
            }
        }
        self.backend
            .model()
            .format_field(field, value, &|_, id| id.to_string())
    }

    pub fn cell(&self, entity: &str, field_id: &str, record: &Record) -> String {
        let generation = self.backend.generation();
        let key = (entity.to_string(), record.id.clone(), field_id.to_string());
        {
            let cache = self.cell_cache.borrow();
            if cache.generation == generation {
                if let Some(hit) = cache.values.get(&key) {
                    return hit.clone();
                }
            }
        }
        let text = self.compute_cell(entity, field_id, record);
        let mut cache = self.cell_cache.borrow_mut();
        if cache.generation != generation {
            cache.values.clear();
            cache.generation = generation;
        }
        cache.values.insert(key, text.clone());
        text
    }

    fn compute_cell(&self, entity: &str, field_id: &str, record: &Record) -> String {
        let Some(field) = self
            .backend
            .model()
            .entity(entity)
            .and_then(|ent| ent.field(field_id))
            .cloned()
        else {
            return String::new();
        };
        let Some(value) = value_at(&record.values, field_id).cloned() else {
            return String::new();
        };
        self.display(&field, &value)
    }

    pub fn perform(&mut self, id: &str) -> Effect {
        match id {
            "palette.open" => {
                if self.palette.open && matches!(self.palette.mode, PaletteMode::Commands) {
                    self.palette.open = false;
                } else {
                    self.palette.open_commands();
                }
                self.menu = None;
            }
            "theme.toggle" => {
                self.file.theme = self.file.theme.toggled();
                self.status = format!("Theme: {}", self.file.theme.label());
            }
            "workspace.save" => {
                self.status = self.save().unwrap_or_else(|err| err);
            }
            "workspace.reload" => self.reload_workspace(),
            "workspace.reset" => {
                self.file = default_workspace(self.backend.model());
                self.chooser_for = None;
                self.master_sync.borrow_mut().clear();
                self.reconcile();
                self.status = "Workspace reset from the model".into();
            }
            "workspace.new" => self.begin_template(),
            "workspace.duplicate" => self.duplicate_workspace(),
            "workspace.rename" => self.begin_rename(),
            "workspace.close" => self.close_workspace(),
            "view.open_graph" => self.open_graph(None),
            "record.show_in_graph" => self.show_in_graph(),
            "record.search" => {
                self.palette.open_commands();
                self.menu = None;
                self.status = "Type a name. Record matches appear with the commands.".into();
                self.refresh_search();
            }
            "data.reload" => self.reload_data(),
            "app.quit" => {
                let _ = self.save();
                return Effect::Quit;
            }
            "view.close" => self.close_focused(),
            "view.zoom" => self.zoom_focused(),
            "view.focus_next" => self.focus_delta(1),
            "view.focus_prev" => self.focus_delta(-1),
            "view.move_left" => self.move_focused(Zone::Left),
            "view.move_right" => self.move_focused(Zone::Right),
            "view.move_up" => self.move_focused(Zone::Top),
            "view.move_down" => self.move_focused(Zone::Bottom),
            "view.new_table" => self.begin_new(ViewKind::Table),
            "view.new_detail" => self.begin_new(ViewKind::Detail),
            "view.new_tree" => self.begin_new(ViewKind::Tree),
            "view.open" => self.begin_open(),
            "view.fields" => self.toggle_chooser(self.file.focus.clone()),
            "view.link" => self.begin_link(self.file.focus.clone()),
            "view.unlink" => {
                let focus = self.file.focus.clone();
                self.clear_link(focus.as_deref());
            }
            "selection.next" => self.nudge_selection(1),
            "selection.prev" => self.nudge_selection(-1),
            _ => self.status = format!("Unknown action {id}"),
        }
        Effect::None
    }

    fn nudge_selection(&mut self, delta: i32) {
        if self.graph_focused() && !self.palette.open {
            self.graph.as_mut().map(|board| board.arrow(if delta > 0 { "down" } else { "up" }));
            return;
        }
        if self.palette.open {
            let len = self.palette_len();
            if len == 0 {
                return;
            }
            let next = self.palette.cursor as i32 + delta;
            self.palette.cursor = next.clamp(0, len as i32 - 1) as usize;
            return;
        }
        let Some(focus) = self.file.focus.clone() else { return };
        let ids = self.visible_ids(&focus);
        if ids.is_empty() {
            return;
        }
        let current = self
            .file
            .selections
            .get(&focus)
            .and_then(|id| ids.iter().position(|row| row == id))
            .unwrap_or(0);
        let next = (current as i32 + delta).clamp(0, ids.len() as i32 - 1) as usize;
        self.file.selections.insert(focus, ids[next].clone());
        self.reconcile();
    }

    pub fn palette_len(&self) -> usize {
        self.palette_rows().len()
    }

    pub fn open_view_choices(&self, query: &str) -> Vec<(ViewKind, String, String)> {
        let q = query.trim().to_ascii_lowercase();
        let mut out = Vec::new();
        for entity in &self.backend.model().entities {
            for kind in [ViewKind::Table, ViewKind::Detail, ViewKind::Tree] {
                if kind == ViewKind::Tree && entity.hierarchy.is_none() {
                    continue;
                }
                let kind_label = match kind {
                    ViewKind::Table => "Table",
                    ViewKind::Detail => "Detail",
                    ViewKind::Tree => "Tree",
                    ViewKind::Graph => "Graph",
                };
                let name = if kind == ViewKind::Table {
                    entity.label_plural.as_str()
                } else {
                    entity.label.as_str()
                };
                let title = format!("{kind_label} · {name}");
                if q.is_empty()
                    || title.to_ascii_lowercase().contains(&q)
                    || entity.id.to_ascii_lowercase().contains(&q)
                    || entity.label.to_ascii_lowercase().contains(&q)
                {
                    out.push((kind, entity.id.clone(), title));
                }
            }
        }
        out
    }

    pub fn entity_choices(&self, kind: ViewKind, query: &str) -> Vec<(String, String)> {
        let q = query.trim().to_ascii_lowercase();
        self.backend
            .model()
            .entities
            .iter()
            .filter(|entity| kind != ViewKind::Tree || entity.hierarchy.is_some())
            .filter(|entity| {
                q.is_empty()
                    || entity.label.to_ascii_lowercase().contains(&q)
                    || entity.label_plural.to_ascii_lowercase().contains(&q)
                    || entity.id.to_ascii_lowercase().contains(&q)
            })
            .map(|entity| {
                let label = if kind == ViewKind::Table {
                    entity.label_plural.clone()
                } else {
                    entity.label.clone()
                };
                (entity.id.clone(), label)
            })
            .collect()
    }

    pub fn link_choices(&self, view_id: &str, query: &str) -> Vec<(String, String, String)> {
        let Some(spec) = self.spec(view_id) else { return Vec::new() };
        let q = query.trim().to_ascii_lowercase();
        let mut out = Vec::new();
        let sources: Vec<(String, String)> = self
            .file
            .views
            .iter()
            .filter(|view| view.id != view_id)
            .map(|view| (view.id.clone(), view.entity.clone()))
            .collect();
        for (source_id, source_entity) in sources {
            for option in self
                .backend
                .model()
                .link_options(&source_entity, &spec.entity, spec.kind)
            {
                let label = format!("{} · {}", self.view_title(&source_id), option.label);
                if q.is_empty() || label.to_ascii_lowercase().contains(&q) {
                    out.push((source_id.clone(), option.relationship.clone(), label));
                }
            }
        }
        out
    }

    pub fn confirm_palette(&mut self) -> Effect {
        if !self.palette.open {
            return Effect::None;
        }
        match self.palette.mode.clone() {
            PaletteMode::Commands => {
                let rows = self.palette_rows();
                let Some(row) = rows.get(self.palette.cursor) else {
                    return Effect::None;
                };
                if row.header {
                    return Effect::None;
                }
                if let Some(hit) = row.hit.clone() {
                    self.palette.open = false;
                    self.palette.edit.clear();
                    self.search_hits.clear();
                    self.open_record(&hit.entity, &hit.id);
                    return Effect::None;
                }
                let Some(action) = palette_actions(self.palette.query())
                    .into_iter()
                    .find(|action| action.id == row.detail)
                else {
                    return Effect::None;
                };
                self.palette.open = false;
                self.palette.edit.clear();
                self.search_hits.clear();
                self.perform(action.id)
            }
            PaletteMode::PickTemplate => {
                let names = self.template_choices();
                let Some(name) = names.get(self.palette.cursor).cloned() else {
                    self.status = "No template".into();
                    return Effect::None;
                };
                self.palette.open = false;
                self.add_workspace(&name);
                Effect::None
            }
            PaletteMode::RenameWorkspace => {
                let name = self.palette.query().trim().to_string();
                self.palette.open = false;
                if name.is_empty() {
                    self.status = "Workspace name is empty".into();
                } else if let Some(tab) = self.file.tabs.get_mut(self.file.active) {
                    tab.name = name.clone();
                    self.status = format!("Workspace renamed to {name}");
                }
                Effect::None
            }
            PaletteMode::PickEntity(kind) => {
                let choices = self.entity_choices(kind, self.palette.query());
                let Some((entity, _)) = choices.get(self.palette.cursor) else {
                    self.status = if kind == ViewKind::Tree {
                        "This model has no hierarchical entities".into()
                    } else {
                        "No matching entity".into()
                    };
                    return Effect::None;
                };
                let entity = entity.clone();
                let dock = self.pending_dock.clone();
                self.palette.open = false;
                self.open_view(kind, &entity, dock);
                Effect::None
            }
            PaletteMode::OpenView => {
                let choices = self.open_view_choices(self.palette.query());
                let Some((kind, entity, _)) = choices.get(self.palette.cursor) else {
                    self.status = "No matching view".into();
                    return Effect::None;
                };
                let kind = *kind;
                let entity = entity.clone();
                let dock = self.pending_dock.clone();
                self.palette.open = false;
                self.open_view(kind, &entity, dock);
                Effect::None
            }
            PaletteMode::PickLink(view) => {
                let choices = self.link_choices(&view, self.palette.query());
                let Some((source, relationship, _)) = choices.get(self.palette.cursor) else {
                    self.status = "No compatible source view".into();
                    return Effect::None;
                };
                let source = source.clone();
                let relationship = relationship.clone();
                self.palette.open = false;
                self.set_link(&view, &source, &relationship);
                Effect::None
            }
        }
    }

    pub fn palette_rows(&self) -> Vec<PaletteRow> {
        match &self.palette.mode {
            PaletteMode::Commands => {
                let mut rows: Vec<PaletteRow> = palette_actions(self.palette.query())
                    .into_iter()
                    .map(PaletteRow::from_action)
                    .collect();
                let mut last = String::new();
                for hit in &self.search_hits {
                    if hit.entity_label != last {
                        last = hit.entity_label.clone();
                        rows.push(PaletteRow::header(hit.entity_label.clone()));
                    }
                    rows.push(PaletteRow::hit(hit));
                }
                rows
            }
            PaletteMode::PickEntity(kind) => self
                .entity_choices(*kind, self.palette.query())
                .into_iter()
                .map(|(_, label)| PaletteRow::plain(label))
                .collect(),
            PaletteMode::PickLink(view) => self
                .link_choices(view, self.palette.query())
                .into_iter()
                .map(|(_, _, label)| PaletteRow::plain(label))
                .collect(),
            PaletteMode::OpenView => self
                .open_view_choices(self.palette.query())
                .into_iter()
                .map(|(_, _, title)| PaletteRow::plain(title))
                .collect(),
            PaletteMode::PickTemplate => self.template_choices().into_iter().map(PaletteRow::plain).collect(),
            PaletteMode::RenameWorkspace => Vec::new(),
        }
    }

    fn template_choices(&self) -> Vec<String> {
        let q = self.palette.query().trim().to_ascii_lowercase();
        template_names()
            .iter()
            .filter(|name| q.is_empty() || name.to_ascii_lowercase().contains(&q))
            .map(|name| (*name).to_string())
            .collect()
    }

    fn refresh_search(&mut self) {
        if !self.palette.open || !matches!(self.palette.mode, PaletteMode::Commands) {
            return;
        }
        let query = self.palette.query().to_string();
        if self.backend.is_live() {
            self.backend.schedule_record_search(&query);
        } else {
            self.search_hits = self.backend.search_records(&query, 8);
        }
    }

    pub fn absorb_async(&mut self) -> bool {
        let mut changed = false;
        if let Some(hits) = self.backend.take_record_search() {
            if self.graph_query {
                self.seed_graph(&hits);
                self.graph_query = false;
            }
            self.search_hits = hits;
            changed = true;
        }
        if self.absorb_graph() {
            changed = true;
        }
        for line in self.backend.timing_lines() {
            println!("{line}");
            let _ = std::io::Write::flush(&mut std::io::stdout());
            self.status = line;
            changed = true;
        }
        changed
    }

    pub fn open_record(&mut self, entity: &str, id: &str) {
        let table = self
            .file
            .views
            .iter()
            .find(|view| view.kind == ViewKind::Table && view.entity == entity)
            .map(|view| view.id.clone());
        let table_id = if let Some(table) = table {
            table
        } else {
            let before = self.file.views.len();
            self.open_view(ViewKind::Table, entity, None);
            self.file.views.get(before).map(|view| view.id.clone()).unwrap_or_default()
        };
        if table_id.is_empty() {
            return;
        }
        self.select(&table_id, id);
        let detail = self
            .file
            .views
            .iter()
            .find(|view| {
                view.kind == ViewKind::Detail
                    && view.entity == entity
                    && view.link.as_ref().is_some_and(|link| link.source == table_id && link.relationship == "identity")
            })
            .map(|view| view.id.clone());
        let detail_id = if let Some(detail) = detail {
            detail
        } else {
            let before = self.file.views.len();
            self.file.focus = Some(table_id.clone());
            self.open_view(ViewKind::Detail, entity, Some(table_id.clone()));
            if let Some(created) = self.file.views.get(before) {
                let created_id = created.id.clone();
                if created.link.as_ref().map(|link| link.relationship.as_str()) != Some("identity") {
                    self.set_link(&created_id, &table_id, "identity");
                }
                created_id
            } else {
                return;
            }
        };
        self.file.focus = Some(detail_id.clone());
        self.file.layout.activate(&detail_id);
        self.status = format!("Opened {}", self.view_title(&detail_id));
    }

    pub fn switch_workspace(&mut self, index: usize) {
        if index >= self.file.tabs.len() {
            return;
        }
        self.file.active = index;
        self.master_sync.borrow_mut().clear();
        self.reconcile();
        self.status = format!("Workspace {}", self.file.name);
    }

    fn add_workspace(&mut self, name: &str) {
        let mut created = template_workspace(self.backend.model(), name);
        sanitize(&mut created, self.backend.model());
        if let Some(tab) = created.tabs.into_iter().next() {
            let mut tab = tab;
            tab.name = name.to_string();
            tab.id = format!("ws-{}-{}", name.to_ascii_lowercase().replace(' ', "-"), self.file.tabs.len() + 1);
            self.file.tabs.push(tab);
            self.file.active = self.file.tabs.len() - 1;
            self.master_sync.borrow_mut().clear();
            self.reconcile();
            self.status = format!("Opened workspace {name}");
        }
    }

    fn duplicate_workspace(&mut self) {
        let index = self.file.active;
        let Some(mut copy) = self.file.tabs.get(index).cloned() else { return };
        copy.name = format!("{} copy", copy.name);
        copy.id = format!("{}-copy-{}", copy.id, self.file.tabs.len() + 1);
        let name = copy.name.clone();
        self.file.tabs.push(copy);
        self.file.active = self.file.tabs.len() - 1;
        self.status = format!("Duplicated {name}");
    }

    fn close_workspace(&mut self) {
        if self.file.tabs.len() <= 1 {
            self.status = "Keep at least one workspace".into();
            return;
        }
        let index = self.file.active;
        self.file.tabs.remove(index);
        self.file.active = index.min(self.file.tabs.len() - 1);
        self.master_sync.borrow_mut().clear();
        self.reconcile();
        self.status = format!("Workspace {}", self.file.name);
    }

    fn begin_template(&mut self) {
        self.palette = PaletteState {
            open: true,
            edit: TextEdit::new(),
            cursor: 0,
            mode: PaletteMode::PickTemplate,
        };
        self.menu = None;
    }

    fn begin_rename(&mut self) {
        let name = self.file.name.clone();
        let mut edit = TextEdit::new();
        for ch in name.chars() {
            edit.insert(ch);
        }
        edit.select_all();
        self.palette = PaletteState {
            open: true,
            edit,
            cursor: 0,
            mode: PaletteMode::RenameWorkspace,
        };
        self.menu = None;
    }

    pub fn is_master_table(&self, view_id: &str) -> bool {
        let Some(spec) = self.spec(view_id) else { return false };
        spec.kind == ViewKind::Table
            && spec.link.is_none()
            && self
                .backend
                .model()
                .entity(&spec.entity)
                .and_then(|entity| entity.embedded_in.as_ref())
                .is_none()
    }

    pub fn sync_master(&self, view_id: &str) {
        let Some(spec) = self.spec(view_id) else { return };
        if !self.is_master_table(view_id) {
            return;
        }
        let key = format!(
            "{}\0{}\0{}",
            spec.query,
            spec.sort_field.clone().unwrap_or_default(),
            spec.sort_descending
        );
        if self.master_sync.borrow().get(view_id) == Some(&key) {
            return;
        }
        self.backend.set_master_view(
            &spec.entity,
            &spec.query,
            spec.sort_field.as_deref(),
            spec.sort_descending,
        );
        self.master_sync.borrow_mut().insert(view_id.to_string(), key);
    }

    pub fn apply_filter(&mut self, view_id: &str) {
        let query = self.filter_edit.text.clone();
        if let Some(spec) = self.file.views.iter_mut().find(|spec| spec.id == view_id) {
            spec.query = query;
        }
        self.master_sync.borrow_mut().remove(view_id);
        self.filter_for = None;
        self.sync_master(view_id);
        self.status = "Filter sent to the list".into();
    }

    pub fn toggle_sort(&mut self, view_id: &str, field: &str) {
        if let Some(spec) = self.file.views.iter_mut().find(|spec| spec.id == view_id) {
            if spec.sort_field.as_deref() == Some(field) {
                spec.sort_descending = !spec.sort_descending;
            } else {
                spec.sort_field = Some(field.to_string());
                spec.sort_descending = false;
            }
        }
        self.master_sync.borrow_mut().remove(view_id);
        self.sync_master(view_id);
    }

    pub fn table_len(&self, view_id: &str) -> usize {
        if self.is_master_table(view_id) {
            self.sync_master(view_id);
            let entity = self.spec(view_id).map(|spec| spec.entity.clone()).unwrap_or_default();
            return self.backend.master_total(&entity);
        }
        self.rows_cached(view_id).len()
    }

    fn begin_new(&mut self, kind: ViewKind) {
        if kind == ViewKind::Tree && self.entity_choices(kind, "").is_empty() {
            self.status = "This model has no hierarchical entities".into();
            return;
        }
        self.pending_dock = self.file.focus.clone();
        self.palette = PaletteState {
            open: true,
            edit: TextEdit::new(),
            cursor: 0,
            mode: PaletteMode::PickEntity(kind),
        };
        self.menu = None;
    }

    fn begin_open(&mut self) {
        if self.open_view_choices("").is_empty() {
            self.status = "This model has no entities".into();
            return;
        }
        self.pending_dock = self.file.focus.clone();
        self.palette = PaletteState {
            open: true,
            edit: TextEdit::new(),
            cursor: 0,
            mode: PaletteMode::OpenView,
        };
        self.menu = None;
    }

    fn begin_link(&mut self, view: Option<String>) {
        let Some(view) = view else {
            self.status = "No focused view".into();
            return;
        };
        if self.link_choices(&view, "").is_empty() {
            self.status = "No open view can drive this one".into();
            return;
        }
        self.palette = PaletteState {
            open: true,
            edit: TextEdit::new(),
            cursor: 0,
            mode: PaletteMode::PickLink(view),
        };
    }

    pub fn open_view(&mut self, kind: ViewKind, entity: &str, dock_on: Option<String>) {
        let Some(definition) = self.backend.model().entity(entity).cloned() else {
            self.status = format!("Unknown entity {entity}");
            return;
        };
        let id = format!(
            "{}-{}-{}",
            match kind {
                ViewKind::Table => "table",
                ViewKind::Detail => "detail",
                ViewKind::Tree => "tree",
                ViewKind::Graph => "graph",
            },
            entity,
            self.file.views.len() + 1
        );
        let mut link = None;
        if let Some(source_id) = self.file.focus.clone() {
            if let Some(source) = self.spec(&source_id) {
                let options = self
                    .backend
                    .model()
                    .link_options(&source.entity, entity, kind);
                if options.len() == 1 {
                    link = Some(ViewLink {
                        source: source_id,
                        relationship: options[0].relationship.clone(),
                    });
                }
            }
        }
        let columns = if kind == ViewKind::Table {
            definition.default_columns()
        } else {
            Vec::new()
        };
        let target = dock_on.or_else(|| self.file.focus.clone());
        if let Some(target) = target {
            if self.file.layout.contains(&target) {
                self.file.layout.insert(&target, &id, Zone::Center);
            } else {
                self.file.layout.dock_edge(&id, Zone::Right, 0.34);
            }
        } else if self.file.layout.views().is_empty() {
            self.file.layout = Node::tabs(&id);
        } else {
            self.file.layout.dock_edge(&id, Zone::Right, 0.34);
        }
        self.file.views.push(ViewSpec {
            id: id.clone(),
            kind,
            entity: entity.to_string(),
            columns,
            link,
            query: String::new(),
            sort_field: None,
            sort_descending: false,
        });
        self.file.layout.activate(&id);
        self.file.focus = Some(id.clone());
        self.pending_dock = None;
        self.reconcile();
        self.status = format!("Opened {}", self.view_title(&id));
    }

    fn close_focused(&mut self) {
        let Some(id) = self.file.focus.clone() else {
            self.status = "No focused view".into();
            return;
        };
        self.close_view(&id);
    }

    pub fn close_view(&mut self, id: &str) {
        self.file.layout.remove(id);
        self.file.views.retain(|view| view.id != id);
        for view in &mut self.file.views {
            if view.link.as_ref().is_some_and(|link| link.source == id) {
                view.link = None;
            }
        }
        self.file.selections.remove(id);
        if self.chooser_for.as_deref() == Some(id) {
            self.chooser_for = None;
        }
        if self.file.zoom.as_deref() == Some(id) {
            self.file.zoom = None;
        }
        let next = self.file.layout.views().into_iter().next();
        self.file.focus = next.clone();
        if let Some(focus) = next {
            self.file.layout.activate(&focus);
        }
        if self.graph_view.as_deref() == Some(id) {
            self.graph_view = None;
            self.graph = None;
        }
        self.reconcile();
        self.status = "View closed".into();
    }

    fn zoom_focused(&mut self) {
        let Some(id) = self.file.focus.clone() else { return };
        if self.file.zoom.as_deref() == Some(&id) {
            self.file.zoom = None;
            self.status = "Restored layout".into();
        } else {
            self.file.zoom = Some(id);
            self.status = "View maximized".into();
        }
    }

    fn focus_delta(&mut self, delta: i32) {
        let views = self.file.layout.views();
        if views.is_empty() {
            return;
        }
        let current = self
            .file
            .focus
            .as_ref()
            .and_then(|id| views.iter().position(|view| view == id))
            .unwrap_or(0);
        let next = (current as i32 + delta).rem_euclid(views.len() as i32) as usize;
        self.file.focus = Some(views[next].clone());
        self.file.layout.activate(&views[next]);
    }

    fn move_focused(&mut self, zone: Zone) {
        let Some(id) = self.file.focus.clone() else {
            self.status = "No focused view".into();
            return;
        };
        match self.file.layout.move_dir(&id, zone) {
            Ok(kind) => self.status = format!("Moved view ({kind})"),
            Err(err) => self.status = err,
        }
    }

    pub fn toggle_chooser(&mut self, view: Option<String>) {
        let Some(view) = view else {
            self.status = "No focused view".into();
            return;
        };
        let kind = self.spec(&view).map(|spec| spec.kind);
        if kind != Some(ViewKind::Table) {
            self.status = "Field chooser is for table views".into();
            return;
        }
        if self.chooser_for.as_deref() == Some(&view) {
            self.chooser_for = None;
            self.chooser_filter.clear();
        } else {
            self.chooser_for = Some(view);
            self.chooser_filter.clear();
        }
    }

    pub fn toggle_column(&mut self, view_id: &str, field: &str) {
        let Some(view) = self.file.views.iter_mut().find(|view| view.id == view_id) else { return };
        if view.columns.iter().any(|column| column == field) {
            if view.columns.len() == 1 {
                return;
            }
            view.columns.retain(|column| column != field);
        } else {
            view.columns.push(field.to_string());
        }
    }

    pub fn set_link(&mut self, view: &str, source: &str, relationship: &str) {
        if let Some(spec) = self.file.views.iter_mut().find(|spec| spec.id == view) {
            spec.link = Some(ViewLink {
                source: source.to_string(),
                relationship: relationship.to_string(),
            });
        }
        self.reconcile();
        self.status = self.link_caption(view).unwrap_or_else(|| "Link updated".into());
    }

    fn clear_link(&mut self, view: Option<&str>) {
        let Some(view) = view else { return };
        if let Some(spec) = self.file.views.iter_mut().find(|spec| spec.id == view) {
            spec.link = None;
        }
        self.reconcile();
        self.status = "Source link cleared".into();
    }

    pub fn current_record(&self, view_id: &str) -> Option<Record> {
        let rows = self.rows(view_id);
        if let Some(selected) = self.file.selections.get(view_id) {
            if let Some(row) = rows.iter().find(|row| &row.id == selected) {
                return Some(row.clone());
            }
        }
        rows.into_iter().next()
    }

    pub fn tree_rows(&self, entity: &str) -> Vec<(usize, Record)> {
        let mut out = Vec::new();
        self.collect_tree(entity, None, 0, &mut out);
        out
    }

    fn collect_tree(&self, entity: &str, parent: Option<&str>, depth: usize, out: &mut Vec<(usize, Record)>) {
        let rows = match parent {
            None => self.backend.hierarchy_roots(entity),
            Some(id) => self.backend.hierarchy_children(entity, id),
        };
        for row in rows {
            let id = row.id.clone();
            out.push((depth, row));
            self.collect_tree(entity, Some(&id), depth + 1, out);
        }
    }

    pub fn select(&mut self, view: &str, record: &str) {
        self.file.focus = Some(view.to_string());
        self.file.layout.activate(view);
        self.file.selections.insert(view.to_string(), record.to_string());
        self.reconcile();
    }

    pub fn save(&self) -> Result<String, String> {
        self.file.save(&self.workspace_path)?;
        Ok(format!("Saved {}", self.workspace_path.display()))
    }

    fn reload_workspace(&mut self) {
        match WorkspaceFile::load(&self.workspace_path) {
            Ok(mut file) => {
                sanitize(&mut file, self.backend.model());
                self.file = file;
                self.reconcile();
                self.status = format!("Reloaded {}", self.workspace_path.display());
            }
            Err(err) => self.status = format!("Could not reload workspace: {err}"),
        }
    }

    fn reload_data(&mut self) {
        if self.backend.is_live() {
            match self.backend.restart() {
                Ok(()) => {
                    self.reconcile();
                    self.status = self.backend.connection();
                }
                Err(err) => self.status = format!("Reload failed: {err}"),
            }
            return;
        }
        let Some(path) = self.data_path.clone() else {
            self.status = "No record file to reload".into();
            return;
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => match self.backend.reload_file(&text) {
                Ok(()) => {
                    self.reconcile();
                    self.status = format!("Reloaded {}", path.display());
                }
                Err(err) => self.status = format!("Records rejected: {err}"),
            },
            Err(err) => self.status = format!("Could not read records: {err}"),
        }
    }

    fn edit_text(&mut self, edit: impl FnOnce(&mut TextEdit)) {
        if self.palette.open {
            edit(&mut self.palette.edit);
            self.palette.cursor = 0;
            self.refresh_search();
        } else if self.graph_query {
            edit(&mut self.graph_edit);
        } else if self.chooser_for.is_some() {
            edit(&mut self.chooser_filter);
        } else if self.filter_for.is_some() {
            edit(&mut self.filter_edit);
        }
    }

    pub fn type_char(&mut self, ch: char) {
        self.edit_text(|text| text.insert(ch));
    }

    pub fn backspace(&mut self) {
        self.edit_text(|text| text.backspace());
    }

    pub fn delete_forward(&mut self) {
        self.edit_text(|text| text.delete_forward());
    }

    pub fn move_left(&mut self, shift: bool) {
        self.edit_text(|text| text.move_left(shift));
    }

    pub fn move_right(&mut self, shift: bool) {
        self.edit_text(|text| text.move_right(shift));
    }

    pub fn move_home(&mut self, shift: bool) {
        self.edit_text(|text| text.home(shift));
    }

    pub fn move_end(&mut self, shift: bool) {
        self.edit_text(|text| text.end(shift));
    }

    pub fn select_all_text(&mut self) {
        self.edit_text(|text| text.select_all());
    }

    pub fn escape(&mut self) {
        if self.palette.open {
            self.palette.open = false;
        } else if self.graph_query {
            self.graph_query = false;
        } else if self.graph_focused() {
            self.graph.as_mut().map(|board| board.clear_selection());
        } else if self.chooser_for.is_some() {
            self.chooser_for = None;
            self.chooser_filter.clear();
        } else if self.filter_for.is_some() {
            self.filter_for = None;
            self.filter_edit.clear();
        } else if self.menu.is_some() {
            self.menu = None;
        } else if self.file.zoom.is_some() {
            self.file.zoom = None;
        }
    }
}

pub struct PaletteRow {
    pub title: String,
    pub shortcut: String,
    pub detail: String,
    pub header: bool,
    pub hit: Option<SearchHit>,
}

impl PaletteRow {
    fn from_action(action: &Action) -> Self {
        Self {
            title: action.title.to_string(),
            shortcut: action.shortcut.map(crate::actions::shortcut_label).unwrap_or_default(),
            detail: action.id.to_string(),
            header: false,
            hit: None,
        }
    }

    fn plain(title: String) -> Self {
        Self { title, shortcut: String::new(), detail: String::new(), header: false, hit: None }
    }

    fn header(title: String) -> Self {
        Self { title, shortcut: String::new(), detail: String::new(), header: true, hit: None }
    }

    fn hit(hit: &SearchHit) -> Self {
        Self {
            title: hit.label.clone(),
            shortcut: String::new(),
            detail: hit.detail.clone(),
            header: false,
            hit: Some(hit.clone()),
        }
    }
}

pub fn launch_from_args(args: &[String]) -> Result<Launch, String> {
    load_dotenv();
    let mut export: Option<PathBuf> = None;
    let mut model: Option<PathBuf> = None;
    let mut data: Option<PathBuf> = None;
    let mut values: Option<PathBuf> = None;
    let mut workspace: Option<PathBuf> = None;
    let mut backend_flag: Option<BackendMode> = None;
    let mut debug_dump: Option<PathBuf> = None;
    let mut check = false;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let take = |index: &mut usize| -> Result<PathBuf, String> {
            *index += 1;
            args.get(*index)
                .cloned()
                .map(PathBuf::from)
                .ok_or_else(|| format!("{flag} needs a path"))
        };
        match flag {
            "--export" => export = Some(take(&mut index)?),
            "--model" => model = Some(take(&mut index)?),
            "--data" => data = Some(take(&mut index)?),
            "--values" => values = Some(take(&mut index)?),
            "--workspace" => workspace = Some(take(&mut index)?),
            "--backend" => {
                index += 1;
                let value = args.get(index).ok_or_else(|| "--backend needs mock or live".to_string())?;
                backend_flag = Some(BackendMode::parse(value)?);
            }
            "--debug-dump" => debug_dump = Some(take(&mut index)?),
            "--check-login" => check = true,
            "--help" | "-h" => return Err(help_text()),
            other if other.starts_with('-') => return Err(format!("unknown argument {other}\n{}", help_text())),
            other => return Err(format!("unexpected argument {other}")),
        }
        index += 1;
    }
    let env = read_env().map_err(|err| err)?;
    let mode = backend_flag.unwrap_or(env.mode);
    let export = export.or_else(|| env.model_export.clone());
    let debug_dump = debug_dump.or_else(|| env.debug_dump.clone());
    if check || mode == BackendMode::Live {
        let config = live_config(&env, export.clone(), model.clone(), values.clone(), debug_dump.clone())?;
        if check {
            return match check_login(&config) {
                Ok(message) => Ok(Launch::Done { code: 0, message }),
                Err(err) => Ok(Launch::Done { code: 1, message: err.to_string() }),
            };
        }
        let backend = AppBackend::Live(idmc_backend::LiveBackend::open(config).map_err(|err| err.to_string())?);
        return Ok(Launch::Run(finish_session(backend, None, workspace)?));
    }
    let (backend, data_path) = if let Some(model_path) = model {
        let data_path = data.unwrap_or_else(|| sibling(&model_path, "data.json"));
        let backend = MockBackend::open_json(&model_path, &data_path).map_err(|err| err.to_string())?;
        (AppBackend::Mock(backend), Some(data_path))
    } else if let Some(export_path) = export {
        let backend = MockBackend::open_export(&export_path, values.as_deref(), data.as_deref())
            .map_err(|err| err.to_string())?;
        (AppBackend::Mock(backend), data)
    } else {
        let mock = idmc_backend::find_mock_dir().ok_or_else(|| {
            "could not find mock/. Run from the repository, or pass --export / --model.".to_string()
        })?;
        let backend = MockBackend::open_default().map_err(|err| err.to_string())?;
        (AppBackend::Mock(backend), Some(mock.join("data.json")))
    };
    Ok(Launch::Run(finish_session(backend, data_path, workspace)?))
}

fn live_config(
    env: &idmc_backend::EnvConfig,
    export: Option<PathBuf>,
    model: Option<PathBuf>,
    values: Option<PathBuf>,
    debug_dump: Option<PathBuf>,
) -> Result<LiveConfig, String> {
    let login_url = env.login_url.clone().ok_or("Set IDMC_LOGIN_URL in .env")?;
    let username = env.username.clone().ok_or("Set IDMC_USERNAME in .env")?;
    let password = env.password.clone().ok_or("Set IDMC_PASSWORD in .env")?;
    let values = values.or_else(|| {
        if export.is_some() || model.is_some() {
            None
        } else {
            idmc_backend::find_mock_dir().map(|dir| dir.join("reference-values.json"))
        }
    });
    Ok(LiveConfig {
        login_url,
        username,
        password,
        export: if model.is_some() { None } else { export },
        model_json: model,
        values,
        debug_dump,
    })
}

fn finish_session(
    backend: AppBackend,
    data_path: Option<PathBuf>,
    workspace: Option<PathBuf>,
) -> Result<Session, String> {
    let workspace_path = workspace.unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("workspace.json")
    });
    let mut file = if workspace_path.is_file() {
        match WorkspaceFile::load(&workspace_path) {
            Ok(mut loaded) => {
                sanitize(&mut loaded, backend.model());
                loaded
            }
            Err(_) => default_workspace(backend.model()),
        }
    } else {
        default_workspace(backend.model())
    };
    if file.views.is_empty() {
        file = default_workspace(backend.model());
    }
    let mut session = Session::boot(backend, file, workspace_path, data_path);
    session.reconcile();
    session.status = if session.backend.is_live() {
        session.backend.connection()
    } else {
        session.backend.model().summary()
    };
    Ok(session)
}

fn sibling(path: &Path, name: &str) -> PathBuf {
    path.parent().unwrap_or(Path::new(".")).join(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use idmc_backend::{find_mock_dir, Backend};

    #[test]
    fn selected_person_drives_address_and_phone_rows() {
        let mock = find_mock_dir().expect("mock dir");
        let backend = AppBackend::Mock(MockBackend::open_default().unwrap());
        let file = default_workspace(backend.model());
        let mut session = Session::boot(
            backend,
            file,
            mock.join("workspace.json"),
            Some(mock.join("data.json")),
        );
        session.reconcile();
        let people = session.rows("table-c360.person");
        assert!(people.len() >= 2, "expected several persons");
        let first = people[0].id.clone();
        session.select("table-c360.person", &first);
        let addresses = session.rows("table-c360.person.PostalAddress");
        assert!(!addresses.is_empty(), "selected person should have addresses");
        assert!(addresses.iter().all(|row| row.id.starts_with(&format!("{first}::"))));
        let detail = session.current_record("detail-c360.person").unwrap();
        assert_eq!(detail.id, first);
        let label = session.backend.label("c360.person", &first);
        assert!(label.contains(' '), "description template joins first and last name: {label}");

        let second = people[1].id.clone();
        session.select("table-c360.person", &second);
        let next_addresses = session.rows("table-c360.person.PostalAddress");
        assert!(next_addresses.iter().all(|row| row.id.starts_with(&format!("{second}::"))));
        assert_ne!(addresses[0].id, next_addresses.first().map(|r| r.id.as_str()).unwrap_or(""));
    }

    #[test]
    fn palette_edits_text_and_lists_open_view() {
        let backend = AppBackend::Mock(MockBackend::open_default().unwrap());
        let file = default_workspace(backend.model());
        let mut session = Session::boot(backend, file, PathBuf::from("workspace.json"), None);
        session.perform("palette.open");
        session.type_char('o');
        session.type_char('p');
        session.move_home(false);
        session.move_right(false);
        session.type_char(' ');
        assert_eq!(session.palette.query(), "o p");
        session.move_end(false);
        session.backspace();
        assert_eq!(session.palette.query(), "o ");
        session.select_all_text();
        session.type_char('a');
        assert_eq!(session.palette.query(), "a");
        session.move_home(false);
        session.delete_forward();
        assert!(session.palette.query().is_empty());
        session.perform("view.open");
        let choices = session.open_view_choices("");
        assert!(choices.iter().any(|(kind, id, _)| *kind == ViewKind::Table && id == "c360.person.PostalAddress"));
        assert!(choices.iter().any(|(kind, id, _)| *kind == ViewKind::Detail && id == "c360.person"));
        assert!(choices.iter().all(|(kind, id, _)| !(*kind == ViewKind::Tree && id == "c360.person")));
        assert!(session.palette_rows().iter().any(|row| row.title.contains("Open view") || row.title.starts_with("Table")));
    }

    #[test]
    fn palette_searches_records_and_opens_the_match() {
        let backend = AppBackend::Mock(MockBackend::open_default().unwrap());
        let file = default_workspace(backend.model());
        let mut session = Session::boot(backend, file, PathBuf::from("workspace.json"), None);
        assert!(crate::actions::registry().iter().any(|action| action.id == "record.search"));
        session.perform("record.search");
        assert!(session.palette.open);
        for ch in "Anna".chars() {
            session.type_char(ch);
        }
        let rows = session.palette_rows();
        assert!(rows.iter().any(|row| row.header && row.title == "Person"), "{:?}", rows.iter().map(|row| row.title.clone()).collect::<Vec<_>>());
        let index = rows.iter().position(|row| row.hit.as_ref().is_some_and(|hit| hit.label.contains("Anna"))).expect("hit");
        let hit = rows[index].hit.clone().unwrap();
        assert!(!hit.detail.is_empty());
        session.palette.cursor = index;
        session.confirm_palette();
        assert!(!session.palette.open);
        assert!(session.file.focus.as_deref().unwrap_or("").starts_with("detail-"));
        assert!(session.file.selections.values().any(|id| id == &hit.id));
    }

    #[test]
    fn workspaces_duplicate_rename_and_close() {
        let backend = AppBackend::Mock(MockBackend::open_default().unwrap());
        let file = default_workspace(backend.model());
        let mut session = Session::boot(backend, file, PathBuf::from("workspace.json"), None);
        assert_eq!(session.file.tabs.len(), 1);
        session.perform("workspace.duplicate");
        assert_eq!(session.file.tabs.len(), 2);
        assert!(session.file.name.ends_with("copy"));
        session.perform("workspace.rename");
        session.select_all_text();
        for ch in "Desk".chars() {
            session.type_char(ch);
        }
        session.confirm_palette();
        assert_eq!(session.file.name, "Desk");
        session.perform("workspace.new");
        assert!(session.palette_rows().iter().any(|row| row.title == "Data quality"));
        session.palette.cursor = session.palette_rows().iter().position(|row| row.title == "Search & inspect").unwrap();
        session.confirm_palette();
        assert_eq!(session.file.name, "Search & inspect");
        assert!(session.file.views.iter().any(|view| view.kind == ViewKind::Detail));
        session.perform("workspace.close");
        assert_eq!(session.file.name, "Desk");
        session.perform("workspace.close");
        session.perform("workspace.close");
        assert_eq!(session.file.tabs.len(), 1);
    }
}

fn help_text() -> String {
    "idmc-client [--export DIR|ZIP] [--model FILE --data FILE] [--values FILE] [--workspace FILE]\n\
     [--backend mock|live] [--check-login] [--debug-dump FILE]\n\
     Default model: mock/idmc-export/person (Customer 360 Person). Default backend: mock.\n\
     Live reads use IDMC_LOGIN_URL, IDMC_USERNAME, and IDMC_PASSWORD from .env.\n\
     Second sample: --export mock/idmc-export/item\n\
     Hand-written model: --model mock/sample/model.json"
        .into()
}
