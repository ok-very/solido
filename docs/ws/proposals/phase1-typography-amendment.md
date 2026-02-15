# Phase 1 Typography Amendment

**Date:** 2026-02-14
**Author:** HUD Architect (Opus)
**Status:** Proposed -- amends `phase1-primitives.md` with typography integration
**Amends:** Phase 1 Architectural Blueprint (2026-02-14)

---

## Scope

The design brief (`HUD_ARCHITECT_BRIEF.md` Section 2) defines a three-tier typography system that the Phase 1 proposal does not address. This amendment specifies how typography tokens, font resources, and font application integrate into the existing Phase 0/1 architecture without rewriting the base proposal.

Changes touch: HudTokens (Phase 0), HudTheme (Phase 0), Chip primitive (Phase 1), GlassPane primitive (Phase 1 -- via ContentMargin consumers), and a new `hud/fonts/` resource directory.

---

## 1. Font Inventory (What Exists on Disk)

All three font families are present at `fonts/` in the repo root:

### Okuda (display)
| File | Weight |
|------|--------|
| `fonts/Okuda-A5PL.ttf` | Regular |
| `fonts/OkudaBold-qA72.ttf` | Bold |
| `fonts/OkudaItalic-JBma.ttf` | Italic |
| `fonts/OkudaBoldItalic-VDOz.ttf` | Bold Italic |

### Rajdhani (UI text)
| File | Weight |
|------|--------|
| `fonts/rajdhani-light.ttf` | Light (300) |
| `fonts/rajdhani-regular.ttf` | Regular (400) |
| `fonts/rajdhani-medium.ttf` | Medium (500) |
| `fonts/rajdhani-semibold.ttf` | SemiBold (600) |
| `fonts/rajdhani-bold.ttf` | Bold (700) |

### Oxanium (numeric/technical)
| File | Weight |
|------|--------|
| `fonts/Oxanium-VariableFont_wght.ttf` | Variable (weight axis) |
| `fonts/static/Oxanium-ExtraLight.ttf` | ExtraLight (200) |
| `fonts/static/Oxanium-Light.ttf` | Light (300) |
| `fonts/static/Oxanium-Regular.ttf` | Regular (400) |
| `fonts/static/Oxanium-Medium.ttf` | Medium (500) |
| `fonts/static/Oxanium-SemiBold.ttf` | SemiBold (600) |
| `fonts/static/Oxanium-Bold.ttf` | Bold (700) |
| `fonts/static/Oxanium-ExtraBold.ttf` | ExtraBold (800) |

**Note:** Okuda has non-standard filename suffixes (e.g., `-A5PL`, `-qA72`). These are font foundry artifacts -- functionally fine. Godot loads them as `FontFile` resources.

---

## 2. Font Loading Strategy

### Godot 4.6 Font Model (Context7-verified)

- **`FontFile`** is the Godot 4 replacement for DynamicFont. TTF files loaded via `load("res://fonts/foo.ttf")` return a `FontFile` resource. No `.tres` wrapper required for basic loading -- Godot imports `.ttf` files natively.
- **`FontVariation`** wraps a `FontFile` to add: extra glyph spacing (tracking), extra space spacing, embolden, OpenType variation axes (for variable fonts like Oxanium). This is how we get tracking adjustments without modifying the font file.
- **Theme font application:** `Control.add_theme_font_override("font", font_resource)` and `Control.add_theme_font_size_override("font_size", px)` apply per-node. A `Theme` resource can set fonts by type name: `theme.set_font("font", "Label", font_resource)` applies to all Labels under that theme.
- **Theme type variations:** A Theme can define a custom type (e.g., `"HudRailHeader"`) marked as a variation of `"Label"`. A Label with `theme_type_variation = "HudRailHeader"` uses that type's fonts/sizes first, falling back to base `Label` type.

### Decision: Static FontFile Preloads, FontVariation for Tracking

