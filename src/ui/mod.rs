pub mod header;
pub mod tabs;
pub mod panels;
pub mod widgets;

use std::collections::HashMap;

use crate::audio::mixer_state::MixerState;
use crate::module::{ModuleId, PortId};
use crate::reactor::SeedReactor;
use crate::recorder::Recorder;
use crate::tuning::gravity_control::GravityState;

pub struct PanelVisibility {
    pub debug: bool,
    pub recorder: bool,
    pub mixer: bool,
    pub organisms: bool,
    pub controls: bool,
    pub ledger: bool,
    pub presets: bool,
    pub spawn: bool,
    pub wells: bool,
    pub effects: bool,
    pub synth_detail: bool,
    pub mc20: bool,
    pub groove: bool,
    pub ecology: bool,
}

impl Default for PanelVisibility {
    fn default() -> Self {
        Self {
            debug: true,
            recorder: false,
            mixer: true,
            organisms: true,
            controls: false,
            ledger: false,
            presets: false,
            spawn: false,
            wells: false,
            effects: true,
            synth_detail: false,
            mc20: true,
            groove: false,
            ecology: true,
        }
    }
}

pub struct WorkspaceState {
    pub panels: PanelVisibility,
    /// Opacity of the debug inspector window [0.0, 1.0].
    pub debug_opacity: f32,
    /// Ledger view filter/scroll state.
    pub ledger_view: panels::ledger_view::LedgerViewState,
    /// Ecology graph (force-directed node graph) persistent state.
    pub ecology_graph: panels::ecology_graph::EcologyGraphState,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            panels: PanelVisibility::default(),
            debug_opacity: 0.85,
            ledger_view: panels::ledger_view::LedgerViewState::default(),
            ecology_graph: panels::ecology_graph::EcologyGraphState::default(),
        }
    }
}

/// Module IDs needed by UI panels for downcasting.
pub struct DebugModuleIds {
    pub kbd_id: ModuleId,
    pub quantizer_id: ModuleId,
    pub analysis_id: ModuleId,
    pub raga_id: ModuleId,
    pub tala_id: ModuleId,
    pub scale_id: ModuleId,
}

/// Build PortId → ("module_name", "port_name") lookup from reactor schemas.
pub fn build_port_names(reactor: &SeedReactor) -> HashMap<PortId, (String, String)> {
    let mut map = HashMap::new();
    for (_, schema) in reactor.schemas() {
        for port in &schema.inputs {
            map.insert(port.id, (schema.name.clone(), port.name.to_string()));
        }
        for port in &schema.outputs {
            map.insert(port.id, (schema.name.clone(), port.name.to_string()));
        }
    }
    map
}

/// Draw the debug inspector as a floating, draggable window with opacity control.
fn show_debug_window(
    ctx: &egui::Context,
    open: &mut bool,
    opacity: &mut f32,
    reactor: &SeedReactor,
    ids: &DebugModuleIds,
) {
    let bg = egui::Color32::from_rgba_unmultiplied(30, 30, 30, (*opacity * 255.0) as u8);

    egui::Window::new(format!("{} Inspector", egui_phosphor::regular::SLIDERS_HORIZONTAL))
        .open(open)
        .default_pos([10.0, 44.0])
        .default_width(280.0)
        .min_width(200.0)
        .max_width(500.0)
        .resizable(true)
        .collapsible(true)
        .frame(egui::Frame::window(&ctx.style()).fill(bg))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new(
                    egui::RichText::new("Signal Flow").heading(),
                )
                .default_open(true)
                .show(ui, |ui| {
                    panels::signal_flow::show(ui, reactor, ids);
                });

                ui.add_space(4.0);

                egui::CollapsingHeader::new(
                    egui::RichText::new("Reactor").heading(),
                )
                .default_open(true)
                .show(ui, |ui| {
                    panels::reactor_stats::show(ui, reactor);
                });

                ui.add_space(8.0);
                ui.separator();

                // Opacity slider at the bottom
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(egui_phosphor::regular::EYE)
                            .size(14.0),
                    );
                    ui.add(
                        egui::Slider::new(opacity, 0.15..=1.0)
                            .show_value(false)
                            .text("opacity"),
                    );
                });
            });
        });
}

/// Main workspace orchestrator. Call this once per frame from app.rs update().
/// Panel ordering: top header → status bar → bottom recorder → floating debug → floating mixer → ledger → central (caller renders central).
/// The organism panel and spawn panel are called directly from app.rs so their return values (kill/spawn actions) can be handled there.
pub fn show_workspace(
    ctx: &egui::Context,
    state: &mut WorkspaceState,
    reactor: &SeedReactor,
    recorder: &mut Recorder,
    ids: &DebugModuleIds,
    mixer_state: Option<&mut MixerState>,
    gravity: &GravityState,
    beat_phase: f32,
    base_key: u8,
) -> bool {
    // 1. Header (always)
    header::show_header(ctx, &mut state.panels);

    // 2. Edge resize detection for custom frame
    header::handle_window_resize(ctx);

    // 3. Status bar (always)
    panels::status_bar::show_status_bar(ctx, reactor, ids, gravity, beat_phase, base_key);

    // 4. Recorder bottom panel (conditional)
    let mut export_clicked = false;
    if state.panels.recorder {
        export_clicked = panels::recorder::show_recorder_panel(ctx, recorder);
    }

    // 5. Debug inspector floating window (conditional)
    if state.panels.debug {
        show_debug_window(
            ctx,
            &mut state.panels.debug,
            &mut state.debug_opacity,
            reactor,
            ids,
        );
    }

    // 6. Mixer floating window (conditional)
    if let Some(ms) = mixer_state {
        if state.panels.mixer {
            panels::mixer::show_mixer_panel(ctx, &mut state.panels.mixer, ms);
        }
    }

    // 7. Ledger view floating window (conditional)
    if state.panels.ledger {
        panels::ledger_view::show_ledger_panel(
            ctx,
            &mut state.panels.ledger,
            reactor,
            &mut state.ledger_view,
        );
    }

    // 8. CentralPanel is rendered by the caller (app.rs) after this returns
    // Organism panel + spawn panel are also called from app.rs (they return actions).

    export_clicked
}
