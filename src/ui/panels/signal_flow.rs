use crate::modules::keyboard_input::KeyboardInputModule;
use crate::modules::quantizer::QuantizerModule;
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

    // --- Audio Analysis ---
    ui.strong("Audio Analysis");
    if reactor.module_ref(ids.analysis_id).is_some() {
        ui.label("  (receives rms/peak via graph)");
    }
}
