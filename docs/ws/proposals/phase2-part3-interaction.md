# Phase 2 — Demo HUD Blueprint

## Part 3: Interaction Model & Signal Flow

---

## 1. Interaction Model Per Module

### 1.1 StackPanel (Left — NAV)

| Element | Gesture | Result |
|---------|---------|--------|
| Rail segment (LAYERS/NODES/HISTORY) | **Tap** | Switch active section; corresponding chip toggles; StackList repopulates |
| Rail segment | **Long-press** | Context menu (rename layer group, reorder sections) |
| Chip (LAYERS/NODES/HISTORY) | **Tap** | Same as rail segment tap — synchronized toggle group |
| Chip | **Long-press** | Context action (filter options for that section) |
| List item (in StackList) | **Tap** | Select item; emits `stack.active` port change; highlights connected nets |
| List item | **Long-press** | Context menu (duplicate, delete, rename) |
| List item | **Drag** | Reorder within list |
| Rail edge (leftmost 1u strip) | **Swipe right→left** | Collapse StackPanel to rail-only mode (chips + content hidden) |
| Rail edge (collapsed) | **Swipe left→right** | Expand StackPanel |

**Touch targets:**
- Rail segments: thickness 3u (72px) — meets minimum. Segment gap 0.5u provides visual separation; actual tap target extends to gap midpoint.
- Chips: minimum_size 3u×3u (72×72px) — meets minimum. Horizontal gap 1u.
- List items: row height 3u minimum, full-width tap target.

### 1.2 InspectorPanel (Right — EDIT)

| Element | Gesture | Result |
|---------|---------|--------|
| Rail segment (HEADER/TRANSFORM/MATERIAL/RENDER) | **Tap** | Scroll InspectorSections to corresponding section |
| Rail segment | **Long-press** | Context menu (collapse all, expand all, pin section) |
| Section header Chip (TRANSFORM/MATERIAL/RENDER) | **Tap** | Toggle section collapse/expand |
| Section header Chip | **Long-press** | Context action (reset section to defaults) |
| Param row value | **Tap** | Enter edit mode (numeric: show scrub overlay; color: show picker) |
| Param row value | **Drag horizontal** | Scrub numeric value (sensitivity scaled by importance) |
| Param row label | **Tap** | Select param; highlights connected connectors |
| Param row label | **Long-press** | Context menu (reset, copy value, keyframe) |
| LOCK chip | **Tap** | Toggle inspector lock (prevents selection changes from updating content) |

**Touch targets:**
- Rail segments: thickness 2.5u (60px) — below 3u minimum. Compensate: `custom_minimum_size.x = 3u` on the rail, which adds 0.5u invisible tap padding. The visual rail remains 2.5u; the interactive zone extends.
- Section header chips: 3u×3u minimum — meets target.
- Param rows: 3u row height (72px). Label takes ~40% width, value takes ~60%.

### 1.3 StatusStrip (Bottom — TELEMETRY)

| Element | Gesture | Result |
|---------|---------|--------|
| Mode chip (SELECT/MOVE/ROTATE/SCULPT) | **Tap** | Activate tool mode (radio group — only one active) |
| Mode chip | **Long-press** | Context menu (tool options, sub-modes) |
| Rail segment (MODE/SELECTION/PERF/RENDER) | **Tap** | Expand that status section (shows detail overlay above strip) |
| Rail segment | **Long-press** | No action (informational segments) |
| Telemetry value (FPS/VRAM/TRIS) | **Tap** | Toggle detailed readout (expanded view with graph) |
| Render mode chip (SOLID/WIRE) | **Tap** | Toggle render mode (radio group) |

**Touch targets:**
- Mode chips: 3u×3u minimum. Horizontal gap 1u between chips.
- Rail: thickness 2u (48px) — below minimum. Compensate: `custom_minimum_size.y = 3u`, adding 1u invisible padding above the visual rail.
- Telemetry values: block height 3u (micro label + value stacked).

### 1.4 SplineConnector Interaction

| Element | Gesture | Result |
|---------|---------|--------|
| Connector polyline | **Hover** (mouse/pen only) | Rim alpha boost to 1.0; connected module ports flash rim pulse |
| Connector polyline | **Tap** | Select net; both endpoint modules show "target brackets" (Bracket primitives at port positions) |
| Connector polyline | **Long-press** | Context menu (inspect net, reroute, hide) |
| Connector polyline | **Tap** (when already selected) | Deselect |

**Hit detection:** `_is_near_polyline()` with threshold `1.5u` (36px) — generous for touch. The HitArea Control in SplineConnector handles this.

**Touch adaptation:** Since connectors can't hover on touch, the first tap on a *module* highlights all connected connectors (alpha + rim boost). Tapping the connector directly also works but is secondary to module-based highlighting.

---

## 2. Signal Flow Diagrams

### 2.1 Rail Segment → Section Sync (StackPanel)

```
StackRail.segment_pressed(index: int)
    │
    ▼
stack_panel.gd::_on_rail_segment_pressed(index)
    │
    ├── Update chip toggle states (untoggle all, toggle matching chip)
    │   └── Chip.set_toggled(true) → Chip.toggled_changed signal (unused internally)
    │
    ├── Switch visible content in StackList
    │   └── StackList children: show matching section, hide others
    │
    └── [No connector update — section switching is internal navigation]
```

### 2.2 Chip Toggle → Section Collapse (InspectorPanel)

