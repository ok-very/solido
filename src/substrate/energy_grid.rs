//! CPU-side substrate energy grid — authoritative energy state for the living ecology.
//!
//! Block-based grid (16×16 pixel blocks). Video frames replenish energy,
//! organisms deplete it at their position. Each cell carries RGB energy,
//! derived pitch class (from HSV hue), pitch energy, and rhythm energy.
//! Key changes rotate the hue→pitch mapping. Scale/raga filters attenuate
//! non-active pitch classes.

/// Pixels per grid cell (each dimension).
pub const BLOCK_SIZE: u32 = 16;

/// A cell in the substrate energy grid.
#[derive(Clone, Copy)]
pub struct EnergyCell {
    /// RGB energy [0, 1] per channel. Depleted toward 0 by organisms, replenished by video.
    pub rgb: [f32; 3],
    /// Total energy (mean of RGB). Cached for fast organism sampling.
    pub energy: f32,
    /// Dominant pitch class at this cell [0, 11] (from HSV hue + key offset).
    pub pitch_class: u8,
    /// Pitch energy: saturation × scale_weight for this cell's pitch class.
    pub pitch_energy: f32,
    /// Rhythm energy: brightness (V channel) — drives tempo modulation.
    pub rhythm_energy: f32,
}

impl Default for EnergyCell {
    fn default() -> Self {
        Self {
            rgb: [0.0; 3],
            energy: 0.0,
            pitch_class: 0,
            pitch_energy: 0.0,
            rhythm_energy: 0.0,
        }
    }
}

/// Local sight data sampled from a neighborhood of grid cells.
pub struct LocalSight {
    pub brightness: f32,
    pub warmth: f32,
    pub motion: f32,
    pub edge: f32,
    pub dominant_pc: u8,
    pub pitch_diversity: f32,
    pub rhythm_energy: f32,
}

/// Block-resolution energy grid over the viewport.
pub struct SubstrateGrid {
    pub cells: Vec<EnergyCell>,
    pub cols: u32,
    pub rows: u32,
    pub viewport_w: u32,
    pub viewport_h: u32,
    /// Key offset for hue→pitch mapping. 0=C, 1=C#, ..., 11=B.
    pub key_offset: i8,
    /// Scale weights [0, 2] per pitch class. Attenuates non-scale hue bins.
    pub scale_weights: [f32; 12],
    /// Previous frame energy per cell (for motion detection).
    prev_energy: Vec<f32>,
}

impl SubstrateGrid {
    pub fn new(viewport_w: u32, viewport_h: u32) -> Self {
        let cols = (viewport_w / BLOCK_SIZE).max(1);
        let rows = (viewport_h / BLOCK_SIZE).max(1);
        let count = (cols * rows) as usize;
        Self {
            cells: vec![EnergyCell::default(); count],
            cols,
            rows,
            viewport_w,
            viewport_h,
            key_offset: 0,
            scale_weights: [1.0; 12], // Chromatic: all pitch classes equal
            prev_energy: vec![0.0; count],
        }
    }

    /// Resize if viewport changed. Resets energy to zero.
    pub fn resize_if_needed(&mut self, viewport_w: u32, viewport_h: u32) {
        let cols = (viewport_w / BLOCK_SIZE).max(1);
        let rows = (viewport_h / BLOCK_SIZE).max(1);
        if cols != self.cols || rows != self.rows {
            self.cols = cols;
            self.rows = rows;
            self.viewport_w = viewport_w;
            self.viewport_h = viewport_h;
            let count = (cols * rows) as usize;
            self.cells = vec![EnergyCell::default(); count];
            self.prev_energy = vec![0.0; count];
        }
    }

