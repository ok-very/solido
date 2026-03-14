//! Per-organism musical context bus — aggregates all musical state into a
//! single snapshot-friendly struct for observability and future context-aware
//! behaviors.
//!
//! Each field has ONE writer (annotated in the struct doc). Updated at 60Hz
//! on the control thread, never on the audio thread.

/// Per-organism musical context. ~72 bytes, `Copy`, one cache line.
///
/// Populated by OrganismModule bridge handlers. Each field has a single writer:
/// - Pitch fields: `receive_signal(pitch_hz_port)`
/// - Scale fields: `receive_gravity_weights()`
/// - Gamaka fields: `receive_gamaka_config()`
/// - Rhythm fields: `receive_beat_phase()` / `receive_beat_trigger()`
/// - Audio fields: `tick()` from DspAnalysis
/// - Identity fields: constructor (immutable)
#[derive(Clone, Copy, Debug)]
pub struct MusicalContext {
    // --- Pitch (writer: receive_signal on pitch_hz_port) ---
    /// External prompted pitch in Hz (before personality transform).
    pub prompted_pitch_hz: f32,
    /// Actual pitch after species personality transform + gravity.
    pub actual_pitch_hz: f32,
    /// Current scale degree index (0-11 pitch class).
    pub scale_degree: u8,
    /// Signed distance from nearest scale degree in cents.
    pub pitch_deviation_cents: f32,

    // --- Scale (writer: receive_gravity_weights) ---
    /// Active scale affinity blend = scale_affinity × fidelity.
    pub scale_blend: f32,
    /// True after first gravity_weights received from RagaModule.
    pub scale_active: bool,

    // --- Gamaka (writer: receive_gamaka_config) ---
    /// Current gamaka state: 0=idle, 1=sliding, 2=vibrating.
    pub gamaka_state: u8,
    /// Effective gamaka depth after affinity blend.
    pub gamaka_depth: f32,

    // --- Rhythm (writer: receive_beat_phase / receive_beat_trigger) ---
    /// Position in tala cycle [0,1].
    pub beat_phase: f32,
    /// Ticks since last beat trigger (reset on trigger, saturating increment).
    pub ticks_since_beat: u16,
    /// Rhythm sync mode: 0=none, 1=soft, 2=hard.
    pub rhythm_sync_mode: u8,
    /// Rhythm affinity from DNA [0,1].
    #[allow(dead_code)]
    pub rhythm_affinity: f32,

    // --- Direction (writer: tick, from DirectionTracker) ---
    /// True when melody is ascending, false when descending.
    pub melodic_direction: bool,

    // --- Audio feedback (writer: tick, from DspAnalysis) ---
    /// Current RMS from audio thread.
    pub rms: f32,
    /// Sequencer gate state (from DspAnalysis bridge).
    pub seq_gate: bool,

    // --- Identity (writer: constructor, immutable) ---
    /// Species code: 0=dron, 1=hoso, 2=spgl, 3=acid, 4=tblk, 5=kkit, 6=other, 7=isao.
    pub species_code: u8,
    /// DNA fidelity [0,1].
    #[allow(dead_code)]
    pub fidelity: f32,

    // --- Timing ---
    /// Reactor tick at which this context was last updated.
    pub tick: u64,
}

impl Default for MusicalContext {
    fn default() -> Self {
        Self {
            prompted_pitch_hz: 261.63,
            actual_pitch_hz: 261.63,
            scale_degree: 0,
            pitch_deviation_cents: 0.0,
            scale_blend: 0.0,
            scale_active: false,
            gamaka_state: 0,
            gamaka_depth: 0.0,
            melodic_direction: true,
            beat_phase: 0.0,
            ticks_since_beat: u16::MAX,
            rhythm_sync_mode: 0,
            rhythm_affinity: 0.0,
            rms: 0.0,
            seq_gate: false,
            species_code: 6,
            fidelity: 0.5,
            tick: 0,
        }
    }
}

impl MusicalContext {
    /// Initialize from DNA fields (species personality, immutable fields).
    pub fn from_dna(species: &str, fidelity: f32, scale_affinity: f32,
                    rhythm_affinity: f32, rhythm_sync: &str) -> Self {
        Self {
            species_code: species_code_from_str(species),
            fidelity,
            scale_blend: scale_affinity * fidelity,
            rhythm_affinity,
            rhythm_sync_mode: match rhythm_sync {
                "soft" => 1,
                "hard" => 2,
                _ => 0,
            },
            ..Self::default()
        }
    }

