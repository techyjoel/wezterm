# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Refer to ../SPEC.md (if available) for a detailed spec of this project and use TASKS.md to track the project and tasks.

Think critically and be skeptical of prior work. Think like a lead software engineer and architect. Fix problems you find that will break things (but don't do needless work). Consider multiple concepts for how to solve problems before you write code, and pick the best one that aligns with the codebase. Before editing things, use a subagent to examine the current codebase to ensure you fully understand all relevant portions (but subagents should not modify code, you must tell them not to). Don't make guesses, ensure you understand!

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
# Type-check without building, only show errors (use this form unless requested to fix warnings by the user)
cargo check 2>&1 | awk '/^error/ {print; in_block=1; next} in_block { if (/^$/) in_block=0; else print }'

# Type-check without building (fastest iteration)
cargo check

# Build in debug mode
cargo build

# Build in release mode
cargo build --release 2>&1 | tail -50

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

# Run the built binary with debug logging
WEZTERM_LOG=debug ./target/release/wezterm
```

### Development process
The workflow you should use is:
1. Understand the desired change thoroughly. Ask the user questions if you're not sure about any aspects
2. Think carefully about how to properly implement the change within the codebase
3. Create an outline of the proposed work for the user to review, or use an existing out line in TASKS.md if it exists
4. Create a branch if doing any notable work (feature or other material change)
5. Once in agreement on the proposed work, implement the change
6. Run auto-format of code
7. You MUST run a type check and then a release mode build to test if changes compile successfully before proceeding
9. If you made GUI changes then prompt the user to run the build to test it (since you won't be able to see the graphic results). DO NOT move on to further steps until this step is complete.
10. Git add and commit (only after succesfully compiling and testing). Then if on a branch, git push (if not then the user will push when desired)
11. Update TASKS.md (if it is in use)
  - Check off tasks that are fully complete
  - Note partiallly-completed work that's done (and what's left to do), but do not check off partially-completed tasks
  - Correct the task items to reflect the as-built conditions (i.e. re-word things or add things so the task list reflects the new codebase)
  - Add implementation details that have been built which will need to be referenced for future tasks

### Git commits
   - Include "Created with AI assistance" in your git commit messages.
   - DO NOT say anything else about AI like "co-authored" or anything else. 
   - DO NOT mention Claude.
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
   - Our custom config file is located at ./clibuddy/wezterm.lua

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

### Other Implementation Details

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

**Note**: The TermWiz widget system is primarily for standalone terminal-based applications. WezTerm's GUI components (including sidebars) use the Element-based Box Model system described below, not TermWiz widgets.

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
- Multi-line text wrapping via `ElementContent::WrappedText`

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

### Cut-a-Hole Rendering Pattern

For complex layered UI like sidebars with scrollable regions, we have used a "cut-a-hole" rendering approach:

- Render scrollable content at a lower z-index (e.g., z-index 10)
- Render the container background at a higher z-index (e.g., z-index 12) with rectangular sections excluded
- The lower content shows through the "hole" in the background
- This allows independent scrolling while maintaining proper visual hierarchy

This pattern is used in the right sidebar to separate the activity log scrolling from fixed UI elements.

### GPU Blur Effect System

WezTerm includes a GPU-accelerated blur system for creating neon glow effects on UI elements:

**Key Components:**
- `termwindow/render/blur.rs` - Main blur renderer with WebGPU/OpenGL backends
- `termwindow/render/effects_overlay.rs` - Manages effects layer rendered after main content
- `termwindow/render/icon_to_texture.rs` - Converts glyphs/icons to textures for blur input
- `shaders/blur.wgsl` and `blur-*.glsl` - GPU shaders for 2-pass Gaussian blur

**Usage Example:**
```rust
// In your render method:
render_neon_glyph(
    params,
    element,           // The UI element to apply glow to
    glyph,            // The icon/character
    style,            // NeonStyle with color and glow settings
    metrics,          // Font metrics
    underline_height,
    cell_size
)?;
```

**How It Works:**
1. Icon/glyph is rasterized to a texture with padding for blur
2. Horizontal Gaussian blur pass
3. Vertical Gaussian blur pass  
4. Result is additively blended at the original position

**Configuration:**
- Blur radius: Up to ~15-16 pixels (default 8px)
- Intensity: Configurable via `style.glow_intensity`
- Caching: LRU cache with 50MB limit for reusing blurred textures

**Performance:**
- 2 GPU passes vs previous 240 CPU passes
- Supports both WebGPU and OpenGL backends
- Automatic backend selection based on `config.front_end`

**Blur Algorithm:**
- Sigma: `(radius + 1.0) / 2.0`
- Kernel size: `ceil(sigma * 3.0)` for 3-sigma coverage
- Maximum 63 kernel elements


## Z-Index and Layer System Documentation

### Overview

WezTerm uses a two-level rendering system that can be confusing at first. This document clarifies how it works based on code analysis.

### The Two-Level System

#### Level 1: Z-Index (RenderLayer)
- **Purpose**: Determines rendering order between different UI components
- **Range**: Any `i8` value (-128 to 127)
- **Created dynamically**: New `RenderLayer` objects are created as needed
- **Code**: `renderstate.rs` - `layer_for_zindex(zindex: i8)`

#### Level 2: Sub-Layers (within each RenderLayer)
- **Purpose**: Separates content types within a single z-index
- **Count**: Exactly 3 sub-layers per z-index (hardcoded)
- **Indices**: 0, 1, 2 only
- **Code**: `renderstate.rs` - `pub vb: RefCell<[TripleVertexBuffer; 3]>`

### How It Works

#### 1. Z-Index Creates RenderLayers
```rust
// renderstate.rs:
pub fn layer_for_zindex(&self, zindex: i8) -> anyhow::Result<Rc<RenderLayer>> {
    // Checks if layer exists, creates if not
    // Keeps layers sorted by zindex for rendering order
}
```

#### 2. Each RenderLayer Has Fixed Sub-Layers
```rust
// quad.rs:339-344 (HeapQuadAllocator::allocate)
match layer_num {
    0 => &mut self.layer0,
    1 => &mut self.layer1,
    2 => &mut self.layer2,
    _ => unreachable!(),  // PANICS if > 2
}

// renderstate.rs: (BorrowedLayers)
fn allocate(&mut self, layer_num: usize) -> anyhow::Result<QuadImpl> {
    self.layers[layer_num].allocate()  // Array access - panics if > 2
}
```

#### 3. Sub-Layer Usage Convention
- **Sub-layer 0**: Backgrounds, underlines, block cursors
- **Sub-layer 1**: Text glyphs
- **Sub-layer 2**: Sprites, UI elements, bar cursors

### Code Examples

#### Terminal Rendering (z-index 0)
```rust
// paint.rs:
let layer = gl_state.layer_for_zindex(0)?;
let mut layers = layer.quad_allocator();

// paint.rs:
self.filled_rectangle(&mut layers, 0, rect, background)?;  // Sub-layer 0 for background
```

#### Element Rendering
```rust
// box_model.rs:
let layer = gl_state.layer_for_zindex(element.zindex)?;
let mut layers = layer.quad_allocator();

// Different content types use different sub-layers:
let mut quad = layers.allocate(2)?;  // Sprites use sub-layer 2
let mut quad = layers.allocate(1)?;  // Glyphs use sub-layer 1
```

#### Z-Index Inheritance
```rust
// box_model.rs:
zindex: element.zindex + context.zindex,  // Elements inherit parent z-index
```

### Z-Index Assignments
- **Z-index 0**: Terminal content (existing)
- **Z-index 1**: Tab bar (existing)
- **Z-index 10**: Right sidebar activity log content
- **Z-index 12**: Right sidebar background
- **Z-index 14**: Right sidebar main content
- **Z-index 16**: Right sidebar scrollbars(s) and buttons
- **Z-index 20**: Right sidebar overlays (e.g. modals)
- **Z-index 22**: Right sidebar overlay content within overlays (such as sidebars within overlays)
- **Z-index 30**: Left sidebar content for scrolling
- **Z-index 32**: Left sidebar background
- **Z-index 34**: Left sidebar main content
- **Z-index 36**: Left Sidebar scrollbar(s) and buttons
- **Z-index 38**: Left sidebar overlays  (e.g. modals)
- **Z-index 40**: Left sidebar overlay content within overlays (such as sidebars within overlays)


## Important Implementation Notes

### Click Detection for UI Elements

**Always use the UIItemType pattern** for clickable elements - WezTerm automatically tracks exact rendered bounds.

1. Add variant to `UIItemType` enum in `termwindow/mod.rs`:
   ```rust
   UIItemType::MyButton(MyButtonData),
   ```

2. Set on Element during rendering:
   ```rust
   chip.with_item_type(UIItemType::MyButton(data))
   ```

3. Handle in `mouseevent.rs`:
   ```rust
   UIItemType::MyButton(data) => {
       self.mouse_event_my_button(data, event, context);
   }
   ```

Never manually track bounds or calculate click positions - the UIItem system handles this automatically.

### Window Resizing with Sidebars

When implementing sidebars that expand the window width, it's critical to understand the resize flow:

1. **Window Expansion Calculation**: The `get_window_expansion()` method in `SidebarManager` should only return a non-zero value when the sidebar is meant to be visible, NOT during animations. Use `animation_target_visible` instead of `is_animating()`:
   ```rust
   // CORRECT - only expand when sidebar should be shown
   let should_expand = self.config.mode == SidebarMode::Expand && 
      self.right_state.animation_target_visible;
   
   // WRONG - causes resize calculation issues during animations
   let should_expand = self.config.mode == SidebarMode::Expand && 
      (self.right_state.animation_target_visible || self.right_state.is_animating());
   ```

2. **Window Resize Logic**: The `set_inner_size()` wrapper should NOT add expansion when called from sidebar toggle operations. The resize dimensions already include the desired expansion state.

3. **Key Issue**: If `get_window_expansion()` returns a value during collapse animations, it creates a circular problem where the window resize calculations become incorrect, preventing the window from shrinking.

4. **Sidebar Initialization**: When `show_on_startup` is true, ensure `set_right_visible(true)` is called during setup. Don't rely on `is_right_visible()` for this check in Expand mode as it always returns true.


### Sidebar Font System

Sidebars use a multi-font architecture for proper typography:

**Font Bundle Structure:**
```rust
pub struct SidebarFonts {
    pub heading: Rc<LoadedFont>,  // For titles, headers (Roboto Bold)
    pub body: Rc<LoadedFont>,     // For content, chips (Roboto Regular/Light)
    pub code: Rc<LoadedFont>,     // For code blocks (JetBrains Mono Light)
}
```

**Usage Pattern:**
- Fonts are resolved in the main thread during rendering
- Passed to sidebar render methods via `&SidebarFonts` parameter
- Thread-safe design avoids storing `Rc<LoadedFont>` in sidebars
- Configuration via `clibuddy.right_sidebar.fonts` in wezterm.lua

## Implementation Plans

All implementation plans and task breakdowns have been moved to TASKS.md for centralized tracking and management. Please refer to TASKS.md for:

- Detailed project phases and timelines
- Task breakdowns with dependencies
- Library recommendations
- Integration points and patterns
- Testing strategies
