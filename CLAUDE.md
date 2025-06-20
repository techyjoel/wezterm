# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Refer to ../SPEC.md (if available) for a detailed spec of this project.

## Build and Development Commands

### Initial Setup
```bash
# Install system dependencies
./get-deps

# Check Rust version (minimum 1.71.0)
./ci/check-rust-version.sh
```

### Common Development Commands
```bash
# Type-check without building (fastest iteration)
cargo check

# Build in debug mode
cargo build

# Build in release mode
cargo build --release

# Run in debug mode
cargo run

# Run with backtrace for debugging
RUST_BACKTRACE=1 cargo run

# Run all tests
cargo test --all

# Auto-format code (required before PR submission)
cargo +nightly fmt --all

# Build and serve documentation locally
ci/build-docs.sh serve

# Debug with gdb
cargo build
gdb ./target/debug/wezterm
# In gdb: break rust_panic, run, bt
```

### Development process
The workflow you should use is:
1. Understand the desired change thoroughly. Ask the user questions if you're not sure about any aspects
2. Think carefully about how to properly implement the change within the codebase
3. Create an outline of the proposed work for the user to review
4. Once in agreement, implement the change
5. Run auto-format of code
6. Run a type check and then release mode build to test if changes compile successfully
7. Git add and commit

### Git commits
   - Include "Created with AI assistance" in your git commit messages. DO NOT say anything else about AI like "co-authored" or anything else. DO NOT mention Claude.
   - Don't use any emojii in git commit messages

## High-Level Architecture

### Core Components

1. **Terminal Model (`term/`)** - Platform-agnostic terminal emulation core
   - Handles escape sequences, xterm compatibility
   - Core terminal state machine and buffer management
   - No GUI dependencies

2. **GUI Frontend (`wezterm-gui/`)** - GPU-accelerated terminal renderer
   - WebGPU/OpenGL rendering pipeline
   - Window management and input handling
   - Tab and pane management UI

3. **Multiplexer Server (`wezterm-mux-server/`)** - Headless terminal multiplexer
   - Client-server architecture for remote sessions
   - Domain management (local, SSH, TLS)
   - Session persistence

4. **Configuration System (`config/`)** - Lua-based configuration
   - Runtime configuration via Lua scripts
   - Key bindings, appearance, behavior settings
   - Dynamic reloading support

5. **SSH Integration (`wezterm-ssh/`)** - Native SSH client implementation
   - Pure Rust SSH client (no OpenSSH dependency)
   - SFTP support
   - SSH agent forwarding

### Key Architectural Patterns

1. **Domain Abstraction** - Different connection types (local, SSH, TLS) implement a common `Domain` trait
   - Located in `mux/src/domain.rs`
   - Allows uniform handling of local and remote sessions

2. **Pane/Tab Model** - Hierarchical window management
   - Window → Tab → Pane structure
   - Each pane can be connected to different domains
   - Located in `mux/src/`

3. **Renderer Architecture** - GPU-accelerated rendering pipeline
   - WebGPU primary, OpenGL fallback
   - Glyph cache and texture atlas for efficient text rendering
   - Located in `wezterm-gui/src/glyphcache.rs` and `renderstate.rs`

4. **Event System** - Lua-based event handling
   - GUI events, mux events, key events
   - Extensible via user Lua scripts
   - Event definitions in `config/src/lua.rs`

### Important Implementation Details

1. **Escape Sequence Parser** (`wezterm-escape-parser/`) - State machine-based parser using `vtparse` for performance

2. **Font System** (`wezterm-font/`) - Cross-platform font loading and shaping
   - HarfBuzz for text shaping
   - FreeType/CoreText/DirectWrite backends

3. **Window System Abstraction** (`window/`) - Platform-specific window creation
   - Supports X11, Wayland, macOS, Windows
   - Common trait-based interface

4. **Cell Storage** (`wezterm-surface/`) - Efficient terminal cell storage with clustering for wide characters and grapheme clusters

### Workspace Structure

The project uses Cargo workspaces with these key members:
- `wezterm` - Main CLI entry point
- `wezterm-gui` - GUI application
- `wezterm-mux-server` - Multiplexer server binary
- `term` - Core terminal emulation
- `config` - Configuration handling
- Supporting crates for specific functionality

### Development Tips

