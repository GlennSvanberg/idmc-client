//! Hand-rolled force layout with the same parameters as the Braian graph
//! (https://github.com/braian-io/braian, Apache-2.0):
//! link distance `72 + r1 + r2` at strength 0.25, many-body −560 with a max
//! distance of 820, centering, collision at `r + 4` for two iterations,
//! `alphaDecay` 0.032, `velocityDecay` 0.5. New nodes use a phyllotaxis seed.
//! Merging reheats to 0.3 and keeps the positions nodes already have.

use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Body {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub fx: Option<f32>,
    pub fy: Option<f32>,
    pub radius: f32,
    pub ring: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Spring {
    pub source: usize,
    pub target: usize,
}

#[derive(Clone, Debug)]
pub struct Simulation {
    pub nodes: Vec<Body>,
    pub links: Vec<Spring>,
    pub alpha: f32,
    pub alpha_min: f32,
    pub alpha_decay: f32,
    pub velocity_decay: f32,
    pub charge: f32,
    pub distance_max: f32,
    pub link_strength: f32,
    pub link_gap: f32,
}

impl Default for Simulation {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            links: Vec::new(),
            alpha: 1.0,
            alpha_min: 0.001,
            alpha_decay: 0.032,
            velocity_decay: 0.5,
            charge: -560.0,
            distance_max: 820.0,
            link_strength: 0.25,
            link_gap: 72.0,
        }
    }
}

impl Simulation {
    pub fn link_distance(&self, source: usize, target: usize) -> f32 {
        self.link_gap + self.nodes[source].radius + self.nodes[target].radius
    }

    /// Golden-angle seed. `ring` scales the spiral so hubs and leaves start apart.
    pub fn phyllotaxis(ring: f32, index: usize) -> (f32, f32) {
        const GOLDEN: f32 = 2.3999631;
        let n = index as f32 + 1.0;
        let radius = ring.max(0.4) * 14.0 * n.sqrt();
        let angle = n * GOLDEN;
        (angle.cos() * radius, angle.sin() * radius)
    }

    pub fn reheat(&mut self, alpha: f32) {
        self.alpha = alpha;
    }