```
SectionHeader Chip.pressed
    │
    ▼
collapsible_section.gd::_on_header_pressed()
    │
    ├── Toggle SectionContent.visible
    │   └── VBoxContainer auto-relayouts
    │
    ├── InspectorPanel.size changed (container shrink/grow)
    │   └── NOTIFICATION_RESIZED propagates
    │       │
    │       ├── InspectorGlass._notification(RESIZED) → update BackBufferCopy rect
    │       │
    │       └── inspector_panel.gd → recalculate port positions
    │           └── port_positions_changed signal
    │               │
    │               ▼
    │           connector_manager.gd::_on_port_moved(port_id, new_pos)
    │               └── Re-route affected nets
    │                   └── SplineConnector.set_curve_points(...)
    │
    └── [Bracket beside header: no state change — static decoration]
```

### 2.3 List Item Selection → Connector Highlight (Cross-Module)

```
StackList item tap
    │
    ▼
stack_panel.gd::_on_item_selected(item_data)
    │
    ├── Emit signal: stack_panel.selection_changed(item_data)
    │
    ├── Update stack.active port state (new selection context)
    │   └── port_positions_changed (if selected item is at different Y position)
    │
    └── connector_manager.gd receives selection_changed
        │
        ├── Find nets connected to stack.active
        │   └── active_to_focus net (importance=90)
        │
        ├── Boost net visual: SplineConnector.set_importance(95) temporarily
        │   └── Triggers scanline animation, rim glow
        │
        ├── Flash target module ports:
        │   └── InspectorPanel.inspector.focus → show Bracket overlay at port position
        │
        └── Update InspectorPanel content (if not locked):
            └── inspector_panel.gd::_on_context_changed(item_data)
                └── Repopulate sections with item_data parameters
```

### 2.4 Mode Chip → Tool State (StatusStrip)

```
StatusModeChip.pressed (e.g., "MOVE")
    │
    ▼
status_strip.gd::_on_mode_chip_pressed(chip: Chip)
    │
    ├── Radio group: untoggle all mode chips, toggle pressed chip
    │   └── for chip in mode_chips: chip.set_toggled(chip == pressed_chip)
    │
    ├── Emit signal: status_strip.mode_changed(mode_name: String)
    │
    └── connector_manager.gd receives mode_changed
        │
        ├── Update mode_to_inspector net styling
        │   └── SplineConnector role changes to match new tool mode
        │       └── MOVE → role=EDIT, SCULPT → role=BLEND
        │
        └── [Tool mode propagates to viewport input handler — outside HUD scope]
```

### 2.5 Palette Switch → Full Cascade

```
HudTheme.set_palette(new_palette)
    │
    ├── push_all_uniforms() → every registered ShaderMaterial gets new u_tint_color, u_glass_tint, u_rim_color
    │   └── Affects: GlassPane (×3), LcarsElbow (×2), LcarsEndcap (×4), SplineConnector (×5)
    │
    ├── _rebuild_theme() → all type variations get new font_color bindings
    │   └── Affects: every Label in the tree via theme inheritance
    │
    └── palette_changed.emit()
        │
        ├── LcarsRail._on_palette_changed() → _apply_role_colors() on all segments
        ├── Chip._on_palette_changed() → _apply_role_style() + label color update
        ├── Bracket._on_palette_changed() → _apply_role_color() on all Line2D children
        ├── SplineConnector._on_palette_changed() → rim line color update
        ├── GlassPane._on_palette_changed() → (no-op, uniforms pushed by HudTheme)
        ├── LcarsElbow._on_palette_changed() → (no-op, uniforms pushed by HudTheme)
        └── LcarsEndcap._on_palette_changed() → (no-op, uniforms pushed by HudTheme)
```

### 2.6 Resize → Layout Cascade

```
Window resize / SubViewportContainer resize
    │
    ▼
StackPanel NOTIFICATION_RESIZED
    ├── StackGlass._notification(RESIZED)
    │   ├── _update_back_buffer_rect() → BackBufferCopy.rect = get_rect()
    │   └── port_positions_changed.emit(get_port_positions())
    │
    ├── StackRail._notification(RESIZED)
    │   └── port_positions_changed.emit(get_port_positions())
    │
    └── stack_panel.gd → recalculate composed port positions
        └── connector_manager receives port updates
            └── Re-route all affected nets

InspectorPanel NOTIFICATION_RESIZED
    └── [same cascade as StackPanel]

StatusStrip NOTIFICATION_RESIZED
    └── [same cascade]

glass_layer.gd watches all GlassPane resizes:
    └── Updates BackBufferCopy rects between panes
```

---

## 3. Signal Registry

All custom signals used in the composed layer (not in primitives — those are established):

| Source | Signal | Payload | Receivers |
|--------|--------|---------|-----------|
| `stack_panel.gd` | `selection_changed` | `item_data: Dictionary` | `connector_manager`, `inspector_panel` |
| `stack_panel.gd` | `section_switched` | `section_index: int` | internal (chip sync) |
| `status_strip.gd` | `mode_changed` | `mode_name: String` | `connector_manager`, viewport handler |
| `inspector_panel.gd` | `param_changed` | `param_id: String, value: Variant` | application logic |
| `inspector_panel.gd` | `lock_toggled` | `locked: bool` | `stack_panel` (stops pushing selection) |
| `connector_manager.gd` | `net_selected` | `net_id: String` | `stack_panel`, `inspector_panel` (port highlight) |
| `connector_manager.gd` | `net_deselected` | `net_id: String` | `stack_panel`, `inspector_panel` |
| `collapsible_section.gd` | `collapsed_changed` | `is_collapsed: bool` | `inspector_panel` (relayout) |
| `glass_layer.gd` | `glass_resized` | `pane_index: int` | internal (BackBufferCopy sync) |
