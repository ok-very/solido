//! Ecology Graph — live force-directed node graph of affinity topology.
//!
//! Organisms as nodes (colored by hue, sized by RMS, bordered by valence),
//! affinity edges as lines (thickness = weight, color = satisfaction).
//! Click to select, hover for info, weight threshold slider for filtering.
//! Replaces: edges, emotions, signal_log panels.

use std::collections::{HashMap, HashSet};

use crate::affinity::edge::EdgeId;
use crate::module::ModuleId;
use crate::organism::module::OrganismModule;
use crate::reactor::SeedReactor;
use crate::ui::panels::organism_panel::{hue_to_color32, OrganismPanelState};

// ─── State ───────────────────────────────────────────────────────────

pub struct EcologyGraphState {
    /// Node positions in panel-local coordinates (relative to graph rect).
    positions: HashMap<ModuleId, egui::Vec2>,
    velocities: HashMap<ModuleId, egui::Vec2>,
    /// Currently selected (clicked) node.
    pub selected: Option<ModuleId>,
    /// Minimum edge weight to display.
    pub weight_threshold: f32,
    initialized: bool,
}

impl Default for EcologyGraphState {
    fn default() -> Self {
        Self {
            positions: HashMap::new(),
            velocities: HashMap::new(),
            selected: None,
            weight_threshold: 0.05,
            initialized: false,
        }
    }
}

// ─── Node Data ───────────────────────────────────────────────────────

struct NodeData {
    id: ModuleId,
    name: String,
    hue: f32,
    rms: f32,
    valence: f32,
    arousal: f32,
}

// ─── Constants ───────────────────────────────────────────────────────

const BG: egui::Color32 = egui::Color32::from_rgb(10, 10, 16);
/// Reference panel size for scaling forces/radii.
const REF_W: f32 = 500.0;
const REF_H: f32 = 400.0;
const DAMPING: f32 = 0.85;
const SIM_STEPS: usize = 5;

// ─── Main ────────────────────────────────────────────────────────────

