# Phase 2 — Demo HUD Blueprint

## Part 4: Decision Log & Implementation Order

---

## 1. Decision Log

### D1: Single CanvasLayer vs. Multiple CanvasLayers

**Chosen:** Single CanvasLayer (layer=10) with z_index separation.

**Considered:**
- **Multiple CanvasLayers** (glass on layer 10, structure on layer 11, text on layer 12): cleaner semantic separation, each layer gets its own draw pass. Rejected because glass shaders use `hint_screen_texture` + `SCREEN_UV` which samples everything rendered *before* the current CanvasItem. With multiple CanvasLayers, ordering between layers is guaranteed, but `SCREEN_UV` may sample content from the *same* CanvasLayer that hasn't finished rendering. A single CanvasLayer with z_index-ordered children gives predictable draw order within one pass.
- **CanvasLayer per glass pane**: maximum isolation but over-engineered for 3 panes. BackBufferCopy between siblings in the same layer handles overlap correctly.

**Rationale:** Godot's `SCREEN_TEXTURE` reads the back buffer. `BackBufferCopy` captures the current render state *at that sibling position* in the tree. Single CanvasLayer with BackBufferCopy between glass panes gives correct sampling. Multiple CanvasLayers would require more complex buffer management with no benefit.

---

### D2: Container Strategy (Anchors vs. Manual Positioning)

**Chosen:** Anchor-based layout with manual positioning for structural LCARS elements.

**Considered:**
- **Pure anchors + MarginContainer**: Godot's container system. Rejected because LCARS layout has irregular geometry (elbows, endcaps, asymmetric rails) that doesn't map to uniform margins. The elbow occupies a corner; the rail extends from the elbow; the content area fills the remainder. This is not a grid.
- **Pure manual positioning** (absolute coordinates): fragile, doesn't scale.
- **Hybrid** (chosen): Top-level panels use anchors for proportional screen placement (StackPanel anchored left, InspectorPanel anchored right, StatusStrip anchored bottom). *Within* each panel, LCARS structure (elbow + rail + endcap) is positioned manually relative to the panel's rect, using port attachment edges for alignment. Content areas use Godot containers (VBoxContainer, HBoxContainer) for standard flow layout.

**Rationale:** The macro layout (left/center/right/bottom) is a proportional problem — anchors solve it. The micro layout (elbow attaches to rail attaches to endcap) is a geometric attachment problem — port edges solve it. Content within glass panes is standard UI — containers solve it. Each problem uses the right tool.

---

### D3: Collapsible Sections — New Primitive vs. Composition

**Chosen:** Composition (collapsible_section.tscn) using existing primitives.

**Considered:**
- **New primitive** (`CollapsibleContainer extends PanelContainer`): adds to the primitive count, requires shader/stylebox, tests, registration. Rejected — collapsible behavior is a *layout pattern*, not a visual primitive.
- **Composition** (chosen): A scene containing a Chip (header toggle) + Bracket (decoration) + VBoxContainer (content). Chip.pressed → toggle content.visible. The VBoxContainer auto-collapses when content is hidden. No new visual rendering, no shader, no registration needed.

**Rationale:** The 7 primitives are the complete visual vocabulary. Collapsible sections are *arrangements* of primitives, not new shapes. Adding a primitive for every layout pattern defeats the composability principle.

---

### D4: Glass Pane Nesting — Nested vs. Sibling

**Chosen:** Sibling glass panes in a flat GlassLayer, not nested.

