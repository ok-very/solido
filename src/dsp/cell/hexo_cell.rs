use crate::dsp::shared::{shared, Shared};
use hexodsp::nodes::NodeAudioContext;
use hexodsp::{
    dsp::MAX_BLOCK_SIZE, new_node_engine, Cell, CellDir, Matrix, MatrixCellChain, NodeExecutor,
    NodeId, ParamId, SAtom,
};

use crate::dsp::cell::DspCell;
use crate::dsp::command::{DspAnalysis, DspCommand};

// ---------------------------------------------------------------------------
// NodeAudioContext adapter
// ---------------------------------------------------------------------------

struct HexoContext<'a> {
    nframes: usize,
    input_l: &'a [f32],
    input_r: &'a [f32],
    output_l: &'a mut [f32],
    output_r: &'a mut [f32],
}

impl<'a> NodeAudioContext for HexoContext<'a> {
    #[inline]
    fn nframes(&self) -> usize {
        self.nframes
    }

    #[inline]
    fn output(&mut self, channel: usize, frame: usize, v: f32) {
        match channel {
            0 => self.output_l[frame] = v,
            1 => self.output_r[frame] = v,
            _ => {}
        }
    }

    #[inline]
    fn input(&mut self, channel: usize, frame: usize) -> f32 {
        match channel {
            0 => self.input_l[frame],
            1 => self.input_r[frame],
            _ => 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// HexoCell: generic adapter wrapping a HexoDSP mini-graph as a DspCell
// ---------------------------------------------------------------------------

/// Parameter bridge entry: Shared handle (read on audio thread) → HexoDSP ParamId.
pub(crate) struct ParamBridge {
    pub handle: Shared,
    pub param_id: ParamId,
    pub prev_value: f32,
}

impl ParamBridge {
    pub(crate) fn new(handle: Shared, param_id: ParamId, initial: f32) -> Self {
        Self {
            handle,
            param_id,
            prev_value: initial,
        }
    }
}

/// Filter envelope state: exponential decay targeting a HexoDSP param.
/// On NoteOn, `value` resets to 1.0. Each block, `value *= decay_coeff`.
/// The param is set to `base_cutoff + depth * value`.
pub(crate) struct FilterEnvState {
    pub value: f32,
    pub decay_coeff: f32,
    pub cutoff_param: ParamId,
    pub base_cutoff: f32,
    pub depth: f32,
}

impl FilterEnvState {
    /// Compute the per-block decay coefficient from decay time in milliseconds.
    pub(crate) fn decay_coeff_from_ms(decay_ms: f32, sample_rate: f32) -> f32 {
        let blocks_per_sec = sample_rate / MAX_BLOCK_SIZE as f32;
        let decay_secs = decay_ms / 1000.0;
        (-1.0 / (decay_secs * blocks_per_sec)).exp()
    }
}

/// LFO state: sine oscillator modulating a HexoDSP param at block rate.
/// Output is `base_value * 2^(depth * sin(phase))` for musical octave scaling.
pub(crate) struct LfoState {
    pub phase: f32,
    pub rate_hz: f32,
    pub depth: f32,
    pub target_param: ParamId,
    pub base_value: f32,
    pub blocks_per_sec: f32,
}

/// Voice-specific control: stores ParamIds for gate and freq so NoteOn/NoteOff
/// can trigger HexoDSP envelopes and set oscillator pitch.
pub(crate) struct VoiceControl {
    pub freq_params: Vec<ParamId>,
    pub gate_params: Vec<ParamId>,
    pub velocity: f32,
    pub filter_env: Option<FilterEnvState>,
}

/// A DspCell backed by a HexoDSP node graph. Buffers input samples up to
/// block_size, processes through NodeExecutor, returns output sample-by-sample.
pub struct HexoCell {
    matrix: Matrix,
    node_exec: NodeExecutor,
    params: Vec<ParamBridge>,
    voice: Option<VoiceControl>,
    lfo: Option<LfoState>,
    // Block buffering
    in_buf_l: Vec<f32>,
    in_buf_r: Vec<f32>,
    out_buf_l: Vec<f32>,
    out_buf_r: Vec<f32>,
    in_pos: usize,
    out_pos: usize,
    out_len: usize,
    block_size: usize,
    // Analysis
    rms_acc: f32,
    peak: f32,
    sample_count: u32,
    // Metadata
    n_inputs: usize,
    n_outputs: usize,
    cell_name: String,
}

impl HexoCell {
    /// Construct a HexoCell from pre-built components.
    /// Used by voice files that build the Matrix directly via Matrix::place().
    pub(crate) fn from_parts(
        matrix: Matrix,
        node_exec: NodeExecutor,
        params: Vec<ParamBridge>,
        voice: Option<VoiceControl>,
        lfo: Option<LfoState>,
        n_inputs: usize,
        n_outputs: usize,
        cell_name: String,
    ) -> Self {
        let block_size = MAX_BLOCK_SIZE;
        Self {
            matrix,
            node_exec,
            params,
            voice,
            lfo,
            in_buf_l: vec![0.0; block_size],
            in_buf_r: vec![0.0; block_size],
            out_buf_l: vec![0.0; block_size],
            out_buf_r: vec![0.0; block_size],
            in_pos: 0,
            out_pos: 0,
            out_len: 0,
            block_size,
            rms_acc: 0.0,
            peak: 0.0,
            sample_count: 0,
            n_inputs,
            n_outputs,
            cell_name,
        }
    }

    fn process_block(&mut self) {
        // Forward changed Shared values to HexoDSP via Matrix param bridge
        for p in &mut self.params {
            let val = p.handle.value();
            if (val - p.prev_value).abs() > 1.0e-7 {
                p.prev_value = val;
                // Normalize human-readable value for HexoDSP's internal range
                let norm = p.param_id.norm(val);
                self.matrix.set_param(p.param_id, SAtom::param(norm));
            }
        }

        // Filter envelope: exponential decay per block
        if let Some(ref mut voice) = self.voice {
            if let Some(ref mut fenv) = voice.filter_env {
                fenv.value *= fenv.decay_coeff;
                let cutoff = fenv.base_cutoff + fenv.depth * fenv.value;
                let norm = fenv.cutoff_param.norm(cutoff);
                self.matrix.set_param(fenv.cutoff_param, SAtom::param(norm));
            }
        }

        // LFO modulation
        if let Some(ref mut lfo) = self.lfo {
            lfo.phase += lfo.rate_hz / lfo.blocks_per_sec;
            if lfo.phase >= 1.0 {
                lfo.phase -= 1.0;
            }
            let mod_val = (lfo.phase * std::f32::consts::TAU).sin();
            let cutoff = lfo.base_value * 2.0f32.powf(lfo.depth * mod_val);
            let norm = lfo.target_param.norm(cutoff);
            self.matrix.set_param(lfo.target_param, SAtom::param(norm));
        }

        // Pick up param updates from ring buffer
        self.node_exec.process_graph_updates();

        let nframes = self.in_pos.min(self.block_size).max(1);

        let mut ctx = HexoContext {
            nframes,
            input_l: &self.in_buf_l[..nframes],
            input_r: &self.in_buf_r[..nframes],
            output_l: &mut self.out_buf_l[..nframes],
            output_r: &mut self.out_buf_r[..nframes],
        };

        self.node_exec.process(&mut ctx);

        self.out_pos = 0;
        self.out_len = nframes;
        self.in_pos = 0;
    }
}

impl DspCell for HexoCell {
    fn tick(&mut self, input: &[f32], output: &mut [f32]) {
        // Accumulate input sample
        if self.in_pos < self.block_size {
            if self.n_inputs > 0 {
                self.in_buf_l[self.in_pos] =
                    if !input.is_empty() { input[0] } else { 0.0 };
                self.in_buf_r[self.in_pos] =
                    if input.len() > 1 { input[1] } else { self.in_buf_l[self.in_pos] };
            }
            self.in_pos += 1;
        }

        // If output buffer exhausted, process a new block
        if self.out_pos >= self.out_len {
            self.process_block();
        }

        // Read output sample
        let l = if self.out_pos < self.out_len { self.out_buf_l[self.out_pos] } else { 0.0 };
        let r = if self.out_pos < self.out_len { self.out_buf_r[self.out_pos] } else { 0.0 };
        self.out_pos += 1;

        // Apply velocity scaling for voice cells
        let vel = self.voice.as_ref().map_or(1.0, |v| v.velocity);
        output[0] = l * vel;
        if self.n_outputs > 1 && output.len() > 1 {
            output[1] = r * vel;
        }

        // Analysis
        self.rms_acc += l * l;
        self.peak = self.peak.max(l.abs()).max(r.abs());
        self.sample_count += 1;
    }

    fn handle_command(&mut self, cmd: &DspCommand) {
        if let Some(ref mut voice) = self.voice {
            match cmd {
                DspCommand::NoteOn { freq, velocity } => {
                    for pid in &voice.freq_params {
                        let norm = pid.norm(*freq);
                        self.matrix.set_param(*pid, SAtom::param(norm));
                    }
                    for pid in &voice.gate_params {
                        let norm = pid.norm(1.0);
                        self.matrix.set_param(*pid, SAtom::param(norm));
                    }
                    voice.velocity = *velocity;
                    // Reset filter envelope
                    if let Some(ref mut fenv) = voice.filter_env {
                        fenv.value = 1.0;
                    }
                }
                DspCommand::NoteOff => {
                    for pid in &voice.gate_params {
                        let norm = pid.norm(0.0);
                        self.matrix.set_param(*pid, SAtom::param(norm));
                    }
                }
                DspCommand::Reset | DspCommand::Panic => {
                    self.reset();
                }
            }
        }
    }

    fn analysis(&self) -> DspAnalysis {
        let rms = if self.sample_count > 0 {
            (self.rms_acc / self.sample_count as f32).sqrt()
        } else {
            0.0
        };
        DspAnalysis { rms, peak: self.peak }
    }

    fn output_channels(&self) -> usize {
        self.n_outputs
    }

    fn reset(&mut self) {
        self.in_buf_l.iter_mut().for_each(|s| *s = 0.0);
        self.in_buf_r.iter_mut().for_each(|s| *s = 0.0);
        self.out_buf_l.iter_mut().for_each(|s| *s = 0.0);
        self.out_buf_r.iter_mut().for_each(|s| *s = 0.0);
        self.in_pos = 0;
        self.out_pos = 0;
        self.out_len = 0;
        self.rms_acc = 0.0;
        self.peak = 0.0;
        self.sample_count = 0;
    }

    fn name(&self) -> &str {
        &self.cell_name
    }
}

// ---------------------------------------------------------------------------
// HexoCellBuilder — linear chain builder (kept for simple graphs)
// ---------------------------------------------------------------------------

/// Describes a single step when building a MatrixCellChain.
pub enum ChainStep {
    /// First node: has output port only.
    Out {
        node: String,
        out_port: String,
        params: Vec<(String, f32)>,
        settings: Vec<(String, i64)>,
    },
    /// Middle node: has both input and output ports.
    Io {
        node: String,
        in_port: String,
        out_port: String,
        params: Vec<(String, f32)>,
        settings: Vec<(String, i64)>,
    },
    /// Last node: has input port only.
    Inp {
        node: String,
        in_port: String,
    },
}

/// Builder that constructs a HexoCell from a linear node chain description.
pub struct HexoCellBuilder {
    cell_name: String,
    n_inputs: usize,
    n_outputs: usize,
    sample_rate: f32,
    steps: Vec<ChainStep>,
    /// Shared-handle parameters: (display_name, chain_index, param_name, default_denorm).
    shared_params: Vec<(String, usize, String, f32)>,
    /// Voice freq control: (chain_index, param_name).
    voice_freq: Option<(usize, String)>,
    /// Voice gate control: (chain_index, param_name).
    voice_gate: Option<(usize, String)>,
    /// Optional filter envelope config: (chain_idx for filter, param_name, base_cutoff, depth, decay_ms).
    filter_env: Option<(usize, String, f32, f32, f32)>,
}

impl HexoCellBuilder {
    pub fn new(name: &str, sample_rate: f32) -> Self {
        Self {
            cell_name: name.to_string(),
            n_inputs: 0,
            n_outputs: 1,
            sample_rate,
            steps: Vec::new(),
            shared_params: Vec::new(),
            voice_freq: None,
            voice_gate: None,
            filter_env: None,
        }
    }

    pub fn inputs(mut self, n: usize) -> Self {
        self.n_inputs = n;
        self
    }

    pub fn outputs(mut self, n: usize) -> Self {
        self.n_outputs = n;
        self
    }

    /// Add a node with output only (first in chain). `params` are (name, denorm_value).
    pub fn chain_out(mut self, node: &str, out_port: &str, params: &[(&str, f32)]) -> Self {
        self.steps.push(ChainStep::Out {
            node: node.into(),
            out_port: out_port.into(),
            params: params.iter().map(|(n, v)| (n.to_string(), *v)).collect(),
            settings: Vec::new(),
        });
        self
    }

    /// Add a node with input and output (middle of chain). `params` are (name, denorm_value).
    pub fn chain_io(
        mut self,
        node: &str,
        in_port: &str,
        out_port: &str,
        params: &[(&str, f32)],
    ) -> Self {
        self.steps.push(ChainStep::Io {
            node: node.into(),
            in_port: in_port.into(),
            out_port: out_port.into(),
            params: params.iter().map(|(n, v)| (n.to_string(), *v)).collect(),
            settings: Vec::new(),
        });
        self
    }

    /// Add a node with input only (last in chain — typically "out").
    pub fn chain_inp(mut self, node: &str, in_port: &str) -> Self {
        self.steps.push(ChainStep::Inp {
            node: node.into(),
            in_port: in_port.into(),
        });
        self
    }

    /// Set an integer setting on the last added chain step (e.g., wtype, ftype).
    pub fn setting(mut self, name: &str, value: i64) -> Self {
        if let Some(step) = self.steps.last_mut() {
            match step {
                ChainStep::Out { settings, .. } | ChainStep::Io { settings, .. } => {
                    settings.push((name.to_string(), value));
                }
                ChainStep::Inp { .. } => {}
            }
        }
        self
    }

    /// Set the oscillator frequency param for voice NoteOn control.
    pub fn voice_freq(mut self, chain_idx: usize, param: &str) -> Self {
        self.voice_freq = Some((chain_idx, param.to_string()));
        self
    }

    /// Set the envelope gate param for voice NoteOn/NoteOff control.
    pub fn voice_gate(mut self, chain_idx: usize, param: &str) -> Self {
        self.voice_gate = Some((chain_idx, param.to_string()));
        self
    }

    /// Configure a filter envelope for the voice.
    /// On NoteOn, the target param jumps to `base_cutoff + depth` then decays to `base_cutoff`.
    pub fn filter_envelope(
        mut self,
        chain_idx: usize,
        param: &str,
        base_cutoff: f32,
        depth: f32,
        decay_ms: f32,
    ) -> Self {
        self.filter_env = Some((chain_idx, param.to_string(), base_cutoff, depth, decay_ms));
        self
    }

    /// Expose a parameter as a Shared handle for the control thread.
    /// `chain_idx` is the index into the step list. `param` is the HexoDSP param name.
    /// `default` is the denormalized default value.
    pub fn shared_param(
        mut self,
        display_name: &str,
        chain_idx: usize,
        param: &str,
        default: f32,
    ) -> Self {
        self.shared_params
            .push((display_name.into(), chain_idx, param.into(), default));
        self
    }

    /// Build the HexoCell. Returns (cell, shared_handles) matching CellFactory.
    pub fn build(self) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let (node_conf, mut node_exec) = new_node_engine();
        node_exec.set_sample_rate(self.sample_rate);

        let grid_size = (self.steps.len() + 2).max(4);
        let mut matrix = Matrix::new(node_conf, grid_size, grid_size);

        // Build the chain
        let mut chain = MatrixCellChain::new(CellDir::B);

        for step in &self.steps {
            match step {
                ChainStep::Out { node, out_port, params, settings } => {
                    chain.node_out(node, out_port);
                    for (name, val) in params {
                        chain.set_denorm(name, *val);
                    }
                    for (name, val) in settings {
                        chain.set_atom(name, SAtom::setting(*val));
                    }
                }
                ChainStep::Io { node, in_port, out_port, params, settings } => {
                    chain.node_io(node, in_port, out_port);
                    for (name, val) in params {
                        chain.set_denorm(name, *val);
                    }
                    for (name, val) in settings {
                        chain.set_atom(name, SAtom::setting(*val));
                    }
                }
                ChainStep::Inp { node, in_port } => {
                    chain.node_inp(node, in_port);
                }
            }
        }

        chain.place(&mut matrix, 0, 0).ok()?;
        matrix.sync().ok()?;

        // Build Shared param handles
        let mut param_bridges: Vec<ParamBridge> = Vec::new();
        let mut handles: Vec<(String, Shared)> = Vec::new();

        for (display_name, chain_idx, param_name, default) in &self.shared_params {
            // Get the NodeId that was placed at chain_idx
            if let Some(node_id) = self.node_id_at(*chain_idx) {
                if let Some(param_id) = node_id.inp_param(param_name) {
                    let norm = param_id.norm(*default);
                    matrix.set_param(param_id, SAtom::param(norm));

                    let handle = shared(*default);
                    param_bridges.push(ParamBridge::new(handle.clone(), param_id, *default));
                    handles.push((display_name.clone(), handle));
                }
            }
        }

        // Resolve voice control params
        let mut voice = match (&self.voice_freq, &self.voice_gate) {
            (Some((freq_idx, freq_param)), Some((gate_idx, gate_param))) => {
                let freq_nid = self.node_id_at(*freq_idx)?;
                let gate_nid = self.node_id_at(*gate_idx)?;
                let freq_pid = freq_nid.inp_param(freq_param)?;
                let gate_pid = gate_nid.inp_param(gate_param)?;
                Some(VoiceControl {
                    freq_params: vec![freq_pid],
                    gate_params: vec![gate_pid],
                    velocity: 0.0,
                    filter_env: None,
                })
            }
            _ => None,
        };

        // Resolve filter envelope
        if let Some((filt_idx, filt_param, base, depth, decay_ms)) = &self.filter_env {
            if let Some(ref mut vc) = voice {
                if let Some(nid) = self.node_id_at(*filt_idx) {
                    if let Some(pid) = nid.inp_param(filt_param) {
                        vc.filter_env = Some(FilterEnvState {
                            value: 0.0,
                            decay_coeff: FilterEnvState::decay_coeff_from_ms(
                                *decay_ms,
                                self.sample_rate,
                            ),
                            cutoff_param: pid,
                            base_cutoff: *base,
                            depth: *depth,
                        });
                    }
                }
            }
        }

        // Flush param updates
        node_exec.process_graph_updates();

        let cell = HexoCell::from_parts(
            matrix,
            node_exec,
            param_bridges,
            voice,
            None, // no LFO for chain-built cells
            self.n_inputs,
            self.n_outputs,
            self.cell_name,
        );

        Some((Box::new(cell), handles))
    }

    /// Resolve the NodeId for a given chain step index.
    fn node_id_at(&self, idx: usize) -> Option<NodeId> {
        let step = self.steps.get(idx)?;
        let name = match step {
            ChainStep::Out { node, .. } => node,
            ChainStep::Io { node, .. } => node,
            ChainStep::Inp { node, .. } => node,
        };
        let nid = NodeId::from_str(name);
        if nid == NodeId::Nop && name != "nop" {
            None
        } else {
            Some(nid)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 44100.0;

    #[test]
    fn hexodsp_spike_sin_to_out() {
        let (node_conf, mut node_exec) = new_node_engine();
        let mut matrix = Matrix::new(node_conf, 3, 3);
        let mut chain = MatrixCellChain::new(CellDir::B);

        chain.node_out("sin", "sig").node_inp("out", "ch1");
        chain.place(&mut matrix, 0, 0).unwrap();
        matrix.sync().unwrap();

        let (out_l, _out_r) = node_exec.test_run(0.1, false, &[]);

        let rms: f32 = (out_l.iter().map(|s| s * s).sum::<f32>() / out_l.len() as f32).sqrt();
        assert!(rms > 0.01, "HexoDSP Sin->Out should produce audio: rms={rms}");
    }

    #[test]
    fn hexocell_builder_sin_produces_audio() {
        let result = HexoCellBuilder::new("test_sin", SR)
            .outputs(1)
            .chain_out("sin", "sig", &[])
            .chain_inp("out", "ch1")
            .build();

        assert!(result.is_some(), "HexoCellBuilder should succeed");
        let (mut cell, _handles) = result.unwrap();

        let mut buf = Vec::new();
        let mut out = [0.0f32; 1];
        for _ in 0..4410 {
            cell.tick(&[], &mut out);
            buf.push(out[0]);
        }

        let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(rms > 0.01, "HexoCell Sin->Out should produce audio: rms={rms}");
    }

    #[test]
    fn hexocell_passes_input_through_amp() {
        let result = HexoCellBuilder::new("test_passthrough", SR)
            .inputs(1)
            .outputs(1)
            .chain_out("inp", "sig1", &[("vol", 1.0)])
            .chain_io("amp", "inp", "sig", &[("att", 1.0)])
            .chain_inp("out", "ch1")
            .build();

        assert!(result.is_some(), "Passthrough chain should build");
        let (mut cell, _handles) = result.unwrap();

        // Feed signal through; first block buffers, second block outputs
        let input = [0.5f32];
        let mut out = [0.0f32; 1];
        for _ in 0..MAX_BLOCK_SIZE * 3 {
            cell.tick(&input, &mut out);
        }

        assert!(
            out[0].abs() > 0.001,
            "Passthrough cell should pass audio: out={}",
            out[0]
        );
    }

    #[test]
    fn hexocell_direct_placement_two_sin_to_mix() {
        // Test direct Matrix placement: two Sin oscillators → Mix3 → Out
        let (node_conf, mut node_exec) = new_node_engine();
        node_exec.set_sample_rate(SR);
        let mut matrix = Matrix::new(node_conf, 3, 5);

        let sin0 = NodeId::Sin(0);
        let sin1 = NodeId::Sin(1);
        let mix = NodeId::Mix3(0);
        let out = NodeId::Out(0);

        // Sin(0) at (1,0), output B
        matrix.place(
            1, 0,
            Cell::empty(sin0).out(None, None, sin0.out("sig")),
        );
        // Sin(1) at (0,1), output BR → Mix3 TL
        matrix.place(
            0, 1,
            Cell::empty(sin1).out(None, sin1.out("sig"), None),
        );
        // Mix3 at (1,1), input T=ch1, TL=ch2, output B
        matrix.place(
            1, 1,
            Cell::empty(mix)
                .input(mix.inp("ch1"), mix.inp("ch2"), None)
                .out(None, None, mix.out("sig")),
        );
        // Out at (1,2), input T=ch1
        matrix.place(
            1, 2,
            Cell::empty(out).input(out.inp("ch1"), None, None),
        );

        matrix.sync().unwrap();

        let (out_l, _out_r) = node_exec.test_run(0.1, false, &[]);
        let rms: f32 = (out_l.iter().map(|s| s * s).sum::<f32>() / out_l.len() as f32).sqrt();
        assert!(
            rms > 0.01,
            "Two Sin → Mix3 → Out via direct placement should produce audio: rms={rms}"
        );
    }
}
