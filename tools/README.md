# Tools

Utility scripts for development and CI/CD workflows.

## watch.py

Monitors schema files for changes and triggers regeneration in headless mode.

```bash
# Install dependencies
pip install watchdog

# Watch all modules
python tools/watch.py

# Watch specific module
python tools/watch.py modules/terrain

# Use custom Godot binary
GODOT_BIN=/path/to/godot python tools/watch.py
```

## regenerate_module.gd

Headless script for regenerating modules via command line.

```bash
# Regenerate terrain module
godot --headless --script tools/regenerate_module.gd -- modules/terrain
```

## Usage in CI/CD

```yaml
# Example GitHub Actions workflow
- name: Regenerate all modules
  run: |
    for module in modules/*/; do
      godot --headless --script tools/regenerate_module.gd -- $module
    done
```
