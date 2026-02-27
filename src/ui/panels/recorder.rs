use crate::recorder::Recorder;

/// Draw recorder controls in a bottom panel. Returns true if Export was clicked.
pub fn show_recorder_panel(ctx: &egui::Context, recorder: &mut Recorder) -> bool {
    let mut export_clicked = false;

    egui::TopBottomPanel::bottom("recorder_timeline")
        .exact_height(48.0)
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                // Record / Stop / Clear
                if recorder.is_recording {
                    if ui.button("STOP").clicked() {
                        recorder.stop_recording();
                    }
                } else if ui.button("REC").clicked() {
                    recorder.start_recording();
                }

                if ui.button("CLEAR").clicked() {
                    recorder.clear();
                }

                ui.separator();

                // Frame count + memory
                let mem_mb = recorder.memory_bytes() as f64 / (1024.0 * 1024.0);
                ui.label(format!(
                    "{}/{} frames ({:.0} MB)",
                    recorder.frame_count(),
                    recorder.max_frames,
                    mem_mb
                ));

                ui.separator();

                // Range selectors
                let max_idx = recorder.frame_count().saturating_sub(1);
                ui.label("Start");
                ui.add(egui::DragValue::new(&mut recorder.selection_start).range(0..=max_idx));
                ui.label("End");
                ui.add(egui::DragValue::new(&mut recorder.selection_end).range(0..=max_idx));

                ui.separator();

                // Scrubber
                if recorder.frame_count() > 0 {
                    let mut s = recorder.scrubber as f64;
                    ui.add(
                        egui::Slider::new(&mut s, 0.0..=(max_idx as f64)).show_value(false),
                    );
                    recorder.scrubber = s as usize;
                }

                ui.separator();

                // Export path + button
                ui.add(
                    egui::TextEdit::singleline(&mut recorder.export_dir).desired_width(120.0),
                );
                if ui.button("Export").clicked() {
                    export_clicked = true;
                }

                // Status message
                if let Some(msg) = &recorder.last_export_msg {
                    ui.label(msg.as_str());
                }
            });
        });

    export_clicked
}
