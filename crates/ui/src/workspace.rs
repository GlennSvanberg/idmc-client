//! Saved workspace: docking tree plus the views that occupy it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::ops::{Deref, DerefMut};
use std::path::Path;

use idmc_layout::{Dir, Node};
use idmc_model::{Cardinality, ModelDefinition, ViewKind};
use serde::{Deserialize, Serialize};

use crate::theme::ThemeMode;

/// Several dock layouts. `layout`, `views`, `focus`, `zoom`, and `selections`
/// on this file are the active tab, via deref.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFile {
    pub version: u32,
    pub theme: ThemeMode,
    #[serde(default)]
    pub active: usize,
    pub tabs: Vec<WorkspaceTab>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTab {
    pub id: String,
    pub name: String,
    pub layout: Node,
    pub views: Vec<ViewSpec>,
    #[serde(default)]
    pub focus: Option<String>,
    #[serde(default)]
    pub zoom: Option<String>,
    #[serde(default)]
    pub selections: BTreeMap<String, String>,
}

impl Deref for WorkspaceFile {
    type Target = WorkspaceTab;
    fn deref(&self) -> &WorkspaceTab {
        self.tabs.get(self.active).unwrap_or(&self.tabs[0])
    }
}

impl DerefMut for WorkspaceFile {
    fn deref_mut(&mut self) -> &mut WorkspaceTab {
        let index = self.active.min(self.tabs.len().saturating_sub(1));
        &mut self.tabs[index]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewSpec {
    pub id: String,
    pub kind: ViewKind,
    pub entity: String,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub link: Option<ViewLink>,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub sort_field: Option<String>,
    #[serde(default)]
    pub sort_descending: bool,
    /// Pixel widths keyed by field id. Missing keys use the default width.
    #[serde(default)]
    pub column_widths: BTreeMap<String, f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewLink {
    pub source: String,
    pub relationship: String,
}

impl WorkspaceFile {
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|err| err.to_string())?;
        fs::write(path, text).map_err(|err| err.to_string())
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path).map_err(|err| err.to_string())?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(text).map_err(|err| err.to_string())?;
        if value.get("tabs").is_some() {
            return serde_json::from_value(value).map_err(|err| err.to_string());
        }
        let theme = value
            .get("theme")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|err| err.to_string())?
            .unwrap_or(ThemeMode::Dark);
        let layout = value
            .get("layout")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|err| err.to_string())?
            .unwrap_or_default();
        let views = value
            .get("views")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|err| err.to_string())?
            .unwrap_or_default();
        let focus = value.get("focus").and_then(|v| v.as_str()).map(str::to_string);
        let zoom = value.get("zoom").and_then(|v| v.as_str()).map(str::to_string);
        let selections = value
            .get("selections")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|err| err.to_string())?
            .unwrap_or_default();
        Ok(pack("Browse", theme, layout, views, focus, zoom, selections))
    }

    pub fn tab_names(&self) -> Vec<String> {
        self.tabs.iter().map(|tab| tab.name.clone()).collect()
    }
}

fn pack(
    name: &str,
    theme: ThemeMode,
    layout: Node,
    views: Vec<ViewSpec>,
    focus: Option<String>,
    zoom: Option<String>,
    selections: BTreeMap<String, String>,
) -> WorkspaceFile {
    let id = format!("ws-{}", name.to_ascii_lowercase().replace(' ', "-"));
    WorkspaceFile {
        version: 2,
        theme,
        active: 0,
        tabs: vec![WorkspaceTab { id, name: name.to_string(), layout, views, focus, zoom, selections }],
    }
}

pub fn default_workspace(model: &ModelDefinition) -> WorkspaceFile {
    if let Some(entity) = model
        .entities
        .iter()
        .find(|e| e.hierarchy.is_some() && e.embedded_in.is_none())
    {
        return hierarchy_workspace(model, &entity.id);
    }
    let root = model
        .entities
        .iter()
        .find(|e| e.embedded_in.is_none() && model.embedded_children(&e.id).next().is_some())
        .or_else(|| model.entities.iter().find(|e| e.embedded_in.is_none()));
    let Some(root) = root else {
        return empty_workspace();
    };
    collection_workspace(model, &root.id)
}

