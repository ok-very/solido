use crate::app::AudioStatus;
use crate::modules::raga_module::RagaModule;
use crate::modules::scale_module::ScaleModule;
use crate::modules::tala_module::TalaModule;
use crate::reactor::SeedReactor;
use crate::tuning::gravity_control::GravityState;
use crate::tuning::gravity_well::pitch_class_name;
use crate::ui::DebugModuleIds;

/// Bottom status bar showing live system state.
pub fn show_status_bar(
    ctx: &egui::Context,
    reactor: &SeedReactor,
    ids: &DebugModuleIds,
    gravity: &GravityState,
    beat_phase: f32,
    base_key: u8,
    audio_status: &AudioStatus,
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

                // Audio status indicator
                {
                    let (dot_color, audio_label) = match audio_status {
                        AudioStatus::Running { sample_rate, channels, .. } => (
                            egui::Color32::from_rgb(60, 200, 80),
                            format!("[{}kHz {}ch]", sample_rate / 1000, channels),
                        ),
                        AudioStatus::Unavailable => (
                            egui::Color32::from_rgb(200, 60, 60),
                            "[No Audio]".into(),
                        ),
                        AudioStatus::Error(_) => (
                            egui::Color32::from_rgb(200, 60, 60),
                            "[Audio Err]".into(),
                        ),
                    };
                    let (dot_rect, _) =
                        ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter()
                        .circle_filled(dot_rect.center(), 4.0, dot_color);
                    ui.label(mono(audio_label));
                }

                // Base key
                ui.label(mono(format!("[Key:{}]", pitch_class_name(base_key))));

                // Scale name
                let scale_name = reactor
                    .module_ref(ids.scale_id)
                    .and_then(|m| m.as_any().downcast_ref::<ScaleModule>())
                    .map(|s| s.current_scale_name().to_string())
                    .unwrap_or_else(|| "?".into());
                ui.label(mono(format!("[{}]", scale_name)));

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

                // Module + edge counts
                ui.label(mono(format!(
                    "[modules:{}] [edges:{}]",
                    reactor.module_count(),
                    reactor.edge_count()
                )));
            });
        });
}
