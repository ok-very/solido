use std::io::BufWriter;
use std::path::Path;

pub struct CapturedFrame {
    pub pixels: Vec<u8>, // RGBA8, straight alpha
    pub width: u32,
    pub height: u32,
    pub frame_number: u32,
    pub timestamp_secs: f32,
}

pub struct Recorder {
    pub is_recording: bool,
    pub pending_capture: bool,
    pub frames: Vec<CapturedFrame>,
    pub max_frames: usize,
    pub selection_start: usize,
    pub selection_end: usize,
    pub scrubber: usize,
    pub export_dir: String,
    pub last_export_msg: Option<String>,
    frame_counter: u32,
}

impl Recorder {
    pub fn new(max_frames: usize) -> Self {
        Self {
            is_recording: false,
            pending_capture: false,
            frames: Vec::new(),
            max_frames,
            selection_start: 0,
            selection_end: 0,
            scrubber: 0,
            export_dir: "capture".into(),
            last_export_msg: None,
            frame_counter: 0,
        }
    }

    pub fn start_recording(&mut self) {
        self.is_recording = true;
    }

    pub fn stop_recording(&mut self) {
        self.is_recording = false;
        self.pending_capture = false;
    }

    pub fn clear(&mut self) {
        self.frames.clear();
        self.frame_counter = 0;
        self.selection_start = 0;
        self.selection_end = 0;
        self.scrubber = 0;
        self.pending_capture = false;
    }

    pub fn push_frame(&mut self, frame: CapturedFrame) {
        if self.frames.len() >= self.max_frames {
            self.frames.remove(0);
        }
        self.frames.push(frame);
        self.selection_end = self.frames.len().saturating_sub(1);
        self.scrubber = self.frames.len().saturating_sub(1);
    }

    pub fn next_frame_number(&mut self) -> u32 {
        let n = self.frame_counter;
        self.frame_counter += 1;
        n
    }

    fn memory_bytes(&self) -> usize {
        self.frames
            .iter()
            .map(|f| f.pixels.len())
            .sum()
    }

    pub fn export_range_to_dir(&self, dir: &Path) -> Result<usize, Box<dyn std::error::Error>> {
        std::fs::create_dir_all(dir)?;

        let start = self.selection_start.min(self.frames.len().saturating_sub(1));
        let end = self.selection_end.min(self.frames.len().saturating_sub(1));
        let (lo, hi) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };

        let mut exported = 0usize;
        for (i, frame) in self.frames[lo..=hi].iter().enumerate() {
            let path = dir.join(format!("frame_{:05}.png", lo + i));
            let file = std::fs::File::create(&path)?;
            let w = BufWriter::new(file);

            let mut encoder = png::Encoder::new(w, frame.width, frame.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header()?;
            writer.write_image_data(&frame.pixels)?;
            exported += 1;
        }

        Ok(exported)
    }

    /// Draw timeline bottom panel. Returns true when Export is clicked.
    pub fn ui(&mut self, ctx: &egui::Context) -> bool {
        let mut export_clicked = false;

        egui::TopBottomPanel::bottom("recorder_timeline")
            .exact_height(48.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    // Record / Stop / Clear
                    if self.is_recording {
                        if ui.button("STOP").clicked() {
                            self.stop_recording();
                        }
                    } else if ui.button("REC").clicked() {
                        self.start_recording();
                    }

                    if ui.button("CLEAR").clicked() {
                        self.clear();
                    }

                    ui.separator();

                    // Frame count + memory
                    let mem_mb = self.memory_bytes() as f64 / (1024.0 * 1024.0);
                    ui.label(format!(
                        "{}/{} frames ({:.0} MB)",
                        self.frames.len(),
                        self.max_frames,
                        mem_mb
                    ));

                    ui.separator();

                    // Range selectors
                    let max_idx = self.frames.len().saturating_sub(1);
                    ui.label("Start");
                    ui.add(
                        egui::DragValue::new(&mut self.selection_start)
                            .range(0..=max_idx),
                    );
                    ui.label("End");
                    ui.add(
                        egui::DragValue::new(&mut self.selection_end)
                            .range(0..=max_idx),
                    );

                    ui.separator();

                    // Scrubber
                    if !self.frames.is_empty() {
                        let mut s = self.scrubber as f64;
                        ui.add(
                            egui::Slider::new(&mut s, 0.0..=(max_idx as f64))
                                .show_value(false),
                        );
                        self.scrubber = s as usize;
                    }

                    ui.separator();

                    // Export path + button
                    ui.add(
                        egui::TextEdit::singleline(&mut self.export_dir)
                            .desired_width(120.0),
                    );
                    if ui.button("Export").clicked() {
                        export_clicked = true;
                    }

                    // Status message
                    if let Some(msg) = &self.last_export_msg {
                        ui.label(msg.as_str());
                    }
                });
            });

        export_clicked
    }
}
