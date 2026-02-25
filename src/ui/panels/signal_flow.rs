use crate::modules::keyboard_input::KeyboardInputModule;
use crate::modules::quantizer::QuantizerModule;
use crate::modules::voice_module::VoiceModule;
use crate::reactor::SeedReactor;
use crate::ui::DebugModuleIds;

pub fn show(ui: &mut egui::Ui, reactor: &SeedReactor, ids: &DebugModuleIds) {
    // --- Keyboard ---
    ui.strong("Keyboard");
    if let Some(kbd) = reactor.module_ref(ids.kbd_id) {
        if let Some(kbd) = kbd.as_any().downcast_ref::<KeyboardInputModule>() {
            ui.label(format!("  pending_keys: {}", kbd.pending_key_count()));
        }
    }
    ui.add_space(4.0);

    // --- Quantizer ---
    ui.strong("Quantizer");
    if let Some(quant) = reactor.module_ref(ids.quantizer_id) {
        if let Some(quant) = quant.as_any().downcast_ref::<QuantizerModule>() {
            let raw = quant
                .last_raw_pitch()
                .map(|v| format!("{:.3}", v))
                .unwrap_or_else(|| "none".to_string());
            ui.label(format!("  raw_pitch: {}", raw));
            ui.label(format!("  pitch_hz:  {:.2} Hz", quant.output_hz()));
            ui.label(format!(
                "  degree: {}  gravity: {:.2}",
                quant.output_degree() as u32,
                quant.gravity_strength()
            ));
            ui.label(format!("  tuning: {}", quant.current_tuning()));
        }
    }
    ui.add_space(4.0);

    // --- Voice ---
    ui.strong("Voice");
    if let Some(vid) = ids.voice_id {
        if let Some(voice) = reactor.module_ref(vid) {
            if let Some(voice) = voice.as_any().downcast_ref::<VoiceModule>() {
                ui.label(format!(
                    "  cutoff: {:.0}  amp: {:.2}",
                    voice.current_cutoff(),
                    voice.current_amplitude()
                ));
                ui.label(format!(
                    "  voices: {}/8  notes: {}  kills: {}",
                    voice.active_voices(),
                    voice.tracked_voice_count(),
                    voice.pending_kills_count()
                ));

                // RMS bar
                let rms = voice.current_rms();
                let peak = voice.current_peak();
                ui.horizontal(|ui| {
                    ui.label(format!("  rms: {:.3}", rms));
                    let bar_rect = ui.allocate_space(egui::vec2(80.0, 12.0)).1;
                    let painter = ui.painter();
                    painter.rect_filled(bar_rect, 0.0, egui::Color32::from_gray(40));
                    let fill_w = bar_rect.width() * rms.min(1.0);
                    let fill_rect = egui::Rect::from_min_size(
                        bar_rect.left_top(),
                        egui::vec2(fill_w, bar_rect.height()),
                    );
                    let color = if rms > 0.8 {
                        egui::Color32::from_rgb(255, 80, 80)
                    } else if rms > 0.3 {
                        egui::Color32::from_rgb(255, 200, 50)
                    } else {
                        egui::Color32::from_rgb(80, 200, 80)
                    };
                    painter.rect_filled(fill_rect, 0.0, color);
                });
                ui.label(format!("  peak: {:.3}", peak));
            }
        }
    } else {
        ui.label("  (no audio)");
    }
    ui.add_space(4.0);

    // --- Audio Analysis ---
    ui.strong("Audio Analysis");
    if reactor.module_ref(ids.analysis_id).is_some() {
        ui.label("  (receives rms/peak via graph)");
    }
}