fn empty_workspace() -> WorkspaceFile {
    pack("Browse", ThemeMode::Dark, Node::default(), Vec::new(), None, None, BTreeMap::new())
}

fn view(id: &str, kind: ViewKind, entity: &str, columns: Vec<String>, link: Option<ViewLink>) -> ViewSpec {
    ViewSpec {
        id: id.to_string(),
        kind,
        entity: entity.to_string(),
        columns,
        link,
        query: String::new(),
        sort_field: None,
        sort_descending: false,
        column_widths: BTreeMap::new(),
    }
}

/// Person table, person detail, and the first child collections (addresses, phones, emails) as tabs.
fn collection_workspace(model: &ModelDefinition, root: &str) -> WorkspaceFile {
    let entity = model.entity(root).expect("root entity");
    let table_id = format!("table-{root}");
    let detail_id = format!("detail-{root}");
    let mut views = vec![
        view(&table_id, ViewKind::Table, root, entity.default_columns(), None),
        view(
            &detail_id,
            ViewKind::Detail,
            root,
            Vec::new(),
            Some(ViewLink {
                source: table_id.clone(),
                relationship: "identity".into(),
            }),
        ),
    ];
    let mut child_ids = Vec::new();
    for child in model.embedded_children(root).take(3) {
        let Some(rel) = model
            .relationships
            .iter()
            .find(|r| r.embedded && r.to == child.id)
        else {
            continue;
        };
        let id = format!("table-{}", child.id);
        views.push(view(
            &id,
            ViewKind::Table,
            &child.id,
            child.default_columns(),
            Some(ViewLink {
                source: table_id.clone(),
                relationship: rel.id.clone(),
            }),
        ));
        child_ids.push(id);
    }
    let bottom = match child_ids.len() {
        0 => Node::default(),
        1 => Node::tabs(&child_ids[0]),
        _ => Node::Tabs { views: child_ids, active: 0 },
    };
    let layout = if matches!(bottom, Node::Tabs { ref views, .. } if views.is_empty()) {
        Node::Split {
            dir: Dir::Horizontal,
            ratios: vec![0.42, 0.58],
            children: vec![Node::tabs(&table_id), Node::tabs(&detail_id)],
        }
    } else {
        Node::Split {
            dir: Dir::Horizontal,
            ratios: vec![0.40, 0.60],
            children: vec![
                Node::tabs(&table_id),
                Node::Split {
                    dir: Dir::Vertical,
                    ratios: vec![0.46, 0.54],
                    children: vec![Node::tabs(&detail_id), bottom],
                },
            ],
        }
    };
    pack("Browse", ThemeMode::Dark, layout, views, Some(table_id), None, BTreeMap::new())
}