    /// Human-readable species name.
    #[allow(dead_code)]
    pub fn species_name(&self) -> &'static str {
        match self.species_code {
            0 => "dron",
            1 => "hoso",
            2 => "spgl",
            3 => "acid",
            4 => "tblk",
            5 => "kkit",
            7 => "isao",
            _ => "other",
        }
    }
}

fn species_code_from_str(s: &str) -> u8 {
    match s {
        "dron" => 0,
        "hoso" => 1,
        "spgl" => 2,
        "acid" => 3,
        "tblk" => 4,
        "kkit" => 5,
        "isao" => 7,
        _ => 6,
    }
}

/// Given Hz and 12-element gravity weights, return (nearest_degree 0-11, deviation_cents).
pub fn nearest_degree_cents(hz: f32, weights: &[f32]) -> (u8, f32) {
    if weights.is_empty() || hz < 20.0 {
        return (0, 0.0);
    }
    let midi = 12.0 * (hz / 440.0).log2() + 69.0;
    let degree = ((midi % 12.0) + 12.0) % 12.0;
    let mut best_i = 0u8;
    let mut best_dist = f32::MAX;
    for (i, &w) in weights.iter().enumerate().take(12) {
        if w < 0.01 {
            continue;
        }
        let d = i as f32;
        let raw_dist = degree - d;
        let dist = raw_dist.abs().min(12.0 - raw_dist.abs());
        let weighted = dist / w;
        if weighted < best_dist {
            best_dist = weighted;
            best_i = i as u8;
        }
    }
    let raw_dev = (degree - best_i as f32) * 100.0;
    let deviation = if raw_dev > 600.0 {
        raw_dev - 1200.0
    } else if raw_dev < -600.0 {
        raw_dev + 1200.0
    } else {
        raw_dev
    };
    (best_i, deviation)
}

const CONTEXT_HISTORY_CAPACITY: usize = 256;

/// Ring buffer of MusicalContext snapshots.
/// Samples every `interval` ticks (default 4). Capacity 256 = ~17s at 60fps.
/// ~18KB per organism (72 bytes × 256).
pub struct ContextHistory {
    snapshots: Box<[MusicalContext; CONTEXT_HISTORY_CAPACITY]>,
    write_pos: u8,
    len: u16,
    interval: u32,
    ticks_since: u32,
}

impl ContextHistory {
    pub fn new(interval: u32) -> Self {
        Self {
            snapshots: Box::new([MusicalContext::default(); CONTEXT_HISTORY_CAPACITY]),
            write_pos: 0,
            len: 0,
            interval: interval.max(1),
            ticks_since: 0,
        }
    }

    /// Maybe snapshot. Returns true if a snapshot was taken.
    pub fn maybe_snapshot(&mut self, ctx: &MusicalContext) -> bool {
        self.ticks_since += 1;
        if self.ticks_since < self.interval {
            return false;
        }
        self.ticks_since = 0;
        self.snapshots[self.write_pos as usize] = *ctx;
        self.write_pos = self.write_pos.wrapping_add(1);
        if self.len < CONTEXT_HISTORY_CAPACITY as u16 {
            self.len += 1;
        }
        true
    }

    /// Get snapshot closest to a tick (for correlation with EdgeTrajectory).
    #[allow(dead_code)]
    pub fn at_tick(&self, tick: u64) -> Option<&MusicalContext> {
        if self.len == 0 {
            return None;
        }
        let mut best_idx = 0usize;
        let mut best_dist = u64::MAX;
        let total = self.len as usize;
        let start = if total < CONTEXT_HISTORY_CAPACITY {
            0
        } else {
            self.write_pos as usize
        };
        for i in 0..total {
            let idx = (start + i) % CONTEXT_HISTORY_CAPACITY;
            let s = &self.snapshots[idx];
            let dist = if s.tick > tick {
                s.tick - tick
            } else {
                tick - s.tick
            };
            if dist < best_dist {
                best_dist = dist;
                best_idx = idx;
            }
        }
        Some(&self.snapshots[best_idx])
    }

    /// Iterate all snapshots in chronological order.
    #[allow(dead_code)]
    pub fn iter(&self) -> impl Iterator<Item = &MusicalContext> {
        let total = self.len as usize;
        let start = if total < CONTEXT_HISTORY_CAPACITY {
            0
        } else {
            self.write_pos as usize
        };
        (0..total).map(move |i| {
            let idx = (start + i) % CONTEXT_HISTORY_CAPACITY;
            &self.snapshots[idx]
        })
    }

    /// Most recent snapshot.
    #[allow(dead_code)]
    pub fn latest(&self) -> Option<&MusicalContext> {
        if self.len == 0 {
            return None;
        }
        let idx = self.write_pos.wrapping_sub(1) as usize;
        Some(&self.snapshots[idx])
    }

