//! CPU-side substrate energy grid — authoritative energy state for organism nutrition.
//!
//! Block-based grid (16×16 pixel blocks). Video frames replenish energy,
//! organisms deplete it at their position. The GPU renders this as the
//! substrate background with depletion→trail alpha blend.

/// Pixels per grid cell (each dimension).
pub const BLOCK_SIZE: u32 = 16;

/// A cell in the substrate energy grid.
#[derive(Clone, Copy)]
pub struct EnergyCell {
    /// RGB energy [0, 1] per channel. Depleted toward 0 by organisms, replenished by video.
    pub rgb: [f32; 3],
    /// Total energy (mean of RGB). Cached for fast organism sampling.
    pub energy: f32,
}

impl Default for EnergyCell {
    fn default() -> Self {
        Self {
            rgb: [0.0; 3],
            energy: 0.0,
        }
    }
}

/// Block-resolution energy grid over the viewport.
pub struct SubstrateGrid {
    pub cells: Vec<EnergyCell>,
    pub cols: u32,
    pub rows: u32,
    /// Viewport dimensions this grid covers.
    pub viewport_w: u32,
    pub viewport_h: u32,
}

impl SubstrateGrid {
    pub fn new(viewport_w: u32, viewport_h: u32) -> Self {
        let cols = (viewport_w / BLOCK_SIZE).max(1);
        let rows = (viewport_h / BLOCK_SIZE).max(1);
        Self {
            cells: vec![EnergyCell::default(); (cols * rows) as usize],
            cols,
            rows,
            viewport_w,
            viewport_h,
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
            self.cells.resize((cols * rows) as usize, EnergyCell::default());
            for c in &mut self.cells {
                *c = EnergyCell::default();
            }
        }
    }

    /// Replenish energy from a video frame (RGB24, analysis resolution).
    /// The video frame is upsampled to grid resolution via nearest-neighbor.
    /// `refresh_rate` controls how fast video overwrites current energy [0, 1].
    pub fn replenish_from_video(&mut self, pixels: &[u8], video_w: u32, video_h: u32, refresh_rate: f32) {
        let rate = refresh_rate.clamp(0.0, 1.0);
        let inv = 1.0 - rate;
        for row in 0..self.rows {
            for col in 0..self.cols {
                // Map grid cell center to video pixel coordinate
                let cx = (col as f32 + 0.5) / self.cols as f32;
                let cy = (row as f32 + 0.5) / self.rows as f32;
                let vx = (cx * video_w as f32) as u32;
                let vy = (cy * video_h as f32) as u32;
                let vx = vx.min(video_w - 1);
                let vy = vy.min(video_h - 1);
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
    }

    /// Deplete energy at a viewport position. Returns the energy consumed [0, 1].
    /// `radius_px` is the organism's feeding radius in viewport pixels.
    /// `appetite` is the drain rate per tick.
    pub fn deplete(&mut self, x: f32, y: f32, radius_px: f32, appetite: f32) -> f32 {
        let cx = (x / BLOCK_SIZE as f32) as i32;
        let cy = (y / BLOCK_SIZE as f32) as i32;
        let block_radius = (radius_px / BLOCK_SIZE as f32).ceil() as i32;

        let mut consumed = 0.0f32;
        let r2 = radius_px * radius_px;

        for dy in -block_radius..=block_radius {
            for dx in -block_radius..=block_radius {
                let gx = cx + dx;
                let gy = cy + dy;
                if gx < 0 || gy < 0 || gx >= self.cols as i32 || gy >= self.rows as i32 {
                    continue;
                }

                // Distance from organism center to cell center (in pixels)
                let cell_cx = (gx as f32 + 0.5) * BLOCK_SIZE as f32;
                let cell_cy = (gy as f32 + 0.5) * BLOCK_SIZE as f32;
                let dist2 = (cell_cx - x) * (cell_cx - x) + (cell_cy - y) * (cell_cy - y);
                if dist2 > r2 {
                    continue;
                }

                // Falloff: full at center, zero at radius
                let t = 1.0 - (dist2 / r2).sqrt();
                let drain = appetite * t;

                let idx = (gy as u32 * self.cols + gx as u32) as usize;
                let cell = &mut self.cells[idx];
                let taken = drain.min(cell.energy);
                cell.rgb[0] = (cell.rgb[0] - taken).max(0.0);
                cell.rgb[1] = (cell.rgb[1] - taken).max(0.0);
                cell.rgb[2] = (cell.rgb[2] - taken).max(0.0);
                cell.energy = (cell.rgb[0] + cell.rgb[1] + cell.rgb[2]) / 3.0;
                consumed += taken;
            }
        }
        consumed
    }

    /// Sample energy at a viewport position (for organism sight).
    pub fn sample_energy(&self, x: f32, y: f32) -> f32 {
        let gx = (x / BLOCK_SIZE as f32) as u32;
        let gy = (y / BLOCK_SIZE as f32) as u32;
        if gx >= self.cols || gy >= self.rows {
            return 0.0;
        }
        self.cells[(gy * self.cols + gx) as usize].energy
    }

    /// Generate an RGBA8 texture for GPU upload. Each cell maps to one texel.
    /// RGB = energy color, A = energy level (for depletion visualization).
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
        // White 10x10 video frame
        let white = vec![255u8; 10 * 10 * 3];
        grid.replenish_from_video(&white, 10, 10, 1.0);
        for cell in &grid.cells {
            assert!((cell.energy - 1.0).abs() < 0.01, "energy={}", cell.energy);
        }
    }

    #[test]
    fn deplete_reduces_energy() {
        let mut grid = SubstrateGrid::new(320, 320);
        let white = vec![255u8; 10 * 10 * 3];
        grid.replenish_from_video(&white, 10, 10, 1.0);

        let center_x = 160.0;
        let center_y = 160.0;
        let consumed = grid.deplete(center_x, center_y, 32.0, 0.1);
        assert!(consumed > 0.0, "should consume some energy");

        let remaining = grid.sample_energy(center_x, center_y);
        assert!(remaining < 1.0, "energy should be depleted at center");
    }

    #[test]
    fn sample_out_of_bounds_returns_zero() {
        let grid = SubstrateGrid::new(320, 320);
        assert_eq!(grid.sample_energy(-10.0, -10.0), 0.0);
        assert_eq!(grid.sample_energy(9999.0, 9999.0), 0.0);
    }
}
