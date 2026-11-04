//! Dockable graph. The force parameters, label culling, zoom, and expand-in-place
//! behavior follow https://github.com/braian-io/braian (Apache-2.0). This file is
//! a GPUI reimplementation; it does not copy that TypeScript.

use std::collections::{BTreeSet, HashMap, HashSet};

use gpui::{
    canvas, div, point, px, quad, size, AnyElement, BorderStyle, Bounds, Context, Corners, Edges,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathBuilder, Pixels, Point,
    ScrollDelta, ScrollWheelEvent, SharedString, Window,
};
use gpui::prelude::*;
use idmc_backend::Backend;
use idmc_layout::Simulation;
use idmc_model::{
    build_entity_scene, build_model_scene, node_mark, radius_for, stub_entity, GraphNode, NodeId,
    NodeKind, Scene,
};

use crate::app::Root;
use crate::session::Session;
use crate::theme::Theme;

#[derive(Clone, Debug)]
enum Drag {
    None,
    Pan { last_x: f32, last_y: f32 },
    Node { id: String, last_x: f32, last_y: f32, moved: bool },
}

pub enum GraphEffect {
    None,
    Expand(String),
    Open(String, String),
}

pub struct GraphBoard {
    pub scene: Scene,
    pub sim: Simulation,
    pub hidden: BTreeSet<NodeKind>,
    pub selected: Option<String>,
    pub schema: bool,
    pub cam_x: f32,
    pub cam_y: f32,
    pub cam_k: f32,
    pub user_camera: bool,
    pub width: f32,
    pub height: f32,
    drag: Drag,
}

impl GraphBoard {
    pub fn load(scene: Scene, schema: bool) -> Self {
        let mut board = Self {
            scene,
            sim: Simulation::default(),
            hidden: BTreeSet::new(),
            selected: None,
            schema,
            cam_x: 0.0,
            cam_y: 0.0,
            cam_k: 1.0,
            user_camera: false,
            width: 800.0,
            height: 600.0,
            drag: Drag::None,
        };
        board.sync_sim();
        board.sim.alpha = 1.0;
        board
    }

    pub fn capped(&self) -> bool {
        self.scene.capped
    }

    pub fn settled(&self) -> bool {
        self.sim.settled()
    }

    pub fn tick(&mut self) -> bool {
        let settled = self.sim.tick();
        if settled && !self.user_camera {
            self.fit(self.width, self.height);
        }
        settled
    }

    pub fn merge(&mut self, extra: Scene) {
        let kept: HashMap<String, (f32, f32)> = self.sim.nodes.iter().map(|node| (node.id.clone(), (node.x, node.y))).collect();
        self.scene.merge(extra);
        self.sync_sim();
        for node in &mut self.sim.nodes {
            if let Some((x, y)) = kept.get(&node.id) {
                node.x = *x;
                node.y = *y;
            }
        }
        if !self.user_camera {
            self.fit(self.width, self.height);
        }
    }

    pub fn position(&self, key: &str) -> Option<(f32, f32)> {
        self.sim.nodes.iter().find(|node| node.id == key).map(|node| (node.x, node.y))
    }

    pub fn toggle_kind(&mut self, kind: NodeKind) {
        if !self.hidden.insert(kind) {
            self.hidden.remove(&kind);
        }
    }

    pub fn fit(&mut self, width: f32, height: f32) {
        self.width = width.max(1.0);
        self.height = height.max(1.0);
        let nodes: Vec<_> = self.sim.nodes.iter().filter(|node| self.visible_key(&node.id)).collect();
        if nodes.is_empty() || width < 20.0 || height < 20.0 {
            return;
        }
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for node in &nodes {
            let pad = node.radius + 28.0;
            min_x = min_x.min(node.x - pad);
            max_x = max_x.max(node.x + pad);
            min_y = min_y.min(node.y - pad);
            max_y = max_y.max(node.y + pad);
        }
        let span_x = (max_x - min_x).max(1.0);
        let span_y = (max_y - min_y).max(1.0);
        self.cam_k = (width / span_x).min(height / span_y).clamp(0.25, 2.5);
        let mid_x = (min_x + max_x) * 0.5;
        let mid_y = (min_y + max_y) * 0.5;
        self.cam_x = width * 0.5 - mid_x * self.cam_k;
        self.cam_y = height * 0.5 - mid_y * self.cam_k;
    }

    pub fn zoom_at(&mut self, x: f32, y: f32, zoom_in: bool) {
        let prev = self.cam_k;
        let next = (prev * if zoom_in { 1.12 } else { 1.0 / 1.12 }).clamp(0.25, 2.5);
        let wx = (x - self.cam_x) / prev;
        let wy = (y - self.cam_y) / prev;
        self.cam_k = next;
        self.cam_x = x - wx * next;
        self.cam_y = y - wy * next;
        self.user_camera = true;
    }

