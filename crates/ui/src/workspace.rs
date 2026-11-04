//! Saved workspace: docking tree plus the views that occupy it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use idmc_layout::{Dir, Node};
use idmc_model::{Cardinality, ModelDefinition, ViewKind};
use serde::{Deserialize, Serialize};

use crate::theme::ThemeMode;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFile {
    pub version: u32,
    pub theme: ThemeMode,
    pub layout: Node,
    pub views: Vec<ViewSpec>,
    #[serde(default)]
    pub focus: Option<String>,
    #[serde(default)]
    pub zoom: Option<String>,
    #[serde(default)]
    pub selections: BTreeMap<String, String>,
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
        serde_json::from_str(&text).map_err(|err| err.to_string())
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
    WorkspaceFile {
        version: 1,
        theme: ThemeMode::Dark,
        layout: Node::default(),
        views: Vec::new(),
        focus: None,
        zoom: None,
        selections: BTreeMap::new(),
    }
}

fn view(id: &str, kind: ViewKind, entity: &str, columns: Vec<String>, link: Option<ViewLink>) -> ViewSpec {
    ViewSpec {
        id: id.to_string(),
        kind,
        entity: entity.to_string(),
        columns,
        link,
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
    WorkspaceFile {
        version: 1,
        theme: ThemeMode::Dark,
        layout,
        focus: Some(table_id),
        zoom: None,
        selections: BTreeMap::new(),
        views,
    }
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
    WorkspaceFile {
        version: 1,
        theme: ThemeMode::Dark,
        layout,
        focus: Some(tree_id),
        zoom: None,
        selections: BTreeMap::new(),
        views,
    }
}

pub fn sanitize(file: &mut WorkspaceFile, model: &ModelDefinition) {
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
            if view.columns.is_empty() && view.kind == ViewKind::Table {
                view.columns = entity.default_columns();
            }
        }
    }
    if file.focus.as_ref().is_some_and(|id| file.views.iter().all(|v| &v.id != id)) {
        file.focus = file.views.first().map(|v| v.id.clone());
    }
    file.selections.retain(|view, _| file.views.iter().any(|v| &v.id == view));
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
    }
}