fn hierarchy_workspace(model: &ModelDefinition, root: &str) -> WorkspaceFile {
    let entity = model.entity(root).expect("hierarchy entity");
    let tree_id = format!("tree-{root}");
    let mut views = vec![view(
        &tree_id,
        ViewKind::Tree,
        root,
        Vec::new(),
        None,
    )];
    let mut tables = Vec::new();
    for rel in model.relationships.iter().filter(|r| {
        r.from == root && r.cardinality == Cardinality::OneToMany && r.to != root && !r.embedded
    }) {
        if tables.iter().any(|spec: &ViewSpec| spec.entity == rel.to) {
            continue;
        }
        let Some(target) = model.entity(&rel.to) else { continue };
        let id = format!("table-{}", rel.to);
        tables.push(view(
            &id,
            ViewKind::Table,
            &rel.to,
            target.default_columns(),
            Some(ViewLink {
                source: tree_id.clone(),
                relationship: rel.id.clone(),
            }),
        ));
    }
    let detail = tables.first().map(|table| {
        view(
            &format!("detail-{}", table.entity),
            ViewKind::Detail,
            &table.entity,
            Vec::new(),
            Some(ViewLink {
                source: table.id.clone(),
                relationship: "identity".into(),
            }),
        )
    });
    let extra = tables.first().and_then(|table| {
        model.relationships.iter().find(|r| {
            r.from == table.entity && r.cardinality == Cardinality::OneToMany && r.to != table.entity && !r.embedded
        }).and_then(|rel| {
            let target = model.entity(&rel.to)?;
            Some(view(
                &format!("table-{}", rel.to),
                ViewKind::Table,
                &rel.to,
                target.default_columns(),
                Some(ViewLink {
                    source: table.id.clone(),
                    relationship: rel.id.clone(),
                }),
            ))
        })
    });
    views.extend(tables.iter().cloned());
    if let Some(detail) = &detail {
        views.push(detail.clone());
    }
    if let Some(extra) = &extra {
        views.push(extra.clone());
    }
    let layout = match (tables.first(), detail.as_ref(), extra.as_ref()) {
        (Some(table), Some(detail), Some(extra)) => Node::Split {
            dir: Dir::Horizontal,
            ratios: vec![0.28, 0.72],
            children: vec![
                Node::tabs(&tree_id),
                Node::Split {
                    dir: Dir::Vertical,
                    ratios: vec![0.55, 0.45],
                    children: vec![
                        Node::tabs(&table.id),
                        Node::Split {
                            dir: Dir::Horizontal,
                            ratios: vec![0.5, 0.5],
                            children: vec![Node::tabs(&detail.id), Node::tabs(&extra.id)],
                        },
                    ],
                },
            ],
        },
        (Some(table), Some(detail), None) => Node::Split {
            dir: Dir::Horizontal,
            ratios: vec![0.30, 0.70],
            children: vec![
                Node::tabs(&tree_id),
                Node::Split {
                    dir: Dir::Vertical,
                    ratios: vec![0.55, 0.45],
                    children: vec![Node::tabs(&table.id), Node::tabs(&detail.id)],
                },
            ],
        },
        _ => Node::tabs(&tree_id),
    };
    let _ = entity;
    pack("Browse", ThemeMode::Dark, layout, views, Some(tree_id), None, BTreeMap::new())
}

pub fn template_names() -> &'static [&'static str] {
    &["Browse", "Search & inspect", "Data quality"]
}

pub fn template_workspace(model: &ModelDefinition, name: &str) -> WorkspaceFile {
    match name {
        "Search & inspect" => search_workspace(model),
        "Data quality" => quality_workspace(model),
        _ => default_workspace(model),
    }
}

fn primary_root(model: &ModelDefinition) -> Option<String> {
    model
        .entities
        .iter()
        .find(|entity| entity.hierarchy.is_some() && entity.embedded_in.is_none())
        .or_else(|| {
            model.entities.iter().find(|entity| {
                entity.embedded_in.is_none() && model.embedded_children(&entity.id).next().is_some()
            })
        })
        .or_else(|| model.entities.iter().find(|entity| entity.embedded_in.is_none()))
        .map(|entity| entity.id.clone())
}

fn search_workspace(model: &ModelDefinition) -> WorkspaceFile {
    let Some(root) = primary_root(model) else { return empty_workspace() };
    let entity = model.entity(&root).expect("root");
    let table_id = format!("table-{root}");
    let detail_id = format!("detail-{root}");
    let views = vec![
        view(&table_id, ViewKind::Table, &root, entity.default_columns(), None),
        view(
            &detail_id,
            ViewKind::Detail,
            &root,
            Vec::new(),
            Some(ViewLink { source: table_id.clone(), relationship: "identity".into() }),
        ),
    ];
    let layout = Node::Split {
        dir: Dir::Horizontal,
        ratios: vec![0.58, 0.42],
        children: vec![Node::tabs(&table_id), Node::tabs(&detail_id)],
    };
    pack("Search & inspect", ThemeMode::Dark, layout, views, Some(table_id), None, BTreeMap::new())
}