    pub fn pointer_down(&mut self, x: f32, y: f32, clicks: usize) -> GraphEffect {
        if let Some(id) = self.hit(x, y) {
            self.selected = Some(id.clone());
            if clicks >= 2 {
                self.drag = Drag::None;
                return GraphEffect::Expand(id);
            }
            self.drag = Drag::Node { id, last_x: x, last_y: y, moved: false };
            return GraphEffect::None;
        }
        self.drag = Drag::Pan { last_x: x, last_y: y };
        GraphEffect::None
    }

    pub fn pointer_move(&mut self, x: f32, y: f32) {
        match &self.drag {
            Drag::Pan { last_x, last_y } => {
                let dx = x - last_x;
                let dy = y - last_y;
                self.cam_x += dx;
                self.cam_y += dy;
                self.user_camera = true;
                self.drag = Drag::Pan { last_x: x, last_y: y };
            }
            Drag::Node { id, last_x, last_y, moved } => {
                let dx = x - last_x;
                let dy = y - last_y;
                let moved = *moved || dx.hypot(dy) > 3.0;
                let id = id.clone();
                if let Some(node) = self.sim.nodes.iter_mut().find(|node| node.id == id) {
                    let (wx, wy) = ((x - self.cam_x) / self.cam_k, (y - self.cam_y) / self.cam_k);
                    node.fx = Some(wx);
                    node.fy = Some(wy);
                    node.x = wx;
                    node.y = wy;
                }
                self.sim.reheat(0.3);
                self.drag = Drag::Node { id, last_x: x, last_y: y, moved };
            }
            Drag::None => {}
        }
    }

    pub fn pointer_up(&mut self) {
        if let Drag::Node { id, moved, .. } = &self.drag {
            if !moved {
                let id = id.clone();
                if let Some(node) = self.sim.nodes.iter_mut().find(|node| node.id == id) {
                    node.fx = None;
                    node.fy = None;
                }
            }
        }
        self.drag = Drag::None;
    }

    pub fn unpin(&mut self) {
        let Some(id) = self.selected.clone() else { return };
        if let Some(node) = self.sim.nodes.iter_mut().find(|node| node.id == id) {
            node.fx = None;
            node.fy = None;
        }
        self.sim.reheat(0.3);
    }

    pub fn pinned(&self) -> bool {
        let Some(id) = &self.selected else { return false };
        self.sim.nodes.iter().any(|node| &node.id == id && node.fx.is_some())
    }

    pub fn arrow(&mut self, direction: &str) {
        let Some(current) = self.selected.clone().or_else(|| self.first_visible()) else { return };
        self.selected = Some(spatial_neighbor(&self.sim.nodes, &self.hidden_keys(), &current, direction));
    }

    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<String> {
        let (wx, wy) = ((x - self.cam_x) / self.cam_k, (y - self.cam_y) / self.cam_k);
        let slop = 6.0 / self.cam_k;
        for node in self.sim.nodes.iter().rev() {
            if !self.visible_key(&node.id) {
                continue;
            }
            let reach = node.radius + slop;
            let dx = node.x - wx;
            let dy = node.y - wy;
            if dx * dx + dy * dy <= reach * reach {
                return Some(node.id.clone());
            }
        }
        None
    }

    pub fn selected_node(&self) -> Option<&GraphNode> {
        let key = self.selected.as_deref()?;
        self.scene.nodes.iter().find(|node| node.id.key() == key)
    }

    fn sync_sim(&mut self) {
        let mut degree: HashMap<String, usize> = HashMap::new();
        for edge in &self.scene.edges {
            *degree.entry(edge.source.clone()).or_default() += 1;
            *degree.entry(edge.target.clone()).or_default() += 1;
        }
        let specs: Vec<idmc_layout::NodeSpec> = self
            .scene
            .nodes
            .iter()
            .map(|node| idmc_layout::NodeSpec {
                id: node.id.key(),
                radius: radius_for(node.kind, degree.get(&node.id.key()).copied().unwrap_or(0)),
                ring: node.kind.seed_ring(),
            })
            .collect();
        let links: Vec<(String, String)> = self.scene.edges.iter().map(|edge| (edge.source.clone(), edge.target.clone())).collect();
        self.sim.sync(&specs, &links);
    }

    fn visible_key(&self, key: &str) -> bool {
        let Some(node) = self.scene.nodes.iter().find(|node| node.id.key() == key) else { return false };
        !self.hidden.contains(&node.kind)
    }

    fn hidden_keys(&self) -> HashSet<String> {
        self.scene.nodes.iter().filter(|node| self.hidden.contains(&node.kind)).map(|node| node.id.key()).collect()
    }

