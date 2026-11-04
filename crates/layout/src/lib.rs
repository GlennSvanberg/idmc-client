//! Tiling layout tree: splits with ratios, tab groups as leaves.
mod force;

pub use force::{Body, NodeSpec, Simulation, Spring};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Dir { Horizontal, Vertical }

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Zone { Left, Right, Top, Bottom, Center }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node {
    /// Horizontal = children side by side, Vertical = stacked.
    Split { dir: Dir, ratios: Vec<f32>, children: Vec<Node> },
    Tabs { views: Vec<String>, active: usize },
}

#[derive(Clone, Copy, Debug)]
pub struct Rect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }

impl Default for Node { fn default() -> Self { Node::Tabs { views: vec![], active: 0 } } }

impl Node {
    pub fn tabs(v: &str) -> Node { Node::Tabs { views: vec![v.to_string()], active: 0 } }

    pub fn views(&self) -> Vec<String> {
        match self {
            Node::Tabs { views, .. } => views.clone(),
            Node::Split { children, .. } => children.iter().flat_map(|c| c.views()).collect(),
        }
    }
    pub fn contains(&self, v: &str) -> bool { self.views().iter().any(|x| x == v) }

    /// Visible (active) views with their unit-square rects.
    pub fn leaves(&self, r: Rect, out: &mut Vec<(String, Rect)>) {
        match self {
            Node::Tabs { views, active } => { if let Some(v) = views.get(*active).or(views.first()) { out.push((v.clone(), r)); } }
            Node::Split { dir, ratios, children } => {
                let mut off = 0.0;
                for (c, q) in children.iter().zip(ratios) {
                    let cr = match dir {
                        Dir::Horizontal => Rect { x: r.x + off * r.w, y: r.y, w: q * r.w, h: r.h },
                        Dir::Vertical => Rect { x: r.x, y: r.y + off * r.h, w: r.w, h: q * r.h },
                    };
                    c.leaves(cr, out);
                    off += q;
                }
            }
        }
    }

    fn normalize(&mut self) {
        if let Node::Split { ratios, children, .. } = self {
            for c in children.iter_mut() { c.normalize(); }
            let mut i = 0;
            while i < children.len() {
                if matches!(&children[i], Node::Tabs { views, .. } if views.is_empty()) { children.remove(i); ratios.remove(i); } else { i += 1; }
            }
            let s: f32 = ratios.iter().sum();
            if s > 0.0 { for r in ratios.iter_mut() { *r /= s; } }
            if children.len() == 1 { *self = children.remove(0); }
            else if children.is_empty() { *self = Node::default(); }
        }
        if let Node::Tabs { views, active } = self { if *active >= views.len() { *active = views.len().saturating_sub(1); } }
    }

    pub fn remove(&mut self, v: &str) -> bool {
        let found = self.remove_inner(v);
        self.normalize();
        found
    }
    fn remove_inner(&mut self, v: &str) -> bool {
        match self {
            Node::Tabs { views, .. } => { let n = views.len(); views.retain(|x| x != v); n != views.len() }
            Node::Split { children, .. } => children.iter_mut().any(|c| c.remove_inner(v)),
        }
    }

    pub fn activate(&mut self, v: &str) -> bool {
        match self {
            Node::Tabs { views, active } => { if let Some(i) = views.iter().position(|x| x == v) { *active = i; true } else { false } }
            Node::Split { children, .. } => children.iter_mut().any(|c| c.activate(v)),
        }
    }

    /// Dock `v` at a window edge (wraps the whole layout): Left/Right/Top/Bottom; `share` = its part of the window (0.1–0.9).
    pub fn dock_edge(&mut self, v: &str, zone: Zone, share: f32) {
        self.remove(v);
        if zone == Zone::Center || self.views().is_empty() { if self.views().is_empty() { *self = Node::tabs(v); } else { let t = self.views()[0].clone(); self.insert(&t, v, Zone::Center); } return; }
        let dir = if matches!(zone, Zone::Left | Zone::Right) { Dir::Horizontal } else { Dir::Vertical };
        let s = share.clamp(0.1, 0.9);
        let old = std::mem::take(self);
        let (children, ratios) = if matches!(zone, Zone::Left | Zone::Top) { (vec![Node::tabs(v), old], vec![s, 1. - s]) } else { (vec![old, Node::tabs(v)], vec![1. - s, s]) };
        *self = Node::Split { dir, ratios, children };
    }
    /// Move tab `v` to position `idx` within its pane (clamped) and activate it.
    pub fn set_tab_index(&mut self, v: &str, idx: usize) -> bool {
        match self {
            Node::Tabs { views, active } => { let Some(i) = views.iter().position(|x| x == v) else { return false }; let x = views.remove(i); let j = idx.min(views.len()); views.insert(j, x); *active = j; true }
            Node::Split { children, .. } => children.iter_mut().any(|c| c.set_tab_index(v, idx)),
        }
    }