Godot's import system handles `.ttf` -> `FontFile` automatically. We do NOT need to create `.tres` wrappers for the base font files. We DO need `FontVariation` `.tres` resources for any font+tracking combination that deviates from default spacing.

**What gets created as `.tres` resources:**
- FontVariation resources for Okuda display sizes (with increased tracking)
- FontVariation resource for Oxanium numeric readouts (with tightened tracking)
- Rajdhani at default tracking needs no FontVariation -- the raw FontFile suffices

**What stays as raw `.ttf` references:**
- All base FontFile loads use `res://fonts/*.ttf` paths directly

---

## 3. Typography Tokens in HudTokens

### Additions to `hud_tokens.gd`

The existing `HudTokens` resource stores layout and interaction tokens. Typography tokens extend it with font references and the type scale. These are `@export` properties on the `HudTokens` Resource class.

```
# === TYPOGRAPHY: Font References ===
@export_group("Typography")
@export_subgroup("Fonts")
@export var font_display: Font              # Okuda (bold for headers)
@export var font_ui: Font                   # Rajdhani (medium/semibold for labels)
@export var font_ui_bold: Font              # Rajdhani Bold (buttons, emphasis)
@export var font_numeric: Font              # Oxanium (regular/medium for values)

# === TYPOGRAPHY: Type Scale (in u-units, converts to px via unit_px) ===
@export_subgroup("Type Scale")
@export var type_rail_header_u: float = 2.5       # 56-72px -> 2.5u default (60px)
@export var type_section_header_u: float = 1.67   # 36-44px -> 1.67u default (40px)
@export var type_label_u: float = 1.25            # 28-32px -> 1.25u default (30px)
@export var type_value_u: float = 1.5             # 32-40px -> 1.5u default (36px)
@export var type_micro_u: float = 1.0             # 24-28px -> 1.0u default (24px)

# === TYPOGRAPHY: Tracking (extra glyph spacing in px, applied via FontVariation) ===
@export_subgroup("Tracking")
@export var tracking_display_px: float = 2.0      # Slight increase for display labels
@export var tracking_ui_px: float = 0.0           # Default for UI text
@export var tracking_numeric_px: float = -0.5     # Tightened for numeric readouts
```

### Font Role -> Font Reference Mapping

| Brief Role | Token Property | Default Font File | Weight | Usage |
|------------|---------------|-------------------|--------|-------|
| Okuda display | `font_display` | `OkudaBold-qA72.ttf` | Bold | Rail headers, section headers, big mode labels, short identifiers |
| Rajdhani UI | `font_ui` | `rajdhani-semibold.ttf` | SemiBold | Labels, buttons, section titles, tooltips |
| Rajdhani UI bold | `font_ui_bold` | `rajdhani-bold.ttf` | Bold | Button emphasis, active states |
| Oxanium numeric | `font_numeric` | `Oxanium-Medium.ttf` (static) | Medium | Values, units, telemetry, timecodes |

**Why Okuda Bold, not Regular:** LCARS display text is characteristically thick and confident. Okuda Regular is too thin for rail headers at the design scale. Bold matches the reference aesthetic.

**Why Rajdhani SemiBold, not Regular:** At 28-32px on glass backgrounds, Regular Rajdhani loses legibility. SemiBold provides the weight needed for readability over translucent surfaces without being as heavy as Bold (reserved for emphasis).

**Why Oxanium static Medium, not Variable:** The variable font (`Oxanium-VariableFont_wght.ttf`) would require a `FontVariation` with `variation_opentype` to set the weight axis. The static `Oxanium-Medium.ttf` gives us the exact weight we need without that complexity. Variable font is available for future use if we need weight animation or intermediate weights.

### Type Scale -> Pixel Conversion

All type scale values are in u-units. Pixel sizes are computed at runtime:

```
font_size_px = type_*_u * HudTheme.unit_px
```

