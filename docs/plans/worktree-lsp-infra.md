# Worktree + Per-Branch LSP Infrastructure

**Status:** Deferred — revisit when parallel branch work becomes a bottleneck.

## Problem

Single Godot headless editor instance serves LSP for one working directory. Branch switching invalidates `.godot/` cache, and the LSP returns stale diagnostics until restarted. Running parallel branches (e.g., Phase 2 Branches 2+3) would require multiple LSP instances.

## Current Setup

- **Service:** `systemctl --user` unit `godot-lsp` — single instance on port 6005
- **Unit file:** `~/.config/systemd/user/godot-lsp.service`
- **MCP bridge:** `@ryanmazzolini/minimal-godot-mcp` via npx, reads `GODOT_LSP_PORT` env var
- **Config:** `/home/silen/dev/solido/.mcp.json` (tracked in git)
- **Editor settings:** `~/.config/godot/editor_settings-4.6.tres` — no explicit LSP port (uses Godot 4.6 default: 6005)
- **No CLI flag** for LSP port — must be set via editor settings

## Proposed Design

### Worktree layout (nested under project)

```
solido/
├── wt/
│   ├── .gdignore          # Prevents Godot from scanning worktree dirs
│   ├── collapsible/       # git worktree → phase2/collapsible-sections
│   └── glass/             # git worktree → phase2/glass-and-viewport
```

- Add `wt/` to `.gitignore`
- Add `wt/.gdignore` (empty file) to prevent Godot cross-scanning and duplicate class_name conflicts

### Port allocation

| Instance | Worktree Path | Port | Service |
|----------|--------------|------|---------|
| main | `~/dev/solido` | 6005 | `godot-lsp@main` |
| collapsible | `~/dev/solido/wt/collapsible` | 6006 | `godot-lsp@collapsible` |
| glass | `~/dev/solido/wt/glass` | 6007 | `godot-lsp@glass` |

### XDG isolation per instance

Each Godot instance gets its own `XDG_CONFIG_HOME` so it has separate editor settings with a unique LSP port.

```
~/.local/share/godot-lsp/
├── main/config/godot/editor_settings-4.6.tres       (port 6005)
├── collapsible/config/godot/editor_settings-4.6.tres (port 6006)
└── glass/config/godot/editor_settings-4.6.tres       (port 6007)
```

Editor settings port entry (add to tres file):
```
network/language_server/remote_port = 6006
```

### Systemd template service

Replace single `godot-lsp.service` with template `godot-lsp@.service`:

```ini
[Unit]
Description=Godot headless editor LSP (%I)
After=default.target

[Service]
Type=simple
EnvironmentFile=%h/.config/godot-lsp/%I.env
ExecStart=/home/silen/.local/bin/godot --headless --editor --path ${WORKTREE_PATH}
Environment=XDG_CONFIG_HOME=${XDG_CONFIG_DIR}
Environment=XDG_DATA_HOME=${XDG_DATA_DIR}
Environment=XDG_CACHE_HOME=${XDG_CACHE_DIR}
Restart=on-failure
RestartSec=3

[Install]
WantedBy=default.target
```

Per-instance env files at `~/.config/godot-lsp/<instance>.env`:
```bash
WORKTREE_PATH=/home/silen/dev/solido
XDG_CONFIG_DIR=/home/silen/.local/share/godot-lsp/main/config
XDG_DATA_DIR=/home/silen/.local/share/godot-lsp/main/data
XDG_CACHE_DIR=/home/silen/.local/share/godot-lsp/main/cache
```

### Per-worktree .mcp.json

Each worktree gets a modified `.mcp.json` with matching `GODOT_LSP_PORT` and `GODOT_WORKSPACE_PATH`. Use `git update-index --skip-worktree .mcp.json` to hide the local change from git status.

Workflow: separate Claude Code sessions per worktree, each picks up the local `.mcp.json`.

### Branch management

Two independent stackit stacks off main:
- Stack A: `phase2/collapsible-sections` → `phase2/connectors` → `phase2/interaction-wiring`
- Stack B: `phase2/glass-and-viewport` → merges into Stack A at connectors

### Setup commands (when ready)

```bash
# Create branches
cd ~/dev/solido
stackit create phase2/collapsible-sections
stackit create phase2/glass-and-viewport

# Create worktrees
git worktree add wt/collapsible phase2/collapsible-sections
git worktree add wt/glass phase2/glass-and-viewport

# Initial import per worktree
godot --headless --editor --path wt/collapsible --import --quit-after 5
godot --headless --editor --path wt/glass --import --quit-after 5

# Start services
systemctl --user start godot-lsp@main godot-lsp@collapsible godot-lsp@glass
```

## Simpler Alternative (current approach)

Work one branch at a time. Restart LSP on branch switch. Optional post-checkout hook:

```bash
#!/bin/bash
# .git/hooks/post-checkout
godot --headless --editor --path . --import --quit-after 5
systemctl --user restart godot-lsp
```