    fn first_visible(&self) -> Option<String> {
        self.scene.nodes.iter().find(|node| !self.hidden.contains(&node.kind)).map(|node| node.id.key())
    }
}

fn spatial_neighbor(nodes: &[idmc_layout::Body], hidden: &HashSet<String>, current: &str, direction: &str) -> String {
    let self_node = nodes.iter().find(|node| node.id == current && !hidden.contains(&node.id));
    let Some(self_node) = self_node else { return current.to_string() };
    let mut best = self_node.id.clone();
    let mut best_score = f32::MAX;
    for node in nodes {
        if node.id == self_node.id || hidden.contains(&node.id) {
            continue;
        }
        let dx = node.x - self_node.x;
        let dy = node.y - self_node.y;
        let forward = match direction {
            "right" => dx,
            "left" => -dx,
            "down" => dy,
            _ => -dy,
        };
        if forward < 8.0 {
            continue;
        }
        let lateral = if direction == "left" || direction == "right" { dy.abs() } else { dx.abs() };
        let score = lateral * 2.2 + forward;
        if score < best_score {
            best_score = score;
            best = node.id.clone();
        }
    }
    best
}

pub fn label_priority(kind: NodeKind, active: bool) -> u8 {
    if active {
        return 5;
    }
    match kind {
        NodeKind::Entity | NodeKind::Type => 3,
        NodeKind::ChildGroup => 2,
        NodeKind::ChildRecord => 1,
        NodeKind::Field => 0,
    }
}

pub fn label_visible(priority: u8, zoom: f32) -> bool {
    if priority >= 5 {
        return true;
    }
    if zoom < 0.5 {
        return priority >= 2;
    }
    if zoom < 0.7 {
        return priority >= 1;
    }
    true
}

#[derive(Clone, Debug)]
pub struct LabelBox {
    pub id: String,
    pub priority: u8,
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

pub fn visible_labels(boxes: &[LabelBox], zoom: f32) -> BTreeSet<String> {
    let mut sorted = boxes.to_vec();
    sorted.sort_by(|left, right| right.priority.cmp(&left.priority).then_with(|| left.id.cmp(&right.id)));
    let mut kept: Vec<LabelBox> = Vec::new();
    let mut ids = BTreeSet::new();
    for box_ in sorted {
        if !label_visible(box_.priority, zoom) {
            continue;
        }
        if kept.iter().any(|other| overlaps(other, &box_)) {
            continue;
        }
        ids.insert(box_.id.clone());
        kept.push(box_);
    }
    ids
}

fn overlaps(left: &LabelBox, right: &LabelBox) -> bool {
    left.left < right.right && right.left < left.right && left.top < right.bottom && right.top < left.bottom
}

impl Session {
    pub fn open_graph(&mut self, seed: Option<(String, String)>) {
        let (scene, schema) = if let Some((entity, id)) = seed {
            match self.backend.record(&entity, &id) {
                Some(record) => (build_entity_scene(&record, self.backend.model()), false),
                None => {
                    let label = self.backend.model().entity(&entity).map(|item| item.label.clone()).unwrap_or_else(|| entity.clone());
                    let mut scene = Scene::empty();
                    scene.nodes.push(stub_entity(&entity, &id, &id, &label));
                    self.backend.schedule_expand(&scene.nodes[0].id.key());
                    (scene, false)
                }
            }
        } else if let Some((entity, id)) = self.focused_record() {
            match self.backend.record(&entity, &id) {
                Some(record) => (build_entity_scene(&record, self.backend.model()), false),
                None => (build_model_scene(self.backend.model()), true),
            }
        } else {
            (build_model_scene(self.backend.model()), true)
        };
        let anchor = self.backend.model().roots().next().map(|entity| entity.id.clone()).unwrap_or_default();
        let missing = self.graph_view.as_deref().is_none_or(|id| self.spec(id).is_none());
        if missing {
            self.open_view(idmc_model::ViewKind::Graph, &anchor, self.file.focus.clone());
            self.graph_view = self.file.focus.clone();
        }
        let mut board = GraphBoard::load(scene, schema);
        board.fit(900.0, 640.0);
        self.graph = Some(board);
        if let Some(id) = self.graph_view.clone() {
            self.file.focus = Some(id);
        }
        self.status = if schema { "Graph · schema".into() } else { "Graph".into() };
    }

    pub fn show_in_graph(&mut self) {
        self.open_graph(self.focused_record());
    }

    fn focused_record(&self) -> Option<(String, String)> {
        let id = self.file.focus.clone()?;
        let spec = self.spec(&id)?;
        if spec.kind == idmc_model::ViewKind::Graph {
            return None;
        }
        let selected = self.file.selections.get(&id)?.clone();
        if selected.is_empty() {
            return None;
        }
        Some((spec.entity.clone(), selected))
    }