    /// Replace the graph. Nodes that already exist keep their coordinates.
    /// A new node or link reheats to 0.3.
    pub fn sync(&mut self, incoming: &[NodeSpec], links: &[(String, String)]) {
        let old: HashMap<String, Body> = self.nodes.iter().cloned().map(|body| (body.id.clone(), body)).collect();
        let mut added = false;
        let mut nodes = Vec::with_capacity(incoming.len());
        for (index, spec) in incoming.iter().enumerate() {
            if let Some(mut body) = old.get(&spec.id).cloned() {
                body.radius = spec.radius;
                body.ring = spec.ring;
                nodes.push(body);
            } else {
                added = true;
                let (x, y) = Self::phyllotaxis(spec.ring, index);
                nodes.push(Body {
                    id: spec.id.clone(),
                    x,
                    y,
                    vx: 0.0,
                    vy: 0.0,
                    fx: None,
                    fy: None,
                    radius: spec.radius,
                    ring: spec.ring,
                });
            }
        }
        let index: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, body)| (body.id.as_str(), i)).collect();
        let mut springs = Vec::new();
        for (source, target) in links {
            let Some(&from) = index.get(source.as_str()) else { continue };
            let Some(&to) = index.get(target.as_str()) else { continue };
            if from != to {
                springs.push(Spring { source: from, target: to });
            }
        }
        let links_changed = springs != self.links || nodes.len() != self.nodes.len();
        self.nodes = nodes;
        self.links = springs;
        if added || links_changed {
            self.reheat(0.3);
        }
    }

    /// One integration step. Returns true once alpha has fallen below 0.001.
    pub fn tick(&mut self) -> bool {
        if self.nodes.is_empty() {
            self.alpha = 0.0;
            return true;
        }
        if self.alpha < self.alpha_min {
            return true;
        }
        self.alpha += (0.0 - self.alpha) * self.alpha_decay;
        self.many_body();
        self.links_force();
        self.collide();
        self.center();
        let decay = self.velocity_decay;
        for node in &mut self.nodes {
            if let Some(fx) = node.fx {
                node.x = fx;
                node.vx = 0.0;
            } else {
                node.vx *= decay;
                node.x += node.vx;
            }
            if let Some(fy) = node.fy {
                node.y = fy;
                node.vy = 0.0;
            } else {
                node.vy *= decay;
                node.y += node.vy;
            }
        }
        self.alpha < self.alpha_min
    }

    pub fn settled(&self) -> bool {
        self.alpha < self.alpha_min
    }

    fn many_body(&mut self) {
        let n = self.nodes.len();
        let max2 = self.distance_max * self.distance_max;
        let alpha = self.alpha;
        let charge = self.charge;
        let mut acc = vec![(0f32, 0f32); n];
        for i in 0..n {
            for j in (i + 1)..n {
                let mut dx = self.nodes[j].x - self.nodes[i].x;
                let mut dy = self.nodes[j].y - self.nodes[i].y;
                let mut l2 = dx * dx + dy * dy;
                if l2 > max2 {
                    continue;
                }
                if l2 < 1e-8 {
                    dx = jiggle(i);
                    dy = jiggle(j);
                    l2 = dx * dx + dy * dy;
                }
                if l2 < 1.0 {
                    l2 = l2.sqrt();
                }
                let scale = charge * alpha / l2;
                acc[i].0 += dx * scale;
                acc[i].1 += dy * scale;
                acc[j].0 -= dx * scale;
                acc[j].1 -= dy * scale;
            }
        }
        for (node, (vx, vy)) in self.nodes.iter_mut().zip(acc) {
            node.vx += vx;
            node.vy += vy;
        }
    }

    fn links_force(&mut self) {
        let n = self.nodes.len();
        let mut count = vec![0u32; n];
        for link in &self.links {
            count[link.source] += 1;
            count[link.target] += 1;
        }
        let alpha = self.alpha;
        let strength = self.link_strength;
        let gap = self.link_gap;
        for link in &self.links {
            let i = link.source;
            let j = link.target;
            let bias = count[i] as f32 / (count[i] + count[j]) as f32;
            let mut dx = self.nodes[j].x + self.nodes[j].vx - self.nodes[i].x - self.nodes[i].vx;
            let mut dy = self.nodes[j].y + self.nodes[j].vy - self.nodes[i].y - self.nodes[i].vy;
            if dx.abs() < 1e-6 && dy.abs() < 1e-6 {
                dx = jiggle(i);
                dy = jiggle(j);
            }
            let len = (dx * dx + dy * dy).sqrt();
            let distance = gap + self.nodes[i].radius + self.nodes[j].radius;
            let scale = (len - distance) / len * alpha * strength;
            dx *= scale;
            dy *= scale;
            self.nodes[j].vx -= dx * bias;
            self.nodes[j].vy -= dy * bias;
            self.nodes[i].vx += dx * (1.0 - bias);
            self.nodes[i].vy += dy * (1.0 - bias);
        }
    }

    fn collide(&mut self) {
        let n = self.nodes.len();
        for _ in 0..2 {
            for i in 0..n {
                for j in (i + 1)..n {
                    let mut dx = self.nodes[i].x - self.nodes[j].x;
                    let mut dy = self.nodes[i].y - self.nodes[j].y;
                    let l2 = dx * dx + dy * dy;
                    let reach = self.nodes[i].radius + self.nodes[j].radius + 8.0;
                    if l2 >= reach * reach {
                        continue;
                    }
                    let len = if l2 > 1e-8 {
                        l2.sqrt()
                    } else {
                        dx = jiggle(i);
                        dy = jiggle(j);
                        (dx * dx + dy * dy).sqrt()
                    };
                    let mag = (reach - len) / len;
                    dx *= mag;
                    dy *= mag;
                    let i_free = self.nodes[i].fx.is_none();
                    let j_free = self.nodes[j].fx.is_none();
                    if i_free {
                        let share = if j_free { 0.5 } else { 1.0 };
                        self.nodes[i].vx += dx * share;
                        self.nodes[i].vy += dy * share;
                    }
                    if j_free {
                        let share = if i_free { 0.5 } else { 1.0 };
                        self.nodes[j].vx -= dx * share;
                        self.nodes[j].vy -= dy * share;
                    }
                }
            }
        }
    }

    fn center(&mut self) {
        let n = self.nodes.len() as f32;
        if n == 0.0 {
            return;
        }
        let sx = self.nodes.iter().map(|node| node.x).sum::<f32>() / n;
        let sy = self.nodes.iter().map(|node| node.y).sum::<f32>() / n;
        for node in &mut self.nodes {
            node.x -= sx;
            node.y -= sy;
            if let Some(fx) = node.fx.as_mut() {
                *fx -= sx;
            }
            if let Some(fy) = node.fy.as_mut() {
                *fy -= sy;
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct NodeSpec {
    pub id: String,
    pub radius: f32,
    pub ring: f32,
}

fn jiggle(index: usize) -> f32 {
    ((index as f32) * 12.9898).sin() * 1.0e-4
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn pair() -> Simulation {
        let mut sim = Simulation { charge: 0.0, ..Simulation::default() };
        sim.sync(
            &[
                NodeSpec { id: "a".into(), radius: 10.0, ring: 1.0 },
                NodeSpec { id: "b".into(), radius: 12.0, ring: 1.0 },
            ],
            &[("a".into(), "b".into())],
        );
        sim.alpha = 1.0;
        sim
    }

    #[test]
    fn link_distance_is_gap_plus_radii() {
        let sim = pair();
        assert!((sim.link_distance(0, 1) - (72.0 + 10.0 + 12.0)).abs() < 0.01);
    }

    #[test]
    fn linked_pair_settles_near_the_link_distance() {
        let mut sim = pair();
        let mut settled = false;
        for _ in 0..400 {
            settled = sim.tick();
            if settled {
                break;
            }
        }
        assert!(settled, "alpha {}", sim.alpha);
        let dx = sim.nodes[0].x - sim.nodes[1].x;
        let dy = sim.nodes[0].y - sim.nodes[1].y;
        let distance = (dx * dx + dy * dy).sqrt();
        let want = 72.0 + 10.0 + 12.0;
        assert!((distance - want).abs() < 8.0, "distance {distance} want {want}");
    }

    #[test]
    fn many_body_pushes_a_close_pair_apart() {
        let mut sim = Simulation::default();
        sim.sync(
            &[
                NodeSpec { id: "a".into(), radius: 10.0, ring: 1.0 },
                NodeSpec { id: "b".into(), radius: 10.0, ring: 1.0 },
            ],
            &[],
        );
        sim.nodes[0].x = 0.0;
        sim.nodes[1].x = 4.0;
        sim.nodes[0].y = 0.0;
        sim.nodes[1].y = 0.0;
        sim.alpha = 1.0;
        let before = (sim.nodes[0].x - sim.nodes[1].x).abs();
        sim.tick();
        let after = (sim.nodes[0].x - sim.nodes[1].x).abs();
        assert!(after > before, "before {before} after {after}");
    }

    #[test]
    fn reheat_keeps_positions_and_merge_does_not_move_existing_nodes() {
        let mut sim = pair();
        for _ in 0..30 {
            sim.tick();
        }
        let (x, y) = (sim.nodes[0].x, sim.nodes[0].y);
        sim.reheat(0.3);
        assert_eq!(sim.alpha, 0.3);
        assert_eq!((sim.nodes[0].x, sim.nodes[0].y), (x, y));
        sim.sync(
            &[
                NodeSpec { id: "a".into(), radius: 10.0, ring: 1.0 },
                NodeSpec { id: "b".into(), radius: 12.0, ring: 1.0 },
                NodeSpec { id: "c".into(), radius: 8.0, ring: 3.0 },
            ],
            &[("a".into(), "b".into()), ("a".into(), "c".into())],
        );
        let kept = sim.nodes.iter().find(|node| node.id == "a").unwrap();
        assert_eq!((kept.x, kept.y), (x, y));
        assert!((sim.alpha - 0.3).abs() < f32::EPSILON);
        assert!(sim.nodes.iter().any(|node| node.id == "c"));
    }

    #[test]
    fn phyllotaxis_is_deterministic_and_spread() {
        let a = Simulation::phyllotaxis(1.0, 0);
        let b = Simulation::phyllotaxis(1.0, 1);
        assert_eq!(a, Simulation::phyllotaxis(1.0, 0));
        assert!(a != b);
    }

    #[test]
    fn four_hundred_nodes_tick_quickly() {
        let specs: Vec<NodeSpec> = (0..400)
            .map(|index| NodeSpec { id: format!("n{index}"), radius: 14.0, ring: 1.0 + (index % 4) as f32 })
            .collect();
        let links: Vec<(String, String)> = (0..399).map(|index| (format!("n{index}"), format!("n{}", index + 1))).collect();
        let mut sim = Simulation::default();
        sim.sync(&specs, &links);
        let started = Instant::now();
        for _ in 0..8 {
            sim.tick();
        }
        let elapsed = started.elapsed();
        assert!(elapsed.as_millis() < 250, "{elapsed:?}");
        assert!(sim.nodes.len() == 400);
    }
}
