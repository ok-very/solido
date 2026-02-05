#!/usr/bin/env python3
"""
Watch Script for Non-Editor Workflows

Monitors schema.toml files for changes and triggers regeneration.
Useful for CI/CD pipelines and headless workflows.

Usage:
    python tools/watch.py                    # Watch all modules
    python tools/watch.py modules/terrain    # Watch specific module
"""

import os
import sys
import time
import subprocess
from pathlib import Path
from watchdog.observers import Observer
from watchdog.events import FileSystemEventHandler


class SchemaChangeHandler(FileSystemEventHandler):
    """Handles schema.toml file change events"""
    
    def __init__(self, godot_path="godot"):
        self.godot_path = godot_path
        self.last_modified = {}
        
    def on_modified(self, event):
        if event.is_directory:
            return
            
        if event.src_path.endswith("schema.toml"):
            # Debounce: only process if >1 second since last modification
            current_time = time.time()
            last_time = self.last_modified.get(event.src_path, 0)
            
            if current_time - last_time < 1.0:
                return
                
            self.last_modified[event.src_path] = current_time
            
            print(f"\n[Watch] Schema changed: {event.src_path}")
            self._regenerate_module(event.src_path)
    
    def _regenerate_module(self, schema_path):
        """Trigger module regeneration via headless Godot"""
        module_dir = Path(schema_path).parent
        
        print(f"[Watch] Regenerating module: {module_dir}")
        
        # Call Godot headless with custom script
        cmd = [
            self.godot_path,
            "--headless",
            "--script",
            "tools/regenerate_module.gd",
            "--",
            str(module_dir)
        ]
        
        try:
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=30
            )
            
            if result.returncode == 0:
                print(f"[Watch] ✓ Regeneration complete")
                if result.stdout:
                    print(result.stdout)
            else:
                print(f"[Watch] ✗ Regeneration failed")
                print(result.stderr)
                
        except subprocess.TimeoutExpired:
            print("[Watch] ✗ Regeneration timed out")
        except Exception as e:
            print(f"[Watch] ✗ Error: {e}")


def watch_modules(module_path=None):
    """Start watching for schema changes"""
    
    # Determine watch path
    if module_path:
        watch_path = Path(module_path)
        if not watch_path.exists():
            print(f"Error: Module path not found: {module_path}")
            sys.exit(1)
    else:
        watch_path = Path("modules")
        if not watch_path.exists():
            print("Error: modules/ directory not found")
            sys.exit(1)
    
    # Find Godot executable
    godot_path = os.environ.get("GODOT_BIN", "godot")
    
    print(f"[Watch] Starting file watcher...")
    print(f"[Watch] Watching: {watch_path.absolute()}")
    print(f"[Watch] Godot: {godot_path}")
    print(f"[Watch] Press Ctrl+C to stop\n")
    
    # Setup watchdog observer
    event_handler = SchemaChangeHandler(godot_path)
    observer = Observer()
    observer.schedule(event_handler, str(watch_path), recursive=True)
    observer.start()
    
    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        print("\n[Watch] Stopping...")
        observer.stop()
    
    observer.join()
    print("[Watch] Stopped")


if __name__ == "__main__":
    # Check for watchdog dependency
    try:
        import watchdog
    except ImportError:
        print("Error: watchdog package not installed")
        print("Install with: pip install watchdog")
        sys.exit(1)
    
    # Parse arguments
    module_arg = sys.argv[1] if len(sys.argv) > 1 else None
    
    watch_modules(module_arg)