pub fn show_ecology_graph(
    ctx: &egui::Context,
    open: &mut bool,
    reactor: &SeedReactor,
    organisms: Option<&OrganismPanelState>,
    state: &mut EcologyGraphState,
) {
    egui::Window::new(format!("{} Ecology", egui_phosphor::regular::GRAPH))
        .open(open)
        .default_pos([100.0, 60.0])
        .default_size([500.0, 420.0])
        .min_width(280.0)
        .min_height(180.0)
        .resizable(true)
        .collapsible(true)
        .frame(egui::Frame::window(&ctx.style()).fill(BG))
        .show(ctx, |ui| {
            let nodes = collect_nodes(reactor, organisms);
            let org_ids: HashSet<ModuleId> = nodes.iter().map(|n| n.id).collect();

            // Header: threshold + stats
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Edge min:")
                        .size(9.0)
                        .color(egui::Color32::from_gray(80)),
                );
                ui.add(
                    egui::Slider::new(&mut state.weight_threshold, 0.0..=0.5)
                        .max_decimals(2)
                        .show_value(true),
                );
                ui.separator();
                let edge_count = reactor
                    .graph
                    .edges
                    .iter()
                    .filter(|(eid, e)| {
                        org_ids.contains(&eid.0)
                            && org_ids.contains(&eid.2)
                            && e.weight >= state.weight_threshold
                    })
                    .count();
                ui.label(
                    egui::RichText::new(format!("{} nodes  {} edges", nodes.len(), edge_count))
                        .size(9.0)
                        .monospace()
                        .color(egui::Color32::from_gray(70)),
                );
            });

            ui.add_space(2.0);

            // Graph area
            let avail = ui.available_size();
            let (rect, response) =
                ui.allocate_exact_size(avail, egui::Sense::click_and_drag());

            // Initialize or re-initialize when node count changes
            let need_init = !state.initialized
                || nodes.len() != state.positions.len()
                || nodes.iter().any(|n| !state.positions.contains_key(&n.id));
            if need_init {
                init_positions(state, &nodes, rect);
                state.initialized = true;
            }

            // Simulate forces
            simulate(state, &nodes, &reactor.graph.edges, &org_ids, rect);

            // Render
            if ui.is_rect_visible(rect) {
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 0.0, BG);

                let pos_lookup: HashMap<ModuleId, egui::Pos2> = state
                    .positions
                    .iter()
                    .map(|(&id, &v)| (id, egui::pos2(rect.left() + v.x, rect.top() + v.y)))
                    .collect();

                // Draw edges
                for (&edge_id, edge) in &reactor.graph.edges {
                    if edge.weight < state.weight_threshold {
                        continue;
                    }
                    let (src_mod, _, dst_mod, _) = edge_id;
                    if !org_ids.contains(&src_mod) || !org_ids.contains(&dst_mod) {
                        continue;
                    }
                    if src_mod == dst_mod {
                        continue;
                    }

                    let Some(&from) = pos_lookup.get(&src_mod) else {
                        continue;
                    };
                    let Some(&to) = pos_lookup.get(&dst_mod) else {
                        continue;
                    };

                    let base_alpha: u8 = if edge.weight > 0.3 { 200 } else { 100 };
                    let dim = state
                        .selected
                        .map_or(false, |sel| sel != src_mod && sel != dst_mod);
                    let alpha = if dim { base_alpha / 3 } else { base_alpha };

                    let color = sat_color(edge.satisfaction, alpha);
                    let thick = (edge.weight * 4.0).max(0.5);

                    painter.line_segment([from, to], egui::Stroke::new(thick, color));

                    // Weight label at midpoint for prominent edges
                    if thick > 1.5 && !dim {
                        let mid = egui::pos2(
                            (from.x + to.x) / 2.0,
                            (from.y + to.y) / 2.0,
                        );
                        painter.text(
                            mid,
                            egui::Align2::CENTER_CENTER,
                            format!("{:.2}", edge.weight),
                            egui::FontId::monospace(8.0),
                            egui::Color32::from_rgba_unmultiplied(180, 180, 200, alpha),
                        );
                    }
                }

                // Draw nodes — scale with panel size
                let scale = (rect.width() / REF_W).min(rect.height() / REF_H).max(0.4);
                let node_base_r = 14.0 * scale;
                let node_rms_scale = 18.0 * scale;
                for node in &nodes {
                    let Some(&pos) = pos_lookup.get(&node.id) else {
                        continue;
                    };
                    let r = node_base_r + node.rms * node_rms_scale;
                    let is_selected = state.selected == Some(node.id);
                    let fill = hue_to_color32(node.hue);

                    // Arousal glow
                    if node.arousal > 0.3 {
                        let glow_a = (node.arousal * 40.0).min(60.0) as u8;
                        let glow = egui::Color32::from_rgba_unmultiplied(
                            fill.r(),
                            fill.g(),
                            fill.b(),
                            glow_a,
                        );
                        painter.circle_filled(pos, r + 4.0, glow);
                    }

                    painter.circle_filled(pos, r, fill);

                    // Valence border
                    let border = valence_border(node.valence);
                    let bw = if is_selected { 2.5 } else { 1.5 };
                    painter.circle_stroke(pos, r, egui::Stroke::new(bw, border));

                    if is_selected {
                        painter.circle_stroke(
                            pos,
                            r + 3.0,
                            egui::Stroke::new(1.0, egui::Color32::WHITE),
                        );
                    }

                    // Label
                    let lc = if is_selected {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_gray(155)
                    };
                    painter.text(
                        egui::pos2(pos.x, pos.y + r + 4.0),
                        egui::Align2::CENTER_TOP,
                        &node.name,
                        egui::FontId::proportional(9.0),
                        lc,
                    );
                }
            }

            // Click to select
            if response.clicked() {
                state.selected =
                    response.interact_pointer_pos().and_then(|p| {
                        hit_test_node(p, state, &nodes, rect)
                    });
            }

            // Hover tooltip — paint info near the hovered node
            if let Some(pointer) = response.hover_pos() {
                if let Some(node_id) = hit_test_node(pointer, state, &nodes, rect) {
                    if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
                        let conns = reactor
                            .graph
                            .edges
                            .iter()
                            .filter(|(eid, e)| {
                                (eid.0 == node_id || eid.2 == node_id)
                                    && e.weight >= state.weight_threshold
                            })
                            .count();
                        // Paint tooltip directly near the pointer
                        let painter = ui.painter();
                        let tip_pos = egui::pos2(pointer.x + 14.0, pointer.y + 14.0);
                        let tip_text = format!(
                            "{}\nval:{:+.2}  aro:{:.2}  rms:{:.3}\nedges: {}",
                            node.name, node.valence, node.arousal, node.rms, conns,
                        );
                        // Background rect
                        let galley = painter.layout_no_wrap(
                            tip_text.clone(),
                            egui::FontId::monospace(9.0),
                            egui::Color32::from_gray(200),
                        );
                        let tip_rect = egui::Rect::from_min_size(
                            tip_pos,
                            galley.size() + egui::vec2(8.0, 6.0),
                        );
                        painter.rect_filled(
                            tip_rect,
                            3.0,
                            egui::Color32::from_rgba_unmultiplied(20, 20, 28, 230),
                        );
                        painter.rect_stroke(
                            tip_rect,
                            3.0,
                            egui::Stroke::new(0.5, egui::Color32::from_gray(60)),
                            egui::StrokeKind::Outside,
                        );
                        painter.galley(
                            tip_pos + egui::vec2(4.0, 3.0),
                            galley,
                            egui::Color32::PLACEHOLDER,
                        );
                    }
                }
            }
        });
}