    /// Replenish energy from a video frame (RGB24, analysis resolution).
    /// After replenishment, recomputes derived fields (pitch_class, pitch_energy, rhythm_energy).
    pub fn replenish_from_video(&mut self, pixels: &[u8], video_w: u32, video_h: u32, refresh_rate: f32) {
        let rate = refresh_rate.clamp(0.0, 1.0);
        let inv = 1.0 - rate;
        for row in 0..self.rows {
            for col in 0..self.cols {
                let cx = (col as f32 + 0.5) / self.cols as f32;
                let cy = (row as f32 + 0.5) / self.rows as f32;
                let vx = (cx * video_w as f32).min(video_w as f32 - 1.0) as u32;
                let vy = (cy * video_h as f32).min(video_h as f32 - 1.0) as u32;
                let pi = ((vy * video_w + vx) * 3) as usize;

                if pi + 2 < pixels.len() {
                    let r = pixels[pi] as f32 / 255.0;
                    let g = pixels[pi + 1] as f32 / 255.0;
                    let b = pixels[pi + 2] as f32 / 255.0;

                    let idx = (row * self.cols + col) as usize;
                    let cell = &mut self.cells[idx];
                    cell.rgb[0] = cell.rgb[0] * inv + r * rate;
                    cell.rgb[1] = cell.rgb[1] * inv + g * rate;
                    cell.rgb[2] = cell.rgb[2] * inv + b * rate;
                    cell.energy = (cell.rgb[0] + cell.rgb[1] + cell.rgb[2]) / 3.0;
                }
            }
        }
        // Recompute derived fields after replenishment
        self.recompute_derived();
    }

    /// Recompute pitch_class, pitch_energy, rhythm_energy from current RGB + key offset + scale weights.
    fn recompute_derived(&mut self) {
        for cell in &mut self.cells {
            let (h, s, v) = rgb_to_hsv(cell.rgb[0], cell.rgb[1], cell.rgb[2]);
            let raw_pc = (h / 30.0).floor() as i8;
            let pc = ((raw_pc + self.key_offset).rem_euclid(12)) as u8;
            cell.pitch_class = pc;
            cell.pitch_energy = s * self.scale_weights[pc as usize];
            cell.rhythm_energy = v;
        }
    }

    /// Deplete energy at a viewport position. Returns consumed RGB [r, g, b].
    pub fn deplete(&mut self, x: f32, y: f32, radius_px: f32, appetite: f32) -> [f32; 3] {
        let cx = (x / BLOCK_SIZE as f32) as i32;
        let cy = (y / BLOCK_SIZE as f32) as i32;
        let block_radius = (radius_px / BLOCK_SIZE as f32).ceil() as i32;

        let mut consumed = [0.0f32; 3];
        let r2 = radius_px * radius_px;

        for dy in -block_radius..=block_radius {
            for dx in -block_radius..=block_radius {
                let gx = cx + dx;
                let gy = cy + dy;
                if gx < 0 || gy < 0 || gx >= self.cols as i32 || gy >= self.rows as i32 {
                    continue;
                }

                let cell_cx = (gx as f32 + 0.5) * BLOCK_SIZE as f32;
                let cell_cy = (gy as f32 + 0.5) * BLOCK_SIZE as f32;
                let dist2 = (cell_cx - x) * (cell_cx - x) + (cell_cy - y) * (cell_cy - y);
                if dist2 > r2 {
                    continue;
                }

                let t = 1.0 - (dist2 / r2).sqrt();
                let drain = appetite * t;

                let idx = (gy as u32 * self.cols + gx as u32) as usize;
                let cell = &mut self.cells[idx];
                for ch in 0..3 {
                    let taken = (drain * cell.rgb[ch]).min(cell.rgb[ch]);
                    cell.rgb[ch] -= taken;
                    consumed[ch] += taken;
                }
                cell.energy = (cell.rgb[0] + cell.rgb[1] + cell.rgb[2]) / 3.0;
            }
        }
        consumed
    }

    /// Sample energy at a viewport position.
    pub fn sample_energy(&self, x: f32, y: f32) -> f32 {
        let gx = (x / BLOCK_SIZE as f32) as u32;
        let gy = (y / BLOCK_SIZE as f32) as u32;
        if gx >= self.cols || gy >= self.rows {
            return 0.0;
        }
        self.cells[(gy * self.cols + gx) as usize].energy
    }

    /// Sample RGB energy at a viewport position (for nutrient feeding).
    pub fn sample_rgb(&self, x: f32, y: f32) -> [f32; 3] {
        let gx = (x / BLOCK_SIZE as f32) as u32;
        let gy = (y / BLOCK_SIZE as f32) as u32;
        if gx >= self.cols || gy >= self.rows {
            return [0.0; 3];
        }
        self.cells[(gy * self.cols + gx) as usize].rgb
    }

    /// Sample pitch class at a viewport position.
    pub fn sample_pitch_class(&self, x: f32, y: f32) -> u8 {
        let gx = (x / BLOCK_SIZE as f32) as u32;
        let gy = (y / BLOCK_SIZE as f32) as u32;
        if gx >= self.cols || gy >= self.rows {
            return 0;
        }
        self.cells[(gy * self.cols + gx) as usize].pitch_class
    }

