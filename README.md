# Solido Tri-D

**A stable, scalable graphics construction apparatus for GLSL-like shader development**

Solido Tri-D is a greenfield shader development platform built on Godot 4, designed for procedural graphics creation with a focus on experimental control, extension, and thoughtful architecture.

## 🎯 Project Overview

This project provides:
- **Schema-driven procedural generation** via editor plugin
- **GLSL-like shader development** using Godot Shader Language
- **Live preview** of procedural outputs
- **Headless workflows** for CI/CD integration
- **Extensible module system** for custom generators

## 🏗️ Project Structure

```
project/
├── addons/
│   └── procedural_tools/      # Editor plugin for procedural generation
│       ├── plugin.cfg
│       ├── plugin.gd          # EditorPlugin entry point
│       ├── tool_dock.gd       # Main dock UI
│       ├── tool_dock.tscn     # Dock scene
│       ├── schema_parser.gd   # TOML → Dictionary parser
│       ├── ui_builder.gd      # Dictionary → Control nodes
│       └── preview_viewport.gd # SubViewport manager
│
├── modules/                   # Procedural generator modules
│   └── terrain/
│       ├── generator.gd       # Terrain generation logic
│       ├── schema.toml        # Parameter definitions
│       ├── preview.tscn       # 3D preview scene
│       └── preview_controller.gd
│
├── library/                   # Generated outputs (gitignored)
│   └── terrain/
│       └── *.tres            # Saved resources
│
├── shaders/                   # GLSL-like shader implementations
│   ├── base/
│   ├── materials/
│   ├── effects/
│   └── utils/
│
├── tools/                     # Development utilities
│   ├── watch.py              # File watcher for non-editor workflows
│   └── regenerate_module.gd  # Headless regeneration script
│
└── CLAUDE.md                  # AI agent navigation guide
```

## 🚀 Getting Started

### Prerequisites

- **Godot 4.x** (headless capable)
- **Python 3.8+** (for watch script)
- **Git**

### Installation

1. **Clone the repository**
   ```bash
   git clone https://github.com/ok-very/solido.git
   cd solido
   ```

2. **Open in Godot Editor**
   ```bash
   godot --editor .
   ```

3. **Enable the Procedural Tools plugin**
   - Go to `Project → Project Settings → Plugins`
   - Enable "Procedural Tools"
   - The tools dock will appear on the right side

### Using the Procedural Tools Plugin

1. **Select a module** from the dropdown (e.g., "Terrain")
2. **Adjust parameters** in the parameters panel
3. **Click "Generate Preview"** to see results in real-time
4. **Click "Save to Library"** to save the generated resource

### Creating a New Module

```bash
# Create module directory
mkdir -p modules/my_module

# Create required files
touch modules/my_module/schema.toml
touch modules/my_module/generator.gd
touch modules/my_module/preview.tscn
```

See `modules/terrain/` for a complete example.

## 📚 Documentation

- **[CLAUDE.md](CLAUDE.md)** - AI agent navigation and workflow guide
- **[Godot Shader Style Guide](https://docs.godotengine.org/en/stable/tutorials/shaders/shaders_style_guide.html)** - Official shader coding standards
- **[tools/README.md](tools/README.md)** - Development tools documentation
- **[library/README.md](library/README.md)** - Generated resources guide

## 🔧 Development Workflow

### In-Editor Development

1. Open project in Godot Editor
2. Use Procedural Tools dock for interactive generation
3. Preview changes in real-time
4. Save resources to library

### Headless/CI Workflow

```bash
# Install watch dependencies
pip install watchdog

# Watch for schema changes
python tools/watch.py

# Manual regeneration
godot --headless --script tools/regenerate_module.gd -- modules/terrain
```

## 🎨 Shader Development

Shaders are written in Godot Shader Language (GLSL-like syntax):

```glsl
shader_type spatial;

uniform float roughness : hint_range(0.0, 1.0) = 0.5;

void fragment() {
    ALBEDO = vec3(0.8);
    ROUGHNESS = roughness;
}
```

See [CLAUDE.md](CLAUDE.md) for detailed coding patterns and conventions.

## 🏛️ Architecture Principles

1. **Composability Over Monoliths** - Small, reusable components
2. **Explicit Over Implicit** - Clear dependencies and behavior
3. **Performance Through Understanding** - Measure, then optimize
4. **Incremental Stability** - Every commit should be usable
5. **Visual Fidelity With Control** - Balance realism and performance
6. **Documentation Lives With Code** - Update together

## 🤝 Contributing

**For AI Agents**: Read [CLAUDE.md](CLAUDE.md) first - it contains all navigation and workflow guidelines.

**For Humans**:
1. Check `todo/` for active work
2. Follow patterns in existing modules
3. Test before committing
4. Update documentation alongside code

## 📝 License

MIT License - see [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- Built with [Godot Engine](https://godotengine.org/)
- Shader development guided by [Godot Shader Documentation](https://docs.godotengine.org/en/stable/tutorials/shaders/)

---

**Project Status**: 🌱 Greenfield Development - Building from ground up

**Cardinal Rule**: *"A graphics system is only as good as its predictability. Make changes that increase understanding, not magic."*