// ─── Data Collection ─────────────────────────────────────────────────

fn collect_nodes(
    reactor: &SeedReactor,
    organisms: Option<&OrganismPanelState>,
) -> Vec<NodeData> {
    let panel = match organisms {
        Some(p) => p,
        None => return Vec::new(),
    };

    panel
        .organisms
        .iter()
        .map(|org| {
            let emotion = reactor.graph.emotions.get(&org.mod_id);
            let rms = reactor
                .module_ref(org.mod_id)
                .and_then(|m| m.as_any().downcast_ref::<OrganismModule>())
                .map_or(0.0, |o| o.audio_rms());

            NodeData {
                id: org.mod_id,
                name: org.name.clone(),
                hue: org.hue,
                rms,
                valence: emotion.map_or(0.0, |e| e.valence),
                arousal: emotion.map_or(0.0, |e| e.arousal),
            }
        })
        .collect()
}

// ─── Layout ──────────────────────────────────────────────────────────

fn init_positions(
    state: &mut EcologyGraphState,
    nodes: &[NodeData],
    rect: egui::Rect,
) {
    state.positions.clear();
    state.velocities.clear();

    let cx = rect.width() / 2.0;
    let cy = rect.height() / 2.0;
    let radius = (cx.min(cy) * 0.55).max(40.0);
    let n = nodes.len().max(1) as f32;

    for (i, node) in nodes.iter().enumerate() {
        let angle = (i as f32 / n) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        state.positions.insert(
            node.id,
            egui::vec2(cx + radius * angle.cos(), cy + radius * angle.sin()),
        );
        state.velocities.insert(node.id, egui::Vec2::ZERO);
    }
}