    /// Compute the energy gradient at a viewport position.
    /// Returns [dx, dy] pointing toward the steepest energy increase.
    /// Magnitude reflects the strength of the gradient.
    pub fn energy_gradient(&self, x: f32, y: f32) -> [f32; 2] {
        let bs = BLOCK_SIZE as f32;
        let gx = (x / bs) as i32;
        let gy = (y / bs) as i32;
        let cols = self.cols as i32;
        let rows = self.rows as i32;

        let sample = |cx: i32, cy: i32| -> f32 {
            if cx < 0 || cy < 0 || cx >= cols || cy >= rows {
                return 0.0;
            }
            self.cells[(cy * cols + cx) as usize].energy
        };

        // Central differences
        let dx = sample(gx + 1, gy) - sample(gx - 1, gy);
        let dy = sample(gx, gy + 1) - sample(gx, gy - 1);
        [dx * 0.5, dy * 0.5]
    }

    /// Compute local sight features for an organism at a viewport position.
    /// `radius` is in grid cells (not pixels).
    pub fn local_sight(&mut self, x: f32, y: f32, radius: u32) -> LocalSight {
        let gcx = (x / BLOCK_SIZE as f32) as i32;
        let gcy = (y / BLOCK_SIZE as f32) as i32;
        let r = radius as i32;

        let mut sum_energy = 0.0f32;
        let mut sum_r = 0.0f32;
        let mut sum_b = 0.0f32;
        let mut sum_rhythm = 0.0f32;
        let mut sum_motion = 0.0f32;
        let mut energy_sq_sum = 0.0f32;
        let mut pc_counts = [0.0f32; 12];
        let mut count = 0u32;

        for dy in -r..=r {
            for dx in -r..=r {
                let gx = gcx + dx;
                let gy = gcy + dy;
                if gx < 0 || gy < 0 || gx >= self.cols as i32 || gy >= self.rows as i32 {
                    continue;
                }
                let idx = (gy as u32 * self.cols + gx as u32) as usize;
                let cell = &self.cells[idx];
                sum_energy += cell.energy;
                energy_sq_sum += cell.energy * cell.energy;
                sum_r += cell.rgb[0];
                sum_b += cell.rgb[2];
                sum_rhythm += cell.rhythm_energy;
                pc_counts[cell.pitch_class as usize] += cell.pitch_energy;

                // Motion: delta from previous frame
                let prev = self.prev_energy[idx];
                sum_motion += (cell.energy - prev).abs();

                count += 1;
            }
        }

        if count == 0 {
            return LocalSight {
                brightness: 0.0, warmth: 0.0, motion: 0.0, edge: 0.0,
                dominant_pc: 0, pitch_diversity: 0.0, rhythm_energy: 0.0,
            };
        }

        let n = count as f32;
        let mean_energy = sum_energy / n;
        let variance = (energy_sq_sum / n - mean_energy * mean_energy).max(0.0);
        let edge = if mean_energy > 0.001 { variance.sqrt() / mean_energy } else { 0.0 };

        let warmth_denom = (sum_r + sum_b).max(0.001);
        let warmth = (sum_r - sum_b) / warmth_denom;

        // Dominant pitch class and diversity
        let max_pc_energy = pc_counts.iter().cloned().fold(0.0f32, f32::max);
        let dominant_pc = pc_counts.iter().enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i as u8)
            .unwrap_or(0);
        let active_pcs = pc_counts.iter().filter(|&&e| e > max_pc_energy * 0.2).count();
        let pitch_diversity = (active_pcs as f32 / 12.0).clamp(0.0, 1.0);

