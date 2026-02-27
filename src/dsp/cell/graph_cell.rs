use std::collections::HashMap;

use hexodsp::{dsp::MAX_BLOCK_SIZE, new_node_engine, Cell, Matrix, NodeId, SAtom};

use crate::dsp::cell::hexo_cell::{
    FilterEnvState, HexoCell, LfoState, ParamBridge, VoiceControl,
};
use crate::dsp::cell::{param_or, DspCell};
use crate::dsp::shared::{shared, Shared};
use crate::organism::dna::{
    CellDna, FilterEnvDna, GraphDna, GraphEdgeDna, GraphNodeDna, LfoDna, NodeParamRef,
    ParamBindingDna, ParamTransform, SettingBindingDna, SharedParamDna, VoiceControlDna,
};

// ---------------------------------------------------------------------------
// GraphCell: data-driven HexoDSP graph builder
// ---------------------------------------------------------------------------

pub struct GraphCell;

impl GraphCell {
    /// Build a HexoCell from CellDna. If dna.graph is present, uses that topology.
    /// Otherwise, falls back to a built-in template matching dna.cell_type.
    pub fn from_dna(
        dna: &CellDna,
        sr: f32,
    ) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
        let graph = match &dna.graph {
            Some(g) => g.clone(),
            None => builtin_template(&dna.cell_type, dna)?,
        };

        let cell_name = dna.cell_type.clone();
        build_from_graph(dna, &graph, sr, cell_name)
    }
}

// ---------------------------------------------------------------------------
// Core builder
// ---------------------------------------------------------------------------

fn build_from_graph(
    dna: &CellDna,
    graph: &GraphDna,
    sr: f32,
    cell_name: String,
) -> Option<(Box<dyn DspCell>, Vec<(String, Shared)>)> {
    // 1. Resolve node IDs
    let node_map = resolve_nodes(&graph.nodes)?;

    // 2. Compute hex grid layout from topology
    let layout = auto_layout(&graph.nodes, &graph.edges);

    // 3. Build HexoDSP engine
    let (node_conf, mut node_exec) = new_node_engine();
    node_exec.set_sample_rate(sr);

    let grid_size = (graph.nodes.len() + 2).max(4);
    let mut matrix = Matrix::new(node_conf, grid_size, grid_size);

    // 4. Place cells on hex grid with correct port wiring
    place_nodes(&mut matrix, &graph.nodes, &graph.edges, &node_map, &layout);

    matrix.sync().ok()?;

    // 5. Apply param bindings
    for binding in &graph.param_bindings {
        let nid = *node_map.get(&binding.node)?;
        let pid = nid.inp_param(&binding.node_param)?;
        let raw_value = dna.params.get(&binding.dna_param).copied().unwrap_or(binding.default);
        let value = apply_transform(raw_value, &binding.transform);
        let norm = pid.norm(value);
        matrix.set_param(pid, SAtom::param(norm));
    }

    // 6. Apply setting bindings
    for binding in &graph.setting_bindings {
        let nid = *node_map.get(&binding.node)?;
        let pid = nid.inp_param(&binding.node_setting)?;
        // Try string_params first, then fall back to numeric params (cast to string for lookup)
        let str_val: String = if let Some(s) = dna.string_params.get(&binding.dna_param) {
            s.clone()
        } else if let Some(&f) = dna.params.get(&binding.dna_param) {
            (f as i64).to_string()
        } else {
            binding.default.clone()
        };
        let int_val = binding.mapping.get(&str_val).copied().unwrap_or_else(|| {
            binding.mapping.get(&binding.default).copied().unwrap_or(0)
        });
        matrix.set_param(pid, SAtom::setting(int_val));
    }

    // 7. Build shared param handles
    let mut param_bridges = Vec::new();
    let mut handles = Vec::new();

    for sp in &graph.shared_params {
        let nid = *node_map.get(&sp.node)?;
        let pid = nid.inp_param(&sp.node_param)?;
        let raw_default = sp.default;
        let default = apply_transform(raw_default, &sp.transform);
        let norm = pid.norm(default);
        matrix.set_param(pid, SAtom::param(norm));

        let handle = shared(default);
        param_bridges.push(ParamBridge::new(handle.clone(), pid, default));
        handles.push((sp.name.clone(), handle));
    }

    // 8. Build voice control
    let voice = build_voice_control(dna, graph, &node_map, &mut matrix, sr);

    // 9. Build LFO
    let lfo = build_lfo(dna, graph, &node_map, sr);

    // 10. Flush param updates
    node_exec.process_graph_updates();

    let cell = HexoCell::from_parts(
        matrix,
        node_exec,
        param_bridges,
        voice,
        lfo,
        graph.n_inputs,
        graph.n_outputs,
        cell_name,
    );

    Some((Box::new(cell), handles))
}

// ---------------------------------------------------------------------------
// Node resolution
// ---------------------------------------------------------------------------

fn resolve_nodes(nodes: &[GraphNodeDna]) -> Option<HashMap<String, NodeId>> {
    let mut map = HashMap::new();
    for node in nodes {
        let nid = node_id_from_type(&node.node_type, node.instance)?;
        map.insert(node.id.clone(), nid);
    }
    Some(map)
}

fn node_id_from_type(node_type: &str, instance: u8) -> Option<NodeId> {
    let nid = match node_type {
        "bosc" => NodeId::BOsc(instance),
        "sin" => NodeId::Sin(instance),
        "sfilter" => NodeId::SFilter(instance),
        "adsr" => NodeId::Adsr(instance),
        "out" => NodeId::Out(instance),
        "inp" => NodeId::Inp(instance),
        "mix3" => NodeId::Mix3(instance),
        "noise" => NodeId::Noise(instance),
        "amp" => NodeId::Amp(instance),
        _ => {
            // Try HexoDSP's generic parser as fallback
            let nid = NodeId::from_str(node_type);
            if nid == NodeId::Nop && node_type != "nop" {
                return None;
            }
            nid
        }
    };
    Some(nid)
}