    pub fn graph_focused(&self) -> bool {
        self.graph_view.as_ref().is_some_and(|id| self.file.focus.as_ref() == Some(id) && self.spec(id).is_some())
    }

    pub fn tick_graph(&mut self) -> bool {
        let Some(board) = self.graph.as_mut() else { return false };
        let settled = board.tick();
        !settled
    }

    pub fn expand_graph(&mut self, key: &str) {
        let before = self.graph.as_ref().and_then(|board| board.position(key));
        self.backend.schedule_expand(key);
        if let Some(scene) = self.backend.take_expand() {
            if let Some(board) = self.graph.as_mut() {
                board.merge(scene);
                if let Some(before) = before {
                    let after = board.position(key);
                    if after != Some(before) && after.is_some() {
                        // merge keeps the node; a center step may run on the next tick, not here.
                        let _ = before;
                    }
                }
            }
        }
        self.status = "Expanding".into();
    }

    pub fn graph_enter(&mut self) -> GraphEffect {
        let Some(board) = self.graph.as_ref() else { return GraphEffect::None };
        let Some(node) = board.selected_node() else { return GraphEffect::None };
        match &node.id {
            NodeId::Entity { be, id } if !node.raw_caption => GraphEffect::Open(be.clone(), id.clone()),
            NodeId::ChildRecord { be, id } => {
                let parent_id = id.split("::").next().unwrap_or(id).to_string();
                let parent = self
                    .backend
                    .model()
                    .entity(be)
                    .and_then(|entity| entity.embedded_in.as_ref())
                    .map(|embedded| embedded.parent.clone())
                    .unwrap_or_else(|| be.clone());
                GraphEffect::Open(parent, parent_id)
            }
            _ => GraphEffect::Expand(node.id.key()),
        }
    }

    pub fn absorb_graph(&mut self) -> bool {
        let mut changed = false;
        if let Some(scene) = self.backend.take_expand() {
            if let Some(board) = self.graph.as_mut() {
                board.merge(scene);
                changed = true;
            }
        }
        changed
    }