fn quality_workspace(model: &ModelDefinition) -> WorkspaceFile {
    let Some(root) = primary_root(model) else { return empty_workspace() };
    let entity = model.entity(&root).expect("root");
    let mut columns: Vec<String> = entity
        .leaves()
        .into_iter()
        .filter(|field| !field.system)
        .take(12)
        .map(|field| field.id.clone())
        .collect();
    if columns.is_empty() {
        columns = entity.default_columns();
    }
    let table_id = format!("table-{root}");
    let detail_id = format!("detail-{root}");
    let views = vec![
        view(&table_id, ViewKind::Table, &root, columns, None),
        view(
            &detail_id,
            ViewKind::Detail,
            &root,
            Vec::new(),
            Some(ViewLink { source: table_id.clone(), relationship: "identity".into() }),
        ),
    ];
    let layout = Node::Split {
        dir: Dir::Horizontal,
        ratios: vec![0.72, 0.28],
        children: vec![Node::tabs(&table_id), Node::tabs(&detail_id)],
    };
    pack("Data quality", ThemeMode::Dark, layout, views, Some(table_id), None, BTreeMap::new())
}

pub fn sanitize(file: &mut WorkspaceFile, model: &ModelDefinition) {
    let active = file.active;
    let count = file.tabs.len();
    for index in 0..count {
        file.active = index;
        sanitize_active(file, model);
    }
    if file.tabs.is_empty() {
        file.tabs.push(empty_workspace().tabs.remove(0));
    }
    file.active = active.min(file.tabs.len() - 1);
}

fn sanitize_active(file: &mut WorkspaceFile, model: &ModelDefinition) {
    file.views.retain(|view| model.entity(&view.entity).is_some());
    let ids: BTreeSet<String> = file.views.iter().map(|v| v.id.clone()).collect();
    let stale: Vec<String> = file.layout.views().into_iter().filter(|id| !ids.contains(id)).collect();
    for id in stale {
        file.layout.remove(&id);
    }
    let link_ok: Vec<bool> = file
        .views
        .iter()
        .map(|view| {
            view.link.as_ref().is_some_and(|link| {
                if link.relationship == "identity" {
                    return file
                        .views
                        .iter()
                        .any(|source| source.id == link.source && source.entity == view.entity);
                }
                let Some(source) = file.views.iter().find(|source| source.id == link.source) else {
                    return false;
                };
                model
                    .link_options(&source.entity, &view.entity, view.kind)
                    .iter()
                    .any(|opt| opt.relationship == link.relationship)
            })
        })
        .collect();
    for (view, valid) in file.views.iter_mut().zip(link_ok) {
        if view.link.is_some() && !valid {
            view.link = None;
        }
        if let Some(entity) = model.entity(&view.entity) {
            let leaves: BTreeSet<String> = entity.leaves().iter().map(|f| f.id.clone()).collect();
            view.columns.retain(|column| leaves.contains(column));
            view.column_widths.retain(|column, _| leaves.contains(column));
            if view.columns.is_empty() && view.kind == ViewKind::Table {
                view.columns = entity.default_columns();
            }
        }
    }
    if file.focus.as_ref().is_some_and(|id| file.views.iter().all(|v| &v.id != id)) {
        file.focus = file.views.first().map(|v| v.id.clone());
    }
    let live: BTreeSet<String> = file.views.iter().map(|view| view.id.clone()).collect();
    file.selections.retain(|view, _| live.contains(view));
}