// ---------------------------------------------------------------------------
// Auto-layout: topological sort → hex grid placement
// ---------------------------------------------------------------------------

struct NodeLayout {
    row: usize,
    col: usize,
}

fn auto_layout(nodes: &[GraphNodeDna], edges: &[GraphEdgeDna]) -> HashMap<String, NodeLayout> {
    let ids: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let id_to_idx: HashMap<&str, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();

    let n = nodes.len();
    // Build adjacency lists (children + parents)
    let mut children: Vec<Vec<usize>> = vec![vec![]; n];
    let mut parents: Vec<Vec<usize>> = vec![vec![]; n];
    let mut in_degree: Vec<usize> = vec![0; n];

    for edge in edges {
        if let (Some(&src), Some(&dst)) = (
            id_to_idx.get(edge.from_node.as_str()),
            id_to_idx.get(edge.to_node.as_str()),
        ) {
            children[src].push(dst);
            parents[dst].push(src);
            in_degree[dst] += 1;
        }
    }

    // Topological sort using Kahn's algorithm, computing longest path (depth)
    let mut queue: Vec<usize> = (0..n).filter(|i| in_degree[*i] == 0).collect();
    let mut depth: Vec<usize> = vec![0; n];

    let mut head = 0;
    while head < queue.len() {
        let u = queue[head];
        head += 1;
        for &v in &children[u] {
            depth[v] = depth[v].max(depth[u] + 1);
            in_degree[v] -= 1;
            if in_degree[v] == 0 {
                queue.push(v);
            }
        }
    }

    // Hex grid layout strategy:
    // - Primary chain (single-parent nodes) goes at col=1, rows by depth
    // - For Y-merge nodes (in_degree > 1): the first parent stays in the primary
    //   chain, secondary parents are moved to the merge node's row at col=0.
    //   This matches hex adjacency: TL input from (col=0, same_row) via BR output.
    //
    // Reference layout for Y-merge (from hexo_cell.rs test):
    //   osc1 at (1,0) output B → mix at (1,1) input T
    //   osc2 at (0,1) output BR → mix at (1,1) input TL
    //   out at (1,2) input T
    let mut row: Vec<usize> = depth.clone();
    let mut col: Vec<usize> = vec![1; n]; // default: primary column

    // Find merge nodes and relocate secondary parents
    for dst in 0..n {
        if parents[dst].len() > 1 {
            // First parent (by edge order) keeps its depth row on the primary column.
            // Secondary parents get moved to the merge node's row, col=0.
            for &sec_parent in &parents[dst][1..] {
                row[sec_parent] = row[dst];
                col[sec_parent] = 0;
            }
        }
    }

    let mut layout = HashMap::new();
    for i in 0..n {
        layout.insert(ids[i].to_string(), NodeLayout { row: row[i], col: col[i] });
    }

    layout
}

// ---------------------------------------------------------------------------
// Node placement on hex grid
// ---------------------------------------------------------------------------

fn place_nodes(
    matrix: &mut Matrix,
    nodes: &[GraphNodeDna],
    edges: &[GraphEdgeDna],
    node_map: &HashMap<String, NodeId>,
    layout: &HashMap<String, NodeLayout>,
) {
    // Build maps of incoming/outgoing edges per node
    let mut incoming: HashMap<&str, Vec<&GraphEdgeDna>> = HashMap::new();
    let mut outgoing: HashMap<&str, Vec<&GraphEdgeDna>> = HashMap::new();

    for edge in edges {
        incoming
            .entry(edge.to_node.as_str())
            .or_default()
            .push(edge);
        outgoing
            .entry(edge.from_node.as_str())
            .or_default()
            .push(edge);
    }

    for node in nodes {
        let nid = match node_map.get(&node.id) {
            Some(n) => *n,
            None => continue,
        };
        let pos = match layout.get(&node.id) {
            Some(p) => p,
            None => continue,
        };

        let mut cell = Cell::empty(nid);

        // Determine input ports based on incoming edges and relative positions
        let in_edges = incoming.get(node.id.as_str());
        if let Some(edges) = in_edges {
            let mut in_t = None;
            let mut in_tl = None;
            let mut in_tr = None;

            for edge in edges {
                let src_pos = layout.get(edge.from_node.as_str());
                let dir = match src_pos {
                    Some(sp) => direction_from_to(sp, pos),
                    None => InDir::T, // default
                };

                let port = nid.inp(&edge.to_port);
                match dir {
                    InDir::T => in_t = in_t.or(port),
                    InDir::TL => in_tl = in_tl.or(port),
                    InDir::TR => in_tr = in_tr.or(port),
                }
            }
            cell = cell.input(in_t, in_tl, in_tr);
        }

        // Determine output ports based on outgoing edges and relative positions
        let out_edges = outgoing.get(node.id.as_str());
        if let Some(edges) = out_edges {
            let mut out_b = None;
            let mut out_br = None;
            let mut out_bl = None;

            for edge in edges {
                let dst_pos = layout.get(edge.to_node.as_str());
                let dir = match dst_pos {
                    Some(dp) => out_direction_from_to(pos, dp),
                    None => OutDir::B, // default
                };

                let port = nid.out(&edge.from_port);
                match dir {
                    OutDir::B => out_b = out_b.or(port),
                    OutDir::BR => out_br = out_br.or(port),
                    OutDir::BL => out_bl = out_bl.or(port),
                }
            }
            cell = cell.out(out_bl, out_br, out_b);
        }

        matrix.place(pos.col, pos.row, cell);
    }
}

#[derive(Clone, Copy)]
enum InDir {
    T,
    TL,
    TR,
}

#[derive(Clone, Copy)]
enum OutDir {
    B,
    BR,
    BL,
}

/// Determine which input direction a source feeds into the destination.
fn direction_from_to(src: &NodeLayout, dst: &NodeLayout) -> InDir {
    if src.col == dst.col {
        InDir::T // same column, source above
    } else if src.col < dst.col {
        InDir::TL // source to the left
    } else {
        InDir::TR // source to the right
    }
}

