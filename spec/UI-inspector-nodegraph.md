# UI: Inspector & Ledger Revision — Live Node Graph + Structured Readout

**Status**: Spec (blocks pre-union-iteration Phase A)
**Depends on**: OBS (observability — EdgeTrajectory, MusicalContext, ProcessChain), SAT (receiver satisfaction)
**Blocks**: pre-union-iteration (checklist items M1-M4 require visible interaction state)
**Priority**: Immediate

---

## Goal

Replace the linear console-scroll ledger and scattered inspection panels with a unified live node graph that shows the affinity topology as a visual network, with rolling ring-buffered values on hover, and organized sections for params, emotions, and routing. The user should be able to see the ecology as a living graph — not read it from a log.

---

## Current State

**Five separate text-based panels**, none of which show topology:

| Panel | What it shows | Problem |
|-------|--------------|---------|
| Ledger (F3) | 1000-event chronological scroll of edge weight changes | Linear console — can't see topology, events scroll past too fast to read |
| Edges | Sorted edge list (weight, satisfaction, impact, eligibility) | Flat list — no spatial context, can't see which modules connect |
| Emotions | Per-module valence/arousal/activity | Disconnected from graph — can't see what's driving emotion |
| Signal Log | Last 20 interesting signals | Ephemeral — blink and you miss it |
| Debug Inspector | Keyboard, quantizer, analysis state | Static text — no visual relationship to organisms |

**EdgeTrajectory** exists (256-sample ring buffer per edge, tick/weight/satisfaction/impact) but **is never displayed**. This is the most valuable data for understanding interaction dynamics and it's invisible.

**MusicalContext** exists per organism (from OBS spec) but is not exposed in any panel.

---

## 1. Live Node Graph

### 1a. Layout

Replace the Edges panel with a force-directed node-link diagram:

```
┌─── Ecology Graph ─────────────────────────────────┐
│                                                    │
│       [DRON]───────────[HOSO]                      │
│         │  \    0.45   / │                         │
│         │   \        /   │                         │
│    0.12 │    [SPGL]      │ 0.38                    │
│         │   /    \       │                         │
│         │  /  0.22\      │                         │
│       [ISAO]       [ACID]                          │
│         │     0.31    │                            │
│         └─────────────┘                            │
│                                                    │
│  Nodes: organism hue + size ∝ audio_energy         │
│  Edges: thickness ∝ weight, color ∝ satisfaction   │
│  ─── = weight > 0.3    ··· = weight > 0.1         │
│                                                    │
└────────────────────────────────────────────────────┘
```

### 1b. Node Rendering

Each organism module is a colored circle:
- **Color**: organism DNA hue (matches biofield renderer)
- **Size**: proportional to `current_rms` (louder = bigger node)
- **Border**: valence → green (positive), red (negative), gray (neutral)
- **Label**: organism name (e.g., "DRON", "HOSO")
- **Pulse**: arousal drives a subtle glow pulse on the node outline

### 1c. Edge Rendering

Each affinity graph edge between organism modules is a line:
- **Thickness**: proportional to `weight` (0.0 = invisible, 1.0 = 4px)
- **Color**: satisfaction gradient (red=0, yellow=0.5, green=1.0)
- **Style**: solid (weight > 0.3), dashed (0.1-0.3), hidden (< 0.1)
- **Direction**: subtle arrowhead if edge is asymmetric (src→dst weight differs from dst→src)

### 1d. Layout Algorithm

Simple force-directed layout (runs every frame, ~20 iterations):
- **Repulsion**: all nodes repel each other (Coulomb-like, 1/d²)
- **Attraction**: edges pull connected nodes together (spring, proportional to weight)
- **Centering**: gentle pull toward panel center
- **Damping**: velocity decay to prevent oscillation

No need for a full graph layout library — organisms are few (≤16) and the layout can be simple. Positions are panel-local, not related to biofield world positions.

### 1e. Infrastructure Nodes (optional, toggleable)

Infrastructure modules (ScaleModule, RagaModule, TalaModule, QuantizerModule) can optionally appear as smaller square nodes at the top, showing how they feed into organism modules. Toggle via checkbox: "Show infrastructure".

---

## 2. Hover Tooltips with Ring-Buffered Values

### 2a. Node Hover