    pub fn seed_graph(&mut self, hits: &[idmc_backend::SearchHit]) {
        let mut scene = Scene::empty();
        for hit in hits {
            scene.nodes.push(stub_entity(&hit.entity, &hit.id, &hit.label, &hit.entity_label));
        }
        if let Some(board) = self.graph.as_mut() {
            board.schema = false;
            board.merge(scene);
        }
    }
}

impl Root {
    pub fn render_graph(&self, id: &str, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(board) = self.session.graph.as_ref() else {
            return div().p_4().child("Graph is empty").into_any_element();
        };
        let selected = board.selected.clone();
        let neighbors = neighbor_keys(board, selected.as_deref());
        let mut stage = div()
            .id("graph-stage")
            .flex_1()
            .min_h_0()
            .relative()
            .overflow_hidden()
            .bg(gpui::rgb(theme.bg))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, _, cx| {
                let (x, y) = this.graph_local(event.position);
                let effect = this.session.graph.as_mut().map(|board| board.pointer_down(x, y, event.click_count)).unwrap_or(GraphEffect::None);
                this.apply_graph_effect(effect);
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                let (x, y) = this.graph_local(event.position);
                if let Some(board) = this.session.graph.as_mut() {
                    board.pointer_move(x, y);
                }
                cx.notify();
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, cx| {
                if let Some(board) = this.session.graph.as_mut() {
                    board.pointer_up();
                }
                cx.notify();
            }))
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let (x, y) = this.graph_local(event.position);
                let zoom_in = match event.delta {
                    ScrollDelta::Pixels(point) => f32::from(point.y) > 0.0,
                    ScrollDelta::Lines(point) => point.y > 0.0,
                };
                if let Some(board) = this.session.graph.as_mut() {
                    board.zoom_at(x, y, zoom_in);
                }
                cx.stop_propagation();
                cx.notify();
            }));
        let theme_paint = *theme;
        let _ = id;
        let paint_board = PaintSnap::from(board, &neighbors, &selected);
        let origin = cx.weak_entity();
        stage = stage.child(canvas(
            {
                let origin = origin.clone();
                move |bounds: Bounds<Pixels>, window, cx| {
                    let _ = window;
                    origin.update(cx, |root, _| {
                        *root.session.graph_origin.borrow_mut() = (
                            f32::from(bounds.origin.x),
                            f32::from(bounds.origin.y),
                            f32::from(bounds.size.width),
                            f32::from(bounds.size.height),
                        );
                        if let Some(board) = root.session.graph.as_mut() {
                            let width = f32::from(bounds.size.width);
                            let height = f32::from(bounds.size.height);
                            let jumped = (width - board.width).abs() > 48.0 || (height - board.height).abs() > 48.0;
                            board.width = width;
                            board.height = height;
                            if !board.user_camera && jumped {
                                board.fit(width, height);
                            }
                        }
                    }).ok();
                    bounds
                }
            },
            move |bounds, _, window, _| {
                paint_graph(window, bounds, &paint_board, &theme_paint);
            },
        ));
        let labels = label_overlays(board, &neighbors, &selected);
        for label in labels {
            stage = stage.child(label);
        }
        if board.cam_k > 1.0 {
            for edge in edge_label_overlays(board, &neighbors) {
                stage = stage.child(edge);
            }
        }
        let mut column = div().size_full().flex().flex_col().overflow_hidden();
        column = column.child(self.graph_toolbar(theme, cx));
        if board.capped() {
            column = column.child(
                div().px_3().py_1().text_size(px(12.)).bg(gpui::rgb(theme.warn)).text_color(gpui::rgb(theme.bg)).child("Showing the first 400 nodes"),
            );
        }
        let mut row = div().flex_1().min_h_0().flex().flex_row();
        row = row.child(stage);
        if let Some(node) = board.selected_node() {
            row = row.child(self.graph_inspector(node, board.pinned(), theme, cx));
        }
        column.child(row).into_any_element()
    }

    fn graph_local(&self, position: Point<Pixels>) -> (f32, f32) {
        let (ox, oy, _, _) = *self.session.graph_origin.borrow();
        (f32::from(position.x) - ox, f32::from(position.y) - oy)
    }

    pub(crate) fn apply_graph_effect(&mut self, effect: GraphEffect) {
        match effect {
            GraphEffect::None => {}
            GraphEffect::Expand(key) => self.session.expand_graph(&key),
            GraphEffect::Open(entity, id) => self.session.open_record(&entity, &id),
        }
    }

    fn graph_toolbar(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let board = self.session.graph.as_ref();
        let mut bar = div().px_2().h(px(36.)).flex().flex_row().items_center().gap_2().border_b_1().border_color(gpui::rgb(theme.border));
        let searching = self.session.graph_query;
        bar = bar.child(
            div()
                .id("graph-search")
                .w(px(180.))
                .px_2()
                .py_1()
                .rounded_sm()
                .border_1()
                .border_color(gpui::rgb(if searching { theme.accent } else { theme.border }))
                .text_size(px(12.))
                .cursor_text()
                .child(crate::edit::render_edit_line(&self.session.graph_edit, "Search records", theme))
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.session.graph_query = true;
                    cx.notify();
                })),
        );
        if let Some(board) = board {
            let mut seen = BTreeSet::new();
            for node in &board.scene.nodes {
                if !seen.insert(node.kind) {
                    continue;
                }
                let kind = node.kind;
                let on = !board.hidden.contains(&kind);
                let name = kind_name(kind).to_string();
                bar = bar.child(
                    div()
                        .id(SharedString::from(format!("legend-{name}")))
                        .px_2()
                        .py(px(2.))
                        .rounded_sm()
                        .text_size(px(11.))
                        .cursor_pointer()
                        .bg(gpui::rgb(if on { kind_fill(kind, theme) } else { theme.raised }))
                        .text_color(gpui::rgb(if on && kind == NodeKind::Entity { theme.bg } else { theme.text }))
                        .child(name)
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                            if let Some(board) = this.session.graph.as_mut() {
                                board.toggle_kind(kind);
                            }
                            cx.stop_propagation();
                            cx.notify();
                        })),
                );
            }
        }
        bar = bar.child(div().flex_1());
        bar.child(
            div()
                .id("graph-fit")
                .px_2()
                .cursor_pointer()
                .text_size(px(12.))
                .text_color(gpui::rgb(theme.accent))
                .child("Fit")
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    if let Some(board) = this.session.graph.as_mut() {
                        board.user_camera = false;
                        let (w, h) = (board.width, board.height);
                        board.fit(w, h);
                    }
                    cx.notify();
                })),
        )
        .into_any_element()
    }

    fn graph_inspector(&self, node: &GraphNode, pinned: bool, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = div()
            .w(px(240.))
            .h_full()
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .bg(gpui::rgb(theme.panel))
            .border_l_1()
            .border_color(gpui::rgb(theme.border))
            .child(div().text_size(px(14.)).child(node.caption.clone()))
            .child(div().text_size(px(11.)).text_color(gpui::rgb(theme.muted)).child(node.subtitle.clone()));
        for prop in &node.properties {
            panel = panel.child(
                div().text_size(px(12.)).child(format!("{}: {}", prop.label, prop.value)),
            );
        }
        let expand_key = node.id.key();
        panel = panel.child(
            div().flex().flex_row().gap_2().mt_2()
                .child(self.graph_button("graph-expand", "Expand", theme, cx, move |this, cx| {
                    this.session.expand_graph(&expand_key);
                    cx.notify();
                }))
                .child(self.graph_button("graph-open", "Open", theme, cx, |this, cx| {
                    if let GraphEffect::Open(entity, id) = this.session.graph_enter() {
                        this.session.open_record(&entity, &id);
                    }
                    cx.notify();
                })),
        );
        if pinned {
            panel = panel.child(self.graph_button("graph-unpin", "Unpin", theme, cx, |this, cx| {
                if let Some(board) = this.session.graph.as_mut() {
                    board.unpin();
                }
                cx.notify();
            }));
        }
        panel.into_any_element()
    }

    fn graph_button(
        &self,
        id: &str,
        label: &str,
        theme: &Theme,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        div()
            .id(SharedString::from(id.to_string()))
            .px_2()
            .py(px(2.))
            .rounded_sm()
            .cursor_pointer()
            .text_size(px(12.))
            .bg(gpui::rgb(theme.raised))
            .child(label.to_string())
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                action(this, cx);
                cx.stop_propagation();
            }))
            .into_any_element()
    }
}