/// Determine which output direction a source sends to the destination.
fn out_direction_from_to(src: &NodeLayout, dst: &NodeLayout) -> OutDir {
    if src.col == dst.col {
        OutDir::B // same column, destination below
    } else if dst.col > src.col {
        OutDir::BR // destination to the right
    } else {
        OutDir::BL // destination to the left
    }
}

// ---------------------------------------------------------------------------
// Voice control builder
// ---------------------------------------------------------------------------

fn build_voice_control(
    dna: &CellDna,
    graph: &GraphDna,
    node_map: &HashMap<String, NodeId>,
    matrix: &mut Matrix,
    sr: f32,
) -> Option<VoiceControl> {
    let vc_dna = graph.voice.as_ref()?;

    let freq_params: Vec<_> = vc_dna
        .freq_targets
        .iter()
        .filter_map(|r| resolve_param(r, node_map))
        .collect();
    let gate_params: Vec<_> = vc_dna
        .gate_targets
        .iter()
        .filter_map(|r| resolve_param(r, node_map))
        .collect();

    if freq_params.is_empty() && gate_params.is_empty() {
        return None;
    }

    // Build filter envelope if specified
    let filter_env = graph.filter_env.as_ref().and_then(|fe| {
        let pid = resolve_param(&fe.target, node_map)?;
        let base = dna.params.get(&fe.base_param).copied().unwrap_or(fe.base_default);
        let depth = dna.params.get(&fe.depth_param).copied().unwrap_or(fe.depth_default);
        let decay_ms = dna.params.get(&fe.decay_param).copied().unwrap_or(fe.decay_default);

        // Set initial filter param
        let norm = pid.norm(base);
        matrix.set_param(pid, SAtom::param(norm));

        if depth > 0.0 {
            Some(FilterEnvState {
                value: 0.0,
                decay_coeff: FilterEnvState::decay_coeff_from_ms(decay_ms, sr),
                cutoff_param: pid,
                base_cutoff: base,
                depth,
            })
        } else {
            None
        }
    });

    Some(VoiceControl {
        freq_params,
        gate_params,
        velocity: 0.0,
        filter_env,
    })
}

// ---------------------------------------------------------------------------
// LFO builder
// ---------------------------------------------------------------------------

