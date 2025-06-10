# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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
The general workflow you should use is:
1. Understand the desired change thoroughly. Ask the user questions if you're not sure about any aspects
2. Think carefully about how to properly implement the change within the codebase
3. Create an outline of the proposed work for the user to review
4. Once in agreement, implement the change
5. Run auto-format of code
6. Run a type check and then release mode build to test if changes compile successfully
7. Git add and commit
   - Include "Created with AI assistance" in your git commit messages. Don't say anything else about AI
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

## SSH Sidebar Implementation Plan

### Phase 1: Data Model and Configuration (Week 1)

1. **Create SSH Host Entry Model** (`config/src/ssh_hosts.rs`):
```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SshHostEntry {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub port: u16,
    pub username: Option<String>,
    pub identity_file: Option<PathBuf>,
    pub order: usize,
    pub group: Option<String>,
    pub icon: Option<String>,
}
```

2. **Add Configuration Options** (`config/src/config.rs`):
```rust
#[dynamic(default)]
pub ssh_sidebar: SshSidebarConfig,

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SshSidebarConfig {
    pub enabled: bool,
    pub width: u16,
    pub position: SidebarPosition, // Left, Right
    pub show_on_startup: bool,
    pub animation_duration_ms: u64,
}
```

3. **Implement Persistence** (`wezterm-gui/src/ssh_hosts_store.rs`):
- JSON file storage at `~/.config/wezterm/ssh_hosts.json`
- Read/write functionality
- Migration from ssh_config entries

### Phase 2: Sidebar Widget (Week 2)

1. **Create Sidebar Widget** (`wezterm-gui/src/sidebar/ssh_sidebar.rs`):
```rust
pub struct SshSidebar {
    hosts: Vec<SshHostEntry>,
    groups: HashMap<String, bool>, // Expanded state
    selected: Option<String>,
    collapsed: bool,
    animation: ColorEase,
    scroll_offset: usize,
    drag_state: Option<DragState>,
}

impl Widget for SshSidebar {
    // Implement rendering with box model
    // Handle mouse events for selection/drag
    // Use ColorEase for animations
}
```

2. **Integrate with Tab Bar Layout** (`wezterm-gui/src/termwindow/mod.rs`):
- Modify `compute_tab_bar_rects()` to account for sidebar
- Add sidebar to `paint_impl()` rendering
- Register UIItems for mouse handling

3. **Create Host List Renderer** (`wezterm-gui/src/sidebar/host_list.rs`):
- Reuse launcher's list patterns
- Add group headers with expand/collapse
- Implement scrolling with scrollbar

### Phase 3: Interaction and Animations (Week 3)

1. **Implement Drag and Drop** (`wezterm-gui/src/sidebar/drag_drop.rs`):
```rust
struct DragState {
    dragging_id: String,
    start_pos: Point,
    current_pos: Point,
    placeholder_index: usize,
}
```

2. **Add Animation States**:
- Sidebar expand/collapse with ColorEase
- Group expand/collapse animations
- Hover state transitions
- Connection progress indicators

3. **Keyboard Navigation**:
- Up/Down arrow keys for selection
- Enter to connect
- Delete to remove
- Ctrl+N for new host

### Phase 4: Host Management UI (Week 4)

1. **Create Add/Edit Modal** (`wezterm-gui/src/overlay/ssh_host_editor.rs`):
- Form fields using termwiz widgets
- Validation for hostname/port
- SSH key file picker
- Test connection button

2. **Context Menu** (`wezterm-gui/src/sidebar/context_menu.rs`):
- Right-click menu for hosts
- Edit, Delete, Duplicate options
- Group management

3. **Integration with SSH System**:
- Convert `SshHostEntry` to `SshDomain`
- Hook into existing connection flow
- Show connection status in sidebar

### Phase 5: Polish and Events (Week 5)

1. **Lua Event Integration**:
```lua
wezterm.on('ssh-sidebar-connect', function(host_entry)
  -- Allow customization
end)

wezterm.on('ssh-sidebar-before-save', function(hosts)
  -- Allow validation/modification
end)
```

2. **Search and Filter**:
- Add search box at top of sidebar
- Filter by name/hostname
- Recent connections section

3. **Visual Polish**:
- Custom icons per host
- Status indicators (connected/disconnected)
- Tooltips with full connection details
- Smooth scrolling

### Implementation Order

1. Start with data model and config (no UI)
2. Create basic sidebar with static host list
3. Add mouse interaction and selection
4. Implement animations and drag/drop
5. Add host management modals
6. Polish with search, icons, and events

### Key Integration Points

- **Window Layout**: Modify `TermWindow::compute_tab_bar_rects()` in `termwindow/mod.rs`
- **Rendering**: Add to `TermWindow::paint_impl()` after tab bar rendering
- **Mouse Events**: Register UIItems in `TermWindow::ui_items()`
- **Configuration**: Hook into `config::configuration()` system
- **SSH Connection**: Use `mux::domain::SshDomain::connect()`  

### Testing Strategy

1. Unit tests for data model and persistence
2. Integration tests for SSH connection flow
3. Manual testing of drag/drop and animations
4. Performance testing with 100+ hosts
5. Accessibility testing for keyboard navigation

## TODOs

### UI/UX Features

#### SSH Sidebar (Planned)
**Status**: Design phase - see SSH Sidebar Implementation Plan above

**Next Steps**
- Begin Phase 1: Data model and configuration implementation
- Create basic widget framework integration
- Design host management UI workflows

### Performance & Optimization

- Benchmark OS window border update performance across platforms
- Add frame rate monitoring?

### Documentation & Testing

