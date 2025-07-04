# WezTerm Architecture Overview

## System Architecture

WezTerm is organized as a set of Rust crates in a Cargo workspace, with clear separation between terminal emulation, GUI rendering, and multiplexing capabilities.

### Core Components

#### Terminal Model (`term/`)
Platform-agnostic terminal emulation core that handles:
- Escape sequence parsing via state machine (`wezterm-escape-parser/`)
- Terminal state management (`terminalstate.rs`)
- Cell buffer management with Unicode support
- No GUI dependencies - can be used headless

Key interfaces:
- `Terminal` struct - main terminal interface wrapper
- `TerminalState` - core state machine
- `Screen` - cell buffer management

#### GUI Frontend (`wezterm-gui/`)
GPU-accelerated terminal renderer with:
- WebGPU primary, OpenGL fallback (`renderstate.rs`)
- Window management abstraction (`window/`)
- Tab and pane UI (`termwindow/`)
- Glyph cache and texture atlas (`glyphcache.rs`)

Key files:
- `termwindow/mod.rs` - Main window implementation
- `renderstate.rs` - GPU state management
- `quad.rs` - Quad-based rendering primitives

##### Sidebar System
Component-based UI framework for AI assistance:
- `sidebar/` - Main sidebar infrastructure
- Animation coordinator for smooth transitions
- Modal system for overlays and dialogs
- GPU blur effects for visual enhancements

#### Multiplexer (`wezterm-mux-server/`, `mux/`)
Headless terminal multiplexer supporting:
- Client-server architecture
- Multiple connection types (local, SSH, TLS)
- Session persistence
- Domain abstraction pattern

Key concepts:
- `Domain` trait (`mux/src/domain.rs`) - Connection abstraction
- `Mux` - Central multiplexer state
- Window → Tab → Pane hierarchy

#### Configuration System (`config/`)
Lua-based runtime configuration:
- Dynamic reloading without restart
- Event hooks and callbacks
- Key binding definitions
- Appearance and behavior settings

Configuration entry points:
- `config/src/lua.rs` - Lua API definitions
- `config/src/lib.rs` - Config struct definitions

### Key Architectural Patterns

#### Domain Abstraction
All connection types implement the `Domain` trait, enabling uniform handling:
```rust
// See mux/src/domain.rs
pub trait Domain {
    fn spawn_pane(&self, ...) -> Result<Rc<dyn Pane>>;
    fn attach(&self) -> Result<()>;
    // ...
}
```

Local, SSH, and TLS domains all implement this interface.

#### Pane/Tab/Window Model
Hierarchical structure for managing terminal sessions:
- **Window**: Top-level container, can be GUI or headless
- **Tab**: Contains one or more panes, handles splits
- **Pane**: Individual terminal session

See `mux/src/tab.rs` and `mux/src/pane.rs` for implementations.

#### Event System
Lua-based event handling enables user customization:
- GUI events (window focus, resize)
- Mux events (tab creation, pane splits)
- Key events with configurable bindings

Events defined in `config/src/lua.rs`, dispatched throughout system.

### Cross-Platform Abstractions

#### Window System (`window/`)
Platform-specific implementations behind common traits:
- X11/Wayland on Linux
- Cocoa on macOS  
- Win32 on Windows

Common interface in `window/src/lib.rs`.

#### Font System (`wezterm-font/`)
Abstracts font loading and shaping:
- HarfBuzz for text shaping
- Platform backends: FreeType, CoreText, DirectWrite
- Fallback chain support

See `wezterm-font/src/lib.rs` for main interface.

### Performance Considerations

#### Batched Rendering
GPU calls are minimized through batching:
1. Elements processed into vertex buffers
2. Buffers organized by z-index layers
3. Each layer drawn in single GPU call

See `renderstate.rs` and `quad.rs` for implementation.

#### Glyph Caching
Rendered glyphs cached in texture atlas:
- LRU eviction policy
- Automatic atlas growth
- Subpixel variants supported

Implementation in `glyphcache.rs`.

### Extension Points

#### Lua API
Users can extend WezTerm via Lua scripts:
- Custom key bindings
- Event handlers
- Status bar customization
- Color scheme definitions

### Build System

#### Cargo Workspace
Multi-crate workspace with shared dependencies:
- `wezterm` - CLI entry point
- `wezterm-gui` - GUI application
- `wezterm-mux-server` - Multiplexer server
- Supporting crates for specific functionality

#### Platform-Specific Build
- `get-deps` script for system dependencies
- Conditional compilation for platform code
- Minimum Rust version: 1.71.0

See `Cargo.toml` workspace definition and `ci/` directory for build scripts.