At native 3240x2160 (unit_px = 24):
- Rail header: 2.5u = 60px (within 56-72px range)
- Section header: 1.67u = 40px (within 36-44px range)
- Label: 1.25u = 30px (within 28-32px range)
- Value: 1.5u = 36px (within 32-40px range)
- Micro telemetry: 1.0u = 24px (within 24-28px range)

---

## 4. Font Variation Resources (New Files)

Three `FontVariation` `.tres` files are needed for tracking-adjusted fonts. These live in a new `hud/fonts/` directory, separate from the raw `.ttf` files in `fonts/`.

| # | File Path | Type | Purpose |
|---|-----------|------|---------|
| 1 | `hud/fonts/okuda_display.tres` | FontVariation | Okuda Bold + display tracking (+2px glyph spacing) |
| 2 | `hud/fonts/oxanium_numeric.tres` | FontVariation | Oxanium Medium + tightened tracking (-0.5px glyph spacing) |
| 3 | `hud/fonts/rajdhani_ui.tres` | FontVariation | Rajdhani SemiBold + default tracking (0px) -- acts as a stable indirection layer even though no spacing adjustment is needed now |

### FontVariation Construction (pseudocode)

```
# okuda_display.tres
base_font = load("res://fonts/OkudaBold-qA72.ttf")
spacing_glyph = 2    # Extra spacing between every glyph (tracking)
spacing_space = 0
spacing_top = 0
spacing_bottom = 0

# oxanium_numeric.tres
base_font = load("res://fonts/static/Oxanium-Medium.ttf")
spacing_glyph = -0.5  # Tightened glyph spacing for numeric columns
spacing_space = 0
spacing_top = 0
spacing_bottom = 0

# rajdhani_ui.tres
base_font = load("res://fonts/rajdhani-semibold.ttf")
spacing_glyph = 0
spacing_space = 0
spacing_top = 0
spacing_bottom = 0
```

### Why a `hud/fonts/` Directory

Raw font files in `fonts/` are project-wide assets (could be used by non-HUD systems). HUD-specific FontVariation resources with tuned tracking belong under the `hud/` tree alongside tokens, themes, and shaders. This mirrors the established pattern: raw assets at project root, HUD-configured resources under `hud/`.

### Updated hud_tokens.tres Default Values

The `hud_tokens.tres` resource file sets these font properties:

```
font_display = preload("res://hud/fonts/okuda_display.tres")
font_ui = preload("res://hud/fonts/rajdhani_ui.tres")
font_ui_bold = preload("res://fonts/rajdhani-bold.ttf")
font_numeric = preload("res://hud/fonts/oxanium_numeric.tres")
```

Note: `font_ui_bold` uses the raw TTF because bold Rajdhani needs no tracking adjustment. All four export properties are typed as `Font` (the Godot base class that both `FontFile` and `FontVariation` extend), so either type can be assigned.

---

## 5. HudTheme Typography Responsibilities

### Additions to `hud_theme.gd`

HudTheme is the autoload singleton that generates a Godot `Theme` resource from `HudTokens` + `HudPalette`. Typography adds these responsibilities:

**5a. Theme Type Variations**

HudTheme generates Theme type variations so that Controls can opt into typography roles via `theme_type_variation`. This is the Godot-native way to skin subsets of the same Control class differently.

| Type Variation | Base Type | Font (from tokens) | Size (from tokens) | Usage |
|---------------|-----------|--------------------|--------------------|-------|
| `HudRailHeader` | `Label` | `font_display` | `type_rail_header_u * unit_px` | Rail segment labels, big identifiers |
| `HudSectionHeader` | `Label` | `font_display` | `type_section_header_u * unit_px` | Section titles within modules |
| `HudLabel` | `Label` | `font_ui` | `type_label_u * unit_px` | General UI labels, tooltips |
| `HudButton` | `Button` | `font_ui` | `type_label_u * unit_px` | Button text |
| `HudButtonBold` | `Button` | `font_ui_bold` | `type_label_u * unit_px` | Emphasized buttons |
| `HudValue` | `Label` | `font_numeric` | `type_value_u * unit_px` | Numeric readouts, parameter values |
| `HudMicro` | `Label` | `font_numeric` | `type_micro_u * unit_px` | Micro telemetry, timecodes, small readouts |
| `HudChipLabel` | `Label` | `font_ui` | `type_label_u * unit_px` | Chip text (uppercase via script, not font) |

