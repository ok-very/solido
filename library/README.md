# Library

This directory contains all generated procedural resources created by the Procedural Tools addon.

## Structure

```
library/
├── terrain/       # Terrain generator outputs
├── materials/     # Generated materials
├── meshes/        # Generated meshes
└── shaders/       # Generated shader resources
```

## Usage

Resources are automatically saved here when you click "Save to Library" in the Procedural Tools dock.

Each resource is a `.tres` file that can be:
- Loaded in other scenes
- Modified in the inspector
- Version controlled with git
- Re-generated with different parameters

## Organization

Organize your generated resources by:
- **Type** - Group by generator module (terrain, materials, etc.)
- **Purpose** - Create subfolders for specific projects
- **Variation** - Use descriptive names (e.g., `mountain_rocky.tres`, `plains_grassy.tres`)