fn build_lfo(
    dna: &CellDna,
    graph: &GraphDna,
    node_map: &HashMap<String, NodeId>,
    sr: f32,
) -> Option<LfoState> {
    let lfo_dna = graph.lfo.as_ref()?;
    let pid = resolve_param(&lfo_dna.target, node_map)?;
    let rate = dna.params.get(&lfo_dna.rate_param).copied().unwrap_or(lfo_dna.rate_default);
    let depth = dna.params.get(&lfo_dna.depth_param).copied().unwrap_or(lfo_dna.depth_default);
    let base = dna.params.get(&lfo_dna.base_param).copied().unwrap_or(lfo_dna.base_default);

    if depth > 0.0 && rate > 0.0 {
        let blocks_per_sec = sr / MAX_BLOCK_SIZE as f32;
        Some(LfoState {
            phase: 0.0,
            rate_hz: rate,
            depth,
            target_param: pid,
            base_value: base,
            blocks_per_sec,
        })
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn resolve_param(r: &NodeParamRef, node_map: &HashMap<String, NodeId>) -> Option<hexodsp::ParamId> {
    let nid = *node_map.get(&r.node)?;
    nid.inp_param(&r.param)
}

fn apply_transform(value: f32, transform: &Option<ParamTransform>) -> f32 {
    match transform {
        Some(ParamTransform::DivideBy(d)) => value / d,
        Some(ParamTransform::MultiplyBy(m)) => value * m,
        None => value,
    }
}

// ---------------------------------------------------------------------------
// Built-in templates
// ---------------------------------------------------------------------------

fn builtin_template(cell_type: &str, dna: &CellDna) -> Option<GraphDna> {
    match cell_type {
        "hexo_timbre" => Some(timbre_template(dna)),
        "hexo_bed" => Some(bed_template(dna)),
        "hexo_strike" => Some(strike_template(dna)),
        _ => None,
    }
}

/// Timbre voice: dual BOsc → Mix3 → SFilter → Adsr → Out + filter_env + voice
fn timbre_template(dna: &CellDna) -> GraphDna {
    let cutoff = param_or(dna, "cutoff", 2000.0);
    let env_depth = param_or(dna, "env_depth", 3000.0);
    let env_decay = param_or(dna, "env_decay", 300.0);

    let mut wtype_map: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    wtype_map.insert("sin".into(), 0);
    wtype_map.insert("tri".into(), 1);
    wtype_map.insert("saw".into(), 2);
    wtype_map.insert("pulse".into(), 3);

    let mut ftype_map: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    ftype_map.insert("lp1p".into(), 0);
    ftype_map.insert("svf_lp".into(), 4);
    ftype_map.insert("simper_lp".into(), 8);
    ftype_map.insert("simper_hp".into(), 9);
    ftype_map.insert("simper_bp".into(), 10);
    ftype_map.insert("moog".into(), 13);

    GraphDna {
        nodes: vec![
            GraphNodeDna { id: "osc1".into(), node_type: "bosc".into(), instance: 0 },
            GraphNodeDna { id: "osc2".into(), node_type: "bosc".into(), instance: 1 },
            GraphNodeDna { id: "mix".into(), node_type: "mix3".into(), instance: 0 },
            GraphNodeDna { id: "filt".into(), node_type: "sfilter".into(), instance: 0 },
            GraphNodeDna { id: "env".into(), node_type: "adsr".into(), instance: 0 },
            GraphNodeDna { id: "out".into(), node_type: "out".into(), instance: 0 },
        ],
        edges: vec![
            GraphEdgeDna { from_node: "osc1".into(), from_port: "sig".into(), to_node: "mix".into(), to_port: "ch1".into() },
            GraphEdgeDna { from_node: "osc2".into(), from_port: "sig".into(), to_node: "mix".into(), to_port: "ch2".into() },
            GraphEdgeDna { from_node: "mix".into(), from_port: "sig".into(), to_node: "filt".into(), to_port: "inp".into() },
            GraphEdgeDna { from_node: "filt".into(), from_port: "sig".into(), to_node: "env".into(), to_port: "inp".into() },
            GraphEdgeDna { from_node: "env".into(), from_port: "sig".into(), to_node: "out".into(), to_port: "ch1".into() },
        ],
        param_bindings: vec![
            ParamBindingDna { dna_param: "freq".into(), node: "osc1".into(), node_param: "freq".into(), default: 440.0, transform: None },
            ParamBindingDna { dna_param: "freq".into(), node: "osc2".into(), node_param: "freq".into(), default: 440.0, transform: None },
            ParamBindingDna { dna_param: "det".into(), node: "osc2".into(), node_param: "det".into(), default: 7.0, transform: Some(ParamTransform::DivideBy(100.0)) },
            ParamBindingDna { dna_param: "cutoff".into(), node: "filt".into(), node_param: "freq".into(), default: 2000.0, transform: None },
            ParamBindingDna { dna_param: "res".into(), node: "filt".into(), node_param: "res".into(), default: 0.3, transform: None },
            ParamBindingDna { dna_param: "atk".into(), node: "env".into(), node_param: "atk".into(), default: 5.0, transform: None },
            ParamBindingDna { dna_param: "dcy".into(), node: "env".into(), node_param: "dcy".into(), default: 100.0, transform: None },
            ParamBindingDna { dna_param: "sus".into(), node: "env".into(), node_param: "sus".into(), default: 0.7, transform: None },
            ParamBindingDna { dna_param: "rel".into(), node: "env".into(), node_param: "rel".into(), default: 200.0, transform: None },
            ParamBindingDna { dna_param: "osc_mix".into(), node: "mix".into(), node_param: "vol1".into(), default: 0.7, transform: None },
            ParamBindingDna { dna_param: "osc_mix".into(), node: "mix".into(), node_param: "vol2".into(), default: 0.7, transform: None },
        ],
        setting_bindings: vec![
            SettingBindingDna { dna_param: "wtype".into(), node: "osc1".into(), node_setting: "wtype".into(), mapping: wtype_map.clone(), default: "saw".into() },
            SettingBindingDna { dna_param: "wtype".into(), node: "osc2".into(), node_setting: "wtype".into(), mapping: wtype_map, default: "saw".into() },
            SettingBindingDna { dna_param: "ftype".into(), node: "filt".into(), node_setting: "ftype".into(), mapping: ftype_map, default: "simper_lp".into() },
        ],
        shared_params: vec![
            SharedParamDna { name: "det".into(), node: "osc2".into(), node_param: "det".into(), default: 7.0, transform: Some(ParamTransform::DivideBy(100.0)) },
            SharedParamDna { name: "cutoff".into(), node: "filt".into(), node_param: "freq".into(), default: cutoff, transform: None },
            SharedParamDna { name: "res".into(), node: "filt".into(), node_param: "res".into(), default: 0.3, transform: None },
            SharedParamDna { name: "atk".into(), node: "env".into(), node_param: "atk".into(), default: 5.0, transform: None },
            SharedParamDna { name: "dcy".into(), node: "env".into(), node_param: "dcy".into(), default: 100.0, transform: None },
            SharedParamDna { name: "sus".into(), node: "env".into(), node_param: "sus".into(), default: 0.7, transform: None },
            SharedParamDna { name: "rel".into(), node: "env".into(), node_param: "rel".into(), default: 200.0, transform: None },
        ],
        voice: Some(VoiceControlDna {
            freq_targets: vec![
                NodeParamRef { node: "osc1".into(), param: "freq".into() },
                NodeParamRef { node: "osc2".into(), param: "freq".into() },
            ],
            gate_targets: vec![
                NodeParamRef { node: "env".into(), param: "gate".into() },
            ],
        }),
        filter_env: if env_depth > 0.0 {
            Some(FilterEnvDna {
                target: NodeParamRef { node: "filt".into(), param: "freq".into() },
                base_param: "cutoff".into(),
                depth_param: "env_depth".into(),
                decay_param: "env_decay".into(),
                base_default: cutoff,
                depth_default: env_depth,
                decay_default: env_decay,
            })
        } else {
            None
        },
        lfo: None,
        n_inputs: 0,
        n_outputs: 1,
    }
}

/// Bed voice: dual BOsc → Mix3 → SFilter → Out + LFO (no voice/envelope)
fn bed_template(dna: &CellDna) -> GraphDna {
    let cutoff = param_or(dna, "cutoff", 800.0);
    let lfo_rate = param_or(dna, "lfo_rate", 0.07);
    let lfo_depth = param_or(dna, "lfo_depth", 0.3);

    let mut wtype_map: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    wtype_map.insert("sin".into(), 0);
    wtype_map.insert("tri".into(), 1);
    wtype_map.insert("saw".into(), 2);
    wtype_map.insert("pulse".into(), 3);

    let mut ftype_map: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    ftype_map.insert("lp1p".into(), 0);
    ftype_map.insert("svf_lp".into(), 4);
    ftype_map.insert("simper_lp".into(), 8);
    ftype_map.insert("moog".into(), 13);

    GraphDna {
        nodes: vec![
            GraphNodeDna { id: "osc1".into(), node_type: "bosc".into(), instance: 0 },
            GraphNodeDna { id: "osc2".into(), node_type: "bosc".into(), instance: 1 },
            GraphNodeDna { id: "mix".into(), node_type: "mix3".into(), instance: 0 },
            GraphNodeDna { id: "filt".into(), node_type: "sfilter".into(), instance: 0 },
            GraphNodeDna { id: "out".into(), node_type: "out".into(), instance: 0 },
        ],
        edges: vec![
            GraphEdgeDna { from_node: "osc1".into(), from_port: "sig".into(), to_node: "mix".into(), to_port: "ch1".into() },
            GraphEdgeDna { from_node: "osc2".into(), from_port: "sig".into(), to_node: "mix".into(), to_port: "ch2".into() },
            GraphEdgeDna { from_node: "mix".into(), from_port: "sig".into(), to_node: "filt".into(), to_port: "inp".into() },
            GraphEdgeDna { from_node: "filt".into(), from_port: "sig".into(), to_node: "out".into(), to_port: "ch1".into() },
        ],
        param_bindings: vec![
            ParamBindingDna { dna_param: "root_hz".into(), node: "osc1".into(), node_param: "freq".into(), default: 110.0, transform: None },
            ParamBindingDna { dna_param: "root_hz".into(), node: "osc2".into(), node_param: "freq".into(), default: 110.0, transform: None },
            ParamBindingDna { dna_param: "det".into(), node: "osc2".into(), node_param: "det".into(), default: 7.0, transform: Some(ParamTransform::DivideBy(100.0)) },
            ParamBindingDna { dna_param: "cutoff".into(), node: "filt".into(), node_param: "freq".into(), default: 800.0, transform: None },
            ParamBindingDna { dna_param: "res".into(), node: "filt".into(), node_param: "res".into(), default: 0.3, transform: None },
            ParamBindingDna { dna_param: "osc_mix".into(), node: "mix".into(), node_param: "vol1".into(), default: 0.7, transform: None },
            ParamBindingDna { dna_param: "osc_mix".into(), node: "mix".into(), node_param: "vol2".into(), default: 0.7, transform: None },
        ],
        setting_bindings: vec![
            SettingBindingDna { dna_param: "wtype".into(), node: "osc1".into(), node_setting: "wtype".into(), mapping: wtype_map.clone(), default: "saw".into() },
            SettingBindingDna { dna_param: "wtype".into(), node: "osc2".into(), node_setting: "wtype".into(), mapping: wtype_map, default: "saw".into() },
            SettingBindingDna { dna_param: "ftype".into(), node: "filt".into(), node_setting: "ftype".into(), mapping: ftype_map, default: "moog".into() },
        ],
        shared_params: vec![
            SharedParamDna { name: "root_hz".into(), node: "osc1".into(), node_param: "freq".into(), default: 110.0, transform: None },
            SharedParamDna { name: "det".into(), node: "osc2".into(), node_param: "det".into(), default: 7.0, transform: Some(ParamTransform::DivideBy(100.0)) },
            SharedParamDna { name: "cutoff".into(), node: "filt".into(), node_param: "freq".into(), default: cutoff, transform: None },
            SharedParamDna { name: "res".into(), node: "filt".into(), node_param: "res".into(), default: 0.3, transform: None },
        ],
        voice: None,
        filter_env: None,
        lfo: Some(LfoDna {
            target: NodeParamRef { node: "filt".into(), param: "freq".into() },
            rate_param: "lfo_rate".into(),
            depth_param: "lfo_depth".into(),
            base_param: "cutoff".into(),
            rate_default: lfo_rate,
            depth_default: lfo_depth,
            base_default: cutoff,
        }),
        n_inputs: 0,
        n_outputs: 1,
    }
}

/// Strike voice: Noise → SFilter(BP) → Adsr → Out + filter_env (pitch sweep) + voice
fn strike_template(dna: &CellDna) -> GraphDna {
    let membrane_freq = param_or(dna, "membrane_freq", 180.0);
    let bandwidth = param_or(dna, "bandwidth", 0.5);
    let atk = param_or(dna, "atk", 1.0);
    let dcy = param_or(dna, "dcy", 150.0);
    let pitch_sweep = param_or(dna, "pitch_sweep", 3.0);
    let sweep_decay = param_or(dna, "sweep_decay", 50.0);
    // Pitch sweep depth: how many Hz above base the filter starts
    let sweep_depth = membrane_freq * (pitch_sweep - 1.0);

    // Strike uses numeric ftype param (legacy: "ftype": 6.0 in DNA params)
    let mut ftype_map: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    ftype_map.insert("svf_lp".into(), 4);
    ftype_map.insert("4".into(), 4);
    ftype_map.insert("svf_bp".into(), 6);
    ftype_map.insert("6".into(), 6);
    ftype_map.insert("simper_bp".into(), 10);
    ftype_map.insert("10".into(), 10);

    GraphDna {
        nodes: vec![
            GraphNodeDna { id: "noise".into(), node_type: "noise".into(), instance: 0 },
            GraphNodeDna { id: "filt".into(), node_type: "sfilter".into(), instance: 0 },
            GraphNodeDna { id: "env".into(), node_type: "adsr".into(), instance: 0 },
            GraphNodeDna { id: "out".into(), node_type: "out".into(), instance: 0 },
        ],
        edges: vec![
            GraphEdgeDna { from_node: "noise".into(), from_port: "sig".into(), to_node: "filt".into(), to_port: "inp".into() },
            GraphEdgeDna { from_node: "filt".into(), from_port: "sig".into(), to_node: "env".into(), to_port: "inp".into() },
            GraphEdgeDna { from_node: "env".into(), from_port: "sig".into(), to_node: "out".into(), to_port: "ch1".into() },
        ],
        param_bindings: vec![
            ParamBindingDna { dna_param: "membrane_freq".into(), node: "filt".into(), node_param: "freq".into(), default: membrane_freq, transform: None },
            ParamBindingDna { dna_param: "bandwidth".into(), node: "filt".into(), node_param: "res".into(), default: bandwidth, transform: None },
            ParamBindingDna { dna_param: "atk".into(), node: "env".into(), node_param: "atk".into(), default: atk, transform: None },
            ParamBindingDna { dna_param: "dcy".into(), node: "env".into(), node_param: "dcy".into(), default: dcy, transform: None },
            // sustain=0 for percussive one-shot
            ParamBindingDna { dna_param: "_sus_zero".into(), node: "env".into(), node_param: "sus".into(), default: 0.0, transform: None },
            // release matches decay
            ParamBindingDna { dna_param: "dcy".into(), node: "env".into(), node_param: "rel".into(), default: dcy, transform: None },
        ],
        setting_bindings: vec![
            SettingBindingDna { dna_param: "ftype".into(), node: "filt".into(), node_setting: "ftype".into(), mapping: ftype_map, default: "6".into() },
        ],
        shared_params: vec![
            SharedParamDna { name: "membrane_freq".into(), node: "filt".into(), node_param: "freq".into(), default: membrane_freq, transform: None },
            SharedParamDna { name: "bandwidth".into(), node: "filt".into(), node_param: "res".into(), default: bandwidth, transform: None },
            SharedParamDna { name: "atk".into(), node: "env".into(), node_param: "atk".into(), default: atk, transform: None },
            SharedParamDna { name: "dcy".into(), node: "env".into(), node_param: "dcy".into(), default: dcy, transform: None },
        ],
        voice: Some(VoiceControlDna {
            freq_targets: vec![
                NodeParamRef { node: "filt".into(), param: "freq".into() },
            ],
            gate_targets: vec![
                NodeParamRef { node: "env".into(), param: "gate".into() },
            ],
        }),
        filter_env: Some(FilterEnvDna {
            target: NodeParamRef { node: "filt".into(), param: "freq".into() },
            base_param: "membrane_freq".into(),
            depth_param: "_sweep_depth".into(), // computed, not from DNA directly
            decay_param: "sweep_decay".into(),
            base_default: membrane_freq,
            depth_default: sweep_depth,
            decay_default: sweep_decay,
        }),
        lfo: None,
        n_inputs: 0,
        n_outputs: 1,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::command::DspCommand;
    use std::collections::BTreeMap;

    const SR: f32 = 44100.0;

    // --- Timbre voice tests (migrated from hexo_timbre.rs) ---

    fn make_timbre_dna() -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("freq".into(), 440.0);
        params.insert("det".into(), 7.0);
        params.insert("cutoff".into(), 2000.0);
        params.insert("res".into(), 0.3);
        params.insert("atk".into(), 5.0);
        params.insert("dcy".into(), 100.0);
        params.insert("sus".into(), 0.7);
        params.insert("rel".into(), 200.0);
        params.insert("env_depth".into(), 3000.0);
        params.insert("env_decay".into(), 300.0);
        params.insert("osc_mix".into(), 0.7);
        let mut string_params = BTreeMap::new();
        string_params.insert("wtype".into(), "saw".into());
        CellDna {
            cell_type: "hexo_timbre".into(),
            params,
            string_params,
            graph: None,
        }
    }

    #[test]
    fn hexo_timbre_builds() {
        let result = GraphCell::from_dna(&make_timbre_dna(), SR);
        assert!(result.is_some(), "hexo_timbre should build from DNA");
    }

    #[test]
    fn hexo_timbre_produces_audio_on_note_on() {
        let (mut cell, _) = GraphCell::from_dna(&make_timbre_dna(), SR).unwrap();
        cell.handle_command(&DspCommand::NoteOn {
            freq: 440.0,
            velocity: 1.0,
        });

        let mut buf = Vec::new();
        let mut out = [0.0f32];
        for _ in 0..4410 {
            cell.tick(&[], &mut out);
            buf.push(out[0]);
        }

        let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(
            rms > 0.01,
            "hexo_timbre should produce audio on NoteOn: rms={rms}"
        );
    }

    #[test]
    fn hexo_timbre_silent_after_release() {
        let (mut cell, _) = GraphCell::from_dna(&make_timbre_dna(), SR).unwrap();
        cell.handle_command(&DspCommand::NoteOn {
            freq: 440.0,
            velocity: 1.0,
        });

        let mut out = [0.0f32];
        for _ in 0..8820 {
            cell.tick(&[], &mut out);
        }

        cell.handle_command(&DspCommand::NoteOff);

        for _ in 0..44100 {
            cell.tick(&[], &mut out);
        }

        assert!(
            out[0].abs() < 0.05,
            "hexo_timbre should be silent after release: {}",
            out[0]
        );
    }

    #[test]
    fn hexo_timbre_is_mono() {
        let (cell, _) = GraphCell::from_dna(&make_timbre_dna(), SR).unwrap();
        assert_eq!(cell.output_channels(), 1);
    }

    #[test]
    fn hexo_timbre_all_waveforms_build() {
        for wtype in &["sin", "tri", "saw", "pulse"] {
            let mut dna = make_timbre_dna();
            dna.string_params.insert("wtype".into(), wtype.to_string());
            let result = GraphCell::from_dna(&dna, SR);
            assert!(result.is_some(), "wtype={wtype} should build");
        }
    }

    #[test]
    fn hexo_timbre_filter_env_produces_varied_output() {
        let (mut cell, _) = GraphCell::from_dna(&make_timbre_dna(), SR).unwrap();
        cell.handle_command(&DspCommand::NoteOn {
            freq: 220.0,
            velocity: 1.0,
        });

        let mut out = [0.0f32];
        for _ in 0..2205 {
            cell.tick(&[], &mut out);
        }

        let mut early = Vec::new();
        for _ in 0..2205 {
            cell.tick(&[], &mut out);
            early.push(out[0]);
        }

        for _ in 0..44100 {
            cell.tick(&[], &mut out);
        }
        let mut late = Vec::new();
        for _ in 0..2205 {
            cell.tick(&[], &mut out);
            late.push(out[0]);
        }

        let rms = |buf: &[f32]| -> f32 {
            (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt()
        };

        let early_rms = rms(&early);
        let late_rms = rms(&late);

        assert!(early_rms > 0.01, "Early sustain should have audio: rms={early_rms}");
        assert!(late_rms > 0.01, "Late sustain should have audio: rms={late_rms}");

        let ratio = early_rms / late_rms;
        assert!(
            (ratio - 1.0).abs() > 0.01,
            "Filter envelope should cause RMS variation: early={early_rms}, late={late_rms}, ratio={ratio}"
        );
    }

    // --- Bed voice tests (migrated from hexo_bed.rs) ---

    fn make_bed_dna() -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("root_hz".into(), 110.0);
        params.insert("det".into(), 7.0);
        params.insert("cutoff".into(), 800.0);
        params.insert("res".into(), 0.3);
        params.insert("lfo_rate".into(), 0.07);
        params.insert("lfo_depth".into(), 0.3);
        params.insert("osc_mix".into(), 0.7);
        let mut string_params = BTreeMap::new();
        string_params.insert("wtype".into(), "saw".into());
        string_params.insert("ftype".into(), "moog".into());
        CellDna {
            cell_type: "hexo_bed".into(),
            params,
            string_params,
            graph: None,
        }
    }

    #[test]
    fn hexo_bed_builds() {
        let result = GraphCell::from_dna(&make_bed_dna(), SR);
        assert!(result.is_some(), "hexo_bed should build from DNA");
    }

    #[test]
    fn hexo_bed_produces_continuous_audio() {
        let (mut cell, _) = GraphCell::from_dna(&make_bed_dna(), SR).unwrap();

        let mut buf = Vec::new();
        let mut out = [0.0f32];
        for _ in 0..44100 {
            cell.tick(&[], &mut out);
            buf.push(out[0]);
        }

        let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(
            rms > 0.01,
            "hexo_bed should produce continuous audio: rms={rms}"
        );
    }

    #[test]
    fn hexo_bed_is_mono() {
        let (cell, _) = GraphCell::from_dna(&make_bed_dna(), SR).unwrap();
        assert_eq!(cell.output_channels(), 1);
    }

    #[test]
    fn hexo_bed_shared_params_work() {
        let (mut cell, handles) = GraphCell::from_dna(&make_bed_dna(), SR).unwrap();

        for (name, handle) in &handles {
            if name == "root_hz" {
                handle.set(220.0);
            }
        }

        let mut out = [0.0f32];
        for _ in 0..4410 {
            cell.tick(&[], &mut out);
        }

        assert!(true, "Should not panic on param change");
    }

    #[test]
    fn hexo_bed_lfo_modulates_output() {
        let mut dna = make_bed_dna();
        dna.params.insert("lfo_rate".into(), 2.0);
        dna.params.insert("lfo_depth".into(), 0.5);

        let (mut cell, _) = GraphCell::from_dna(&dna, SR).unwrap();

        let chunk_samples = 2205;
        let mut chunk_rms_values = Vec::new();
        let mut out = [0.0f32];

        for _ in 0..20 {
            let mut chunk = Vec::new();
            for _ in 0..chunk_samples {
                cell.tick(&[], &mut out);
                chunk.push(out[0]);
            }
            let rms: f32 = (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
            chunk_rms_values.push(rms);
        }

        let min_rms = chunk_rms_values.iter().copied().fold(f32::MAX, f32::min);
        let max_rms = chunk_rms_values.iter().copied().fold(0.0f32, f32::max);

        assert!(
            max_rms - min_rms > 0.001,
            "LFO should create amplitude variation: min={min_rms}, max={max_rms}"
        );
    }

    // --- Strike voice tests (migrated from hexo_strike.rs) ---

    fn make_strike_dna() -> CellDna {
        let mut params = BTreeMap::new();
        params.insert("membrane_freq".into(), 180.0);
        params.insert("bandwidth".into(), 0.5);
        params.insert("atk".into(), 1.0);
        params.insert("dcy".into(), 150.0);
        params.insert("pitch_sweep".into(), 3.0);
        params.insert("sweep_decay".into(), 50.0);
        CellDna {
            cell_type: "hexo_strike".into(),
            params,
            string_params: BTreeMap::new(),
            graph: None,
        }
    }

    #[test]
    fn hexo_strike_builds() {
        let result = GraphCell::from_dna(&make_strike_dna(), SR);
        assert!(result.is_some(), "hexo_strike should build from DNA");
    }

    #[test]
    fn hexo_strike_produces_audio_on_note_on() {
        let (mut cell, _) = GraphCell::from_dna(&make_strike_dna(), SR).unwrap();
        cell.handle_command(&DspCommand::NoteOn {
            freq: 180.0,
            velocity: 1.0,
        });

        let mut buf = Vec::new();
        let mut out = [0.0f32];
        for _ in 0..4410 {
            cell.tick(&[], &mut out);
            buf.push(out[0]);
        }

        let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(
            rms > 0.001,
            "hexo_strike should produce audio on NoteOn: rms={rms}"
        );
    }

    #[test]
    fn hexo_strike_decays_naturally() {
        let (mut cell, _) = GraphCell::from_dna(&make_strike_dna(), SR).unwrap();
        cell.handle_command(&DspCommand::NoteOn {
            freq: 180.0,
            velocity: 1.0,
        });

        let mut out = [0.0f32];
        for _ in 0..44100 {
            cell.tick(&[], &mut out);
        }

        assert!(
            out[0].abs() < 0.05,
            "hexo_strike should decay naturally: {}",
            out[0]
        );
    }

    #[test]
    fn hexo_strike_silent_without_trigger() {
        let (mut cell, _) = GraphCell::from_dna(&make_strike_dna(), SR).unwrap();
        let mut out = [0.0f32];
        for _ in 0..1000 {
            cell.tick(&[], &mut out);
        }
        assert!(
            out[0].abs() < 0.01,
            "hexo_strike without trigger should be quiet: {}",
            out[0]
        );
    }

    #[test]
    fn hexo_strike_is_mono() {
        let (cell, _) = GraphCell::from_dna(&make_strike_dna(), SR).unwrap();
        assert_eq!(cell.output_channels(), 1);
    }

    #[test]
    fn hexo_strike_pitch_sweep_varies_timbre() {
        let (mut cell, _) = GraphCell::from_dna(&make_strike_dna(), SR).unwrap();
        cell.handle_command(&DspCommand::NoteOn {
            freq: 180.0,
            velocity: 1.0,
        });

        let mut early = Vec::new();
        let mut out = [0.0f32];
        for _ in 0..441 {
            cell.tick(&[], &mut out);
            early.push(out[0]);
        }

        cell.handle_command(&DspCommand::NoteOn {
            freq: 180.0,
            velocity: 1.0,
        });

        let rms: f32 = (early.iter().map(|s| s * s).sum::<f32>() / early.len() as f32).sqrt();
        assert!(
            rms > 0.0001,
            "Strike with pitch sweep should produce audio: rms={rms}"
        );
    }

    #[test]
    fn hexo_strike_no_sweep_builds() {
        let mut dna = make_strike_dna();
        dna.params.insert("pitch_sweep".into(), 1.0);
        let result = GraphCell::from_dna(&dna, SR);
        assert!(result.is_some(), "Strike with no sweep should build");
    }

    // --- GraphCell-specific tests ---

    #[test]
    fn graph_cell_generic_type_with_inline_graph() {
        // Build a simple osc→out graph via inline GraphDna
        let dna = CellDna {
            cell_type: "graph".into(),
            params: BTreeMap::new(),
            string_params: BTreeMap::new(),
            graph: Some(GraphDna {
                nodes: vec![
                    GraphNodeDna { id: "osc".into(), node_type: "sin".into(), instance: 0 },
                    GraphNodeDna { id: "out".into(), node_type: "out".into(), instance: 0 },
                ],
                edges: vec![
                    GraphEdgeDna {
                        from_node: "osc".into(), from_port: "sig".into(),
                        to_node: "out".into(), to_port: "ch1".into(),
                    },
                ],
                param_bindings: vec![],
                setting_bindings: vec![],
                shared_params: vec![],
                voice: None,
                filter_env: None,
                lfo: None,
                n_inputs: 0,
                n_outputs: 1,
            }),
        };

        let result = GraphCell::from_dna(&dna, SR);
        assert!(result.is_some(), "Inline graph should build");

        let (mut cell, _) = result.unwrap();
        let mut buf = Vec::new();
        let mut out = [0.0f32];
        for _ in 0..4410 {
            cell.tick(&[], &mut out);
            buf.push(out[0]);
        }

        let rms: f32 = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(rms > 0.01, "Inline graph Sin→Out should produce audio: rms={rms}");
    }

    #[test]
    fn graph_cell_unknown_type_without_graph_returns_none() {
        let dna = CellDna {
            cell_type: "unknown_voice".into(),
            params: BTreeMap::new(),
            string_params: BTreeMap::new(),
            graph: None,
        };
        assert!(GraphCell::from_dna(&dna, SR).is_none());
    }

    #[test]
    fn auto_layout_linear_chain() {
        let nodes = vec![
            GraphNodeDna { id: "a".into(), node_type: "noise".into(), instance: 0 },
            GraphNodeDna { id: "b".into(), node_type: "sfilter".into(), instance: 0 },
            GraphNodeDna { id: "c".into(), node_type: "out".into(), instance: 0 },
        ];
        let edges = vec![
            GraphEdgeDna { from_node: "a".into(), from_port: "sig".into(), to_node: "b".into(), to_port: "inp".into() },
            GraphEdgeDna { from_node: "b".into(), from_port: "sig".into(), to_node: "c".into(), to_port: "ch1".into() },
        ];
        let layout = auto_layout(&nodes, &edges);

        assert_eq!(layout["a"].row, 0);
        assert_eq!(layout["b"].row, 1);
        assert_eq!(layout["c"].row, 2);
        // Single node per row → col 1
        assert_eq!(layout["a"].col, 1);
        assert_eq!(layout["b"].col, 1);
        assert_eq!(layout["c"].col, 1);
    }

    #[test]
    fn auto_layout_y_merge() {
        // Two sources feeding one mixer
        let nodes = vec![
            GraphNodeDna { id: "osc1".into(), node_type: "bosc".into(), instance: 0 },
            GraphNodeDna { id: "osc2".into(), node_type: "bosc".into(), instance: 1 },
            GraphNodeDna { id: "mix".into(), node_type: "mix3".into(), instance: 0 },
            GraphNodeDna { id: "out".into(), node_type: "out".into(), instance: 0 },
        ];
        let edges = vec![
            GraphEdgeDna { from_node: "osc1".into(), from_port: "sig".into(), to_node: "mix".into(), to_port: "ch1".into() },
            GraphEdgeDna { from_node: "osc2".into(), from_port: "sig".into(), to_node: "mix".into(), to_port: "ch2".into() },
            GraphEdgeDna { from_node: "mix".into(), from_port: "sig".into(), to_node: "out".into(), to_port: "ch1".into() },
        ];
        let layout = auto_layout(&nodes, &edges);

        // Primary parent (osc1) at row 0 col 1
        // Secondary parent (osc2) relocated to merge row at col 0
        // mix at row 1 col 1, out at row 2 col 1
        assert_eq!(layout["osc1"].row, 0);
        assert_eq!(layout["osc1"].col, 1);
        assert_eq!(layout["osc2"].row, 1); // same row as merge target
        assert_eq!(layout["osc2"].col, 0); // secondary column
        assert_eq!(layout["mix"].row, 1);
        assert_eq!(layout["mix"].col, 1);
        assert_eq!(layout["out"].row, 2);
        assert_eq!(layout["out"].col, 1);
    }

    #[test]
    fn apply_transform_divide() {
        assert!((apply_transform(700.0, &Some(ParamTransform::DivideBy(100.0))) - 7.0).abs() < 0.01);
    }

    #[test]
    fn apply_transform_multiply() {
        assert!((apply_transform(2.0, &Some(ParamTransform::MultiplyBy(3.0))) - 6.0).abs() < 0.01);
    }

    #[test]
    fn apply_transform_none() {
        assert!((apply_transform(42.0, &None) - 42.0).abs() < 0.01);
    }
}