- Use `cargo check` for rapid iteration during development
- The terminal model (`term/`) is separate from GUI - test terminal logic independently
- For xterm compatibility reference: https://invisible-island.net/xterm/ctlseqs/ctlseqs.html

## GUI Development Infrastructure

### TermWiz Widget System (`termwiz/src/widgets/`)

WezTerm includes a complete widget framework for building interactive UI components:

**Core Widget Trait:**
```rust
pub trait Widget {
    fn render(&mut self, args: &mut RenderArgs);
    fn get_size_constraints(&self) -> layout::Constraints;
    fn process_event(&mut self, event: &WidgetEvent, args: &mut UpdateArgs) -> bool;
}
```

**Key Components:**
- `Ui` struct - Widget hierarchy management, focus handling, event dispatch
- `layout::Constraints` - Cassowary constraint solver for professional layouts
- Built-in widgets: buttons, labels, text fields, list boxes
- Mouse and keyboard event propagation

**Layout System Features:**
- Percentage and fixed dimensions
- Min/max constraints
- Horizontal/vertical orientations
- Alignment (start, center, end)
- Parent-child relationships

### Animation System (`wezterm-gui/src/colorease.rs`)

Professional animation framework with GPU acceleration:

```rust
ColorEase::new(
    in_duration_ms: 200,     // Animation in duration
    in_function: EasingFunction::EaseOut,
    out_duration_ms: 150,    // Animation out duration
    out_function: EasingFunction::EaseIn,
    start: Some(Instant::now())
)
```

**Easing Functions:**
- Linear, Ease, EaseIn, EaseOut, EaseInOut, Constant
- Custom cubic Bézier curves
- Frame rate control via `animation_fps` config
- Integration with GPU uniforms for hardware acceleration

### Box Model UI System (`wezterm-gui/src/termwindow/box_model.rs`)

CSS-like box model for UI elements:

```rust
struct Element {
    display: DisplayType,      // Inline, Block, Flex
    padding: Padding,
    border: Border,
    margin: Margin,
    hover_colors: Option<ElementColors>,
    content: ElementContent,
}
```

**Features:**
- Hover state support
- Border rendering with corners
- Z-index layering
- Float positioning
- Background colors and gradients

### Mouse Interaction (`wezterm-gui/src/termwindow/mouseevent.rs`)

Comprehensive mouse handling system:

```rust
pub trait UIItem {
    fn hit_test(&self, x: usize, y: usize) -> bool;
    fn process_event(&mut self, event: &MouseEvent) -> UIItemResult;
}
```

**Capabilities:**
- Hit testing for UI elements
- Drag and drop support
- Mouse capture
- Hover tracking
- Coordinate transformation

### Tab Bar Pattern (`wezterm-gui/src/tabbar.rs`)

Example of complex UI integration:

```rust
struct TabBarState {
    items: Vec<TabEntry>,
    active_tab: TabId,
    hover_tab: Option<TabId>,
}
```

**Shows how to:**
- Integrate UI bars into main window
- Handle mouse interactions
- Render with GPU pipeline
- Manage hover states and animations

### Overlay System (`wezterm-gui/src/overlay/`)

Modal and overlay UI framework:

```rust
pub trait Modal {
    fn perform_assignment(&mut self, assignment: &KeyAssignment);
    fn mouse_event(&mut self, event: MouseEvent) -> Result<()>;
    fn render(&mut self) -> Result<()>;
}
```

**Examples:**
- `launcher.rs` - Command palette with search
- `selector.rs` - Generic selection lists
- `quickselect.rs` - Text selection overlays

### Rendering Pipeline Integration

**Key Files:**
- `wezterm-gui/src/renderstate.rs` - GPU state management
- `wezterm-gui/src/quad.rs` - Quad-based rendering
- `wezterm-gui/src/glyphcache.rs` - Text rendering cache
- `wezterm-gui/src/termwindow/render/` - Rendering implementations

**Integration Points:**
- `TermWindow::paint_impl()` - Main render loop
- `UIItem` registration for mouse handling
- Layer system with z-ordering



## Implementation Plans

All implementation plans and task breakdowns have been moved to TASKS.md for centralized tracking and management. Please refer to TASKS.md for:

- Detailed project phases and timelines
- Task breakdowns with dependencies
- Library recommendations
- Integration points and patterns
- Testing strategies
