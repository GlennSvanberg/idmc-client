//! Workspace session: which views exist, what is selected, and how links resolve.
//! No GPUI types, so the behavior can be tested without a window.

use std::path::{Path, PathBuf};

use idmc_backend::{Backend, MockBackend};
use idmc_layout::{Node, Zone};
use idmc_model::{value_at, FieldDef, FieldType, FieldValue, Record, ViewKind};

use crate::actions::{palette_actions, Action};
use crate::workspace::{default_workspace, sanitize, ViewLink, ViewSpec, WorkspaceFile};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    Quit,
}

#[derive(Clone, Debug)]
pub struct PaletteState {
    pub open: bool,
    pub query: String,
    pub cursor: usize,
    pub mode: PaletteMode,
}

#[derive(Clone, Debug)]
pub enum PaletteMode {
    Commands,
    PickEntity(ViewKind),
    PickLink(String),
}

impl PaletteState {
    fn closed() -> Self {
        Self {
            open: false,
            query: String::new(),
            cursor: 0,
            mode: PaletteMode::Commands,
        }
    }

    pub fn open_commands(&mut self) {
        *self = Self {
            open: true,
            query: String::new(),
            cursor: 0,
            mode: PaletteMode::Commands,
        };
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

pub struct Session {
    pub backend: MockBackend,
    pub file: WorkspaceFile,
    pub workspace_path: PathBuf,
    pub data_path: Option<PathBuf>,
    pub status: String,
    pub palette: PaletteState,
    pub menu: Option<MenuState>,
    pub chooser_for: Option<String>,
    pub chooser_filter: String,
    pub dock_hover: Option<(String, Zone)>,
    pub pending_dock: Option<String>,
}

impl Session {
    pub fn spec(&self, id: &str) -> Option<&ViewSpec> {
        self.file.views.iter().find(|view| view.id == id)
    }

    pub fn view_title(&self, id: &str) -> String {
        let Some(spec) = self.spec(id) else { return id.to_string() };
        let Some(entity) = self.backend.model().entity(&spec.entity) else { return spec.entity.clone() };
        match spec.kind {
            ViewKind::Tree => entity
                .hierarchy
                .as_ref()
                .map(|h| h.label.clone())
                .unwrap_or_else(|| entity.label_plural.clone()),
            ViewKind::Table => entity.label_plural.clone(),
            ViewKind::Detail => entity.label.clone(),
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
        self.backend.records(&spec.entity)
    }

    pub fn visible_ids(&self, view_id: &str) -> Vec<String> {
        let Some(spec) = self.spec(view_id) else { return Vec::new() };
        if spec.kind == ViewKind::Tree && spec.link.is_none() {
            let mut out = Vec::new();
            self.walk_tree(&spec.entity, None, &mut out);
            return out;
        }
        self.rows(view_id).into_iter().map(|row| row.id).collect()
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
        self.backend
            .model()
            .format_field(field, value, &|_, id| id.to_string())
    }

    pub fn cell(&self, entity: &str, field_id: &str, record: &Record) -> String {
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
                self.reconcile();
                self.status = "Workspace reset from the model".into();
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
        match &self.palette.mode {
            PaletteMode::Commands => palette_actions(&self.palette.query).len(),
            PaletteMode::PickEntity(kind) => self.entity_choices(*kind, &self.palette.query).len(),
            PaletteMode::PickLink(view) => self.link_choices(view, &self.palette.query).len(),
        }
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
                let items = palette_actions(&self.palette.query);
                let Some(action) = items.get(self.palette.cursor).copied() else {
                    return Effect::None;
                };
                self.palette.open = false;
                self.palette.query.clear();
                self.perform(action.id)
            }
            PaletteMode::PickEntity(kind) => {
                let choices = self.entity_choices(kind, &self.palette.query);
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
            PaletteMode::PickLink(view) => {
                let choices = self.link_choices(&view, &self.palette.query);
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
            PaletteMode::Commands => palette_actions(&self.palette.query)
                .into_iter()
                .map(PaletteRow::from_action)
                .collect(),
            PaletteMode::PickEntity(kind) => self
                .entity_choices(*kind, &self.palette.query)
                .into_iter()
                .map(|(_, label)| PaletteRow { title: label, shortcut: String::new(), detail: String::new() })
                .collect(),
            PaletteMode::PickLink(view) => self
                .link_choices(view, &self.palette.query)
                .into_iter()
                .map(|(_, _, label)| PaletteRow { title: label, shortcut: String::new(), detail: String::new() })
                .collect(),
        }
    }

    fn begin_new(&mut self, kind: ViewKind) {
        if kind == ViewKind::Tree && self.entity_choices(kind, "").is_empty() {
            self.status = "This model has no hierarchical entities".into();
            return;
        }
        self.pending_dock = self.file.focus.clone();
        self.palette = PaletteState {
            open: true,
            query: String::new(),
            cursor: 0,
            mode: PaletteMode::PickEntity(kind),
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
            query: String::new(),
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
        self.file.focus = self.file.layout.views().into_iter().next();
        if let Some(focus) = &self.file.focus {
            self.file.layout.activate(focus);
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
        let Some(path) = self.data_path.clone() else {
            self.status = "No record file to reload".into();
            return;
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => match self.backend.reload_data(&text) {
                Ok(()) => {
                    self.reconcile();
                    self.status = format!("Reloaded {}", path.display());
                }
                Err(err) => self.status = format!("Records rejected: {err}"),
            },
            Err(err) => self.status = format!("Could not read records: {err}"),
        }
    }

    pub fn type_char(&mut self, ch: char) {
        if self.palette.open {
            self.palette.query.push(ch);
            self.palette.cursor = 0;
        } else if self.chooser_for.is_some() {
            self.chooser_filter.push(ch);
        }
    }

    pub fn backspace(&mut self) {
        if self.palette.open {
            self.palette.query.pop();
            self.palette.cursor = 0;
        } else if self.chooser_for.is_some() {
            self.chooser_filter.pop();
        }
    }

    pub fn escape(&mut self) {
        if self.palette.open {
            self.palette.open = false;
        } else if self.chooser_for.is_some() {
            self.chooser_for = None;
            self.chooser_filter.clear();
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
}

impl PaletteRow {
    fn from_action(action: &Action) -> Self {
        Self {
            title: action.title.to_string(),
            shortcut: action.shortcut.map(crate::actions::shortcut_label).unwrap_or_default(),
            detail: action.id.to_string(),
        }
    }
}

pub fn launch_from_args(args: &[String]) -> Result<Session, String> {
    let mut export: Option<PathBuf> = None;
    let mut model: Option<PathBuf> = None;
    let mut data: Option<PathBuf> = None;
    let mut values: Option<PathBuf> = None;
    let mut workspace: Option<PathBuf> = None;
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
            "--help" | "-h" => return Err(help_text()),
            other if other.starts_with('-') => return Err(format!("unknown argument {other}\n{}", help_text())),
            other => return Err(format!("unexpected argument {other}")),
        }
        index += 1;
    }
    let (backend, data_path) = if let Some(model_path) = model {
        let data_path = data.unwrap_or_else(|| sibling(&model_path, "data.json"));
        let backend = MockBackend::open_json(&model_path, &data_path).map_err(|err| err.to_string())?;
        (backend, Some(data_path))
    } else if let Some(export_path) = export {
        let backend = MockBackend::open_export(&export_path, values.as_deref(), data.as_deref())
            .map_err(|err| err.to_string())?;
        (backend, data)
    } else {
        let mock = idmc_backend::find_mock_dir().ok_or_else(|| {
            "could not find mock/. Run from the repository, or pass --export / --model.".to_string()
        })?;
        let backend = MockBackend::open_default().map_err(|err| err.to_string())?;
        (backend, Some(mock.join("data.json")))
    };
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
    let mut session = Session {
        backend,
        file,
        workspace_path,
        data_path,
        status: String::new(),
        palette: PaletteState::closed(),
        menu: None,
        chooser_for: None,
        chooser_filter: String::new(),
        dock_hover: None,
        pending_dock: None,
    };
    session.reconcile();
    session.status = session.backend.model().summary();
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
        let backend = MockBackend::open_default().unwrap();
        let file = default_workspace(backend.model());
        let mut session = Session {
            backend,
            file,
            workspace_path: mock.join("workspace.json"),
            data_path: Some(mock.join("data.json")),
            status: String::new(),
            palette: PaletteState::closed(),
            menu: None,
            chooser_for: None,
            chooser_filter: String::new(),
            dock_hover: None,
            pending_dock: None,
        };
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
}

fn help_text() -> String {
    "idmc-client [--export DIR|ZIP] [--model FILE --data FILE] [--values FILE] [--workspace FILE]\n\
     Default model: mock/idmc-export/person (Customer 360 Person).\n\
     Second sample: --export mock/idmc-export/item\n\
     Hand-written model: --model mock/sample/model.json"
        .into()
}