        LocalSight {
            brightness: mean_energy.clamp(0.0, 1.0),
            warmth: warmth.clamp(-1.0, 1.0),
            motion: (sum_motion / n).clamp(0.0, 1.0),
            edge: edge.clamp(0.0, 1.0),
            dominant_pc,
            pitch_diversity,
            rhythm_energy: (sum_rhythm / n).clamp(0.0, 1.0),
        }
    }

    /// Deposit trail nutrients (mono-channel waste) at a viewport position.
    /// `waste_rgb` is the narrow-spectrum waste color (complement of consumed).
    /// `amount` is small (0.001–0.01 per frame). Blends into existing cell RGB.
    pub fn deposit_trail(&mut self, x: f32, y: f32, waste_rgb: [f32; 3], amount: f32) {
        let gx = (x / BLOCK_SIZE as f32) as i32;
        let gy = (y / BLOCK_SIZE as f32) as i32;
        if gx < 0 || gy < 0 || gx >= self.cols as i32 || gy >= self.rows as i32 {
            return;
        }
        let amt = amount.clamp(0.0, 0.05);
        let inv = 1.0 - amt;
        let idx = (gy as u32 * self.cols + gx as u32) as usize;
        let cell = &mut self.cells[idx];
        cell.rgb[0] = cell.rgb[0] * inv + waste_rgb[0] * amt;
        cell.rgb[1] = cell.rgb[1] * inv + waste_rgb[1] * amt;
        cell.rgb[2] = cell.rgb[2] * inv + waste_rgb[2] * amt;
        cell.energy = (cell.rgb[0] + cell.rgb[1] + cell.rgb[2]) / 3.0;
    }

    /// Snapshot current energy for motion detection (call once per frame after all updates).
    pub fn snapshot_for_motion(&mut self) {
        for (i, cell) in self.cells.iter().enumerate() {
            if i < self.prev_energy.len() {
                self.prev_energy[i] = cell.energy;
            }
        }
    }

    /// Mean energy in a circular region (for well lens energy tracking).
    pub fn mean_energy_in_region(&self, x: f32, y: f32, radius_px: f32) -> f32 {
        let gcx = (x / BLOCK_SIZE as f32) as i32;
        let gcy = (y / BLOCK_SIZE as f32) as i32;
        let block_r = (radius_px / BLOCK_SIZE as f32).ceil() as i32;
        let r2 = radius_px * radius_px;
        let mut sum = 0.0f32;
        let mut count = 0u32;
        for dy in -block_r..=block_r {
            for dx in -block_r..=block_r {
                let gx = gcx + dx;
                let gy = gcy + dy;
                if gx < 0 || gy < 0 || gx >= self.cols as i32 || gy >= self.rows as i32 {
                    continue;
                }
                let cell_cx = (gx as f32 + 0.5) * BLOCK_SIZE as f32;
                let cell_cy = (gy as f32 + 0.5) * BLOCK_SIZE as f32;
                let dist2 = (cell_cx - x) * (cell_cx - x) + (cell_cy - y) * (cell_cy - y);
                if dist2 > r2 { continue; }
                let idx = (gy as u32 * self.cols + gx as u32) as usize;
                sum += self.cells[idx].energy;
                count += 1;
            }
        }
        if count == 0 { 0.0 } else { sum / count as f32 }
    }

    /// Generate RGBA8 texture for GPU upload.
    /// RGB = energy color, A = energy level (depletion visualization).
    pub fn to_rgba8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity((self.cols * self.rows * 4) as usize);
        for cell in &self.cells {
            out.push((cell.rgb[0] * 255.0) as u8);
            out.push((cell.rgb[1] * 255.0) as u8);
            out.push((cell.rgb[2] * 255.0) as u8);
            out.push((cell.energy.clamp(0.0, 1.0) * 255.0) as u8);
        }
        out
    }
}

// ─── HSV Conversion ──────────────────────────────────────────────────────