    /// Number of valid snapshots.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_default_values() {
        let ctx = MusicalContext::default();
        assert!((ctx.prompted_pitch_hz - 261.63).abs() < 0.01);
        assert_eq!(ctx.scale_degree, 0);
        assert!(!ctx.scale_active);
        assert_eq!(ctx.gamaka_state, 0);
        assert_eq!(ctx.ticks_since_beat, u16::MAX);
        assert_eq!(ctx.rhythm_sync_mode, 0);
        assert_eq!(ctx.species_code, 6);
        assert_eq!(ctx.tick, 0);
    }

    #[test]
    fn context_from_dna() {
        let ctx = MusicalContext::from_dna("acid", 0.8, 0.7, 0.8, "hard");
        assert_eq!(ctx.species_code, 3);
        assert!((ctx.fidelity - 0.8).abs() < f32::EPSILON);
        assert!((ctx.scale_blend - 0.56).abs() < 0.01); // 0.7 * 0.8
        assert!((ctx.rhythm_affinity - 0.8).abs() < f32::EPSILON);
        assert_eq!(ctx.rhythm_sync_mode, 2);
    }

    #[test]
    fn context_size_assert() {
        assert!(
            std::mem::size_of::<MusicalContext>() <= 96,
            "MusicalContext too large: {} bytes",
            std::mem::size_of::<MusicalContext>()
        );
    }

    #[test]
    fn context_history_snapshots() {
        let mut history = ContextHistory::new(4);
        let mut ctx = MusicalContext::default();

        // Ticks 1-3: no snapshot
        for t in 1..=3u64 {
            ctx.tick = t;
            assert!(!history.maybe_snapshot(&ctx));
        }
        // Tick 4: snapshot
        ctx.tick = 4;
        assert!(history.maybe_snapshot(&ctx));
        assert_eq!(history.len(), 1);

        // Tick 8: snapshot
        ctx.tick = 8;
        ctx.beat_phase = 0.5;
        for t in 5..=8u64 {
            ctx.tick = t;
            history.maybe_snapshot(&ctx);
        }
        assert_eq!(history.len(), 2);
        let latest = history.latest().unwrap();
        assert_eq!(latest.tick, 8);
    }

    #[test]
    fn context_history_ring_wraps() {
        let mut history = ContextHistory::new(1);
        let mut ctx = MusicalContext::default();

        for t in 1..=300u64 {
            ctx.tick = t;
            history.maybe_snapshot(&ctx);
        }

        assert_eq!(history.len(), 256);
        let first = history.iter().next().unwrap();
        assert_eq!(first.tick, 45); // 300 - 256 + 1
        let latest = history.latest().unwrap();
        assert_eq!(latest.tick, 300);
    }

    #[test]
    fn context_at_tick_correlation() {
        let mut history = ContextHistory::new(4);
        let mut ctx = MusicalContext::default();

        // Snapshots at ticks 4, 8, 12
        for t in 1..=12u64 {
            ctx.tick = t;
            if t == 8 {
                ctx.rms = 0.9;
            }
            history.maybe_snapshot(&ctx);
        }

        // at_tick(7) should find tick 8 (nearest)
        let s = history.at_tick(7).unwrap();
        assert_eq!(s.tick, 8);
        assert!((s.rms - 0.9).abs() < f32::EPSILON);

        // at_tick(4) should find tick 4 (exact)
        let s = history.at_tick(4).unwrap();
        assert_eq!(s.tick, 4);
    }

    #[test]
    fn nearest_degree_cents_basic() {
        // All degrees equal weight → nearest wins
        let weights = [1.0; 12];
        // A4 = 440 Hz = MIDI 69 = degree 9 (A)
        let (deg, dev) = nearest_degree_cents(440.0, &weights);
        assert_eq!(deg, 9);
        assert!(dev.abs() < 1.0);
    }

    #[test]
    fn nearest_degree_cents_weighted() {
        // Only weight on degree 0 (C) and 7 (G)
        let mut weights = [0.0; 12];
        weights[0] = 1.0;
        weights[7] = 1.0;
        // D4 = 293.66 Hz = MIDI 62 = degree 2 → should snap to C (0)
        let (deg, _) = nearest_degree_cents(293.66, &weights);
        assert!(deg == 0 || deg == 7, "should snap to C or G, got {}", deg);
    }

    #[test]
    fn empty_history_queries() {
        let history = ContextHistory::new(4);
        assert!(history.is_empty());
        assert!(history.at_tick(100).is_none());
        assert!(history.latest().is_none());
        assert_eq!(history.iter().count(), 0);
    }
}