**5b. Theme Generation Pseudocode**

```
func _rebuild_theme() -> void:
    # ... existing color/StyleBox generation ...

    # Typography: set base Label/Button fonts from tokens
    var tokens := _active_tokens as HudTokens
    var unit := tokens.unit_px  # or computed from viewport

    # Base Label type
    _theme.set_font("font", "Label", tokens.font_ui)
    _theme.set_font_size("font_size", "Label", int(tokens.type_label_u * unit))

    # Base Button type
    _theme.set_font("font", "Button", tokens.font_ui)
    _theme.set_font_size("font_size", "Button", int(tokens.type_label_u * unit))

    # Type variations
    _theme.add_type("HudRailHeader")
    _theme.set_type_variation("HudRailHeader", "Label")
    _theme.set_font("font", "HudRailHeader", tokens.font_display)
    _theme.set_font_size("font_size", "HudRailHeader", int(tokens.type_rail_header_u * unit))

    _theme.add_type("HudSectionHeader")
    _theme.set_type_variation("HudSectionHeader", "Label")
    _theme.set_font("font", "HudSectionHeader", tokens.font_display)
    _theme.set_font_size("font_size", "HudSectionHeader", int(tokens.type_section_header_u * unit))

    _theme.add_type("HudValue")
    _theme.set_type_variation("HudValue", "Label")
    _theme.set_font("font", "HudValue", tokens.font_numeric)
    _theme.set_font_size("font_size", "HudValue", int(tokens.type_value_u * unit))

    _theme.add_type("HudMicro")
    _theme.set_type_variation("HudMicro", "Label")
    _theme.set_font("font", "HudMicro", tokens.font_numeric)
    _theme.set_font_size("font_size", "HudMicro", int(tokens.type_micro_u * unit))

    _theme.add_type("HudChipLabel")
    _theme.set_type_variation("HudChipLabel", "Label")
    _theme.set_font("font", "HudChipLabel", tokens.font_ui)
    _theme.set_font_size("font_size", "HudChipLabel", int(tokens.type_label_u * unit))

    _theme.add_type("HudButtonBold")
    _theme.set_type_variation("HudButtonBold", "Button")
    _theme.set_font("font", "HudButtonBold", tokens.font_ui_bold)
    _theme.set_font_size("font_size", "HudButtonBold", int(tokens.type_label_u * unit))

    # Font colors: derived from palette, not hardcoded
    # text_on_solid and text_on_glass shades from HudPalette
    _theme.set_color("font_color", "Label", palette.get_text_on_glass(HudRole.NEUTRAL))
    _theme.set_color("font_color", "HudRailHeader", palette.get_text_on_solid(HudRole.NAV))
    # ... per-variation color bindings as needed ...
```

**5c. Theme Propagation**

HudTheme assigns the generated `Theme` to the HUD's root CanvasLayer (or a root Control). All child Controls inherit it via Godot's theme propagation chain. Individual Controls opt into typography roles by setting `theme_type_variation`:

```
# In a scene or script:
rail_label.theme_type_variation = "HudRailHeader"
value_readout.theme_type_variation = "HudValue"
telemetry_label.theme_type_variation = "HudMicro"
```

No `add_theme_font_override()` calls needed on individual nodes unless overriding the type variation system (escape hatch, not the norm).

**5d. Font Color Integration with Palette**

The brief says: "near-black on bright blocks, off-white on dark glass." This maps to the existing `HudPalette` shades:

- `text_on_solid` -- for labels on opaque rails, endcaps, chips (near-black or dark color)
- `text_on_glass` -- for labels on translucent glass panes (off-white or light color)