/// Convert RGB [0,1] to HSV. H in [0, 360), S in [0, 1], V in [0, 1].
fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let v = max;
    let s = if max > 0.001 { delta / max } else { 0.0 };

    let h = if delta < 0.001 {
        0.0
    } else if (max - r).abs() < 0.001 {
        60.0 * (((g - b) / delta) % 6.0)
    } else if (max - g).abs() < 0.001 {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };

    let h = if h < 0.0 { h + 360.0 } else { h };
    (h, s, v)
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_dimensions() {
        let grid = SubstrateGrid::new(1920, 1080);
        assert_eq!(grid.cols, 120);
        assert_eq!(grid.rows, 67);
    }

    #[test]
    fn replenish_fills_energy() {
        let mut grid = SubstrateGrid::new(320, 320);
        let white = vec![255u8; 10 * 10 * 3];
        grid.replenish_from_video(&white, 10, 10, 1.0);
        for cell in &grid.cells {
            assert!((cell.energy - 1.0).abs() < 0.01, "energy={}", cell.energy);
        }
    }

    #[test]
    fn deplete_returns_consumed_rgb() {
        let mut grid = SubstrateGrid::new(320, 320);
        let white = vec![255u8; 10 * 10 * 3];
        grid.replenish_from_video(&white, 10, 10, 1.0);

        let consumed = grid.deplete(160.0, 160.0, 32.0, 0.1);
        assert!(consumed[0] > 0.0, "should consume red");
        assert!(consumed[1] > 0.0, "should consume green");
        assert!(consumed[2] > 0.0, "should consume blue");
    }

    #[test]
    fn hsv_red_maps_to_pc_0() {
        let (h, s, _) = rgb_to_hsv(1.0, 0.0, 0.0);
        assert!((h - 0.0).abs() < 1.0, "red hue should be ~0, got {}", h);
        assert!(s > 0.9, "red saturation should be ~1.0, got {}", s);
        let pc = (h / 30.0).floor() as u8;
        assert_eq!(pc, 0, "red should map to pitch class 0 (C)");
    }

    #[test]
    fn hsv_green_maps_to_pc_4() {
        let (h, _, _) = rgb_to_hsv(0.0, 1.0, 0.0);
        assert!((h - 120.0).abs() < 1.0, "green hue should be ~120, got {}", h);
        let pc = (h / 30.0).floor() as u8;
        assert_eq!(pc, 4, "green should map to pitch class 4 (E)");
    }

    #[test]
    fn key_offset_rotates_pitch_class() {
        let mut grid = SubstrateGrid::new(320, 320);
        // Pure red pixels → PC 0 (C) at key_offset=0
        let red = vec![255u8, 0, 0].repeat(10 * 10);
        grid.replenish_from_video(&red, 10, 10, 1.0);
        assert_eq!(grid.cells[0].pitch_class, 0);

        // Rotate key by +2 (to D) → same red pixel now maps to PC 2
        grid.key_offset = 2;
        grid.recompute_derived();
        assert_eq!(grid.cells[0].pitch_class, 2);
    }

    #[test]
    fn scale_weights_attenuate_pitch_energy() {
        let mut grid = SubstrateGrid::new(320, 320);
        let red = vec![255u8, 0, 0].repeat(10 * 10);
        grid.replenish_from_video(&red, 10, 10, 1.0);

        // Full weight → pitch_energy = saturation * 1.0
        let full_pe = grid.cells[0].pitch_energy;
        assert!(full_pe > 0.5, "should have pitch energy, got {}", full_pe);

        // Zero weight for PC 0 → pitch_energy = 0
        grid.scale_weights[0] = 0.0;
        grid.recompute_derived();
        assert!((grid.cells[0].pitch_energy - 0.0).abs() < 0.01);
    }

    #[test]
    fn local_sight_returns_values() {
        let mut grid = SubstrateGrid::new(320, 320);
        let white = vec![255u8; 10 * 10 * 3];
        grid.replenish_from_video(&white, 10, 10, 1.0);
        grid.snapshot_for_motion();

        let sight = grid.local_sight(160.0, 160.0, 3);
        assert!(sight.brightness > 0.5, "should see brightness");
        assert!(sight.rhythm_energy > 0.5, "should see rhythm energy");
    }

    #[test]
    fn sample_out_of_bounds_returns_zero() {
        let grid = SubstrateGrid::new(320, 320);
        assert_eq!(grid.sample_energy(-10.0, -10.0), 0.0);
        assert_eq!(grid.sample_energy(9999.0, 9999.0), 0.0);
    }

    #[test]
    fn deposit_trail_blends_waste_into_cell() {
        let mut grid = SubstrateGrid::new(320, 320);
        // Start with zero energy
        assert!((grid.sample_energy(160.0, 160.0) - 0.0).abs() < 0.01);

        // Deposit cyan waste
        let cyan = [0.0, 1.0, 1.0];
        grid.deposit_trail(160.0, 160.0, cyan, 0.01);

        let rgb = grid.sample_rgb(160.0, 160.0);
        assert!(rgb[0] < 0.001, "red should stay near 0");
        assert!(rgb[1] > 0.005, "green should increase from deposit");
        assert!(rgb[2] > 0.005, "blue should increase from deposit");
        assert!(grid.sample_energy(160.0, 160.0) > 0.0, "energy should increase");
    }

    #[test]
    fn deposit_trail_out_of_bounds_is_noop() {
        let mut grid = SubstrateGrid::new(320, 320);
        grid.deposit_trail(-10.0, -10.0, [1.0; 3], 0.01);
        grid.deposit_trail(9999.0, 9999.0, [1.0; 3], 0.01);
        // No panic, no effect
    }
}
