# Godot 4 Design Patterns

Reference for agents writing GDScript. Distilled from *Game Development Patterns with Godot 4* (Packt) and project experience.

---

## Scene Composition (not inheritance)

Prefer composing behavior from child nodes over deep inheritance chains.

```gdscript
# Good: Component nodes add capabilities
# Player.tscn tree:
#   CharacterBody2D (Player.gd)
#     ├── CollisionShape2D
#     ├── AnimatedSprite2D
#     ├── RayCast2D          # wall detection component
#     ├── Area2D             # hitbox component
#     └── StateMachine.gd    # behavior component

# Access components via @onready
@onready var wall_detector: RayCast2D = $WallDetectorRayCast2D
@onready var hitbox: Area2D = $HitboxArea2D
```

**Rules:**
- Each node does one thing. A RayCast2D detects walls — it doesn't also handle jump logic.
- Use `@onready var` to cache child references. Never `get_node()` in `_process()`.
- If a script needs 5+ `@export` vars, it's probably doing too much — split into child nodes.
- `super()` is fine for 2-3 levels of inheritance. Beyond that, compose instead.

---

## Signal Patterns

### Emit from setters, connect in `_ready()`

```gdscript
# Emitter: use setter to trigger signals automatically
@export var health: int = 3:
    set(value):
        var old = health
        health = clampi(value, 0, max_health)
        if health < old:
            damage_taken.emit(old - health)
        if health <= 0:
            died.emit()

signal damage_taken(amount: int)
signal died
```

```gdscript
# Connector: wire up in _ready() or after add_child()
func _ready() -> void:
    player.damage_taken.connect(_on_player_damage_taken)
    player.died.connect(_on_player_died)
```

### Disconnect before freeing

```gdscript
# If you manually connected, disconnect before the emitter dies
func _exit_tree() -> void:
    if player and player.damage_taken.is_connected(_on_player_damage_taken):
        player.damage_taken.disconnect(_on_player_damage_taken)
```

### Signal naming

- Past tense for events that happened: `died`, `health_changed`, `item_collected`
- Present tense for requests: `damage_taken`, `input_received`
- Always include the data the listener needs: `signal lives_decreased(amount: int)`

### Tracked signal pattern (for dynamic connections)

When connecting signals dynamically (e.g., building UI controls at runtime), track them for cleanup:

```gdscript
var _connected_signals: Array = []  # [{obj, signal_name, callable}]

func _track_signal(obj: Object, signal_name: String, callable: Callable) -> void:
    obj.connect(signal_name, callable)
    _connected_signals.append({"obj": obj, "signal_name": signal_name, "callable": callable})

func _cleanup_signals() -> void:
    for entry in _connected_signals:
        if is_instance_valid(entry.obj) and entry.obj.is_connected(entry.signal_name, entry.callable):
            entry.obj.disconnect(entry.signal_name, entry.callable)
    _connected_signals.clear()
```

---

## State Machines

### Enum + match in `_physics_process`

```gdscript
enum State { IDLE, RUN, JUMP, FALL, DEAD }
var state: State = State.IDLE

func _physics_process(delta: float) -> void:
    match state:
        State.IDLE:
            animated_sprite.play("idle")
            if Input.is_action_pressed("move"):
                state = State.RUN
        State.RUN:
            animated_sprite.play("run")
            velocity.x = direction * speed
            if not is_on_floor():
                state = State.FALL
        State.JUMP:
            velocity.y = -jump_strength
            state = State.FALL
        State.FALL:
            velocity.y += gravity * delta
            if is_on_floor():
                state = State.IDLE
        State.DEAD:
            velocity = Vector2.ZERO

    move_and_slide()
```

### Async state transitions

```gdscript
func hit(damage: int) -> void:
    health -= damage
    if health >= 1:
        state = State.HIT
        animated_sprite.play("hit")
        await animated_sprite.animation_finished
        state = State.IDLE
    else:
        state = State.DEAD
        died.emit()
```

**Rules:**
- State transitions are explicit — one `state = X` assignment, never implicit.
- `await` is fine for animations. Don't `await` in `_process` or `_physics_process`.
- If states exceed 6-7, extract to a separate StateMachine node.

---

## Node Lifecycle

### Order of execution

```
_init()          # Object created (no tree access)
_enter_tree()    # Added to scene tree (children may not be ready)
_ready()         # All children are ready (safe to access $Child)
_process(delta)  # Every frame
_physics_process(delta)  # Every physics tick
_exit_tree()     # Removed from tree (cleanup here)
```

### Plugin lifecycle (`@tool` scripts)

```gdscript
@tool
extends EditorPlugin

func _enter_tree() -> void:
    # Add docks, register types, connect signals
    add_control_to_dock(DOCK_SLOT_RIGHT_UL, dock)

func _exit_tree() -> void:
    # Disconnect ALL signals, remove docks, free nodes
    remove_control_from_docks(dock)
    dock.queue_free()
```

**Rules:**
- `_ready()` is called once per tree entry. If a node is removed and re-added, `_ready()` fires again.
- `queue_free()` is deferred — the node lives until end of frame. Don't access it after calling.
- `@onready` vars are set just before `_ready()`. Never use them in `_init()` or `_enter_tree()`.
- In `@tool` scripts, always clean up in `_exit_tree()`. Leaked nodes persist in the editor.

---

## Resource & Scene Instancing

### Loading scenes

```gdscript
# Preload: compile-time, use for known scenes
const EnemyScene = preload("res://scenes/enemy.tscn")

# Load: runtime, use for dynamic/optional content
var scene = load("res://modules/" + module_name + "/preview.tscn")
```

### Instancing pattern

```gdscript
func spawn_enemy(position: Vector2) -> void:
    var enemy = EnemyScene.instantiate()
    enemy.global_position = position
    add_child(enemy)  # Now in tree — _ready() fires on enemy
```

### Factory pattern for dynamic spawning

```gdscript
func create_from_type(type: String) -> Node:
    var scene_path = "res://scenes/%s.tscn" % type
    if not ResourceLoader.exists(scene_path):
        push_error("Scene not found: " + scene_path)
        return null
    var scene = load(scene_path)
    return scene.instantiate()
```

**Rules:**
- `preload()` for scenes referenced in code at write time.
- `load()` for scenes chosen at runtime (module selection, factories).
- Always `add_child()` before accessing the instance's children or signals.
- Check `ResourceLoader.exists()` before `load()` for user-provided paths.

---

## GDScript 4.6 Conventions

- `snake_case` for functions, variables, signals, file names
- `PascalCase` for classes, enums, class_name declarations
- `SCREAMING_SNAKE_CASE` for constants
- `_prefixed` for private functions/vars
- Type hints on all function signatures: `func hit(damage: int) -> void:`
- Builtin types (Color, Vector2, etc.) can't be used as class refs in `is` — use `typeof() + TYPE_*`