Hovering over an organism node shows a floating tooltip with live values:

```
┌─── HOSO ────────────────────┐
│ Valence:  ████████░░  0.72  │
│ Arousal:  ██████░░░░  0.58  │
│ RMS:      ███░░░░░░░  0.14  │
│ Pitch:    392 Hz (G4)       │
│ Chaos:    ██░░░░░░░░  0.12  │
│ Scale:    C Major (0.85)    │
│ Sync:     soft (1/8)        │
│ Desire:   ████░░░░░░  0.42  │
│                             │
│  ┌─ Valence (30s) ────────┐ │
│  │    ╱╲  ╱╲╱╲           │ │
│  │ ──╱──╲╱────╲──────── 0│ │
│  │ ╱ 30s ago     now      │ │
│  └────────────────────────┘ │
└─────────────────────────────┘
```

Key features:
- **Live bars**: Valence, arousal, RMS, chaos, desire — updated each frame
- **Pitch readout**: Current sequencer pitch in Hz + note name
- **Scale/sync summary**: Current quantization state
- **Sparkline**: 30-second rolling history from MusicalContext ring buffer (valence shown, configurable)

### 2b. Edge Hover

Hovering over an edge shows the trajectory ring buffer as a mini chart:

```
┌─── DRON → HOSO ─────────────┐
│ Weight:       0.45           │
│ Satisfaction: 0.72           │
│ Impact:       0.31           │
│ Eligibility:  0.88           │
│ Age:          1240 ticks     │
│                              │
│  ┌─ Weight (60s) ──────────┐ │
│  │        ╱──╱╲             │ │
│  │ ──────╱     ╲──────     │ │
│  │ 0.1    60s ago    now   │ │
│  └─────────────────────────┘ │
│  ┌─ Satisfaction (60s) ────┐ │
│  │ ╱╲╱╲╱╲╱╲╱╲╱╲╱╲╱╲      │ │
│  │ 0.0                1.0  │ │
│  └─────────────────────────┘ │
└──────────────────────────────┘
```

The EdgeTrajectory ring buffer (256 samples, ~17 seconds at current interval) drives the sparkline. For longer history, increase the ring buffer or downsample.

### 2c. Click to Pin

Clicking a node or edge pins the tooltip as a floating window that stays visible. Click again or press Escape to unpin. Multiple pins allowed — enables side-by-side comparison of two organisms or edges.

---

## 3. Structured Sections (Sidebar or Tabbed)

Below or beside the node graph, organized sections replace the scattered panels:

### 3a. Params Section

Live parameter readout for the selected (clicked) organism:

```
┌─ Params: HOSO ─────────────┐
│ seq_cell                    │
│   bpm        120.0          │
│   swing        0.15         │
│   chaos        0.12  →mod   │ ← "→mod" = modulation target
│                             │
│ osc_cell                    │
│   freq       392.0 Hz →mod  │
│   det          5.2          │
│   gain         0.85         │
│   pw           0.45  →mod   │
│                             │
│ filter_cell                 │
│   cutoff    2400.0 Hz →mod  │
│   res          0.62         │
└─────────────────────────────┘
```

Groups by cell, shows all params with current values. Modulation targets (wired via ModWire) are tagged with `→mod` indicator. This is the structured version of the synth_detail sliders — read-only but always visible for the selected organism.

### 3b. Connections Section

For the selected organism, show all incoming and outgoing edges:

```
┌─ Connections: HOSO ─────────┐
│ INCOMING                    │
│  ← DRON     w:0.45 sat:0.72│
│  ← SPGL     w:0.22 sat:0.41│
│                             │
│ OUTGOING                    │
│  → DRON     w:0.38 sat:0.68│
│  → ISAO     w:0.12 sat:0.15│
└─────────────────────────────┘
```

### 3c. Activity Section