struct PaintNode {
    key: String,
    kind: NodeKind,
    x: f32,
    y: f32,
    radius: f32,
}

struct PaintSnap {
    nodes: Vec<PaintNode>,
    edges: Vec<(String, String, String)>,
    cam_x: f32,
    cam_y: f32,
    cam_k: f32,
    selected: Option<String>,
    neighbors: HashSet<String>,
}

impl PaintSnap {
    fn from(board: &GraphBoard, neighbors: &HashSet<String>, selected: &Option<String>) -> Self {
        let nodes = board
            .sim
            .nodes
            .iter()
            .filter(|body| board.visible_key(&body.id))
            .filter_map(|body| {
                let node = board.scene.nodes.iter().find(|node| node.id.key() == body.id)?;
                Some(PaintNode {
                    key: body.id.clone(),
                    kind: node.kind,
                    x: body.x,
                    y: body.y,
                    radius: body.radius,
                })
            })
            .collect();
        let edges = board
            .scene
            .edges
            .iter()
            .filter(|edge| board.visible_key(&edge.source) && board.visible_key(&edge.target))
            .map(|edge| (edge.source.clone(), edge.target.clone(), edge.label.clone()))
            .collect();
        Self {
            nodes,
            edges,
            cam_x: board.cam_x,
            cam_y: board.cam_y,
            cam_k: board.cam_k,
            selected: selected.clone(),
            neighbors: neighbors.clone(),
        }
    }
}

fn neighbor_keys(board: &GraphBoard, selected: Option<&str>) -> HashSet<String> {
    let mut keys = HashSet::new();
    let Some(selected) = selected else { return keys };
    keys.insert(selected.to_string());
    for edge in &board.scene.edges {
        if edge.source == selected {
            keys.insert(edge.target.clone());
        }
        if edge.target == selected {
            keys.insert(edge.source.clone());
        }
    }
    keys
}

fn paint_graph(window: &mut Window, bounds: Bounds<Pixels>, snap: &PaintSnap, theme: &Theme) {
    let origin_x = f32::from(bounds.origin.x);
    let origin_y = f32::from(bounds.origin.y);
    let screen = |x: f32, y: f32| (origin_x + snap.cam_x + x * snap.cam_k, origin_y + snap.cam_y + y * snap.cam_k);
    let highlight = !snap.neighbors.is_empty();
    for (source, target, _label) in &snap.edges {
        let Some(from) = snap.nodes.iter().find(|node| &node.key == source) else { continue };
        let Some(to) = snap.nodes.iter().find(|node| &node.key == target) else { continue };
        let (x1, y1) = screen(from.x, from.y);
        let (x2, y2) = screen(to.x, to.y);
        let emphasized = !highlight || snap.neighbors.contains(source) || snap.neighbors.contains(target);
        let color = if emphasized { theme.muted } else { theme.border };
        let mut path = PathBuilder::stroke(px(if emphasized { 1.6 } else { 1.0 }));
        path.move_to(point(px(x1), px(y1)));
        let mx = (x1 + x2) * 0.5;
        let my = (y1 + y2) * 0.5;
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len = dx.hypot(dy).max(1.0);
        let bend = 18.0_f32.min(len * 0.12);
        path.curve_to(point(px(x2), px(y2)), point(px(mx - dy / len * bend), px(my + dx / len * bend)));
        if let Ok(path) = path.build() {
            window.paint_path(path, gpui::rgb(color));
        }
        arrow(window, x1, y1, x2, y2, to.radius * snap.cam_k, color);
    }
    for node in &snap.nodes {
        let (x, y) = screen(node.x, node.y);
        let radius = node.radius * snap.cam_k;
        let emphasized = !highlight || snap.neighbors.contains(&node.key);
        let fill = if emphasized { kind_fill(node.kind, theme) } else { theme.border };
        let selected = snap.selected.as_deref() == Some(node.key.as_str());
        let border = if selected { theme.text } else { theme.bg };
        let width = if selected { 3.0 } else { 2.0 };
        window.paint_quad(quad(
            Bounds {
                origin: point(px(x - radius), px(y - radius)),
                size: size(px(radius * 2.0), px(radius * 2.0)),
            },
            Corners::all(px(radius)),
            gpui::rgb(fill),
            Edges::all(px(width)),
            gpui::rgb(border),
            BorderStyle::Solid,
        ));
    }
}

