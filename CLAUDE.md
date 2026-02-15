# Solido Tri-D

Schema-driven procedural generation platform. Godot 4.6, GLSL-like shaders, headless + editor workflows.

---

## Stack

- **Engine:** Godot 4.6 (Forward+)
- **Languages:** GDScript, Godot Shader Language (GLSL-like)
- **Headless binary:** `/home/silen/.local/bin/godot`
- **Testing:** GUT 9.6.0 (Godot Unit Test, `godot_4_6` branch)
- **Formatter:** gdscript-formatter 0.18.2 (`~/.local/bin/gdscript-formatter`, `--safe` always)
- **MCP:** godot-mcp (run/debug/scene ops), Context7 (Godot docs on demand), Serena (memories + pattern search)
- **Reference:** `.claude/skills/godot-patterns.md` — scene composition, signals, state machines, lifecycle

## Structure (what actually exists)

```
addons/procedural_tools/     # Editor plugin
  plugin.gd                  # EditorPlugin entry, dock lifecycle
  plugin.cfg                 # Plugin metadata
  tool_dock.gd               # Editor dock UI (module select, params, preview)
  tool_dock.tscn             # Dock scene
  schema_parser.gd           # TOML -> Dictionary
  ui_builder.gd              # Dictionary -> Control nodes
  preview_viewport.gd        # SubViewport camera manager

modules/                     # Procedural generator modules
  terrain/
    schema.toml              # Parameter definitions
    generator.gd             # TerrainGenerator (FastNoiseLite, ArrayMesh)
    preview.tscn             # 3D preview scene
    preview_controller.gd    # Applies mesh data to MeshInstance3D

addons/gut/                  # GUT testing framework (9.6.0, godot_4_6 branch)

test/unit/                   # Unit tests (GUT)
  test_schema_parser.gd      # SchemaParser: TOML parsing, validation, edge cases
  test_terrain_generator.gd  # TerrainGenerator: mesh gen, params, determinism
  test_ui_builder.gd         # UIBuilder: control creation, values, round-trips

scenes/main.tscn             # Standalone runtime UI
scripts/main.gd              # Runtime app (no editor required)
tools/watch.py               # File watcher (watchdog)
tools/regenerate_module.gd   # Headless regeneration script
library/terrain/             # Generated outputs (gitignored)
```

## Pipeline

```
schema.toml -> SchemaParser -> Dictionary -> UIBuilder -> Controls
                                                           |
                                                 Generator.generate()
                                                           |
                                              PreviewController.update_preview()
```

## Commands

```bash
# Verify Godot
/home/silen/.local/bin/godot --version --headless

# Run all tests (headless, exit on complete)
/home/silen/.local/bin/godot --headless --path /home/silen/dev/solido -s addons/gut/gut_cmdln.gd -gexit

# Run a single test file
/home/silen/.local/bin/godot --headless --path /home/silen/dev/solido -s addons/gut/gut_cmdln.gd -gexit -gtest=res://test/unit/test_schema_parser.gd

# Run project headlessly
/home/silen/.local/bin/godot --headless --path . --script tools/regenerate_module.gd -- modules/terrain

# Import after adding new GDScript class_names (required before first GUT run)
/home/silen/.local/bin/godot --headless --path /home/silen/dev/solido --import

# Watch for schema changes
pip install watchdog && python tools/watch.py

# Format check (exit 1 if unformatted)
gdscript-formatter --check --safe addons/procedural_tools/*.gd modules/terrain/*.gd scripts/*.gd test/unit/*.gd

# Format all GDScript files
gdscript-formatter --safe addons/procedural_tools/*.gd modules/terrain/*.gd scripts/*.gd test/unit/*.gd
```

## Known issues

- Significant code duplication between `tool_dock.gd` (editor) and `main.gd` (standalone) — same module scanning, param building, preview generation.
- `shaders/` directory structure referenced in README but empty — no actual shader files exist yet.
- Windows Godot 4.6 at `/mnt/c/Users/nealm/dev/deps` (for visual editing).

---

## Agents

| Agent | Model | When | Tools |
|-------|-------|------|-------|
| hud-architect | opus | HUD design decisions, scene structure, composition planning, connector topology | Context7, Serena, godot-mcp |
| godot-dev | sonnet | GDScript, scenes, modules, plugin code | Context7, godot-mcp, Serena |
| shader-writer | sonnet | .gdshader / .gdshaderinc files only | Context7 |
| godot-tester | haiku | Run tests, capture errors, validate | godot-mcp, GUT |
| Explore | sonnet | Codebase research | Serena MCP tools |

### Reference docs (agents load on-demand)

| Doc | Agent | Content |
|-----|-------|---------|
| `.claude/skills/hud-architect.md` | hud-architect | Design system digest, primitives, tokens, decision framework |
| `.claude/skills/godot-patterns.md` | godot-dev | Scene composition, signals, state machines, lifecycle |
| `docs/ws/ui/HUD_ARCHITECT_BRIEF.md` | hud-architect | Visual system spec, layout, shader spec, build phases |
| `docs/ws/ui/CONNECTORS.md` | hud-architect | Connector system: ports, nets, buses, routing, splines |
| `docs/ws/ui/ROUTER_PSEUDOCODE.md` | hud-architect | Routing algorithm, collision, detours, rendering |

### Agent dispatch instructions

**hud-architect** gets this prefix:

> You are the HUD architect for Solido Tri-D, a Godot 4.6 procedural generation platform with an LCARS-inspired glass HUD. Read `.claude/skills/hud-architect.md` first — it contains established architecture (settled decisions), design system digest, primitives, tokens, connector system, and your decision framework. For deep details, read the design docs in `docs/ws/ui/`. Use Context7 (Godot library: `/godotengine/godot-docs`) to resolve Godot 4.6 API questions — never guess. Use Serena to check what exists in the codebase before proposing new structures. You produce architectural blueprints (scene trees, file manifests, shader contracts, connector topology, decision logs) and ALWAYS write them to `docs/ws/proposals/<phase-or-topic>.md` — you do NOT write implementation code. Read `CLAUDE.md` for project context.

**godot-dev** gets this prefix:

> You are building a Godot 4.6 procedural generation platform. Read `.claude/skills/godot-patterns.md` first for scene composition, signal, and lifecycle patterns. Use Context7 (`/websites/godotengine_en_stable`) for API lookups — always query before guessing. After writing GDScript, use godot-mcp `run_project` with projectPath `/home/silen/dev/solido` to test, then `get_debug_output` to check for errors. Fix errors before reporting done. Use `mcp__serena__search_for_pattern` for code navigation. Read `CLAUDE.md` in the project root for architecture context.

**shader-writer** gets this prefix:

> You write Godot Shader Language files (.gdshader, .gdshaderinc) for the Solido Tri-D project. Use Context7 (`/websites/godotengine_en_stable`) to look up shader built-ins, uniforms, and functions. Follow these conventions: `snake_case` for uniforms/functions/variables, `SCREAMING_SNAKE_CASE` for constants. Always declare `shader_type` first. Group code: uniforms, constants, utility functions, vertex(), fragment(). Include a header comment with component name, purpose, and dependencies.

**godot-tester** gets this prefix:

> You validate the Solido Tri-D Godot project. Primary tool: run GUT tests headless with `/home/silen/.local/bin/godot --headless --path /home/silen/dev/solido -s addons/gut/gut_cmdln.gd -gexit`. Check exit code (0 = pass, 1 = fail). Parse output for `[Failed]` lines and report structured results: tests passed, tests failed, specific assertion messages. After tests pass, run format check: `gdscript-formatter --check --safe addons/procedural_tools/*.gd modules/terrain/*.gd scripts/*.gd test/unit/*.gd` (exit 1 = unformatted files). Secondary: use godot-mcp `run_project` with projectPath `/home/silen/dev/solido` + `get_debug_output` for runtime validation.

---

## Patterns

### New Module Checklist

1. Create `modules/<name>/schema.toml` — parameter definitions
2. Create `modules/<name>/generator.gd` — extends Resource, implements `generate() -> Dictionary`
3. Create `modules/<name>/preview.tscn` — scene with MeshInstance3D + preview_controller
4. Create `modules/<name>/preview_controller.gd` — implements `update_preview(data: Dictionary)`
5. Create `test/unit/test_<name>_generator.gd` — extends GutTest, covers output structure + params
6. Test: run GUT headless, verify all green, then run project visually

### Shader File Template

```glsl
shader_type spatial;

/*
 * [Component Name]
 * Purpose: Brief description
 * Dependencies: List dependent shaders
 */

// === UNIFORMS ===
uniform vec4 base_color : source_color = vec4(1.0);
uniform float roughness : hint_range(0.0, 1.0) = 0.5;

// === CONSTANTS ===
const float EPSILON = 0.001;

// === VERTEX ===
void vertex() {
}

// === FRAGMENT ===
void fragment() {
    ALBEDO = base_color.rgb;
    ROUGHNESS = roughness;
}
```

### Schema TOML Format

```toml
[module]
name = "Module Name"
version = "1.0.0"
description = "What it does"

[parameters.param_name]
type = "float"          # int, float, bool, string, enum, color, vector2, vector3
default = 0.5
min = 0.0
max = 1.0
step = 0.01
description = "What this controls"

# For enum type:
[parameters.mode]
type = "enum"
default = "option_a"
options = ["option_a", "option_b", "option_c"]

[output]
type = "Resource"
format = "tres"
base_path = "res://library/<module>/"
```

---

## Agent Operating Rules

### Verify before reporting done
After writing GDScript, run GUT tests headless. Parse output for `[Failed]`. Don't report success without a clean run.

### Use Context7 before guessing API
Query `/websites/godotengine_en_stable` for any Godot API you're not 100% sure about. Especially: signal signatures, Node lifecycle methods, Control property names.

### Scene tree awareness
- Controls must be added to tree before accessing children
- Use `add_child_autofree()` in tests for automatic cleanup
- `queue_free()` is deferred — don't access freed nodes in same frame

### Plugin lifecycle
- `_enter_tree()` for init, `_exit_tree()` for teardown
- Always disconnect signals in `_exit_tree()` — use tracked signal pattern (see `tool_dock.gd`)
- Never orphan SubViewport/Camera3D nodes

### Signal connection
- Connect to the actual input control (SpinBox, Slider, LineEdit), never to the container (VBoxContainer)
- UIBuilder returns containers for most types — drill into children to find the actual widget

### GDScript 4.6 gotchas
- Builtin types (Color, Vector2, etc.) can't be used as class refs in `is` checks — use `typeof()` + `TYPE_*` constants
- `push_error()` in GUT requires matching `assert_push_error("expected text")` or test fails
- String `.capitalize()` converts `snake_case` to `Title Case` (useful for labels)

---

## Principles

1. **Composability** — small reusable components, not monoliths
2. **Explicit** — visible dependencies, predictable params, no magic
3. **Incremental stability** — every commit should run
4. **Verify** — GUT tests first, godot-mcp for runtime, don't assume code works

## Git

Use **stackit** for all branch/PR ops. Commit working states. One logical change per commit.

---

**Last Updated**: 2026-02-14