**Considered:**
- **Nested glass**: InspectorGlass as a child of a parent glass. Would create double-blur (parent blurs world, child blurs parent's blur). Visually muddy, performance cost doubled.
- **Sibling glass** (chosen): All glass panes are siblings under GlassLayer. Each samples the world (or the BackBufferCopy of the previous pane). Flat hierarchy, predictable sampling.
- **Per-section glass** (e.g., each inspector section gets its own GlassPane): too many glass panes = too many BackBufferCopy nodes = too many screen reads. Performance concern for 5+ overlapping blur shaders.

**Rationale:** Three glass panes (left, right, bottom) is the right granularity. Each panel gets one glass surface. Structure (rails, elbows) paints over it at z_index=1. Content flows inside the glass pane's `ContentMargin`. This matches the LCARS aesthetic: glass is a *background surface*, not a per-element treatment.

---

### D5: Connector Layer — Dedicated Control vs. Mixed into Panels

**Chosen:** Dedicated ConnectorLayer (z_index=2) with ConnectorManager.

**Considered:**
- **Connectors as children of panels**: each panel manages its own outgoing connectors. Rejected because connectors span *between* modules — a connector from StackPanel.E to InspectorPanel.W doesn't belong to either panel. Parent hierarchy would be arbitrary.
- **Connectors in a dedicated layer** (chosen): ConnectorManager owns all SplineConnector instances. It listens to port_positions_changed from all modules, re-routes when geometry changes. Connectors render at z_index=2 (above structure, below text).

**Rationale:** Connectors are *inter-module* elements. They need a global view of all ports to route. A dedicated manager with its own layer keeps routing logic centralized and avoids cross-panel dependencies.

---

### D6: InspectorRail Touch Target Compensation

**Chosen:** `custom_minimum_size.x = 3u` on a 2.5u visual rail.

**Considered:**
- **Making the rail 3u**: simple, but the design deliberately uses 2.5u to differentiate Inspector from Stack (3u). Forcing 3u visual thickness loses the hierarchy signal.
- **Invisible padding Control** behind the rail: extra node, adds complexity.
- **Expanding custom_minimum_size** (chosen): the rail's interactive area is 3u (72px) but the visual StyleBoxFlat only fills 2.5u. The extra 0.5u is transparent — the tap target extends but the visual rail stays thinner. This is standard practice for touch accessibility.

**Rationale:** WCAG-adjacent: interactive area ≥ visual area. The design maintains visual hierarchy (thinner inspector rail) while meeting touch minimums.

---

### D7: StatusStrip Rail Height Compensation

**Chosen:** Same approach as D6 — `custom_minimum_size.y = 3u` on a 2u visual rail.

The StatusStrip rail is 2u visually (thinnest rail — hierarchy: navigation > editing > status). The interactive zone extends 1u above the visual rail. Since content is *above* the rail anyway, this invisible padding merges with the content area's bottom margin. No visual artifact.

---

### D8: Bus Rendering — SplineConnector vs. LcarsRail

**Chosen:** SplineConnector with `set_bus_segment()` for bus backbone, not LcarsRail.

**Considered:**
- **LcarsRail in ConnectorLayer**: reuses the rail primitive for bus segments. Visually correct but semantically wrong — rails are structural UI, buses are schematic. Interaction models differ (rail segments are tappable navigation; bus segments are not directly interactive).
- **SplineConnector with bus mode** (chosen): `set_bus_segment(entry, exit)` renders a straight line with the connector shader (core stroke + rim). Same visual language as branch arcs. Hit detection works the same way.

**Rationale:** Buses are connectors with straight geometry. Using SplineConnector keeps all connector rendering in one system, one shader, one interaction model. The bus is just a degenerate spline (2 points, no curve).

---

## 2. Implementation Order

Five stacked branches, each independently testable. Each branch produces a scene that can be instanced and visually verified in isolation.

### Branch 1: `phase2/composed-panels` (foundation)

**Delivers:**
- `hud/composed/stack_panel.{gd,tscn}` — StackPanel with elbow + rail + endcap + chip bar (no content)
- `hud/composed/inspector_panel.{gd,tscn}` — InspectorPanel with elbow + rail + endcap (no sections)
- `hud/composed/status_strip.{gd,tscn}` — StatusStrip with rail + endcaps + mode chips

**Testable:** Each panel can be instanced standalone. Verify:
- Elbow attaches to rail (port alignment)
- Rail attaches to endcap (port alignment)
- Correct roles and colors per panel
- Rail segment tap/long-press still works
- Chip toggle still works
- Anti-Metro checklist passes (varied radii, endcap variety, asymmetric)
- Touch targets ≥ 3u on all interactive elements

**Dependencies:** Phase 1 primitives only.

---

### Branch 2: `phase2/collapsible-sections` (depends on Branch 1)

**Delivers:**
- `hud/composed/collapsible_section.{gd,tscn}` — reusable collapsible section
- `hud/composed/param_row.{gd,tscn}` — parameter row (label + value)
- InspectorPanel updated with 3 collapsible sections (Transform, Material, Render)
- StackPanel updated with list content area

**Testable:** Instance InspectorPanel. Verify:
- Section collapse/expand via chip toggle
- VBoxContainer relayouts correctly (no overlap, no gap)
- Param rows display label + value at correct sizes
- Bracket decorations appear beside section headers
- Mixed bracket styles (ANGLE for Transform/Material, TICK_GROUP for Render)

**Dependencies:** Branch 1.

---

### Branch 3: `phase2/glass-and-viewport` (depends on Branch 1)

**Delivers:**
- `hud/workbench/workbench_hud.{gd,tscn}` — top-level scene with CanvasLayer
- `hud/workbench/glass_layer.gd` — glass pane management + BackBufferCopy sync
- Three GlassPane instances wired to panels
- SubViewportContainer + SubViewport with placeholder 3D scene

**Testable:** Instance WorkbenchHUD. Verify:
- Glass panes show blurred world content behind them
- BackBufferCopy prevents disappearance artifacts where panels overlap
- Glass tint matches role (NAV=amber tint, EDIT=cyan tint, TELEMETRY=green tint)
- Structure renders above glass (z_index ordering)
- Text is crisp above glass (no blur applied to text layer)
- Blur can be toggled per-pane via `set_blur_enabled(false)`

**Dependencies:** Branch 1 (panel positioning for glass alignment).

---

### Branch 4: `phase2/connectors` (depends on Branch 1 + Branch 3)

**Delivers:**
- `hud/connectors/hud_port.gd` — port data class
- `hud/connectors/hud_net.gd` — net data class
- `hud/connectors/hud_bus.gd` — bus data class
- `hud/connectors/connector_router.gd` — routing engine
- `hud/workbench/connector_manager.gd` — runtime connector lifecycle

**Testable:** Instance WorkbenchHUD with connectors enabled. Verify:
- 5 nets render between correct ports
- Primary net (active_to_focus, importance=90) routes through center_bus with scanline
- Secondary nets render at medium thickness, no scanline
- Tertiary nets render as thin schematic traces
- Connector tap selects net (endpoint brackets appear)
- Resize triggers re-route (port positions recalculated, curves updated)
- Determinism: same window size → identical geometry on reload

**Dependencies:** Branches 1 + 3 (panels with ports + viewport for visual context).

---

### Branch 5: `phase2/interaction-wiring` (depends on all above)

**Delivers:**
- Signal wiring: rail segment → chip sync, chip → section collapse, selection → connector highlight
- Mode chip radio group behavior in StatusStrip
- Inspector lock toggle
- Telemetry readout placeholders (static values, update mechanism stubbed)
- Palette switch demo (runtime palette change cascades correctly)

**Testable:** Full WorkbenchHUD interaction. Verify:
- Tap stack rail segment → matching chip toggles, content switches
- Tap inspector section header → section collapses, connectors re-route
- Tap stack list item → connector highlight cascade
- Tap mode chip → radio group behavior, connector role updates
- Toggle inspector lock → selection changes no longer update inspector
- Switch palette → all elements update (glass tint, rail color, chip color, connector color, text color)

**Dependencies:** All previous branches.

---

### Branch Dependency Graph

```
Branch 1 (composed-panels)
    │
    ├── Branch 2 (collapsible-sections)
    │       │
    │       └──────────────────┐
    ├── Branch 3 (glass-and-viewport)
    │       │                  │
    │       └── Branch 4 (connectors)
    │               │          │
    └───────────────┴──────────┘
                    │
            Branch 5 (interaction-wiring)
```

Branches 2 and 3 can be developed **in parallel** after Branch 1. Branch 4 depends on 1 + 3. Branch 5 integrates everything.
