use crate::modules::raga_module::RagaModule;
use crate::modules::tala_module::TalaModule;
use crate::modules::voice_module::VoiceModule;
use crate::reactor::SeedReactor;
use crate::tuning::gravity_control::GravityState;
use crate::ui::DebugModuleIds;

/// Bottom status bar showing live system state.
/// Always visible: [Raga] [Tala BPM] [G:val] [beat:N] [voices:N/8] [modules:N] [edges:N]
pub fn show_status_bar(
    ctx: &egui::Context,
    reactor: &SeedReactor,
    ids: &DebugModuleIds,
    gravity: &GravityState,
    beat_phase: f32,
) {
    egui::TopBottomPanel::bottom("status_bar")
        .exact_height(24.0)
        .frame(
            egui::Frame::NONE
                .fill(egui::Color32::from_rgb(18, 18, 18))
                .inner_margin(egui::Margin::symmetric(12, 0)),
        )
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let mono = |s: String| {
                    egui::RichText::new(s)
                        .monospace()
                        .size(12.0)
                        .color(egui::Color32::from_gray(180))
                };

                // Raga name
                let raga_name = reactor
                    .module_ref(ids.raga_id)
                    .and_then(|m| m.as_any().downcast_ref::<RagaModule>())
                    .map(|r| r.current_raga_name().to_string())
                    .unwrap_or_else(|| "?".into());
                ui.label(mono(format!("[{}]", raga_name)));

                // Tala name + tempo
                let (tala_name, tempo) = reactor
                    .module_ref(ids.tala_id)
                    .and_then(|m| m.as_any().downcast_ref::<TalaModule>())
                    .map(|t| (t.current_tala_name().to_string(), t.tempo_bpm()))
                    .unwrap_or(("?".into(), 120.0));
                ui.label(mono(format!("[{} {:.0}bpm]", tala_name, tempo)));

                // Gravity
                ui.label(mono(format!("[G:{:.1}]", gravity.pitch_gravity)));

                // Beat phase as beat number in a cycle
                let beat_num = (beat_phase * 16.0).floor() as u32 + 1;
                ui.label(mono(format!("[beat:{}]", beat_num)));

                // Active voices
                if let Some(vid) = ids.voice_id {
                    let voices = reactor
                        .module_ref(vid)
                        .and_then(|m| m.as_any().downcast_ref::<VoiceModule>())
                        .map(|v| v.active_voices())
                        .unwrap_or(0);
                    ui.label(mono(format!("[voices:{}/8]", voices)));
                }

                // Module + edge counts
                ui.label(mono(format!(
                    "[modules:{}] [edges:{}]",
                    reactor.module_count(),
                    reactor.edge_count()
                )));
            });
        });
}