HudTheme sets `font_color` per type variation based on typical placement:
- `HudRailHeader`: `text_on_solid` (sits on bright rail)
- `HudSectionHeader`: `text_on_glass` (sits on glass pane header area)
- `HudLabel`: `text_on_glass` (default assumes glass context)
- `HudValue` / `HudMicro`: `text_on_glass` (readouts on glass)
- `HudChipLabel`: `text_on_solid` (chip body is opaque)

Scripts can override `font_color` per-node when a label appears in an atypical context (e.g., a value readout on a solid rail).

---

## 6. Primitive-Level Typography Changes

### 6a. Chip (Phase 1 -- Amended)

The Chip's `ChipLabel` (Label node) currently has no font specification in the Phase 1 proposal. This amendment adds:

**Scene tree change:** None. ChipLabel is already a Label child of Chip.

**Script change to `chip.gd`:**

```
# In _ready():
_chip_label.theme_type_variation = "HudChipLabel"
_chip_label.uppercase = true  # LCARS convention: uppercase chip text
```

The `HudChipLabel` type variation gives it `font_ui` at `type_label_u` size. No `add_theme_font_override` needed -- the Theme propagation handles it.

**Font color:** Set via the type variation's `font_color` in the Theme (bound to `text_on_solid` since chips are opaque). On role change, HudTheme updates the Theme's color for the chip's role.

Alternatively, since chips can have different roles (each with a different `text_on_solid` color), the script sets font color explicitly after role change:

```
# In set_role():
var text_color = HudTheme.get_text_on_solid_color(role)
_chip_label.add_theme_color_override("font_color", text_color)
```

This per-instance color override sits on top of the type variation's font/size (which remain Theme-driven).

### 6b. LcarsRail (Phase 1 -- No Change, Future Note)

Rails in Phase 1 have no text. Rail headers (labels embedded in rail segments or adjacent to them) are a Phase 2 composition concern. When they arrive, they will be Labels with `theme_type_variation = "HudRailHeader"`.

**No change to Phase 1 proposal.**

### 6c. GlassPane (Phase 1 -- No Change, Future Note)

GlassPane's ContentMargin holds composed content (Phase 2+). Text Controls placed inside it inherit the HudTheme via propagation. No font logic in the GlassPane primitive itself.

**No change to Phase 1 proposal.**

### 6d. Bracket (Phase 1 -- No Change)

Brackets are non-textual. No typography impact.

### 6e. LcarsEndcap / LcarsElbow / SplineConnector (Phase 1 -- No Change)

No text content. No typography impact.

---

## 7. Updated File Manifest (Delta Only)

New files added by this amendment:

| # | File Path | Type | Purpose |
|---|-----------|------|---------|
| 1 | `hud/fonts/okuda_display.tres` | FontVariation resource | Okuda Bold + display tracking (+2px glyph spacing) |
| 2 | `hud/fonts/rajdhani_ui.tres` | FontVariation resource | Rajdhani SemiBold + default tracking (indirection layer) |
| 3 | `hud/fonts/oxanium_numeric.tres` | FontVariation resource | Oxanium Medium + tightened tracking (-0.5px glyph spacing) |

**Modified files (Phase 0, spec changes):**

| File Path | Change |
|-----------|--------|
| `hud/tokens/hud_tokens.gd` | Add `font_display`, `font_ui`, `font_ui_bold`, `font_numeric`, `type_*_u` (5 scale values), `tracking_*_px` (3 tracking values) exports |
| `hud/tokens/hud_tokens.tres` | Set font property defaults to preloaded FontVariation/FontFile resources |
| `hud/theme/hud_theme.gd` | Add `_rebuild_theme()` typography section: type variations, font/size/color bindings |

**Modified files (Phase 1, spec changes):**

| File Path | Change |
|-----------|--------|
| `hud/primitives/chip.gd` | Set `ChipLabel.theme_type_variation = "HudChipLabel"`, `uppercase = true`, font color override on role change |