    /// Insert `v` relative to the pane holding `target`. Center = new tab.
    pub fn insert(&mut self, target: &str, v: &str, zone: Zone) -> bool {
        match self {
            Node::Tabs { views, active } if views.iter().any(|x| x == target) => {
                if zone == Zone::Center { views.push(v.into()); *active = views.len() - 1; return true; }
                let dir = if matches!(zone, Zone::Left | Zone::Right) { Dir::Horizontal } else { Dir::Vertical };
                let old = std::mem::take(self);
                let new = Node::tabs(v);
                let children = if matches!(zone, Zone::Left | Zone::Top) { vec![new, old] } else { vec![old, new] };
                *self = Node::Split { dir, ratios: vec![0.5, 0.5], children };
                true
            }
            Node::Tabs { .. } => false,
            Node::Split { children, .. } => children.iter_mut().any(|c| c.insert(target, v, zone)),
        }
    }

    pub fn move_view(&mut self, v: &str, target: &str, zone: Zone) -> Result<(), String> {
        if v == target && zone == Zone::Center { return Ok(()); }
        if !self.contains(target) { return Err(format!("view {target} not in layout")); }
        // Dropping a tab onto its own pane (the usual case: the pane's drop
        // target is the active tab, which is the one being dragged) has to
        // split against a sibling tab. Removing `v` first would delete the
        // target and the move would roll back.
        let dock_on = if v == target {
            self.sibling_in_pane(v).ok_or_else(|| "cannot dock a view onto itself".to_string())?
        } else {
            target.to_string()
        };
        let snapshot = self.clone();
        self.remove(v);
        if !self.contains(&dock_on) || !self.insert(&dock_on, v, zone) {
            *self = snapshot;
            return Err("cannot dock a view onto itself".into());
        }
        self.normalize();
        self.activate(v);
        Ok(())
    }

    /// Another tab in the same pane as `v`, preferring one that stays behind when `v` leaves.
    fn sibling_in_pane(&self, v: &str) -> Option<String> {
        match self {
            Node::Tabs { views, active } if views.iter().any(|x| x == v) => {
                if let Some(current) = views.get(*active) {
                    if current != v {
                        return Some(current.clone());
                    }
                }
                views.iter().find(|x| x.as_str() != v).cloned()
            }
            Node::Tabs { .. } => None,
            Node::Split { children, .. } => children.iter().find_map(|c| c.sibling_in_pane(v)),
        }
    }

    pub fn swap(&mut self, a: &str, b: &str) {
        fn walk(n: &mut Node, a: &str, b: &str) {
            match n {
                Node::Tabs { views, .. } => for x in views.iter_mut() { if x == a { *x = b.into() } else if x == b { *x = a.into() } },
                Node::Split { children, .. } => for c in children { walk(c, a, b) },
            }
        }
        walk(self, a, b)
    }

    /// Move the divider after the pane containing `v` in the nearest split of `dir` by `delta` (fraction).
    pub fn resize(&mut self, v: &str, dir: Dir, delta: f32) -> bool {
        match self {
            Node::Tabs { .. } => false,
            Node::Split { dir: d, ratios, children } => {
                let Some(i) = children.iter().position(|c| c.contains(v)) else { return false };
                if children[i].resize(v, dir, delta) { return true; }
                if *d != dir { return false; }
                let j = if i + 1 < ratios.len() { i + 1 } else if i > 0 { i - 1 } else { return false };
                let (a, b) = if j > i { (i, j) } else { (j, i) };
                let sign = if j > i { 1.0 } else { -1.0 };
                let sum = ratios[a] + ratios[b];
                let na = (ratios[a] + sign * delta).clamp(0.05, sum - 0.05);
                ratios[a] = na; ratios[b] = sum - na;
                true
            }
        }
    }

    /// Set divider `idx` of the split at `path` so the boundary lands at fraction `p`.
    pub fn set_divider(&mut self, path: &[usize], idx: usize, p: f32) {
        let mut n = self;
        for &i in path { if let Node::Split { children, .. } = n { n = &mut children[i]; } else { return; } }
        if let Node::Split { ratios, .. } = n {
            if idx + 1 >= ratios.len() { return; }
            let start: f32 = ratios[..idx].iter().sum();
            let sum = ratios[idx] + ratios[idx + 1];
            let na = (p - start).clamp(0.04, sum - 0.04);
            ratios[idx] = na; ratios[idx + 1] = sum - na;
        }
    }

