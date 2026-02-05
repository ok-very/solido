# CLAUDE.md - AI Agent Navigation Guide for Solido Tri-D

**Purpose**: This document serves as the primary entry point for AI agents working on the Solido Tri-D codebase. Read this file first before making any changes.

**Project Overview**: Solido Tri-D is a greenfield graphics construction apparatus built with GLSL-like Godot Shader script. We're building from the ground up a stable, scalable system where control is added through experimentation, extension, digestion, and thoughtful reconstruction.

---

## 1. Quick Reference for Common Tasks

### Technology Stack
- **Primary Language**: [Godot Shader Language](https://docs.godotengine.org/en/stable/tutorials/shaders/shaders_style_guide.html) (GLSL-like)
- **Engine**: Godot 4 (headless setup)
- **Workflow**: Tools accessed through CI run scripts (e.g., 3D preview)

### Key Directories (As They Emerge)
```
/shaders/          # Core shader implementations
/scenes/           # Godot scene files for testing/preview
/scripts/          # GDScript automation and tooling
/ci/               # CI run scripts for headless operations
/examples/         # Example shader configurations
/docs/             # Technical documentation
/tests/            # Test shaders and validation scripts
/todo/             # Incomplete tasks, bugs, and future work
```

### Critical Files to Understand First
1. `shaders/` - Start here to understand the shader architecture
2. `todo/` - Check active work and known issues before starting
3. `ci/` - Understand how to run headless operations
4. This file (`CLAUDE.md`) - Always read first

### Cross-Reference Pattern
- Each shader file should reference dependent shaders in header comments
- Scene files link to their shader assets
- CI scripts document which shaders/scenes they operate on
- TODO entries link to relevant files

---

## 2. Understanding Change Impact

### Before Making Any Changes

1. **Read First, Propose Second**
   - Read relevant shader files completely
   - Analyze dependencies and includes
   - Confirm understanding by explaining the code's purpose
   - Only then propose changes

2. **Dependency Tracking**
   - Shader dependencies: Check `#include` or `import` statements
   - Scene dependencies: Check `.tscn` files for shader references
   - CI dependencies: Check which scripts reference your target files
   - Use file headers to understand component relationships

3. **Impact Analysis Checklist**
   ```
   [ ] What shaders depend on this component?
   [ ] What scenes use this shader?
   [ ] What CI scripts test this functionality?
   [ ] What parameters are exposed to other systems?
   [ ] Does this change affect shader compilation?
   ```

4. **Logging Incomplete Work**
   - Create entries in `todo/` for:
     - Bugs discovered during analysis
     - Incomplete implementations
     - Future optimization opportunities
     - Breaking changes that need migration
   - Format: `todo/YYYY-MM-DD-short-description.md`

---

## 3. Testing Strategy

### Test Types and Locations

**Visual Tests** (`tests/visual/`)
- Shader output validation against reference images
- Used for regression testing visual changes
- Run through CI with headless rendering

**Unit Tests** (`tests/unit/`)
- Individual shader function tests
- Mathematical correctness validation
- Performance benchmarking

**Integration Tests** (`tests/integration/`)
- Multi-shader pipeline tests
- Scene rendering tests
- Parameter interaction validation

**Example Tests** (`examples/`)
- Each example serves as a living test
- Must remain functional as codebase evolves

### When to Write Which Test

Write **Visual Tests** when:
- Creating new visual effects
- Modifying shader output
- Changing rendering pipelines

Write **Unit Tests** when:
- Implementing mathematical functions
- Creating reusable shader utilities
- Optimizing existing functions

Write **Integration Tests** when:
- Connecting multiple shaders
- Building render passes
- Implementing parameter systems

### Running Tests

```bash
# Run all tests
./ci/run_tests.sh

# Run visual regression tests
./ci/run_visual_tests.sh

# Run unit tests only
./ci/run_unit_tests.sh

# Run specific test
./ci/run_test.sh tests/unit/test_name.gdshader

# Generate test reference images
./ci/generate_references.sh
```

### Anti-Patterns to Avoid

❌ Testing implementation details instead of behavior  
❌ Visual tests without reference images  
❌ Tests that depend on execution order  
❌ Hardcoding shader parameters in tests  
❌ Skipping tests because "it looks right"  

---

## 4. Build and Test Commands

### Project Setup
```bash
# Clone repository
git clone https://github.com/ok-very/solido.git
cd solido

# Verify Godot headless installation
godot --version --headless

# Initialize project
./ci/init_project.sh
```

### Building
```bash
# Compile all shaders
./ci/compile_shaders.sh

# Compile specific shader
./ci/compile_shader.sh shaders/path/to/shader.gdshader

# Validate shader syntax
./ci/validate_shaders.sh
```

### Testing
```bash
# Full test suite
./ci/run_tests.sh

# Fast tests only (unit + syntax)
./ci/run_fast_tests.sh

# Visual tests with diff output
./ci/run_visual_tests.sh --show-diffs

# Performance benchmarks
./ci/run_benchmarks.sh
```

### Preview and Development
```bash
# Launch 3D preview for shader
./ci/preview_shader.sh shaders/example.gdshader

# Generate example gallery
./ci/generate_gallery.sh

# Watch mode for development
./ci/watch_and_preview.sh shaders/working.gdshader
```

### Helper Scripts
```bash
# Create new shader from template
./scripts/new_shader.sh shader_name

# Create new example
./scripts/new_example.sh example_name

# Analyze shader complexity
./scripts/analyze_shader.sh shaders/target.gdshader

# Format all shaders
./scripts/format_shaders.sh
```

---

## 5. Common Patterns to Follow

### Shader File Structure
```glsl
shader_type spatial;
// ^ Always declare shader type first

/* 
 * [Component Name]
 * Purpose: Brief description
 * Dependencies: List dependent shaders
 * Parameters: Key exposed parameters
 */

// === UNIFORMS ===
// Group uniforms by purpose with comments
uniform vec4 base_color : source_color = vec4(1.0);
uniform float roughness : hint_range(0.0, 1.0) = 0.5;

// === CONSTANTS ===
const float PI = 3.14159265359;
const float EPSILON = 0.001;

// === UTILITY FUNCTIONS ===
// Place reusable functions before main functions

// === VERTEX SHADER ===
void vertex() {
    // Transform logic
}

// === FRAGMENT SHADER ===
void fragment() {
    // Material logic
}
```

### Import and Include Style
```glsl
// Use relative paths from shader root
#include "shaders/utils/math.gdshaderinc"
#include "shaders/utils/noise.gdshaderinc"
```

### Naming Conventions
- **Uniforms**: `snake_case` - `base_color`, `metallic_strength`
- **Functions**: `snake_case` - `calculate_normal()`, `apply_fog()`
- **Constants**: `SCREAMING_SNAKE_CASE` - `MAX_ITERATIONS`, `DEFAULT_SCALE`
- **Variables**: `snake_case` - `world_position`, `view_direction`
- **Files**: `snake_case.gdshader` or `snake_case.gdshaderinc`

### Error Handling Conventions
```glsl
// Validate inputs early
if (length(input_vector) < EPSILON) {
    input_vector = vec3(0.0, 1.0, 0.0); // Safe default
}

// Clamp parameters to valid ranges
float safe_value = clamp(parameter, 0.0, 1.0);

// Document undefined behavior
// NOTE: This function assumes normalized input vectors
```

### Code Generation Patterns
- Use preprocessor defines for feature toggles
- Generate variant shaders through CI scripts, not manual duplication
- Document generated files with clear headers indicating generation source

### Comment Style
```glsl
// Single-line explanations for complex operations
vec3 result = complicated_operation(); // Why this is needed

/* 
 * Multi-line block comments for:
 * - Function documentation
 * - Complex algorithm explanations
 * - Mathematical derivations
 */

// TODO: Future improvement description
// FIXME: Known issue description
// NOTE: Important implementation detail
// PERF: Performance consideration
```

---

## 6. Working Process / Workflow

### Approaching a Problem

1. **Understand the Request**
   - Read the full request carefully
   - Identify the core requirement vs. nice-to-haves
   - Check `todo/` for related work

2. **Survey the Landscape**
   - Search for similar existing implementations
   - Read related shader files completely
   - Map out dependencies and impacts

3. **Design Before Coding**
   - Sketch the shader pipeline on paper/pseudocode
   - Identify reusable components
   - Plan test strategy
   - Note potential performance concerns

4. **Implement Incrementally**
   - Start with the simplest working version
   - Test at each step
   - Add complexity gradually
   - Commit working states frequently

### When to Ask Questions vs. Proceed

**Ask when:**
- The change affects core architecture principles
- Multiple valid approaches exist with trade-offs
- Requirements are ambiguous or contradictory
- You need access to external resources (textures, models)
- Performance implications are unclear
- Breaking changes are necessary

**Proceed when:**
- Following established patterns
- Making localized improvements
- Adding tests
- Fixing clear bugs
- Implementing well-defined features
- Refactoring within component boundaries

### Breaking Down Work

Use this hierarchy for task decomposition:
1. **Feature** - User-facing capability
2. **Component** - Reusable shader module
3. **Function** - Specific shader function
4. **Test** - Validation for above
5. **Documentation** - Explanation of above

Each level should be independently committable and testable.

### What to Do When Stuck

1. **Technical Blocks**
   - Re-read relevant documentation
   - Check similar implementations in codebase
   - Simplify to minimal reproduction
   - Add debug visualization
   - Ask for help with specific questions

2. **Design Blocks**
   - Return to requirements
   - Sketch alternatives visually
   - Prototype simplified versions
   - Ask for design guidance

3. **Performance Blocks**
   - Profile current implementation
   - Research shader optimization techniques
   - Test on reference hardware
   - Ask for performance requirements

---

## 7. Implementation Strategy

### Incremental Change Guidelines

**Change Granularity**
- One logical change per commit
- Each commit should compile and pass tests
- Group related changes that must work together
- Split large features into milestone commits

**Safe Progression Steps**
1. Add new code without deleting old code
2. Test new code thoroughly
3. Switch systems to use new code
4. Remove old code after validation period
5. Update documentation

### When to Stop and Ask

**Required Human Input Signals:**
- Architecture changes affecting multiple components
- Introduction of new dependencies or tools
- Performance trade-offs without clear benchmarks
- Changes to public APIs or parameter interfaces
- Uncertainty about user requirements
- Need for artistic direction on visuals
- Security or safety concerns
- Breaking changes to examples

### Approach Reassessment Signals

**Red Flags - Stop and Reconsider:**
- Implementation becoming significantly more complex than expected
- Frequent need to work around limitations
- Tests becoming convoluted to pass
- Performance degrading notably
- File growing beyond ~300 lines
- Copy-pasting code more than once
- "Just one more edge case" repeating

**Green Lights - Continue:**
- Code following existing patterns naturally
- Tests writing themselves based on clear behavior
- Performance meeting or exceeding baseline
- Each addition making system more coherent
- Documentation flowing easily

### Decision Points Requiring Human Input

1. **Architecture Decisions**
   - New shader pipeline stages
   - Coordinate system changes
   - Resource management strategies

2. **API Design**
   - Parameter naming and organization
   - Uniform value ranges and defaults
   - Feature toggle approaches

3. **Performance Trade-offs**
   - Quality vs. speed decisions
   - Memory vs. computation trade-offs
   - Compatibility vs. modern feature use

4. **Visual/Artistic Decisions**
   - Default aesthetic choices
   - Color space decisions
   - Effect intensity defaults

---

## 8. Architecture Principles

These core principles guide all changes to Solido Tri-D:

1. **Composability Over Monoliths**
   - Build small, reusable shader components
   - Combine simple pieces to create complexity
   - Each component should have a single clear purpose

2. **Explicit Over Implicit**
   - Make dependencies visible and clear
   - Parameter behavior should be predictable
   - No hidden state or magic values
   - Document assumptions explicitly

3. **Performance Through Understanding**
   - Measure before optimizing
   - Understand GPU architecture implications
   - Readable code first, then optimize hot paths
   - Profile on target hardware

4. **Incremental Stability**
   - Every commit should leave the project usable
   - Add through extension, not modification
   - Keep backwards compatibility until deliberate breaking change
   - Test continuously, not at milestones

5. **Visual Fidelity With Control**
   - Provide knobs for artistic control
   - Balance realism with performance
   - Make defaults look good out of box
   - Allow power users full access

6. **Documentation Lives With Code**
   - Comments explain why, not what
   - Examples demonstrate real usage
   - Update docs in same commit as code
   - Architecture decisions recorded in markdown

---

## 9. Quick Component Summary

### Core Shaders (`shaders/`)
*Component descriptions will populate as codebase develops*

- `shaders/base/` - Fundamental shader building blocks
- `shaders/materials/` - Surface material implementations
- `shaders/effects/` - Visual effects and post-processing
- `shaders/utils/` - Reusable utility functions

### CI and Tooling (`ci/`, `scripts/`)
- `ci/run_tests.sh` - Main test execution entry point
- `ci/preview_shader.sh` - Interactive 3D shader preview
- `scripts/new_shader.sh` - Scaffold new shader from template
- `scripts/format_shaders.sh` - Apply code formatting standards

### Examples (`examples/`)
- Living documentation of shader usage
- Each example is a minimal demonstration
- Serves as integration test

### Documentation (`docs/`)
- Architecture decision records
- Shader pipeline documentation
- Performance guidelines
- Getting started guides

---

## 10. File Reference

Map of key documentation and where to find information:

| Need to Understand... | Read This File |
|----------------------|----------------|
| How to start contributing | This file (`CLAUDE.md`) |
| Active work and known issues | `todo/` directory |
| Shader coding standards | [Godot Shader Style Guide](https://docs.godotengine.org/en/stable/tutorials/shaders/shaders_style_guide.html) |
| How shaders are tested | `tests/README.md` |
| CI pipeline details | `ci/README.md` |
| Project vision and goals | `docs/VISION.md` |
| Architecture decisions | `docs/architecture/` |
| Performance expectations | `docs/PERFORMANCE.md` |
| Example usage patterns | `examples/README.md` |
| Shader API reference | `docs/API.md` |

---

## 11. Context Files

### Directory-Specific Context

Load these context files when working in specific areas:

**Working in `/shaders/materials/`**
- Read: `shaders/materials/CONTEXT.md`
- Understand: Material pipeline and PBR principles
- Reference: Existing material implementations

**Working in `/shaders/effects/`**
- Read: `shaders/effects/CONTEXT.md`
- Understand: Effect composition and ordering
- Reference: Performance budgets for effects

**Working in `/ci/`**
- Read: `ci/CONTEXT.md`
- Understand: Headless Godot constraints
- Reference: CI environment capabilities

**Working in `/tests/`**
- Read: `tests/CONTEXT.md`
- Understand: Test methodology and validation criteria
- Reference: Test utilities and helpers

**Working in `/examples/`**
- Read: `examples/CONTEXT.md`
- Understand: Example standards and patterns
- Reference: Template example structure

---

## Final Reminder

**Before touching any code:**
1. Read this file (`CLAUDE.md`)
2. Check `todo/` for related work
3. Read the target shader file completely
4. Understand dependencies and impact
5. Plan your changes and tests
6. Only then write code

**During implementation:**
- Test continuously, not just at the end
- Commit working states frequently
- Update documentation alongside code
- Add TODO entries for future work

**The cardinal rule:** *"A graphics system is only as good as its predictability. Make changes that increase understanding, not magic."*

---

**Document Version**: 1.0  
**Last Updated**: 2026-02-04  
**Maintained By**: Solido Tri-D Contributors
