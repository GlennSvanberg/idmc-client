//! Workspace session: which views exist, what is selected, and how links resolve.
//! No GPUI types, so the behavior can be tested without a window.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use idmc_backend::{
    check_login, load_dotenv, project_children, read_env, AppBackend, Backend, BackendMode, LiveConfig,
    MockBackend, SearchHit, WriteKind, WriteOp,
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
struct PaletteFrame {
    mode: PaletteMode,
    edit: TextEdit,
    cursor: usize,
}

pub struct PaletteState {
    pub open: bool,
    pub edit: TextEdit,
    pub cursor: usize,
    pub mode: PaletteMode,
    stack: Vec<PaletteFrame>,
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
            stack: Vec::new(),
        }
    }

    pub fn open_commands(&mut self) {
        *self = Self {
            open: true,
            edit: TextEdit::new(),
            cursor: 0,
            mode: PaletteMode::Commands,
            stack: Vec::new(),
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
    /// Grow toward the left when the anchor sits on the right edge.
    pub grow_left: bool,
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
    OpenLinked {
        source: String,
        record: String,
        kind: ViewKind,
        entity: String,
        relationship: String,
    },
    Dismiss,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellCursor {
    pub view: String,
    pub row: usize,
    pub col: usize,
}

#[derive(Clone, Debug)]
pub struct ColumnResize {
    pub view: String,
    pub field: String,
    pub origin_x: f32,
    pub origin_width: f32,
}

pub const MIN_COLUMN_WIDTH: f32 = 64.;
pub const DEFAULT_COLUMN_WIDTH: f32 = 148.;

#[derive(Clone, Debug)]
pub struct MockSource {
    pub export: Option<PathBuf>,
    pub model: Option<PathBuf>,
    pub data: Option<PathBuf>,
    pub values: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowTarget {
    pub label: String,
    pub entity: String,
    pub relationship: String,
    pub kind: ViewKind,
}

struct CachedRows {
    generation: u64,
    revision: u64,
    source: Option<String>,
    rows: Arc<Vec<Record>>,
}

struct CellCache {
    generation: u64,
    revision: u64,
    values: HashMap<(String, String, String), String>,
}

#[derive(Clone, Debug)]
pub struct CellEdit {
    pub view: String,
    pub record_id: String,
    pub entity: String,
    pub field: String,
    pub edit: TextEdit,
    pub choices: Vec<(String, String)>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditMove {
    Stay,
    NextField,
    PrevField,
    Down,
    Up,
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
    pub editing: Option<CellEdit>,
    pub drafts: HashMap<String, Record>,
    draft_rev: u64,
    new_seq: u64,
    pub pending_delete: Option<(String, String, String)>,
    pub cell: Option<CellCursor>,
    pub scroll_cell: std::cell::Cell<bool>,
    pub page_rows: usize,
    pub column_resize: Option<ColumnResize>,
    pub column_drop: Option<(String, usize)>,
    pub fallback: Option<MockSource>,
    login_line: Option<String>,
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
                revision: 0,
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
            editing: None,
            drafts: HashMap::new(),
            draft_rev: 0,
            new_seq: 0,
            pending_delete: None,
            cell: None,
            scroll_cell: std::cell::Cell::new(false),
            page_rows: 12,
            column_resize: None,
            column_drop: None,
            fallback: None,
            login_line: None,
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
            if hit.generation == generation && hit.revision == self.draft_rev && hit.source == source {
                return Arc::clone(&hit.rows);
            }
        }
        let rows = self.fresh_rows(view_id);
        self.row_cache.borrow_mut().insert(
            view_id.to_string(),
            CachedRows {
                generation,
                revision: self.draft_rev,
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
            let mut rows = self.backend.shared_records(&spec.entity).as_ref().clone();
            self.merge_drafts(&spec.entity, &mut rows);
            return Arc::new(rows);
        }
        Arc::new(self.compute_rows(view_id))
    }

    fn merge_drafts(&self, entity: &str, rows: &mut Vec<Record>) {
        for row in rows.iter_mut() {
            if let Some(draft) = self.drafts.get(&draft_key(&row.entity, &row.id)) {
                *row = draft.clone();
            }
        }
        let mut extra = Vec::new();
        for draft in self.drafts.values() {
            if draft.entity == entity && draft.id.starts_with("new:") && !rows.iter().any(|row| row.id == draft.id) {
                extra.push(draft.clone());
            }
        }
        if !extra.is_empty() {
            extra.append(rows);
            *rows = extra;
        }
    }

    fn compute_rows(&self, view_id: &str) -> Vec<Record> {
        let Some(spec) = self.spec(view_id) else { return Vec::new() };
        if let Some(link) = &spec.link {
            let Some(selected) = self.file.selections.get(&link.source) else { return Vec::new() };
            if link.relationship == "identity" {
                return self
                    .backend
                    .record(&spec.entity, selected)
                    .map(|row| self.overlay_row(row))
                    .into_iter()
                    .collect();
            }
            if let Some(rel) = self.backend.model().relationship(&link.relationship).cloned() {
                if rel.embedded {
                    let key = draft_key(&rel.from, selected);
                    if let Some(parent) = self.drafts.get(&key) {
                        return idmc_backend::project_children(self.backend.model(), parent, &rel.to);
                    }
                }
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
            if cache.generation == generation && cache.revision == self.draft_rev {
                if let Some(hit) = cache.values.get(&key) {
                    return hit.clone();
                }
            }
        }
        let text = self.compute_cell(entity, field_id, record);
        let mut cache = self.cell_cache.borrow_mut();
        if cache.generation != generation || cache.revision != self.draft_rev {
            cache.values.clear();
            cache.generation = generation;
            cache.revision = self.draft_rev;
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
            "record.new" => self.new_record(),
            "record.new_child" => self.new_child(),
            "record.save" => self.save_records(),
            "record.revert" => self.revert_records(),
            "record.delete" => self.ask_delete(),
            "record.delete_confirm" => self.confirm_delete(),
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
            let rows = self.palette_rows();
            if rows.is_empty() {
                return;
            }
            let step = if delta >= 0 { 1 } else { -1 };
            let mut next = self.palette.cursor as i32;
            for _ in 0..rows.len() {
                next += step;
                if next < 0 || next >= rows.len() as i32 {
                    return;
                }
                if !rows[next as usize].header {
                    self.palette.cursor = next as usize;
                    return;
                }
            }
            return;
        }
        if self.editing.is_none() && self.focused_table().is_some() {
            self.move_cell(0, delta);
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
                if crate::actions::fuzzy_match(&q, &title)
                    || crate::actions::fuzzy_match(&q, &entity.id)
                    || crate::actions::fuzzy_match(&q, &entity.label)
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
                crate::actions::fuzzy_match(&q, &entity.label)
                    || crate::actions::fuzzy_match(&q, &entity.label_plural)
                    || crate::actions::fuzzy_match(&q, &entity.id)
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
                if crate::actions::fuzzy_match(&q, &label) {
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
                if is_drill(action.id) {
                    self.perform(action.id);
                    return Effect::None;
                }
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
                let actions = palette_actions(self.palette.query());
                let mut rows = Vec::new();
                if self.palette.query().trim().is_empty() {
                    for section in ["General", "Workspace", "Views", "Records", "Data"] {
                        let group: Vec<_> = actions.iter().copied().filter(|action| action_section(action.id) == section).collect();
                        if group.is_empty() {
                            continue;
                        }
                        rows.push(PaletteRow::header(section.to_string()));
                        rows.extend(group.into_iter().map(PaletteRow::from_action));
                    }
                } else {
                    rows.extend(actions.into_iter().map(PaletteRow::from_action));
                }
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
            .filter(|name| crate::actions::fuzzy_match(&q, name))
            .map(|name| (*name).to_string())
            .collect()
    }

    fn refresh_search(&mut self) {
        if !self.palette.open || !matches!(self.palette.mode, PaletteMode::Commands) {
            return;
        }
        let query = self.palette.query().to_string();
        if query.trim().is_empty() {
            self.search_hits.clear();
            return;
        }
        if self.backend.is_live() {
            self.backend.schedule_record_search(&query);
        } else {
            self.search_hits = self.backend.search_records(&query, 8);
        }
    }

    pub fn absorb_async(&mut self) -> bool {
        let mut changed = false;
        if self.absorb_writes() {
            changed = true;
        }
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
        let failure = self.backend.login_failure();
        let line = if self.backend.login_busy() {
            Some("Retrying login…".to_string())
        } else {
            failure.as_ref().map(|failure| format!("{} · {}", failure.cause.title(), failure.detail))
        };
        if line != self.login_line {
            self.login_line = line.clone();
            if let Some(line) = line {
                self.status = line;
            }
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
        self.enter_palette(PaletteMode::PickTemplate, TextEdit::new());
    }

    fn begin_rename(&mut self) {
        let name = self.file.name.clone();
        let mut edit = TextEdit::new();
        for ch in name.chars() {
            edit.insert(ch);
        }
        edit.select_all();
        self.enter_palette(PaletteMode::RenameWorkspace, edit);
    }

    fn enter_palette(&mut self, mode: PaletteMode, edit: TextEdit) {
        if self.palette.open {
            self.palette.stack.push(PaletteFrame {
                mode: self.palette.mode.clone(),
                edit: self.palette.edit.clone(),
                cursor: self.palette.cursor,
            });
        }
        self.palette.open = true;
        self.palette.mode = mode;
        self.palette.edit = edit;
        self.palette.cursor = 0;
        self.menu = None;
        self.land_palette_cursor();
    }

    pub fn palette_back(&mut self) {
        let Some(frame) = self.palette.stack.pop() else { return };
        self.palette.mode = frame.mode;
        self.palette.edit = frame.edit;
        self.palette.cursor = frame.cursor;
        self.refresh_search();
        self.land_palette_cursor();
    }

    pub fn palette_crumb(&self) -> Vec<String> {
        let mut names: Vec<String> = self.palette.stack.iter().map(|frame| palette_title(&frame.mode)).collect();
        names.push(palette_title(&self.palette.mode));
        names
    }

    pub fn drill_row(&self, row: &PaletteRow) -> bool {
        !row.header && row.hit.is_none() && is_drill(row.detail.as_str())
    }

    pub fn palette_key_right(&mut self) -> bool {
        if !self.palette.open {
            return false;
        }
        let at_end = self.palette.edit.caret == self.palette.edit.text.chars().count();
        if !at_end {
            return false;
        }
        let rows = self.palette_rows();
        let Some(row) = rows.get(self.palette.cursor) else { return false };
        if !self.drill_row(row) {
            return false;
        }
        let id = row.detail.clone();
        self.perform(&id);
        true
    }

    pub fn palette_key_back(&mut self) -> bool {
        if self.palette.open && self.palette.query().is_empty() && !self.palette.stack.is_empty() {
            self.palette_back();
            true
        } else {
            false
        }
    }

    fn land_palette_cursor(&mut self) {
        let rows = self.palette_rows();
        if rows.get(self.palette.cursor).is_some_and(|row| row.header) {
            if let Some(index) = rows.iter().position(|row| !row.header) {
                self.palette.cursor = index;
            }
        }
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
            return self.pending_count(&entity) + self.backend.master_total(&entity);
        }
        self.rows_cached(view_id).len()
    }

    fn begin_new(&mut self, kind: ViewKind) {
        if kind == ViewKind::Tree && self.entity_choices(kind, "").is_empty() {
            self.status = "This model has no hierarchical entities".into();
            return;
        }
        self.pending_dock = self.file.focus.clone();
        self.enter_palette(PaletteMode::PickEntity(kind), TextEdit::new());
    }

    fn begin_open(&mut self) {
        if self.open_view_choices("").is_empty() {
            self.status = "This model has no entities".into();
            return;
        }
        self.pending_dock = self.file.focus.clone();
        self.enter_palette(PaletteMode::OpenView, TextEdit::new());
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
        self.enter_palette(PaletteMode::PickLink(view), TextEdit::new());
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
            column_widths: BTreeMap::new(),
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
        if let Some(index) = self.row_index(view, record) {
            let col = self
                .cell
                .as_ref()
                .filter(|cursor| cursor.view == view)
                .map(|cursor| cursor.col)
                .unwrap_or(0);
            let cols = self.spec(view).map(|spec| spec.columns.len()).unwrap_or(1).max(1);
            self.cell = Some(CellCursor {
                view: view.to_string(),
                row: index,
                col: col.min(cols - 1),
            });
        }
        self.reconcile();
    }

    pub fn focused_table(&self) -> Option<String> {
        let id = self.file.focus.clone()?;
        let spec = self.spec(&id)?;
        (spec.kind == ViewKind::Table).then_some(id)
    }

    pub fn column_width(&self, view: &str, field: &str) -> f32 {
        self.spec(view)
            .and_then(|spec| spec.column_widths.get(field).copied())
            .unwrap_or(DEFAULT_COLUMN_WIDTH)
            .clamp(MIN_COLUMN_WIDTH, 640.)
    }

    pub fn set_column_width(&mut self, view: &str, field: &str, width: f32) {
        let width = width.clamp(MIN_COLUMN_WIDTH, 640.);
        if let Some(spec) = self.file.views.iter_mut().find(|spec| spec.id == view) {
            spec.column_widths.insert(field.to_string(), width);
        }
    }

    pub fn drop_column(&mut self, view: &str, field: &str, insert_at: usize) {
        let Some(spec) = self.file.views.iter_mut().find(|spec| spec.id == view) else { return };
        let Some(from) = spec.columns.iter().position(|column| column == field) else { return };
        let mut to = insert_at.min(spec.columns.len());
        let column = spec.columns.remove(from);
        if from < to {
            to -= 1;
        }
        let to = to.min(spec.columns.len());
        spec.columns.insert(to, column);
    }

    pub fn autofit_column(&mut self, view: &str, field: &str) {
        let Some(spec) = self.spec(view).cloned() else { return };
        let label = self
            .backend
            .model()
            .entity(&spec.entity)
            .and_then(|entity| entity.field(field))
            .map(|field| field.label.clone())
            .unwrap_or_else(|| field.to_string());
        let mut samples = Vec::new();
        let limit = self.table_len(view).min(40);
        for index in 0..limit {
            if let Some(row) = self.row_at(view, index) {
                samples.push(self.cell(&spec.entity, field, &row));
            }
        }
        self.set_column_width(view, field, fitted_width(&label, &samples));
    }

    pub fn move_cell(&mut self, dcol: i32, drow: i32) {
        let Some(view) = self.focused_table() else { return };
        let cols = self.spec(&view).map(|spec| spec.columns.len()).unwrap_or(0);
        let rows = self.table_len(&view);
        if cols == 0 || rows == 0 {
            return;
        }
        let current = self.cell.clone().filter(|cursor| cursor.view == view).unwrap_or(CellCursor {
            view: view.clone(),
            row: 0,
            col: 0,
        });
        let col = (current.col as i32 + dcol).clamp(0, cols as i32 - 1) as usize;
        let row = (current.row as i32 + drow).clamp(0, rows as i32 - 1) as usize;
        self.cell = Some(CellCursor { view: view.clone(), row, col });
        if let Some(id) = self.row_at(&view, row).map(|record| record.id) {
            self.file.focus = Some(view.clone());
            self.file.layout.activate(&view);
            self.file.selections.insert(view, id);
            self.reconcile();
        }
        self.scroll_cell.set(true);
    }

    pub fn cell_home(&mut self, document: bool) {
        let Some(view) = self.focused_table() else { return };
        let cols = self.spec(&view).map(|spec| spec.columns.len()).unwrap_or(0);
        if cols == 0 {
            return;
        }
        if document {
            self.cell = Some(CellCursor { view: view.clone(), row: 0, col: 0 });
        } else if let Some(cursor) = self.cell.as_mut().filter(|cursor| cursor.view == view) {
            cursor.col = 0;
        } else {
            self.cell = Some(CellCursor { view: view.clone(), row: 0, col: 0 });
        }
        self.sync_cell_selection();
    }

    pub fn cell_end(&mut self, document: bool) {
        let Some(view) = self.focused_table() else { return };
        let cols = self.spec(&view).map(|spec| spec.columns.len()).unwrap_or(0);
        let rows = self.table_len(&view);
        if cols == 0 || rows == 0 {
            return;
        }
        if document {
            self.cell = Some(CellCursor { view: view.clone(), row: rows - 1, col: cols - 1 });
        } else if let Some(cursor) = self.cell.as_mut().filter(|cursor| cursor.view == view) {
            cursor.col = cols - 1;
        } else {
            self.cell = Some(CellCursor { view: view.clone(), row: 0, col: cols - 1 });
        }
        self.sync_cell_selection();
    }

    pub fn cell_page(&mut self, delta: i32) {
        let step = delta.signum() * self.page_rows.max(1) as i32;
        self.move_cell(0, step);
    }

    pub fn edit_focused_cell(&mut self) {
        let Some(cursor) = self.cell.clone() else { return };
        if self.focused_table().as_deref() != Some(cursor.view.as_str()) {
            return;
        }
        let Some(field) = self.spec(&cursor.view).and_then(|spec| spec.columns.get(cursor.col)).cloned() else {
            return;
        };
        let Some(record) = self.row_at(&cursor.view, cursor.row) else {
            self.status = "That row is not loaded".into();
            return;
        };
        self.begin_edit(&cursor.view, &record.id, &field);
    }

    fn sync_cell_selection(&mut self) {
        let Some(cursor) = self.cell.clone() else { return };
        if let Some(id) = self.row_at(&cursor.view, cursor.row).map(|record| record.id) {
            self.file.focus = Some(cursor.view.clone());
            self.file.layout.activate(&cursor.view);
            self.file.selections.insert(cursor.view, id);
            self.reconcile();
        }
        self.scroll_cell.set(true);
    }

    fn row_index(&self, view: &str, record: &str) -> Option<usize> {
        let total = self.table_len(view);
        let scan = total.min(400);
        (0..scan).find(|index| self.row_at(view, *index).is_some_and(|row| row.id == record))
    }

    fn row_at(&self, view: &str, index: usize) -> Option<Record> {
        if self.is_master_table(view) {
            let entity = self.spec(view)?.entity.clone();
            return self.master_row(&entity, index);
        }
        self.rows_cached(view).get(index).cloned()
    }

    pub fn row_targets(&self, entity: &str) -> Vec<RowTarget> {
        let model = self.backend.model();
        let mut out = Vec::new();
        for child in model.embedded_children(entity) {
            let Some(rel) = model.relationships.iter().find(|rel| rel.embedded && rel.from == entity && rel.to == child.id) else {
                continue;
            };
            let label = if child.label_plural.is_empty() { child.label.clone() } else { child.label_plural.clone() };
            out.push(RowTarget {
                label,
                entity: child.id.clone(),
                relationship: rel.id.clone(),
                kind: ViewKind::Table,
            });
        }
        for rel in &model.relationships {
            if rel.embedded || rel.from != entity {
                continue;
            }
            if model.entity(&rel.to).and_then(|entry| entry.embedded_in.as_ref()).is_some() {
                continue;
            }
            let kind = match rel.cardinality {
                idmc_model::Cardinality::OneToMany => ViewKind::Table,
                idmc_model::Cardinality::ManyToOne | idmc_model::Cardinality::OneToOne => ViewKind::Detail,
            };
            let label = if rel.label.is_empty() {
                model.entity(&rel.to).map(|entry| entry.label.clone()).unwrap_or_else(|| rel.to.clone())
            } else {
                rel.label.clone()
            };
            out.push(RowTarget {
                label,
                entity: rel.to.clone(),
                relationship: rel.id.clone(),
                kind,
            });
        }
        out
    }

    pub fn open_linked_view(&mut self, source: &str, kind: ViewKind, entity: &str, relationship: &str) {
        let source = source.to_string();
        self.file.focus = Some(source.clone());
        let before = self.file.views.len();
        self.open_view(kind, entity, Some(source.clone()));
        let Some(created) = self.file.views.get(before).map(|view| view.id.clone()) else { return };
        self.set_link(&created, &source, relationship);
    }

    pub fn reveal_offset(offset_x: f32, view_width: f32, cell_left: f32, cell_width: f32) -> f32 {
        if view_width <= 1. {
            return offset_x;
        }
        let visible_left = -offset_x;
        let visible_right = visible_left + view_width;
        let cell_right = cell_left + cell_width;
        if cell_left < visible_left {
            -cell_left
        } else if cell_right > visible_right {
            -(cell_right - view_width).max(0.)
        } else {
            offset_x
        }
    }

    pub fn continue_with_mock(&mut self) -> Result<(), String> {
        let source = self.fallback.clone().unwrap_or(MockSource {
            export: None,
            model: None,
            data: None,
            values: None,
        });
        let (backend, data_path) = open_mock(&source)?;
        self.backend = backend;
        if data_path.is_some() {
            self.data_path = data_path;
        }
        self.login_line = None;
        self.row_cache.borrow_mut().clear();
        self.seen_generation = u64::MAX;
        self.reconcile();
        self.status = "Showing mock data".into();
        Ok(())
    }

    pub fn retry_login(&mut self) {
        self.backend.retry_login();
        self.status = "Retrying login…".into();
    }

    pub fn dirty_records(&self) -> bool {
        !self.drafts.is_empty() || self.editing.is_some()
    }

    pub fn pending_count(&self, entity: &str) -> usize {
        self.drafts
            .values()
            .filter(|row| row.entity == entity && row.id.starts_with("new:"))
            .count()
    }

    pub fn master_row(&self, entity: &str, index: usize) -> Option<Record> {
        let mut pending: Vec<Record> = self
            .drafts
            .values()
            .filter(|row| row.entity == entity && row.id.starts_with("new:"))
            .cloned()
            .collect();
        pending.sort_by(|left, right| left.id.cmp(&right.id));
        if index < pending.len() {
            return Some(pending[index].clone());
        }
        let row = self.backend.master_at(entity, index - pending.len())?;
        Some(self.overlay_row(row))
    }

    fn overlay_row(&self, row: Record) -> Record {
        self.drafts.get(&draft_key(&row.entity, &row.id)).cloned().unwrap_or(row)
    }

    pub fn begin_edit(&mut self, view: &str, record_id: &str, field_id: &str) {
        let Some(entity) = self.spec(view).map(|spec| spec.entity.clone()) else { return };
        let Some(field) = self.backend.model().entity(&entity).and_then(|entry| entry.field(field_id)).cloned() else {
            return;
        };
        if field.system || field.ty == FieldType::Group || field.many {
            self.status = format!("{} is not edited in a cell", field.label);
            return;
        }
        let Some(record) = self.record_for_edit(&entity, record_id) else {
            self.status = "That row is not loaded".into();
            return;
        };
        let text = idmc_model::edit_text(self.backend.model(), &field, idmc_model::read_field(&record, &field));
        let mut edit = TextEdit::new();
        for ch in text.chars() {
            edit.insert(ch);
        }
        let choices = idmc_model::lookup_choices(self.backend.model(), field.lookup.as_deref());
        self.file.focus = Some(view.to_string());
        self.editing = Some(CellEdit {
            view: view.to_string(),
            record_id: record_id.to_string(),
            entity,
            field: field_id.to_string(),
            edit,
            choices,
            error: None,
        });
    }

    fn record_for_edit(&self, entity: &str, id: &str) -> Option<Record> {
        if let Some(draft) = self.drafts.get(&draft_key(entity, id)) {
            return Some(draft.clone());
        }
        if self.backend.model().entity(entity).and_then(|entry| entry.embedded_in.clone()).is_some() {
            if let Some((parent_id, _, _)) = idmc_model::split_child_id(id) {
                let parent_entity = self.backend.model().entity(entity)?.embedded_in.as_ref()?.parent.clone();
                let parent = self
                    .drafts
                    .get(&draft_key(&parent_entity, &parent_id))
                    .cloned()
                    .or_else(|| self.backend.record(&parent_entity, &parent_id))?;
                return project_children(self.backend.model(), &parent, entity)
                    .into_iter()
                    .find(|row| row.id == id);
            }
        }
        self.backend.record(entity, id).or_else(|| {
            self.drafts.values().find(|row| row.entity == entity && row.id == id).cloned()
        })
    }

    pub fn commit_edit(&mut self, next: EditMove) -> bool {
        let Some(edit) = self.editing.clone() else { return true };
        let Some(field) = self.backend.model().entity(&edit.entity).and_then(|entry| entry.field(&edit.field)).cloned() else {
            self.editing = None;
            return true;
        };
        let value = match idmc_model::parse_field(&field, &edit.edit.text, self.backend.model()) {
            Ok(value) => value,
            Err(message) => {
                if let Some(slot) = self.editing.as_mut() {
                    slot.error = Some(message.clone());
                }
                self.status = message;
                return false;
            }
        };
        if let Err(message) = self.write_field(&edit.entity, &edit.record_id, &field, value) {
            self.status = message;
            return false;
        }
        self.editing = None;
        if next != EditMove::Stay {
            self.open_neighbor(&edit, next);
        }
        true
    }

    pub fn cancel_edit(&mut self) {
        self.editing = None;
    }

    fn write_field(&mut self, entity: &str, record_id: &str, field: &FieldDef, value: FieldValue) -> Result<(), String> {
        if let Some(embedded) = self.backend.model().entity(entity).and_then(|entry| entry.embedded_in.clone()) {
            let (parent_id, group, index) = idmc_model::split_child_id(record_id).ok_or("child row id is not usable")?;
            let mut parent = self
                .drafts
                .get(&draft_key(&embedded.parent, &parent_id))
                .cloned()
                .or_else(|| self.backend.record(&embedded.parent, &parent_id))
                .ok_or_else(|| "parent record is not loaded".to_string())?;
            if !idmc_model::set_child_field(&mut parent, &group, index, &field.name, value) {
                return Err("that child row is gone".into());
            }
            self.remember_draft(parent);
            return Ok(());
        }
        let mut record = self
            .drafts
            .get(&draft_key(entity, record_id))
            .cloned()
            .or_else(|| self.backend.record(entity, record_id))
            .ok_or_else(|| "record is not loaded".to_string())?;
        if matches!(value, FieldValue::Null) {
            record.values.remove(&field.name);
        } else {
            record.values.insert(field.name.clone(), value);
        }
        self.remember_draft(record);
        Ok(())
    }

    fn remember_draft(&mut self, record: Record) {
        self.drafts.insert(draft_key(&record.entity, &record.id), record);
        self.draft_rev = self.draft_rev.wrapping_add(1);
    }

    fn open_neighbor(&mut self, edit: &CellEdit, next: EditMove) {
        let fields = self.edit_fields(&edit.view, &edit.entity);
        let rows = self.edit_rows(&edit.view);
        if fields.is_empty() || rows.is_empty() {
            return;
        }
        let field_at = fields.iter().position(|id| id == &edit.field).unwrap_or(0);
        let row_at = rows.iter().position(|id| id == &edit.record_id).unwrap_or(0);
        let (field_at, row_at) = match next {
            EditMove::Stay => return,
            EditMove::NextField => ((field_at + 1).min(fields.len() - 1), row_at),
            EditMove::PrevField => (field_at.saturating_sub(1), row_at),
            EditMove::Down => (field_at, (row_at + 1).min(rows.len() - 1)),
            EditMove::Up => (field_at, row_at.saturating_sub(1)),
        };
        let field = &fields[field_at];
        let row = &rows[row_at];
        if field == &edit.field && row == &edit.record_id {
            return;
        }
        self.begin_edit(&edit.view, row, field);
    }

    fn edit_fields(&self, view: &str, entity: &str) -> Vec<String> {
        if let Some(spec) = self.spec(view) {
            if spec.kind == ViewKind::Table && !spec.columns.is_empty() {
                return spec.columns.iter().filter(|column| self.field_editable(entity, column)).cloned().collect();
            }
        }
        self.backend
            .model()
            .entity(entity)
            .map(|entry| {
                entry
                    .fields
                    .iter()
                    .filter(|field| self.field_editable(entity, &field.id))
                    .map(|field| field.id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn field_editable(&self, entity: &str, field_id: &str) -> bool {
        self.backend.model().entity(entity).and_then(|entry| entry.field(field_id)).is_some_and(|field| {
            !field.system && field.ty != FieldType::Group && !field.many
        })
    }

    fn edit_rows(&self, view: &str) -> Vec<String> {
        if self.is_master_table(view) {
            let entity = self.spec(view).map(|spec| spec.entity.clone()).unwrap_or_default();
            let total = self.pending_count(&entity) + self.backend.master_total(&entity);
            return (0..total).filter_map(|index| self.master_row(&entity, index).map(|row| row.id)).collect();
        }
        self.visible_ids(view)
    }

    pub fn edit_insert(&mut self, ch: char) {
        if let Some(edit) = self.editing.as_mut() {
            edit.edit.insert(ch);
            edit.error = None;
        }
    }

    pub fn edit_backspace(&mut self) {
        if let Some(edit) = self.editing.as_mut() {
            edit.edit.backspace();
            edit.error = None;
        }
    }

    pub fn edit_delete_forward(&mut self) {
        if let Some(edit) = self.editing.as_mut() {
            edit.edit.delete_forward();
        }
    }

    pub fn edit_left(&mut self, shift: bool) {
        if let Some(edit) = self.editing.as_mut() {
            edit.edit.move_left(shift);
        }
    }

    pub fn edit_right(&mut self, shift: bool) {
        if let Some(edit) = self.editing.as_mut() {
            edit.edit.move_right(shift);
        }
    }

    pub fn edit_home(&mut self, shift: bool) {
        if let Some(edit) = self.editing.as_mut() {
            edit.edit.home(shift);
        }
    }

    pub fn edit_end(&mut self, shift: bool) {
        if let Some(edit) = self.editing.as_mut() {
            edit.edit.end(shift);
        }
    }

    pub fn cycle_choice(&mut self, delta: i32) {
        let Some(edit) = self.editing.as_mut() else { return };
        if edit.choices.is_empty() {
            return;
        }
        let current = edit
            .choices
            .iter()
            .position(|(code, label)| label.eq_ignore_ascii_case(&edit.edit.text) || code.eq_ignore_ascii_case(&edit.edit.text))
            .unwrap_or(0);
        let len = edit.choices.len() as i32;
        let next = (current as i32 + delta).rem_euclid(len) as usize;
        let label = edit.choices[next].1.clone();
        edit.edit = TextEdit::new();
        for ch in label.chars() {
            edit.edit.insert(ch);
        }
        edit.error = None;
    }

    pub fn pick_choice(&mut self, index: usize) {
        let Some(edit) = self.editing.as_mut() else { return };
        let Some((_, label)) = edit.choices.get(index).cloned() else { return };
        edit.edit = TextEdit::new();
        for ch in label.chars() {
            edit.edit.insert(ch);
        }
        edit.error = None;
    }

    pub fn save_records(&mut self) {
        if self.editing.is_some() && !self.commit_edit(EditMove::Stay) {
            return;
        }
        let keys: Vec<String> = self.drafts.keys().cloned().collect();
        if keys.is_empty() {
            self.status = "Nothing to save".into();
            return;
        }
        for key in keys {
            let Some(record) = self.drafts.get(&key).cloned() else { continue };
            if let Err(err) = self.write_record(record) {
                self.status = err;
                return;
            }
        }
        self.draft_rev = self.draft_rev.wrapping_add(1);
        self.status = "Saved".into();
    }

    fn write_record(&mut self, record: Record) -> Result<(), String> {
        idmc_model::validate_edit(self.backend.model(), &record.entity, &record.values).map_err(|err| err.to_string())?;
        let exists = self.backend.record(&record.entity, &record.id).is_some();
        let kind = if record.id.starts_with("new:") || !exists {
            WriteKind::Create
        } else {
            WriteKind::Update
        };
        let source_pkey = if kind == WriteKind::Create {
            format!("idmc-{}", record.id.trim_start_matches("new:"))
        } else {
            String::new()
        };
        let before = self.backend.record(&record.entity, &record.id);
        let saved = self
            .backend
            .submit_write(WriteOp {
                kind,
                entity: record.entity.clone(),
                id: record.id.clone(),
                source_system: idmc_model::DEFAULT_SOURCE_SYSTEM.into(),
                source_pkey,
                values: record.values.clone(),
                before,
            })
            .map_err(|err| err.to_string())?;
        self.drafts.remove(&draft_key(&record.entity, &record.id));
        self.retarget_id(&record.id, &saved.id);
        Ok(())
    }

    fn retarget_id(&mut self, from: &str, to: &str) {
        if from == to {
            return;
        }
        for selection in self.file.selections.values_mut() {
            if selection == from {
                *selection = to.to_string();
            }
        }
    }

    pub fn revert_records(&mut self) {
        self.editing = None;
        self.drafts.clear();
        self.pending_delete = None;
        self.draft_rev = self.draft_rev.wrapping_add(1);
        self.status = "Reverted unsaved changes".into();
    }

    pub fn new_record(&mut self) {
        let Some(view) = self.file.focus.clone() else {
            self.status = "Focus a table or detail view first".into();
            return;
        };
        let Some(spec) = self.spec(&view).cloned() else { return };
        if self.backend.model().entity(&spec.entity).and_then(|entry| entry.embedded_in.clone()).is_some() {
            self.new_child();
            return;
        }
        if matches!(spec.kind, ViewKind::Graph | ViewKind::Tree) {
            self.status = "Create records from a table or a detail view".into();
            return;
        }
        self.new_seq = self.new_seq.wrapping_add(1);
        let id = format!("new:{}", self.new_seq);
        let record = Record {
            entity: spec.entity.clone(),
            id: id.clone(),
            values: std::collections::BTreeMap::new(),
        };
        self.remember_draft(record);
        self.select(&view, &id);
        if let Some(field) = self.edit_fields(&view, &spec.entity).into_iter().next() {
            self.begin_edit(&view, &id, &field);
        }
        self.status = "New record. Fill the required fields, then save.".into();
    }

    pub fn new_child(&mut self) {
        let Some(view) = self.file.focus.clone() else {
            self.status = "Focus a child table first".into();
            return;
        };
        let Some(spec) = self.spec(&view).cloned() else { return };
        let Some(embedded) = self.backend.model().entity(&spec.entity).and_then(|entry| entry.embedded_in.clone()) else {
            self.status = "This view is not a child table".into();
            return;
        };
        let Some(link) = spec.link.clone() else {
            self.status = "Link this child table to a parent row first".into();
            return;
        };
        let Some(parent_id) = self.file.selections.get(&link.source).cloned() else {
            self.status = "Select a parent row first".into();
            return;
        };
        let Some(mut parent) = self
            .drafts
            .get(&draft_key(&embedded.parent, &parent_id))
            .cloned()
            .or_else(|| self.backend.record(&embedded.parent, &parent_id))
        else {
            self.status = "The parent record is not loaded".into();
            return;
        };
        let index = idmc_model::push_child(&mut parent, &embedded.field, std::collections::BTreeMap::new());
        let child_id = idmc_model::child_record_id(&parent.id, &embedded.field, index);
        self.remember_draft(parent);
        self.select(&view, &child_id);
        if let Some(field) = self.edit_fields(&view, &spec.entity).into_iter().next() {
            self.begin_edit(&view, &child_id, &field);
        }
        self.status = "New child row. Save when the fields are ready.".into();
    }

    pub fn ask_delete(&mut self) {
        if self.pending_delete.is_some() {
            self.confirm_delete();
            return;
        }
        let Some(view) = self.file.focus.clone() else {
            self.status = "Focus a row first".into();
            return;
        };
        let Some(id) = self.file.selections.get(&view).cloned() else {
            self.status = "Select a row to delete".into();
            return;
        };
        let entity = self.spec(&view).map(|spec| spec.entity.clone()).unwrap_or_default();
        if id.starts_with("new:") {
            self.drafts.remove(&draft_key(&entity, &id));
            self.file.selections.remove(&view);
            self.editing = None;
            self.draft_rev = self.draft_rev.wrapping_add(1);
            self.status = "Discarded the new row".into();
            return;
        }
        let label = self.backend.label(&entity, &id);
        self.pending_delete = Some((entity, id, label));
        self.status = "Confirm delete to remove this record.".into();
    }

    pub fn confirm_delete(&mut self) {
        let Some((entity, id, label)) = self.pending_delete.take() else {
            self.status = "Nothing is waiting to be deleted".into();
            return;
        };
        if let Some(embedded) = self.backend.model().entity(&entity).and_then(|entry| entry.embedded_in.clone()) {
            let Some((parent_id, field, index)) = idmc_model::split_child_id(&id) else {
                self.status = "child row id is not usable".into();
                return;
            };
            let Some(mut parent) = self
                .drafts
                .get(&draft_key(&embedded.parent, &parent_id))
                .cloned()
                .or_else(|| self.backend.record(&embedded.parent, &parent_id))
            else {
                self.status = "The parent record is not loaded".into();
                return;
            };
            if !idmc_model::remove_child(&mut parent, &field, index) {
                self.status = "That child row is already gone".into();
                return;
            }
            self.remember_draft(parent.clone());
            if let Err(err) = self.write_record(parent) {
                self.status = err;
                return;
            }
            self.draft_rev = self.draft_rev.wrapping_add(1);
            self.status = format!("Deleted {label}");
            return;
        }
        let before = self.backend.record(&entity, &id);
        let result = self.backend.submit_write(WriteOp {
            kind: WriteKind::Delete,
            entity: entity.clone(),
            id: id.clone(),
            source_system: idmc_model::DEFAULT_SOURCE_SYSTEM.into(),
            source_pkey: String::new(),
            values: std::collections::BTreeMap::new(),
            before,
        });
        match result {
            Ok(_) => {
                self.drafts.remove(&draft_key(&entity, &id));
                self.file.selections.retain(|_, value| value != &id);
                self.draft_rev = self.draft_rev.wrapping_add(1);
                self.status = format!("Deleted {label}");
            }
            Err(err) => self.status = err.to_string(),
        }
    }

    pub fn cancel_delete(&mut self) {
        self.pending_delete = None;
        self.status = "Delete cancelled".into();
    }

    fn absorb_writes(&mut self) -> bool {
        let mut changed = false;
        while let Some(note) = self.backend.take_write_notice() {
            changed = true;
            if let Some(message) = note.error {
                if let Some(record) = note.restore {
                    self.remember_draft(record);
                }
                self.status = format!("Save failed: {message}");
            } else {
                if let Some(to) = note.to_id.as_deref() {
                    self.retarget_id(&note.from_id, to);
                }
                if let Some(message) = note.message {
                    self.status = message;
                }
            }
        }
        changed
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
            self.land_palette_cursor();
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

fn is_drill(id: &str) -> bool {
    matches!(
        id,
        "workspace.new" | "workspace.rename" | "view.new_table" | "view.new_detail" | "view.new_tree" | "view.open" | "view.link"
    )
}

fn action_section(id: &str) -> &'static str {
    if id.starts_with("workspace.") {
        "Workspace"
    } else if id.starts_with("view.") {
        "Views"
    } else if id.starts_with("record.") || id.starts_with("selection.") {
        "Records"
    } else if id.starts_with("data.") {
        "Data"
    } else {
        "General"
    }
}

fn palette_title(mode: &PaletteMode) -> String {
    match mode {
        PaletteMode::Commands => "Commands".into(),
        PaletteMode::PickEntity(kind) => match kind {
            ViewKind::Table => "New table".into(),
            ViewKind::Detail => "New detail".into(),
            ViewKind::Tree => "New tree".into(),
            ViewKind::Graph => "Graph".into(),
        },
        PaletteMode::PickLink(_) => "Source link".into(),
        PaletteMode::OpenView => "Open view".into(),
        PaletteMode::PickTemplate => "Templates".into(),
        PaletteMode::RenameWorkspace => "Rename".into(),
    }
}

pub fn fitted_width(label: &str, samples: &[String]) -> f32 {
    let longest = samples
        .iter()
        .map(|sample| sample.chars().count())
        .max()
        .unwrap_or(0)
        .max(label.chars().count());
    ((longest as f32) * 7.2 + 28.).clamp(MIN_COLUMN_WIDTH, 480.)
}

fn open_mock(source: &MockSource) -> Result<(AppBackend, Option<PathBuf>), String> {
    if let Some(model_path) = &source.model {
        let data_path = source.data.clone().unwrap_or_else(|| sibling(model_path, "data.json"));
        let backend = MockBackend::open_json(model_path, &data_path).map_err(|err| err.to_string())?;
        return Ok((AppBackend::Mock(backend), Some(data_path)));
    }
    if let Some(export_path) = &source.export {
        let backend = MockBackend::open_export(export_path, source.values.as_deref(), source.data.as_deref())
            .map_err(|err| err.to_string())?;
        return Ok((AppBackend::Mock(backend), source.data.clone()));
    }
    let mock = idmc_backend::find_mock_dir().ok_or_else(|| "could not find mock/".to_string())?;
    let backend = MockBackend::open_default().map_err(|err| err.to_string())?;
    Ok((AppBackend::Mock(backend), Some(mock.join("data.json"))))
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
        let mut session = finish_session(backend, None, workspace)?;
        session.fallback = Some(MockSource { export, model, data, values });
        return Ok(Launch::Run(session));
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

fn draft_key(entity: &str, id: &str) -> String {
    format!("{entity}\0{id}")
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

    #[test]
    fn inline_edit_saves_creates_a_child_and_deletes() {
        let backend = AppBackend::Mock(MockBackend::open_default().unwrap());
        let file = default_workspace(backend.model());
        let mut session = Session::boot(backend, file, PathBuf::from("workspace.json"), None);
        session.reconcile();
        session.file.focus = Some("table-c360.person".into());
        let people = session.rows("table-c360.person");
        let first = people[0].id.clone();
        let original = session.backend.record("c360.person", &first).unwrap().values.get("firstName").cloned();
        session.select("table-c360.person", &first);
        session.begin_edit("table-c360.person", &first, "firstName");
        session.editing.as_mut().unwrap().edit.select_all();
        for ch in "Ada".chars() {
            session.edit_insert(ch);
        }
        assert!(session.commit_edit(EditMove::Stay));
        assert!(session.dirty_records());
        session.revert_records();
        assert!(!session.dirty_records());
        assert_eq!(session.backend.record("c360.person", &first).unwrap().values.get("firstName"), original.as_ref());

        session.begin_edit("table-c360.person", &first, "firstName");
        session.editing.as_mut().unwrap().edit.select_all();
        for ch in "Ada".chars() {
            session.edit_insert(ch);
        }
        assert!(session.commit_edit(EditMove::Stay));
        session.save_records();
        assert_eq!(
            session.backend.record("c360.person", &first).unwrap().values.get("firstName"),
            Some(&FieldValue::String("Ada".into()))
        );
        assert!(!session.dirty_records());

        session.file.focus = Some("table-c360.person".into());
        session.perform("record.new");
        let new_id = session.file.selections.get("table-c360.person").cloned().unwrap();
        assert!(new_id.starts_with("new:"));
        let definition = session.backend.model().entity("c360.person").unwrap().clone();
        for field in definition.fields.iter().filter(|field| field.mandatory && field.ty != FieldType::Group && !field.many && !field.system) {
            session.begin_edit("table-c360.person", &new_id, &field.id);
            let sample = match field.ty {
                FieldType::Integer => "1".to_string(),
                FieldType::Decimal => "1.5".to_string(),
                FieldType::Boolean => "true".to_string(),
                FieldType::Date => "2024-01-02".to_string(),
                FieldType::DateTime => "2024-01-02T00:00:00".to_string(),
                FieldType::Lookup => session.editing.as_ref().and_then(|edit| edit.choices.first().map(|(_, label)| label.clone())).unwrap_or_else(|| "X".into()),
                _ => "Bo".into(),
            };
            session.editing.as_mut().unwrap().edit.select_all();
            for ch in sample.chars() {
                session.edit_insert(ch);
            }
            assert!(session.commit_edit(EditMove::Stay), "{}", session.status);
        }
        session.save_records();
        assert_eq!(session.status, "Saved", "{}", session.status);
        let created = session.file.selections.get("table-c360.person").cloned().unwrap();
        assert!(session.backend.record("c360.person", &created).is_some());

        session.select("table-c360.person", &first);
        session.file.focus = Some("table-c360.person.PostalAddress".into());
        let before = session.rows("table-c360.person.PostalAddress").len();
        session.new_child();
        assert!(session.rows("table-c360.person.PostalAddress").len() > before);
        let child_id = session.file.selections.get("table-c360.person.PostalAddress").cloned().unwrap();
        let child = session.backend.model().entity("c360.person.PostalAddress").unwrap().clone();
        for field in child.fields.iter().filter(|field| field.mandatory && field.ty != FieldType::Group && !field.many && !field.system) {
            session.begin_edit("table-c360.person.PostalAddress", &child_id, &field.id);
            let sample = match field.ty {
                FieldType::Lookup => session
                    .editing
                    .as_ref()
                    .and_then(|edit| edit.choices.first().map(|(_, label)| label.clone()))
                    .unwrap_or_else(|| "SE".into()),
                FieldType::Integer => "1".into(),
                FieldType::Boolean => "true".into(),
                FieldType::Date => "2024-01-02".into(),
                _ => "Main street".into(),
            };
            session.editing.as_mut().unwrap().edit.select_all();
            for ch in sample.chars() {
                session.edit_insert(ch);
            }
            assert!(session.commit_edit(EditMove::Stay), "{}", session.status);
        }
        session.save_records();
        assert_eq!(session.status, "Saved", "{}", session.status);
        assert!(session.backend.related("c360.person.PostalAddress#items", &first).unwrap().len() > before);

        session.file.focus = Some("table-c360.person".into());
        session.select("table-c360.person", &created);
        session.ask_delete();
        assert!(session.pending_delete.is_some());
        session.confirm_delete();
        assert!(session.backend.record("c360.person", &created).is_none());
    }

    #[test]
    fn cell_cursor_moves_and_selects_the_row() {
        let backend = AppBackend::Mock(MockBackend::open_default().unwrap());
        let file = default_workspace(backend.model());
        let mut session = Session::boot(backend, file, PathBuf::from("workspace.json"), None);
        session.file.focus = Some("table-c360.person".into());
        session.page_rows = 2;
        session.move_cell(1, 1);
        let cursor = session.cell.clone().unwrap();
        assert_eq!((cursor.row, cursor.col), (1, 1));
        let selected = session.file.selections.get("table-c360.person").cloned().unwrap();
        assert_eq!(session.row_at("table-c360.person", 1).unwrap().id, selected);
        session.cell_home(false);
        assert_eq!(session.cell.as_ref().unwrap().col, 0);
        session.cell_end(false);
        let last = session.spec("table-c360.person").unwrap().columns.len() - 1;
        assert_eq!(session.cell.as_ref().unwrap().col, last);
        session.cell_end(true);
        assert_eq!(session.cell.as_ref().unwrap().row, session.table_len("table-c360.person") - 1);
        session.cell_home(true);
        assert_eq!((session.cell.as_ref().unwrap().row, session.cell.as_ref().unwrap().col), (0, 0));
        session.cell_page(1);
        assert_eq!(session.cell.as_ref().unwrap().row, 2);
        session.edit_focused_cell();
        assert!(session.editing.is_some());
        assert!(Session::reveal_offset(0., 100., 200., 40.) < 0.);
        assert_eq!(fitted_width("Name", &["Ann".into()]), fitted_width("Name", &[]));
    }

    #[test]
    fn columns_resize_reorder_and_round_trip() {
        let backend = AppBackend::Mock(MockBackend::open_default().unwrap());
        let file = default_workspace(backend.model());
        let mut session = Session::boot(backend, file, PathBuf::from("workspace.json"), None);
        let view = "table-c360.person";
        let first = session.spec(view).unwrap().columns[0].clone();
        let second = session.spec(view).unwrap().columns[1].clone();
        session.set_column_width(view, &first, 10.);
        assert_eq!(session.column_width(view, &first), MIN_COLUMN_WIDTH);
        session.set_column_width(view, &first, 220.);
        session.drop_column(view, &second, 0);
        assert_eq!(session.spec(view).unwrap().columns[0], second);
        let saved = session.file.clone();
        let text = serde_json::to_string(&saved).unwrap();
        let loaded: crate::workspace::WorkspaceFile = serde_json::from_str(&text).unwrap();
        let spec = loaded.views.iter().find(|item| item.id == view).unwrap();
        assert_eq!(spec.column_widths.get(&first).copied(), Some(220.));
        assert_eq!(spec.columns[0], second);
    }

    #[test]
    fn row_menu_targets_open_a_linked_child_view() {
        let backend = AppBackend::Mock(MockBackend::open_default().unwrap());
        let file = default_workspace(backend.model());
        let mut session = Session::boot(backend, file, PathBuf::from("workspace.json"), None);
        let targets = session.row_targets("c360.person");
        assert!(targets.iter().any(|target| target.entity == "c360.person.PostalAddress"));
        assert!(targets.iter().any(|target| target.entity == "c360.person.Phone"));
        assert!(targets.iter().any(|target| target.entity == "c360.person.ElectronicAddress"));
        session.file.focus = Some("table-c360.person".into());
        let person = session.rows("table-c360.person")[0].id.clone();
        session.select("table-c360.person", &person);
        session.open_linked_view(
            "table-c360.person",
            ViewKind::Table,
            "c360.person.PostalAddress",
            "c360.person.PostalAddress#items",
        );
        let created = session.file.views.last().unwrap();
        assert_eq!(created.link.as_ref().unwrap().source, "table-c360.person");
        assert_eq!(created.link.as_ref().unwrap().relationship, "c360.person.PostalAddress#items");
        let rows = session.rows(&created.id);
        assert!(rows.iter().all(|row| row.id.starts_with(&format!("{person}::"))));
    }

    #[test]
    fn palette_drills_into_new_table_and_fuzzy_finds_it() {
        let backend = AppBackend::Mock(MockBackend::open_default().unwrap());
        let file = default_workspace(backend.model());
        let mut session = Session::boot(backend, file, PathBuf::from("workspace.json"), None);
        session.perform("palette.open");
        assert!(session.palette_rows().iter().any(|row| row.header && row.title == "Views"));
        let index = session.palette_rows().iter().position(|row| row.detail == "view.new_table").unwrap();
        session.palette.cursor = index;
        assert!(session.palette_key_right());
        assert!(session.palette_crumb().join(" ").contains("New table"));
        session.palette_back();
        assert!(matches!(session.palette.mode, PaletteMode::Commands));
        session.palette.edit.clear();
        for ch in "newt".chars() {
            session.type_char(ch);
        }
        assert!(session.palette_rows().iter().any(|row| row.detail == "view.new_table"));
        session.palette.edit.clear();
        session.backspace();
        assert!(session.palette.query().is_empty());
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
