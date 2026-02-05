# Scripts

Runtime application scripts for standalone execution.

## main.gd

Main application controller for the runtime UI with 3D preview.

### Features

- **Module Discovery** - Automatically scans `modules/` directory
- **Dynamic UI** - Builds parameter controls from schema.toml files
- **Live Preview** - Real-time 3D visualization in SubViewport
- **Camera Controls** - Middle mouse to rotate, scroll wheel to zoom
- **Resource Saving** - Export generated resources to library

### Usage

**In Editor:**
```bash
godot res://scenes/main.tscn
```

**Exported Application:**
```bash
./SolidoTriD.exe  # Windows
./SolidoTriD.app  # macOS
./SolidoTriD.x86_64  # Linux
```

### Controls

- **Left Panel** - Parameter adjustment
- **Right Panel** - 3D preview viewport
- **Middle Mouse + Drag** - Rotate camera
- **Scroll Wheel** - Zoom camera
- **Generate Button** - Manually trigger generation
- **Save Button** - Save current configuration to library

### Architecture

The runtime UI reuses components from the editor plugin:
- `schema_parser.gd` - TOML parsing
- `ui_builder.gd` - Dynamic UI generation

This allows the same modules to work in both editor and runtime contexts.

### Adding Modules

Place new modules in `modules/` with:
- `schema.toml` - Parameter definitions
- `generator.gd` - Generation logic (extends Resource)
- `preview.tscn` - 3D preview scene
- `preview_controller.gd` - Script with `update_preview(data)` method

The runtime app will automatically discover and load them.