**Total delta: 3 new files, 4 modified files.**

---

## 8. Updated Directory Tree

```
hud/
|-- fonts/                                 # NEW (this amendment)
|   |-- okuda_display.tres                 # FontVariation: Okuda Bold + tracking
|   |-- rajdhani_ui.tres                   # FontVariation: Rajdhani SemiBold
|   |-- oxanium_numeric.tres               # FontVariation: Oxanium Medium + tracking
|-- tokens/                                # Phase 0 (amended)
|   |-- hud_tokens.gd                      # +typography exports
|   |-- hud_tokens.tres                    # +font defaults
|   |-- hud_palette.gd
|   |-- hud_role.gd
|   |-- palette_*.tres (x6)
|-- theme/                                 # Phase 0 (amended)
|   |-- hud_theme.gd                       # +type variations, font/size/color
|-- shaders/                               # Phase 1 (unchanged)
|   |-- ...
|-- primitives/                            # Phase 1 (chip.gd amended)
    |-- ...
```

---

## 9. Updated Build Order (Delta)

The original Phase 1 build order is:

```
Step 0: Phase 0 stubs
Step 1: LcarsRail | Chip | Bracket (parallel, no shader deps)
Step 2: hud_sdf.gdshaderinc | hud_noise.gdshaderinc
Step 3: LcarsEndcap | LcarsElbow
Step 4: GlassPane
Step 5: SplineConnector
```

Typography integration inserts into Step 0 and modifies Step 1:

### Step 0 Additions (Phase 0 Stubs)

The Phase 0 stub requirements grow to include:

**Previously required:**
- `hud/tokens/hud_role.gd`
- `hud/tokens/hud_tokens.gd` + `.tres`
- `hud/tokens/hud_palette.gd` + `palette_ops_amber.tres`
- `hud/theme/hud_theme.gd`

**Now also required:**
- `hud/fonts/okuda_display.tres` -- FontVariation resource (can be created immediately; no code dependency)
- `hud/fonts/rajdhani_ui.tres` -- FontVariation resource
- `hud/fonts/oxanium_numeric.tres` -- FontVariation resource
- `hud/tokens/hud_tokens.gd` must include the typography `@export` properties (font refs + type scale + tracking)
- `hud/tokens/hud_tokens.tres` must set font property defaults
- `hud/theme/hud_theme.gd` must include the type variation generation in `_rebuild_theme()`

The FontVariation `.tres` files have zero code dependencies -- they are pure resource files referencing raw TTFs. They can be created at any time, even before any GDScript exists.

### Step 1 Modification

Chip's `_ready()` now includes `theme_type_variation` assignment and `uppercase = true`. This is a two-line addition that depends on the Theme type variation existing at runtime. If Phase 0 stubs include the type variation setup, this works. If Phase 0 stubs are truly minimal (no Theme generation), the Chip still functions -- it just uses Godot's default font until the Theme is wired up. **No blocking dependency change.**

### Dependency Graph Additions

```
fonts/*.ttf (raw assets, already on disk)
    |
    v
hud/fonts/*.tres (FontVariation resources -- Step 0, no code deps)
    |
    v
hud/tokens/hud_tokens.gd (typography exports -- Step 0)
    |
    v
hud/tokens/hud_tokens.tres (preloads FontVariation .tres -- Step 0)
    |
    v
hud/theme/hud_theme.gd (reads tokens, generates Theme with type variations -- Step 0)
    |
    v
chip.gd sets theme_type_variation = "HudChipLabel" (Step 1, soft dependency)
```

**No changes to Steps 2-5.** Typography does not affect shader files, shader-based primitives, or SplineConnector.

---

## 10. UiThemeBinder Interface Compliance

The design brief (Section 6) requires a `UiThemeBinder` interface that "applies fonts + Theme defaults + shader presets." In our architecture, HudTheme IS the UiThemeBinder. This amendment confirms that HudTheme's `_rebuild_theme()` method fulfills the UiThemeBinder contract for typography:

| UiThemeBinder Responsibility | HudTheme Implementation |
|-----------------------------|------------------------|
| Apply fonts to Control tree | `Theme.set_font()` per type variation, propagated via Godot theme chain |
| Apply Theme defaults | `Theme.set_font_size()`, `Theme.set_color("font_color")` per type variation |
| Apply shader presets | Existing push model (unchanged by this amendment) |

No separate `UiThemeBinder` class is needed. HudTheme consolidates all three responsibilities.

---

## 11. Uppercase Transform Convention

The brief says: "Prefer uppercase for LCARS labels (or a consistent transform step)."

**Decision:** Uppercase is applied per-Control via `Label.uppercase = true`, NOT via text content or font shaping.

**Rationale:**
- Godot's `Label.uppercase` property transforms display text to uppercase without modifying the underlying `text` value. This is reversible and non-destructive.
- Applying uppercase at the data level (storing "BLEND" instead of "Blend") couples display convention to data. If the convention changes, all data must change.
- Applying at the font level (OpenType `smcp` or `case` features) depends on font support. Okuda is a fan font with uncertain OpenType feature tables. Rajdhani and Oxanium may support `case` but we cannot guarantee it. `Label.uppercase` works regardless.

**Where uppercase applies:**
- Chip labels: `uppercase = true` (set in `chip.gd _ready()`)
- Rail headers: `uppercase = true` (set by composition scripts in Phase 2)
- Section headers: case-as-authored (mixed case for readability in longer titles)
- Values/micro: case-as-authored (units like "ms", "fps", "px" should not be uppercased)

---

## 12. Multi-Resolution Typography Behavior

The brief requires type scale consistency across resolutions (Section 8, acceptance tests: "Type scale remains consistent").

**Mechanism:** All type scale values are in u-units. `unit_px` is the single scale factor derived from viewport dimensions:

```
unit_px = viewport_width / (3240.0 / 24.0)  # = viewport_width / 135.0
```

At different resolutions:
| Resolution | unit_px | Rail Header (2.5u) | Label (1.25u) | Micro (1.0u) |
|-----------|---------|-------------------|---------------|-------------|
| 3240x2160 (native) | 24.0 | 60px | 30px | 24px |
| 1920x1080 | 14.2 | 35px | 18px | 14px |
| 2560x1440 | 19.0 | 47px | 24px | 19px |
| 3840x2160 (4K) | 28.4 | 71px | 36px | 28px |
| 1280x720 | 9.5 | 24px | 12px | 9px |

**Concern at 1280x720:** Micro telemetry at 9px is below legibility threshold. This is a Phase 3 responsiveness issue. Options: clamp minimum font size to 12px, or hide micro-telemetry at small viewports.