fn simulate(
    state: &mut EcologyGraphState,
    nodes: &[NodeData],
    edges: &HashMap<EdgeId, crate::affinity::edge::EdgeAffinity>,
    org_ids: &HashSet<ModuleId>,
    rect: egui::Rect,
) {
    if nodes.len() < 2 {
        return;
    }
    let center = egui::vec2(rect.width() / 2.0, rect.height() / 2.0);
    // Scale forces relative to reference panel size
    let area_scale = (rect.width() * rect.height()) / (REF_W * REF_H);
    let repulsion = 6000.0 * area_scale;
    let attraction = 0.0008 / area_scale.max(0.1);
    let centering = 0.012;
    let min_dist = 30.0 * area_scale.sqrt();

    for _ in 0..SIM_STEPS {
        let mut forces: HashMap<ModuleId, egui::Vec2> =
            nodes.iter().map(|n| (n.id, egui::Vec2::ZERO)).collect();

        // Repulsion: all pairs
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                let a = nodes[i].id;
                let b = nodes[j].id;
                let pa = state.positions.get(&a).copied().unwrap_or(center);
                let pb = state.positions.get(&b).copied().unwrap_or(center);
                let delta = pa - pb;
                let dist = delta.length().max(min_dist);
                let f = repulsion / (dist * dist);
                let dir = delta / dist;

                if let Some(fa) = forces.get_mut(&a) {
                    *fa += dir * f;
                }
                if let Some(fb) = forces.get_mut(&b) {
                    *fb -= dir * f;
                }
            }
        }

        // Attraction: along edges
        for (&(src, _, dst, _), edge) in edges {
            if !org_ids.contains(&src) || !org_ids.contains(&dst) || src == dst {
                continue;
            }
            let pa = state.positions.get(&src).copied().unwrap_or(center);
            let pb = state.positions.get(&dst).copied().unwrap_or(center);
            let delta = pb - pa;
            let dist = delta.length().max(1.0);
            let f = attraction * dist * edge.weight;
            let dir = delta / dist;

            if let Some(fa) = forces.get_mut(&src) {
                *fa += dir * f;
            }
            if let Some(fb) = forces.get_mut(&dst) {
                *fb -= dir * f;
            }
        }

        // Centering
        for node in nodes {
            let pos = state.positions.get(&node.id).copied().unwrap_or(center);
            if let Some(f) = forces.get_mut(&node.id) {
                *f += (center - pos) * centering;
            }
        }

        // Integrate
        for node in nodes {
            let force = forces.get(&node.id).copied().unwrap_or(egui::Vec2::ZERO);
            let vel = state.velocities.entry(node.id).or_insert(egui::Vec2::ZERO);
            *vel = (*vel + force) * DAMPING;
            let speed = vel.length();
            if speed > 10.0 {
                *vel *= 10.0 / speed;
            }

            let pos = state.positions.entry(node.id).or_insert(center);
            *pos += *vel;
            let pad = 25.0;
            pos.x = pos.x.clamp(pad, rect.width() - pad);
            pos.y = pos.y.clamp(pad, rect.height() - pad);
        }
    }
}

// ─── Hit Testing ─────────────────────────────────────────────────────

fn hit_test_node(
    pointer: egui::Pos2,
    state: &EcologyGraphState,
    nodes: &[NodeData],
    rect: egui::Rect,
) -> Option<ModuleId> {
    let scale = (rect.width() / REF_W).min(rect.height() / REF_H).max(0.4);
    let base_r = 14.0 * scale;
    let rms_scale = 18.0 * scale;
    for node in nodes {
        let v = state.positions.get(&node.id)?;
        let pos = egui::pos2(rect.left() + v.x, rect.top() + v.y);
        let r = base_r + node.rms * rms_scale;
        let dist = ((pointer.x - pos.x).powi(2) + (pointer.y - pos.y).powi(2)).sqrt();
        if dist <= r + 6.0 * scale {
            return Some(node.id);
        }
    }
    None
}

// ─── Color Helpers ───────────────────────────────────────────────────

fn sat_color(satisfaction: f32, alpha: u8) -> egui::Color32 {
    let hue = satisfaction.clamp(0.0, 1.0) * 0.33;
    let rgb = egui::Color32::from(egui::ecolor::Hsva::new(hue, 0.7, 0.8, 1.0));
    egui::Color32::from_rgba_unmultiplied(rgb.r(), rgb.g(), rgb.b(), alpha)
}

fn valence_border(valence: f32) -> egui::Color32 {
    if valence > 0.2 {
        egui::Color32::from_rgb(80, 200, 100)
    } else if valence < -0.2 {
        egui::Color32::from_rgb(200, 80, 80)
    } else {
        egui::Color32::from_gray(100)
    }
}