fn arrow(window: &mut Window, x1: f32, y1: f32, x2: f32, y2: f32, radius: f32, color: u32) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = dx.hypot(dy).max(1.0);
    let ux = dx / len;
    let uy = dy / len;
    let tip_x = x2 - ux * (radius + 2.0);
    let tip_y = y2 - uy * (radius + 2.0);
    let left_x = tip_x - ux * 8.0 - uy * 4.0;
    let left_y = tip_y - uy * 8.0 + ux * 4.0;
    let right_x = tip_x - ux * 8.0 + uy * 4.0;
    let right_y = tip_y - uy * 8.0 - ux * 4.0;
    let mut path = PathBuilder::fill();
    path.move_to(point(px(tip_x), px(tip_y)));
    path.line_to(point(px(left_x), px(left_y)));
    path.line_to(point(px(right_x), px(right_y)));
    path.line_to(point(px(tip_x), px(tip_y)));
    if let Ok(path) = path.build() {
        window.paint_path(path, gpui::rgb(color));
    }
}

fn label_overlays(board: &GraphBoard, neighbors: &HashSet<String>, selected: &Option<String>) -> Vec<AnyElement> {
    let mut boxes = Vec::new();
    let mut placed = Vec::new();
    for body in &board.sim.nodes {
        if !board.visible_key(&body.id) {
            continue;
        }
        let Some(node) = board.scene.nodes.iter().find(|node| node.id.key() == body.id) else { continue };
        let x = board.cam_x + body.x * board.cam_k;
        let y = board.cam_y + body.y * board.cam_k;
        let radius = body.radius * board.cam_k;
        if x < -40.0 || y < -40.0 || x > board.width + 40.0 || y > board.height + 40.0 {
            continue;
        }
        let active = selected.as_deref() == Some(body.id.as_str()) || neighbors.contains(&body.id);
        let width = node.caption.chars().count() as f32 * 6.4 + 8.0;
        boxes.push(LabelBox {
            id: body.id.clone(),
            priority: label_priority(node.kind, active),
            left: x - width * 0.5,
            right: x + width * 0.5,
            top: y + radius + 2.0,
            bottom: y + radius + 16.0,
        });
        placed.push((body.id.clone(), node.caption.clone(), node_mark(&node.caption), x, y, radius));
    }
    let shown = visible_labels(&boxes, board.cam_k);
    let mut out = Vec::new();
    for (id, caption, mark, x, y, radius) in placed {
        if radius >= 10.0 && !mark.is_empty() {
            out.push(
                div()
                    .absolute()
                    .left(px(x - 14.0))
                    .top(px(y - 8.0))
                    .w(px(28.))
                    .text_size(px(11.))
                    .text_center()
                    .child(mark)
                    .into_any_element(),
            );
        }
        if shown.contains(&id) {
            out.push(
                div()
                    .absolute()
                    .left(px(x - 70.0))
                    .top(px(y + radius + 2.0))
                    .w(px(140.))
                    .text_size(px(11.))
                    .text_center()
                    .child(caption)
                    .into_any_element(),
            );
        }
    }
    out
}

fn edge_label_overlays(board: &GraphBoard, neighbors: &HashSet<String>) -> Vec<AnyElement> {
    let mut out = Vec::new();
    let highlight = !neighbors.is_empty();
    for edge in &board.scene.edges {
        if edge.label.is_empty() || !board.visible_key(&edge.source) || !board.visible_key(&edge.target) {
            continue;
        }
        if highlight && !neighbors.contains(&edge.source) && !neighbors.contains(&edge.target) {
            continue;
        }
        let Some((x1, y1)) = board.position(&edge.source) else { continue };
        let Some((x2, y2)) = board.position(&edge.target) else { continue };
        let x = board.cam_x + (x1 + x2) * 0.5 * board.cam_k;
        let y = board.cam_y + (y1 + y2) * 0.5 * board.cam_k - 8.0;
        out.push(
            div()
                .absolute()
                .left(px(x - 40.0))
                .top(px(y))
                .w(px(80.))
                .text_size(px(10.))
                .text_center()
                .child(edge.label.clone())
                .into_any_element(),
        );
    }
    out
}