**Font rendering at varying sizes:** `FontFile` uses FreeType for rasterization. Godot 4 applies subpixel antialiasing and hinting by default. At 260 DPI (native), text is crisp. At lower DPI (1920x1080 on a 13.5" screen), hinting becomes more important. The FontVariation resources do not override hinting -- Godot's defaults are appropriate.

---

## 13. Decision Log

### Decision T1: FontVariation Resources vs Per-Node Overrides

**Chosen:** FontVariation `.tres` resources for tracking-adjusted fonts, referenced by HudTokens, applied via Theme type variations.

**Rejected alternative:** `add_theme_font_override()` on every Label/Button node individually.

**Rationale:** Per-node overrides create maintenance burden. Every new Label added to a composition scene would need font override calls in script. Theme type variations are declarative: set `theme_type_variation` on a node once, and the Theme handles font, size, and color. When tokens change (different tracking, different font weight), the Theme regenerates and all nodes update automatically.

### Decision T2: Separate `hud/fonts/` Directory vs Inline in `hud/tokens/`

**Chosen:** `hud/fonts/` as a peer directory to `hud/tokens/`, `hud/theme/`, etc.

**Rejected alternative:** FontVariation `.tres` files alongside palette `.tres` files in `hud/tokens/`.

**Rationale:** `hud/tokens/` contains the token Resource classes and their instances (`.gd` + `.tres`). Font resources are a different concern -- they are pre-configured assets referenced BY tokens, not tokens themselves. Mixing them muddies the directory's purpose. `hud/fonts/` is explicit about what it contains.

### Decision T3: Static Oxanium vs Variable Font

**Chosen:** Static `Oxanium-Medium.ttf` from `fonts/static/`.

**Rejected alternative:** Variable `Oxanium-VariableFont_wght.ttf` with `variation_opentype` weight axis set via FontVariation.

**Rationale:** We need exactly one weight (Medium) for numeric readouts. The variable font adds complexity (FontVariation must set the `wght` axis via OpenType tag) with no benefit unless we later need weight interpolation or animation. The static file is simpler, and the variable font remains available if requirements change.

### Decision T4: Rajdhani SemiBold vs Regular for UI Text

**Chosen:** SemiBold (600) as the default UI weight.

**Rejected alternative:** Regular (400).

**Rationale:** UI labels appear on glass panes (translucent backgrounds with blur and grain). Regular weight at 28-32px over these busy backgrounds loses contrast and becomes harder to read, especially at non-native resolutions. SemiBold maintains legibility without the heaviness of Bold. Bold (700) is reserved for emphasis (buttons, active states) to maintain a clear weight hierarchy: SemiBold (default) < Bold (emphasis) < Okuda Bold (display).

### Decision T5: Theme Type Variations vs Custom Theme Types

**Chosen:** Godot's built-in `theme_type_variation` system.

**Rejected alternatives:**
- Custom `class_name` per label type (e.g., `class_name HudRailHeaderLabel extends Label`): Creates class proliferation for what is purely a styling concern.
- String-based manual font lookup (e.g., `HudTheme.get_font("rail_header")`): Bypasses Godot's theme system, loses editor preview, requires explicit calls everywhere.

**Rationale:** Type variations are the Godot-native mechanism for exactly this use case. A Label with `theme_type_variation = "HudRailHeader"` uses the HudRailHeader theme entry first, falls back to base Label, falls back to default. This works in the editor (font previews in scene view), works with Godot's theme propagation (no manual wiring), and is documented Godot API.

### Decision T6: Uppercase via Label.uppercase vs Data-Level Transform

**Chosen:** `Label.uppercase = true` property on Controls.

**Rejected alternatives:**
- Storing text as uppercase in data/schema
- OpenType `case` or `smcp` features

**Rationale:** Documented in Section 11 above. `Label.uppercase` is non-destructive, reversible, font-independent, and Godot-native.

---

## 14. Open Questions for Phase 2

These are NOT blocking for Phase 1. Flagged for future resolution:

1. **Font color per role per context:** The current proposal uses `text_on_solid` and `text_on_glass` from HudPalette. If a Label needs to be on a role-colored rail (bright amber), its `text_on_solid` should be near-black. If the same Label type appears on a neutral glass pane, it needs off-white. Should this be handled by additional type variations per role (e.g., `HudLabelOnNav`, `HudLabelOnNeutral`) or by script-level color overrides? Phase 2 composition will reveal the real patterns.

2. **Minimum font size clamp:** At 1280x720, micro telemetry hits 9px. Should HudTheme clamp all font sizes to a minimum (e.g., 12px)? Or should micro-telemetry Controls hide themselves below a viewport size threshold? Phase 3 responsiveness testing will determine this.

3. **Font loading performance:** Three FontVariation `.tres` resources each reference one `.ttf` file. Godot deduplicates FontFile instances loaded from the same path, so the three Oxanium/Okuda/Rajdhani TTFs are each loaded once regardless of how many FontVariation resources reference them. No preloading concern at this scale. If the font count grows significantly (additional weights for different contexts), audit memory usage.

---

End of Phase 1 Typography Amendment.