pub fn links_are_legal(file: &WorkspaceFile, model: &ModelDefinition) -> bool {
    file.views.iter().all(|view| match &view.link {
        None => true,
        Some(link) if link.relationship == "identity" => file
            .views
            .iter()
            .any(|source| source.id == link.source && source.entity == view.entity),
        Some(link) => file.views.iter().any(|source| {
            source.id == link.source
                && model
                    .link_options(&source.entity, &view.entity, view.kind)
                    .iter()
                    .any(|opt| opt.relationship == link.relationship)
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use idmc_backend::{Backend, MockBackend};
    use idmc_model::{IdmcExportSource, JsonModelSource, ModelSource};
    use std::path::PathBuf;

    fn mock() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mock")
    }

    #[test]
    fn person_default_links_table_detail_and_addresses() {
        let backend = MockBackend::open_export(
            &mock().join("idmc-export/person"),
            Some(&mock().join("reference-values.json")),
            Some(&mock().join("data.json")),
        )
        .unwrap();
        let file = default_workspace(backend.model());
        assert!(links_are_legal(&file, backend.model()));
        let people = file.views.iter().find(|v| v.entity == "c360.person" && v.kind == idmc_model::ViewKind::Table).unwrap();
        let detail = file.views.iter().find(|v| v.entity == "c360.person" && v.kind == idmc_model::ViewKind::Detail).unwrap();
        assert_eq!(detail.link.as_ref().unwrap().relationship, "identity");
        assert_eq!(detail.link.as_ref().unwrap().source, people.id);
        let addresses = file.views.iter().find(|v| v.entity == "c360.person.PostalAddress").unwrap();
        assert_eq!(addresses.link.as_ref().unwrap().source, people.id);
        assert!(file.views.iter().any(|v| v.entity == "c360.person.Phone"));
        assert!(file.views.iter().any(|v| v.entity == "c360.person.ElectronicAddress"));
        assert!(file.layout.views().contains(&people.id));
        assert!(file.layout.views().contains(&addresses.id));
        let mut broken = file.clone();
        broken.views.push(ViewSpec {
            id: "ghost".into(),
            kind: idmc_model::ViewKind::Table,
            entity: "does.not.exist".into(),
            columns: vec!["nope".into()],
            link: Some(ViewLink { source: people.id.clone(), relationship: "not-a-link".into() }),
            query: String::new(),
            sort_field: None,
            sort_descending: false,
            column_widths: BTreeMap::new(),
        });
        broken.layout.insert(&people.id, "ghost", idmc_layout::Zone::Right);
        sanitize(&mut broken, backend.model());
        assert!(broken.views.iter().all(|v| v.entity != "does.not.exist"));
        assert!(!broken.layout.contains("ghost"));
    }

    #[test]
    fn sample_model_defaults_to_a_tree() {
        let model = JsonModelSource::open(&mock().join("sample/model.json")).unwrap().load().unwrap();
        let file = default_workspace(&model);
        assert!(file.views.iter().any(|v| v.kind == idmc_model::ViewKind::Tree && v.entity == "Category"));
        assert!(links_are_legal(&file, &model));
    }

    #[test]
    fn workspace_roundtrip() {
        let model = IdmcExportSource::open(&mock().join("idmc-export/person")).unwrap().load().unwrap();
        let file = default_workspace(&model);
        let json = serde_json::to_string(&file).unwrap();
        let back: WorkspaceFile = serde_json::from_str(&json).unwrap();
        assert_eq!(file, back);
        let value = serde_json::to_value(&file).unwrap();
        let v1 = serde_json::json!({
            "version": 1,
            "theme": "dark",
            "layout": value["tabs"][0]["layout"],
            "views": value["tabs"][0]["views"],
            "focus": value["tabs"][0]["focus"],
        });
        let migrated = WorkspaceFile::parse(&v1.to_string()).unwrap();
        assert_eq!(migrated.tabs.len(), 1);
        assert_eq!(migrated.name, "Browse");
        assert_eq!(migrated.views.len(), file.views.len());
    }

    #[test]
    fn templates_come_from_the_model() {
        let model = IdmcExportSource::open(&mock().join("idmc-export/person")).unwrap().load().unwrap();
        for name in template_names() {
            let file = template_workspace(&model, name);
            assert_eq!(file.name, *name);
            assert!(file.views.iter().all(|view| model.entity(&view.entity).is_some()));
            assert!(links_are_legal(&file, &model));
        }
        let quality = template_workspace(&model, "Data quality");
        let table = quality.views.iter().find(|view| view.kind == idmc_model::ViewKind::Table).unwrap();
        assert!(table.columns.len() > 4, "{}", table.columns.len());
    }
}
