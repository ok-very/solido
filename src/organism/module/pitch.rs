//! Species-specific pitch personality and scale quantization.

use super::OrganismModule;

impl OrganismModule {
    /// Map pitch_hz to the appropriate shared handles for this species.
    pub(super) fn apply_pitch_hz(&self, hz: f32) {
        let hz = hz.clamp(20.0, 20000.0);
        match self.dna.species.as_str() {
            "tblk" => {
                // StrikeVoice membrane frequency
                if let Some(h) = self.shared_handles.get("cell1.membrane_freq") {
                    h.set(hz);
                }
            }
            "dron" => {
                // HarmonicBed root frequency
                if let Some(h) = self.shared_handles.get("cell0.root_hz") {
                    h.set(hz);
                }
            }
            "melo" => {
                // TimbreVoice frequency
                if let Some(h) = self.shared_handles.get("cell1.freq") {
                    h.set(hz);
                }
            }
            _ => {
                // Generic: set any "freq" handle on any cell
                for (key, handle) in &self.shared_handles {
                    if key.ends_with(".freq") || key.ends_with(".root_hz") {
                        handle.set(hz);
                    }
                }
            }
        }
    }

    /// Species-specific pitch personality transform (S19).
    ///
    /// Blends external prompted pitch with organism's internal intent
    /// based on DNA fidelity and affinity weight. Each species responds
    /// differently to prompts.
    pub(super) fn personality_transform_pitch(&mut self, prompted_hz: f32) -> f32 {
        let fidelity = self.dna.fidelity.clamp(0.0, 1.0);
        // TODO: Get actual affinity_weight from graph edge (placeholder: 1.0)
        let affinity_weight = 1.0;
        let blend = fidelity * affinity_weight;

        // For now, internal pitch intent is just the last pitch
        // (will be replaced by seq_cell/func_gen_cell outputs in S20+)
        let internal_hz = self.last_actual_pitch;

        match self.dna.species.as_str() {
            "dron" => {
                // Slowly slew toward prompted pitch (simplification: direct lerp)
                // TODO S20: replace with actual slew_cell output
                internal_hz * (1.0 - blend * 0.1) + prompted_hz * blend * 0.1
            }
            "hoso" => {
                // Rigid follower — direct blend
                internal_hz * (1.0 - blend) + prompted_hz * blend
            }
            "spgl" => {
                // Barely acknowledges — very slow blend
                internal_hz * (1.0 - blend * 0.01) + prompted_hz * blend * 0.01
            }
            "acid" => {
                // Follows tightly — melodic lead responds expressively to raga gravity
                internal_hz * (1.0 - blend) + prompted_hz * blend
            }
            "isao" => {
                // FM lead — follows pitch cleanly with slight portamento smoothing.
                // The slew_cell in DNA handles the actual portamento; personality
                // transform just provides moderate direct tracking.
                internal_hz * (1.0 - blend * 0.8) + prompted_hz * blend * 0.8
            }
            "tblk" => {
                // Follows but quantizes to nearest membrane mode (simplified: direct blend)
                internal_hz * (1.0 - blend) + prompted_hz * blend
            }
            "kkit" => {
                // Ignores pitch entirely
                internal_hz
            }
            _ => {
                // Default: moderate following
                internal_hz * (1.0 - blend * 0.5) + prompted_hz * blend * 0.5
            }
        }
    }
}

/// Quantize a Hz value to the nearest weighted scale degree.
/// (Now used only in tests — audio-thread quantization uses quantize_to_scale_fast in organism_dsp.rs)
///
/// `gravity` is a 12-element array of per-semitone weights [0,1].
/// Degrees with weight < 0.1 are skipped. Higher weight = stronger pull.
/// `blend` controls how much the output follows the quantized value vs raw.
#[cfg(test)]
pub(super) fn quantize_to_scale(raw_hz: f32, gravity: &[f32], blend: f32) -> f32 {
    if gravity.is_empty() || blend < 0.01 {
        return raw_hz;
    }
    let midi = 12.0 * (raw_hz / 440.0).log2() + 69.0;
    let octave = (midi / 12.0).floor();
    let degree = midi - octave * 12.0;

    let mut best_degree = degree;
    let mut best_dist = f32::MAX;
    for (i, &weight) in gravity.iter().enumerate().take(12) {
        if weight < 0.1 {
            continue;
        }
        let d = i as f32;
        let dist = (degree - d).abs().min(12.0 - (degree - d).abs());
        let weighted_dist = dist / weight;
        if weighted_dist < best_dist {
            best_dist = weighted_dist;
            best_degree = d;
        }
    }

    let quantized_midi = octave * 12.0 + best_degree;
    let quantized_hz = 440.0 * 2.0f32.powf((quantized_midi - 69.0) / 12.0);
    // Blend between raw and quantized
    raw_hz * (1.0 - blend) + quantized_hz * blend
}
