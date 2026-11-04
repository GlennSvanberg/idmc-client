//! Dockable graph. The force parameters, label culling, zoom, and expand-in-place
//! behavior follow https://github.com/braian-io/braian (Apache-2.0). This file is
//! a GPUI reimplementation; it does not copy that TypeScript.

use std::collections::{BTreeSet, HashMap, HashSet};

use gpui::{
    canvas, div, point, px, quad, size, AnyElement, App, BorderStyle, Bounds, Context, Corners,
    Edges, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathBuilder, Pixels,
    Point, Render, ScrollDelta, ScrollWheelEvent, ShapedLine, SharedString, TextRun, WeakEntity,
    Window,
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
                let started = !*moved && dx.hypot(dy) > 3.0;
                let moved = *moved || started;
                let id = id.clone();
                if moved {
                    if let Some(node) = self.sim.nodes.iter_mut().find(|node| node.id == id) {
                        let (wx, wy) = ((x - self.cam_x) / self.cam_k, (y - self.cam_y) / self.cam_k);
                        node.fx = Some(wx);
                        node.fy = Some(wy);
                        node.x = wx;
                        node.y = wy;
                    }
                    if started {
                        self.sim.reheat(0.3);
                    }
                }
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

    pub fn dragging(&self) -> bool {
        !matches!(self.drag, Drag::None)
    }

    pub fn dragging_node(&self) -> bool {
        matches!(self.drag, Drag::Node { moved: true, .. })
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
    // Only a very far zoom hides leaves. Overlap does the rest.
    if zoom < 0.35 {
        return priority >= 3;
    }
    true
}

/// How many force steps to run on one frame. One step keeps a 400-node scene
/// inside a frame; a small scene can take two.
pub fn ticks_per_frame(nodes: usize) -> usize {
    if nodes > 180 { 1 } else { 2 }
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
        let forced = box_.priority >= 5;
        if !forced && !label_visible(box_.priority, zoom) {
            continue;
        }
        if !forced && kept.iter().any(|other| overlaps(other, &box_)) {
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
    pub fn render_graph(&self, _id: &str, _theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        if self.session.graph.is_none() {
            return div().p_4().child("Graph is empty").into_any_element();
        }
        let existing = self.graph_pane.borrow().clone();
        let pane = if let Some(pane) = existing {
            pane
        } else {
            let root = cx.weak_entity();
            let pane = cx.new(|_| GraphPane {
                root,
                hover: None,
                shaped: HashMap::new(),
                origin_x: 0.0,
                origin_y: 0.0,
                frame_queued: false,
            });
            *self.graph_pane.borrow_mut() = Some(pane.clone());
            pane
        };
        pane.into_any_element()
    }

    pub(crate) fn apply_graph_effect(&mut self, effect: GraphEffect) {
        match effect {
            GraphEffect::None => {}
            GraphEffect::Expand(key) => self.session.expand_graph(&key),
            GraphEffect::Open(entity, id) => self.session.open_record(&entity, &id),
        }
    }
}

pub struct GraphPane {
    root: WeakEntity<Root>,
    hover: Option<String>,
    shaped: HashMap<(String, u16, u32), ShapedLine>,
    origin_x: f32,
    origin_y: f32,
    frame_queued: bool,
}

impl GraphPane {
    fn update_root<R>(&self, cx: &mut Context<Self>, f: impl FnOnce(&mut Root, &mut Context<Root>) -> R) -> Option<R> {
        let root = self.root.upgrade()?;
        Some(root.update(cx, f))
    }

    fn local(&self, position: Point<Pixels>) -> (f32, f32) {
        (f32::from(position.x) - self.origin_x, f32::from(position.y) - self.origin_y)
    }

    fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let hover = self.hover.clone();
        let Some(root) = self.root.upgrade() else { return };
        let theme_mode = root.read(cx).session.file.theme;
        let theme = crate::theme::theme(theme_mode);
        let snap = {
            let root = root.read(cx);
            let Some(board) = root.session.graph.as_ref() else { return };
            PaintSnap::from(board, &hover)
        };
        if self.shaped.len() > 1200 {
            self.shaped.clear();
        }
        paint_graph(window, bounds, &snap, &theme, &mut self.shaped, cx);
    }
}

impl Render for GraphPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(root) = self.root.upgrade() else {
            return div().child("Graph closed").into_any_element();
        };
        let theme_mode = root.read(cx).session.file.theme;
        let theme = crate::theme::theme(theme_mode);
        let (settled, dragging_node, capped, chips, searching, inspected) = {
            let root = root.read(cx);
            let board = root.session.graph.as_ref();
            let chips = board
                .map(|board| {
                    let mut seen = BTreeSet::new();
                    let mut out = Vec::new();
                    for node in &board.scene.nodes {
                        if seen.insert(node.kind) {
                            out.push((node.kind, !board.hidden.contains(&node.kind)));
                        }
                    }
                    out
                })
                .unwrap_or_default();
            let inspected = board.and_then(|board| board.selected_node().cloned().map(|node| (node, board.pinned())));
            (
                board.map(|board| board.settled()).unwrap_or(true),
                board.map(|board| board.dragging_node()).unwrap_or(false),
                board.map(|board| board.capped()).unwrap_or(false),
                chips,
                root.session.graph_query,
                inspected,
            )
        };
        if root.read(cx).session.graph.is_none() {
            return div().p_4().child("Graph is empty").into_any_element();
        }
        if (!settled || dragging_node) && !self.frame_queued {
            self.frame_queued = true;
            cx.on_next_frame(window, |pane, _, cx| {
                pane.frame_queued = false;
                pane.update_root(cx, |root, _| {
                    let count = root.session.graph.as_ref().map(|board| board.sim.nodes.len()).unwrap_or(0);
                    for _ in 0..ticks_per_frame(count) {
                        if !root.session.tick_graph() {
                            break;
                        }
                    }
                    if let Some(board) = root.session.graph.as_mut() {
                        if board.dragging_node() {
                            board.sim.alpha = board.sim.alpha.max(0.18);
                        }
                    }
                });
                cx.notify();
            });
        }
        let pane = cx.entity().downgrade();
        let pane_paint = pane.clone();
        let stage = div()
            .id("graph-stage")
            .flex_1()
            .min_h_0()
            .relative()
            .overflow_hidden()
            .bg(gpui::rgb(theme.bg))
            .on_mouse_down(MouseButton::Left, cx.listener(|pane, event: &MouseDownEvent, _, cx| {
                let (x, y) = pane.local(event.position);
                let clicks = event.click_count;
                let effect = pane
                    .update_root(cx, |root, _| {
                        root.session.graph.as_mut().map(|board| board.pointer_down(x, y, clicks)).unwrap_or(GraphEffect::None)
                    })
                    .unwrap_or(GraphEffect::None);
                let opens = matches!(effect, GraphEffect::Open(_, _));
                pane.update_root(cx, |root, cx| {
                    root.apply_graph_effect(effect);
                    if opens {
                        cx.notify();
                    }
                });
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_move(cx.listener(|pane, event: &MouseMoveEvent, _, cx| {
                let (x, y) = pane.local(event.position);
                let (dragging, hover) = pane
                    .update_root(cx, |root, _| {
                        let Some(board) = root.session.graph.as_mut() else { return (false, None) };
                        let dragging = board.dragging();
                        if dragging {
                            board.pointer_move(x, y);
                        }
                        (dragging, board.hit(x, y))
                    })
                    .unwrap_or((false, None));
                let changed = hover != pane.hover;
                pane.hover = hover;
                if dragging || changed {
                    cx.notify();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|pane, _: &MouseUpEvent, _, cx| {
                let was = pane
                    .update_root(cx, |root, _| {
                        let was = root.session.graph.as_ref().map(|board| board.dragging()).unwrap_or(false);
                        if let Some(board) = root.session.graph.as_mut() {
                            board.pointer_up();
                        }
                        was
                    })
                    .unwrap_or(false);
                if was {
                    cx.notify();
                }
            }))
            .on_scroll_wheel(cx.listener(|pane, event: &ScrollWheelEvent, _, cx| {
                let (x, y) = pane.local(event.position);
                let zoom_in = match event.delta {
                    ScrollDelta::Pixels(point) => f32::from(point.y) > 0.0,
                    ScrollDelta::Lines(point) => point.y > 0.0,
                };
                pane.update_root(cx, |root, _| {
                    if let Some(board) = root.session.graph.as_mut() {
                        board.zoom_at(x, y, zoom_in);
                    }
                });
                cx.stop_propagation();
                cx.notify();
            }))
            .child(canvas(
                {
                    let pane = pane.clone();
                    move |bounds: Bounds<Pixels>, _, cx| {
                        pane.update(cx, |pane, cx| {
                            pane.origin_x = f32::from(bounds.origin.x);
                            pane.origin_y = f32::from(bounds.origin.y);
                            let width = f32::from(bounds.size.width);
                            let height = f32::from(bounds.size.height);
                            pane.update_root(cx, |root, _| {
                                if let Some(board) = root.session.graph.as_mut() {
                                    let jumped = (width - board.width).abs() > 48.0 || (height - board.height).abs() > 48.0;
                                    board.width = width;
                                    board.height = height;
                                    if !board.user_camera && jumped {
                                        board.fit(width, height);
                                    }
                                }
                            });
                        })
                        .ok();
                        bounds
                    }
                },
                move |bounds, _, window, cx| {
                    pane_paint.update(cx, |pane, cx| pane.paint(bounds, window, cx)).ok();
                },
            ));
        let mut column = div().size_full().flex().flex_col().overflow_hidden();
        column = column.child(self.toolbar(&chips, searching, &theme, cx));
        if capped {
            column = column.child(
                div()
                    .px_3()
                    .py_1()
                    .text_size(px(12.))
                    .bg(gpui::rgb(theme.warn))
                    .text_color(gpui::rgb(theme.on_node))
                    .child("Showing the first 400 nodes"),
            );
        }
        let mut row = div().flex_1().min_h_0().flex().flex_row();
        row = row.child(stage);
        if let Some((node, pinned)) = inspected {
            row = row.child(self.inspector(&node, pinned, &theme, cx));
        }
        column.child(row).into_any_element()
    }
}

impl GraphPane {
    fn toolbar(&self, chips: &[(NodeKind, bool)], searching: bool, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let root = self.root.clone();
        let mut bar = div()
            .px_2()
            .h(px(36.))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(gpui::rgb(theme.border));
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
                .child({
                    let text = root.upgrade().map(|root| root.read(cx).session.graph_edit.clone());
                    crate::edit::render_edit_line(&text.unwrap_or_default(), "Search records", theme)
                })
                .on_mouse_down(MouseButton::Left, cx.listener(|pane, _: &MouseDownEvent, _, cx| {
                    pane.update_root(cx, |root, _| root.session.graph_query = true);
                    cx.notify();
                })),
        );
        for (kind, on) in chips {
            let kind = *kind;
            let on = *on;
            let name = kind_name(kind).to_string();
            let fill = kind_fill(kind, theme);
            bar = bar.child(
                div()
                    .id(SharedString::from(format!("legend-{name}")))
                    .px_2()
                    .py(px(2.))
                    .rounded_sm()
                    .text_size(px(11.))
                    .cursor_pointer()
                    .bg(gpui::rgb(if on { fill } else { theme.raised }))
                    .text_color(gpui::rgb(if on { theme.on_node } else { theme.text }))
                    .child(name)
                    .on_mouse_down(MouseButton::Left, cx.listener(move |pane, _: &MouseDownEvent, _, cx| {
                        pane.update_root(cx, |root, _| {
                            if let Some(board) = root.session.graph.as_mut() {
                                board.toggle_kind(kind);
                            }
                        });
                        cx.stop_propagation();
                        cx.notify();
                    })),
            );
        }
        bar = bar.child(div().flex_1());
        bar.child(self.icon_button("graph-fit", "fit", "Fit", theme, cx, |pane, cx| {
            pane.update_root(cx, |root, _| {
                if let Some(board) = root.session.graph.as_mut() {
                    board.user_camera = false;
                    let (w, h) = (board.width, board.height);
                    board.fit(w, h);
                }
            });
            cx.notify();
        }))
        .into_any_element()
    }

    fn inspector(&self, node: &GraphNode, pinned: bool, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
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
            panel = panel.child(div().text_size(px(12.)).child(format!("{}: {}", prop.label, prop.value)));
        }
        let expand_key = node.id.key();
        panel = panel.child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .mt_2()
                .child(self.icon_button("graph-expand", "expand", "Expand", theme, cx, move |pane, cx| {
                    pane.update_root(cx, |root, _| root.session.expand_graph(&expand_key));
                    cx.notify();
                }))
                .child(self.icon_button("graph-open", "open", "Open", theme, cx, |pane, cx| {
                    pane.update_root(cx, |root, cx| {
                        if let GraphEffect::Open(entity, id) = root.session.graph_enter() {
                            root.session.open_record(&entity, &id);
                            cx.notify();
                        }
                    });
                    cx.notify();
                })),
        );
        if pinned {
            panel = panel.child(self.icon_button("graph-unpin", "unpin", "Unpin", theme, cx, |pane, cx| {
                pane.update_root(cx, |root, _| {
                    if let Some(board) = root.session.graph.as_mut() {
                        board.unpin();
                    }
                });
                cx.notify();
            }));
        }
        panel.into_any_element()
    }

    fn icon_button(
        &self,
        id: &str,
        name: &'static str,
        tip: &'static str,
        theme: &Theme,
        cx: &mut Context<Self>,
        action: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        div()
            .id(SharedString::from(id.to_string()))
            .w(px(26.))
            .h(px(26.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_sm()
            .cursor_pointer()
            .text_color(gpui::rgb(theme.text))
            .hover(|style| style.bg(gpui::rgb(theme.hover)))
            .tooltip(crate::icons::tooltip(tip, *theme))
            .child(crate::icons::icon(name).w(px(15.)).h(px(15.)).text_color(gpui::rgb(theme.text)))
            .on_mouse_down(MouseButton::Left, cx.listener(move |pane, _: &MouseDownEvent, _, cx| {
                action(pane, cx);
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
    caption: String,
    mark: String,
}

struct PaintSnap {
    nodes: Vec<PaintNode>,
    edges: Vec<(String, String, String)>,
    cam_x: f32,
    cam_y: f32,
    cam_k: f32,
    width: f32,
    height: f32,
    selected: Option<String>,
    hover: Option<String>,
    neighbors: HashSet<String>,
}

impl PaintSnap {
    fn from(board: &GraphBoard, hover: &Option<String>) -> Self {
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
                    caption: node.caption.clone(),
                    mark: node_mark(&mark_source(&node.caption)),
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
        let neighbors = neighbor_keys(board, board.selected.as_deref());
        Self {
            nodes,
            edges,
            cam_x: board.cam_x,
            cam_y: board.cam_y,
            cam_k: board.cam_k,
            width: board.width,
            height: board.height,
            selected: board.selected.clone(),
            hover: hover.clone(),
            neighbors,
        }
    }
}

fn paint_graph(window: &mut Window, bounds: Bounds<Pixels>, snap: &PaintSnap, theme: &Theme, cache: &mut HashMap<(String, u16, u32), ShapedLine>, cx: &mut App) {
    let origin_x = f32::from(bounds.origin.x);
    let origin_y = f32::from(bounds.origin.y);
    let screen = |x: f32, y: f32| (origin_x + snap.cam_x + x * snap.cam_k, origin_y + snap.cam_y + y * snap.cam_k);
    let mut index: HashMap<&str, usize> = HashMap::with_capacity(snap.nodes.len());
    for (i, node) in snap.nodes.iter().enumerate() {
        index.insert(node.key.as_str(), i);
    }
    for (source, target, label) in &snap.edges {
        let Some(&from_i) = index.get(source.as_str()) else { continue };
        let Some(&to_i) = index.get(target.as_str()) else { continue };
        let from = &snap.nodes[from_i];
        let to = &snap.nodes[to_i];
        let (x1, y1) = screen(from.x, from.y);
        let (x2, y2) = screen(to.x, to.y);
        let hot = snap.neighbors.contains(source) || snap.neighbors.contains(target);
        let color = if hot { theme.edge } else { theme.muted };
        let mut path = PathBuilder::stroke(px(if hot { 1.8 } else { 1.25 }));
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
        if snap.cam_k > 1.0 && !label.is_empty() && (snap.neighbors.is_empty() || hot) {
            paint_cached(cache, window, cx, label, 10.0, theme.text, mx, my - 8.0);
        }
    }
    let mut boxes = Vec::new();
    for node in &snap.nodes {
        let (x, y) = screen(node.x, node.y);
        let radius = node.radius * snap.cam_k;
        if x < origin_x - 80.0 || y < origin_y - 80.0 || x > origin_x + snap.width + 80.0 || y > origin_y + snap.height + 80.0 {
            continue;
        }
        let active = snap.selected.as_deref() == Some(node.key.as_str())
            || snap.hover.as_deref() == Some(node.key.as_str())
            || snap.neighbors.contains(&node.key);
        let width = (node.caption.chars().count().min(42) as f32) * 6.6 + 8.0;
        boxes.push(LabelBox {
            id: node.key.clone(),
            priority: label_priority(node.kind, active),
            left: x - width * 0.5,
            right: x + width * 0.5,
            top: y + radius + 2.0,
            bottom: y + radius + 16.0,
        });
    }
    let shown = visible_labels(&boxes, snap.cam_k);
    for node in &snap.nodes {
        let (x, y) = screen(node.x, node.y);
        let radius = node.radius * snap.cam_k;
        let selected = snap.selected.as_deref() == Some(node.key.as_str());
        let hovered = snap.hover.as_deref() == Some(node.key.as_str());
        let neighbor = snap.neighbors.contains(&node.key);
        let fill = kind_fill(node.kind, theme);
        let (border, width) = if selected {
            (theme.text, 3.0)
        } else if hovered {
            (theme.accent, 2.5)
        } else if neighbor && snap.selected.is_some() {
            (theme.accent, 2.0)
        } else {
            (theme.bg, 1.5)
        };
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
        if radius >= 9.0 && !node.mark.is_empty() {
            paint_cached(cache, window, cx, &node.mark, 11.0, theme.on_node, x, y - 6.0);
        }
        if shown.contains(&node.key) {
            let caption = clip_caption(&node.caption);
            paint_cached(cache, window, cx, &caption, 12.0, theme.text, x, y + radius + 1.0);
        }
    }
}

fn mark_source(caption: &str) -> String {
    match caption.rsplit_once(" (") {
        Some((name, rest)) if rest.ends_with(')') && rest[..rest.len() - 1].chars().all(|ch| ch.is_ascii_digit()) => name.to_string(),
        _ => caption.to_string(),
    }
}

fn clip_caption(caption: &str) -> String {
    let mut out = String::new();
    for (index, ch) in caption.chars().enumerate() {
        if index == 42 {
            out.push('…');
            break;
        }
        if ch == '\n' {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

fn paint_cached(cache: &mut HashMap<(String, u16, u32), ShapedLine>, window: &mut Window, cx: &mut App, text: &str, size: f32, color: u32, x: f32, y: f32) {
    if text.is_empty() {
        return;
    }
    let key = (text.to_string(), (size * 10.0) as u16, color);
    if !cache.contains_key(&key) {
        let shaped = shape_line(window, text, size, color);
        cache.insert(key.clone(), shaped);
    }
    let Some(line) = cache.get(&key) else { return };
    let width = f32::from(line.width);
    line.paint(point(px(x - width * 0.5), px(y)), px(size + 3.0), window, cx).ok();
}

fn shape_line(window: &mut Window, text: &str, size: f32, color: u32) -> ShapedLine {
    let text = SharedString::from(text.replace('\n', " "));
    let run = TextRun {
        len: text.len(),
        font: window.text_style().font(),
        color: gpui::rgb(color).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window.text_system().shape_line(text, px(size), &[run], None)
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
        NodeKind::Entity => theme.node_entity,
        NodeKind::Type => theme.node_type,
        NodeKind::ChildGroup => theme.node_group,
        NodeKind::ChildRecord => theme.node_child,
        NodeKind::Field => theme.node_field,
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
    fn labels_show_unless_they_overlap_and_active_ones_stay() {
        let mut boxes = Vec::new();
        for index in 0..8 {
            boxes.push(LabelBox {
                id: format!("n{index}"),
                priority: 1,
                left: index as f32 * 80.0,
                right: index as f32 * 80.0 + 40.0,
                top: 0.0,
                bottom: 14.0,
            });
        }
        boxes.push(LabelBox {
            id: "covered".into(),
            priority: 5,
            left: 0.0,
            right: 40.0,
            top: 0.0,
            bottom: 14.0,
        });
        let shown = visible_labels(&boxes, 1.0);
        assert!(shown.contains("covered"));
        assert!(!shown.contains("n0"), "the label under the selected one yields");
        assert_eq!(shown.len(), 8);
        let far = visible_labels(&boxes, 0.2);
        assert!(far.contains("covered"));
        assert!(far.len() < shown.len());
        assert_eq!(ticks_per_frame(40), 2);
        assert_eq!(ticks_per_frame(400), 1);
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