Rolling event stream (replaces the ledger's linear scroll), filtered to the selected organism:

```
┌─ Activity: HOSO ────────────┐
│ +3.2s  ← DRON  +0.02 (Hebb)│
│ +2.1s  → DRON  +0.01 (Delv)│
│ +1.4s  ← SPGL  -0.01 (Decy)│
│ +0.3s  ← DRON  +0.03 (Hebb)│
└─────────────────────────────┘
```

Relative timestamps (seconds ago), direction, weight delta, reason. Only shows events for the selected organism. Max 20 recent events.

---

## 4. Panel Consolidation

### Remove

- **Edges panel** → absorbed into node graph + connections section
- **Signal Log panel** → absorbed into activity section (signals are now edge-contextual)
- **Emotions panel** → absorbed into node hover tooltips (valence/arousal bars)

### Keep

- **Ledger (F3)** → kept as optional raw data view for power users, but hidden by default
- **Debug Inspector (F1)** → kept for infrastructure module inspection (keyboard, quantizer)

### New

- **Ecology Graph panel** — the main unified observability surface (replaces edges, emotions, signal log)

---

## 5. Data Sources

| Display | Source | Update Rate |
|---------|--------|-------------|
| Node position | Force-directed layout (panel-local) | Every frame |
| Node size | OrganismModule.current_rms | Every frame (from analysis drain) |
| Node border | OrganismModule valence/arousal | Every frame |
| Edge weight | AffinityGraph edge data | Every frame (from graph query) |
| Edge satisfaction | AffinityGraph edge data | Every frame |
| Hover sparkline (node) | MusicalContext history ring buffer (OBS) | 256 samples |
| Hover sparkline (edge) | EdgeTrajectory ring buffer | 256 samples, 4-tick interval |
| Params readout | OrganismUiState.cells[].params | Every frame (Shared atomic reads) |
| Connections list | AffinityGraph.edges_for(module_id) | Every frame |
| Activity stream | Ledger ring buffer, filtered by module | 1000 events |

All data sources are already computed — this panel is a visualization layer, not a new computation.

---

## 6. Implementation

### Phase 1 — Node Graph Core

1. New panel: `src/ui/panels/ecology_graph.rs`
2. Force-directed layout algorithm (simple spring + repulsion, ≤16 nodes)
3. Node rendering (circles with hue, size, border)
4. Edge rendering (lines with thickness, color, style)
5. Node/edge hit detection (hover + click)

### Phase 2 — Tooltips

6. Node hover tooltip (live bars: valence, arousal, RMS, chaos, desire)
7. Edge hover tooltip (weight, satisfaction, impact, trajectory sparkline)
8. Sparkline widget (generic: takes a ring buffer slice, renders as mini line chart)
9. Pin-to-window on click

### Phase 3 — Sidebar Sections

10. Params section (read-only cell params with modulation tags)
11. Connections section (incoming/outgoing edges for selected node)
12. Activity section (filtered ledger events for selected node)

### Phase 4 — Consolidation

13. Register panel in PanelVisibility + header tabs (Graph icon)
14. Remove Edges, Emotions, Signal Log panels (absorbed)
15. Default visibility: ecology graph ON, ledger OFF
16. Keyboard shortcut: F3 toggles ecology graph (repurposed from ledger)

---

## Sparkline Widget

Reusable egui widget for any ring buffer visualization:

```rust
pub fn sparkline(ui: &mut egui::Ui, data: &[f32], min: f32, max: f32, size: egui::Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    // Map data points to rect coordinates, draw polyline
}
```

Used for: valence history, weight trajectory, RMS envelope, chaos evolution. Fixed height (20-30px), configurable width.

---

## Critical Files

| File | Changes |
|------|---------|
| `src/ui/panels/ecology_graph.rs` | **NEW** — node graph, tooltips, sparklines, sidebar sections |
| `src/ui/panels/mod.rs` | Register ecology_graph, remove edges/emotions/signal_log |
| `src/ui/mod.rs` | Add ecology_graph to PanelVisibility, update defaults |
| `src/ui/tabs.rs` | Add ecology graph tab (Graph icon), repurpose F3 |
| `src/app.rs` | Wire ecology graph panel, pass AffinityGraph + ledger data |
| `src/affinity/graph.rs` | Add `edges_for(module_id)` query method |
| `src/affinity/trajectory.rs` | Expose ring buffer slice for sparkline rendering |
| `src/organism/module/mod.rs` | Expose MusicalContext history for node tooltips |
| `src/ui/panels/edges.rs` | **REMOVE** (absorbed) |
| `src/ui/panels/emotions.rs` | **REMOVE** (absorbed) |
| `src/ui/panels/signal_log.rs` | **REMOVE** (absorbed) |