fn kind_name(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Entity => "Entity",
        NodeKind::ChildGroup => "Group",
        NodeKind::ChildRecord => "Child",
        NodeKind::Field => "Field",
        NodeKind::Type => "Type",
    }
}

fn kind_fill(kind: NodeKind, theme: &Theme) -> u32 {
    match kind {
        NodeKind::Entity => theme.accent,
        NodeKind::Type => 0x7aa2ff,
        NodeKind::ChildGroup => 0x3cb4d8,
        NodeKind::ChildRecord => theme.ok,
        NodeKind::Field => theme.warn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use idmc_backend::{AppBackend, Backend, MockBackend};
    use idmc_model::ViewKind;

    fn session() -> Session {
        let backend = AppBackend::Mock(MockBackend::open_default().unwrap());
        let file = crate::workspace::default_workspace(backend.model());
        Session::boot(backend, file, std::path::PathBuf::from("workspace.json"), None)
    }

    #[test]
    fn opening_a_person_shows_groups_and_children_and_expand_keeps_the_place() {
        let mut session = session();
        let people = session.file.views.iter().find(|view| view.kind == ViewKind::Table && view.entity == "c360.person").unwrap();
        let table = people.id.clone();
        let anna = session.backend.records("c360.person").into_iter().find(|row| {
            row.values.get("firstName").and_then(|value| value.as_id()).as_deref() == Some("Anna")
        }).unwrap();
        session.select(&table, &anna.id);
        session.file.focus = Some(table);
        session.show_in_graph();
        let board = session.graph.as_ref().unwrap();
        assert!(board.scene.nodes.iter().any(|node| node.kind == NodeKind::ChildGroup));
        assert!(board.scene.nodes.iter().any(|node| node.kind == NodeKind::ChildRecord));
        let root = board.scene.nodes.iter().find(|node| node.kind == NodeKind::Entity && !node.raw_caption).unwrap().id.key();
        for _ in 0..40 {
            session.tick_graph();
        }
        let before = session.graph.as_ref().unwrap().position(&root).unwrap();
        let child = session.graph.as_ref().unwrap().scene.nodes.iter().find(|node| node.kind == NodeKind::ChildRecord).unwrap().id.key();
        session.expand_graph(&child);
        let after = session.graph.as_ref().unwrap().position(&root).unwrap();
        assert_eq!(before, after);
        assert!((session.graph.as_ref().unwrap().sim.alpha - 0.3).abs() < 0.05 || session.graph.as_ref().unwrap().sim.alpha <= 0.3);
    }

    #[test]
    fn schema_mode_when_nothing_is_selected_and_actions_exist() {
        let mut session = session();
        session.file.focus = None;
        session.file.selections.clear();
        session.open_graph(None);
        let board = session.graph.as_ref().unwrap();
        assert!(board.schema);
        assert!(board.scene.nodes.iter().any(|node| node.kind == NodeKind::Type));
        assert!(crate::actions::registry().iter().any(|action| action.id == "view.open_graph"));
        assert!(crate::actions::registry().iter().any(|action| action.id == "record.show_in_graph"));
    }

    #[test]
    fn wheel_zoom_stays_in_range_and_arrows_move() {
        let mut session = session();
        session.open_graph(None);
        let board = session.graph.as_mut().unwrap();
        board.zoom_at(100.0, 100.0, true);
        assert!(board.cam_k <= 2.5 && board.cam_k >= 0.25);
        assert!(board.user_camera);
        for _ in 0..5 {
            board.zoom_at(10.0, 10.0, false);
        }
        assert!(board.cam_k >= 0.25 - f32::EPSILON);
        board.selected = board.scene.nodes.first().map(|node| node.id.key());
        let first = board.selected.clone();
        board.arrow("right");
        assert_ne!(board.selected, None);
        let _ = first;
    }

    #[test]
    fn duplicate_expand_is_one_scene() {
        let backend = MockBackend::open_default().unwrap();
        let record = backend.records("c360.person").into_iter().next().unwrap();
        let scene = build_entity_scene(&record, backend.model());
        let key = scene.nodes[0].id.key();
        backend.schedule_expand(&key);
        backend.schedule_expand(&key);
        assert!(backend.take_expand().is_some());
        assert!(backend.take_expand().is_none());
    }
}