    /// Move `v` one slot in `zone`: swap with the neighbouring pane's visible view when `v` is alone in its pane;
    /// join the neighbour pane as a tab when `v` shares its pane (leaving an empty slot behind is impossible);
    /// at the window edge, dock into a new split on that side. Returns "swap" | "join" | "split".
    pub fn move_dir(&mut self, v: &str, zone: Zone) -> Result<&'static str, String> {
        if zone == Zone::Center { return Err("direction must be left/right/top/bottom".into()); }
        if !self.contains(v) { return Err(format!("view {v} not in layout")); }
        let alone = self.pane_len(v) == 1;
        match self.neighbour(v, zone) {
            Some(n) if alone => { self.swap(v, &n); Ok("swap") }
            Some(n) => { self.move_view(v, &n, Zone::Center)?; self.activate(v); Ok("join") }
            None => {
                if self.views().len() < 2 { return Err("only one view in the layout".into()); }
                self.dock_edge(v, zone, 0.5); Ok("split")
            }
        }
    }
    fn pane_len(&self, v: &str) -> usize {
        match self {
            Node::Tabs { views, .. } => if views.iter().any(|x| x == v) { views.len() } else { 0 },
            Node::Split { children, .. } => children.iter().map(|c| c.pane_len(v)).max().unwrap_or(0),
        }
    }

    /// Neighbour of `v` in a direction, by geometry of visible panes.
    pub fn neighbour(&self, v: &str, zone: Zone) -> Option<String> {
        let mut ls = vec![];
        self.leaves(Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, &mut ls);
        let (_, r) = ls.iter().find(|(x, _)| x == v).cloned().or_else(|| {
            // v may be a hidden tab: use its group's active view rect
            None
        })?;
        let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        ls.iter().filter(|(x, o)| x != v && match zone {
            Zone::Left => o.x + o.w <= r.x + 1e-3,
            Zone::Right => o.x >= r.x + r.w - 1e-3,
            Zone::Top => o.y + o.h <= r.y + 1e-3,
            Zone::Bottom => o.y >= r.y + r.h - 1e-3,
            Zone::Center => false,
        }).min_by(|(_, a), (_, b)| {
            let d = |o: &Rect| { let (x, y) = (o.x + o.w / 2.0 - cx, o.y + o.h / 2.0 - cy); x * x + y * y };
            d(a).total_cmp(&d(b))
        }).map(|(x, _)| x.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_move_resize() {
        let mut n = Node::tabs("a");
        assert!(n.insert("a", "b", Zone::Right));
        assert!(n.insert("b", "c", Zone::Bottom));
        assert_eq!(n.views(), vec!["a", "b", "c"]);
        assert_eq!(n.neighbour("a", Zone::Right).as_deref(), Some("b"));
        assert_eq!(n.neighbour("b", Zone::Bottom).as_deref(), Some("c"));
        n.move_view("c", "a", Zone::Center).unwrap();
        assert_eq!(n.views(), vec!["a", "c", "b"]);
        assert!(n.resize("a", Dir::Horizontal, 0.2));
        if let Node::Split { ratios, .. } = &n { assert!((ratios[0] - 0.7).abs() < 1e-4); } else { panic!() }
        n.remove("b");
        assert!(matches!(n, Node::Tabs { .. }));
        n.swap("a", "c");
        assert_eq!(n.views(), vec!["c", "a"]);
    }

    #[test]
    fn dock_active_tab_to_the_right_splits_the_pane() {
        let mut n = Node::Tabs {
            views: vec!["addresses".into(), "phones".into(), "emails".into()],
            active: 1,
        };
        n.move_view("phones", "phones", Zone::Right).unwrap();
        match &n {
            Node::Split { dir: Dir::Horizontal, children, .. } => {
                match &children[0] {
                    Node::Tabs { views, .. } => {
                        assert!(views.contains(&"addresses".to_string()));
                        assert!(views.contains(&"emails".to_string()));
                        assert!(!views.contains(&"phones".to_string()));
                    }
                    other => panic!("left pane should stay a tab group, got {other:?}"),
                }
                match &children[1] {
                    Node::Tabs { views, active } => {
                        assert_eq!(views, &vec!["phones".to_string()]);
                        assert_eq!(*active, 0);
                    }
                    other => panic!("phones should be alone on the right, got {other:?}"),
                }
            }
            other => panic!("expected a horizontal split, got {other:?}"),
        }
        let mut alone = Node::tabs("phones");
        assert!(alone.move_view("phones", "phones", Zone::Right).is_err());
        assert_eq!(alone.views(), vec!["phones".to_string()]);
        alone.move_view("phones", "phones", Zone::Center).unwrap();
        assert_eq!(alone.views(), vec!["phones".to_string()]);
    }

    #[test]
    fn roundtrip_json() {
        let mut n = Node::tabs("a");
        assert!(n.insert("a", "b", Zone::Right));
        let json = serde_json::to_string(&n).unwrap();
        let back: Node = serde_json::from_str(&json).unwrap();
        assert_eq!(n, back);
    }
}
