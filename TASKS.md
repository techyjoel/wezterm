# CLiBuddy Terminal - Project Task List

## Overview

This document outlines the phased implementation plan for CLiBuddy Terminal, a fork of WezTerm with integrated AI capabilities. The project focuses on building a right sidebar AI assistant first, with a shared GUI framework that will later support a left sidebar for settings and SSH host management.

## Key Findings from Codebase Analysis

### What WezTerm Already Provides:
- **Widget System**: termwiz provides a complete widget framework with layout constraints
- **Box Model UI**: CSS-like styling with hover states, borders, and z-indexing
- **Animation System**: ColorEase for smooth transitions and effects
- **SSH Infrastructure**: Full SSH client implementation in wezterm-ssh
- **PTY Handling**: Complete terminal emulation and PTY management
- **Configuration**: Dynamic Lua-based config with hot reloading

### What We Need to Build:
- **Sidebar Framework**: No existing sidebar implementation
- **WebSocket Client**: No WebSocket support (need tokio-tungstenite)
- **Secret Detection**: No existing secret filtering (need custom implementation)
- **Terminal Capture**: Hook into existing PTY stream at read_from_pane_pty
- **State Management**: Follow mux patterns with Arc<Mutex<>> for shared state

### Recommended Libraries:
- **WebSocket**: `tokio-tungstenite` for async WebSocket client
- **Secrets**: `secrecy` crate for secure string handling
- **Credentials**: `keyring` for cross-platform credential storage
- **JSON**: `serde_json` (already in use)
- **Compression**: `flate2` for message compression
- **Markdown**: `pulldown-cmark` for chat rendering
- **Regex**: `regex` with `lazy_static` for pattern caching
- **Profiling**: `pprof` or `tracy` for performance analysis

## Project Structure

The implementation is divided into 7 phases:
1. **Foundation** - Shared GUI framework and sidebar infrastructure
2. **AI Sidebar UI** - Right sidebar interface components
3. **Terminal Integration** - Capture pipeline and block detection
4. **Backend Communication** - WebSocket client and state management
5. **AI Features** - Command execution and environment synchronization
6. **Left Sidebar** - Settings and SSH host management (lower priority)
7. **Polish & UX** - Animations, persistence, and refinements

---

## Phase 1: Foundation - Shared GUI Framework

### 1.1 Sidebar Infrastructure
- [x] **1.1.1** Create sidebar module structure (`wezterm-gui/src/sidebar/`)
  - Create `sidebar/mod.rs` as main module file with trait definitions
  - Define `SidebarPosition` enum (Left, Right)
  - Create `SidebarState` struct for animation and visibility tracking via Instant timestamps
  - Add `SidebarConfig` with width: 300, position: Right, show_on_startup: false, animation_duration_ms: 200
  - **Note**: Uses Element-based rendering instead of termwiz widgets
- [x] **1.1.2** Implement `Sidebar` trait
  ```rust
  pub trait Sidebar: Send + Sync {
      fn render(&mut self);
      fn get_width(&self) -> u16;
      fn is_visible(&self) -> bool;
      fn toggle_visibility(&mut self);
      fn get_position(&self) -> SidebarPosition;
      fn set_width(&mut self, width: u16);
      fn handle_mouse_event(&mut self, event: &MouseEvent) -> Result<bool>;
      fn handle_key_event(&mut self, key: &KeyCode) -> Result<bool>;
  }
  ```
  - **Note**: Added Send + Sync bounds for thread safety
  - **Note**: Uses WezTerm's MouseEvent/KeyCode types, returns Result<bool>
- [x] **1.1.3** Create `SidebarManager` to handle multiple sidebars
  - Manage left and right sidebar instances
  - Handle visibility states
  - Coordinate animations
  - **Note**: Uses `Arc<Mutex<dyn Sidebar>>` for thread-safe storage
  - **Note**: Includes animation progress calculation and update methods

### 1.2 Layout System Integration
- [x] **1.2.1** Modify `TermWindow` layout calculations
  - Update `compute_tab_bar_rects()` in `termwindow/mod.rs`
  - Account for sidebar widths in terminal content area
  - Handle dynamic resizing when sidebars show/hide
  - **Existing**: Tab bar already modifies layout; follow similar pattern
  - Added `sidebar_manager` field to TermWindow struct
  - Added `effective_window_width()` and `get_terminal_content_rect()` methods
  - Added SidebarMode enum (Overlay vs Expand) to control behavior
  - Left sidebar: Overlay mode (doesn't affect window size)
  - Right sidebar: Expand mode (expands window width)
  - **Window Resize Fix**: The key to proper window resizing is that `get_window_expansion()` must only return the sidebar width when `animation_target_visible` is true, NOT during animations. Otherwise, the resize calculations get confused and the window won't shrink properly.
  - **Partial Sidebar Visibility**: Right sidebar maintains MIN_SIDEBAR_WIDTH (25px) visibility when collapsed to prevent button cutoff
  - **Button Alignment**: Toggle button aligns with left edge of scrollbar, positioned at window_width - padding - border
  - **Startup State**: Call `set_right_visible(true)` unconditionally when show_on_startup is true
- [x] **1.2.2** Integrate sidebar rendering into main paint loop
  - Add sidebar rendering to `paint_impl()` and `paint_pass()`
  - Implement proper z-ordering (sidebars above terminal content)
  - Set up clipping regions
  - **Existing**: Render after panes but before/with tab bar in paint_pass()
  - Created `termwindow/render/sidebar.rs` with paint methods
  - Added `paint_sidebars()` call to paint_pass() after panes, before tab bar
  - Basic background rendering for both sidebars with animation support
  - Fixed quad allocation using TripleLayerQuadAllocatorTrait
  - Animations slide in from left/right edges based on progress
- [x] **1.2.3** Add sidebar toggle buttons to window chrome
  - Create button UI items for left sidebar icons (gear, SSH)
    - Look at NerdFonts: na-fa-gear, nf-md-lan_connect
  - Create button UI item for right sidebar (AI icon)
    - Look at NerdFonts: nf-md-assistant
  - Wire up click handlers
  - **Existing**: Add UIItemType::SidebarButton variant; follow tab bar button pattern
  - Added UIItemType::SidebarButton(SidebarPosition) variant
  - Implemented `paint_sidebar_toggle_buttons()` in sidebar.rs
  - Added mouse event handling in mouseevent.rs
  - Created `mouse_event_sidebar_button()` that toggles sidebars
  - Right sidebar button renders as 40x40 pixel square
  - Icon rendering and left button to be added when implementing content

### 1.3 Shared UI Components
- [x] **1.3.1** Create reusable card component (`sidebar/components/card.rs`)
  - Support title, content, and actions (e.g. buttons)
  - Implement hover states
  - Add expand/collapse functionality
  - **Existing**: Use box_model.rs Element for rendering
  - **Note**: Includes CardState enum (Normal, Expanded, Collapsed)
  - **Note**: Builder pattern with expandable headers showing ▶/▼ indicators
- [x] **1.3.2** Create scrollable container component
  - Implement virtual scrolling for performance
  - Add scrollbar with auto-hide
  - Support smooth scrolling
  - **Existing**: Use ScrollHit pattern from tab bar
  - **Note**: Renders only visible items, includes scrollbar with mouse wheel support
  - **Note**: is_over_scrollbar() not yet implemented (returns false)
- [x] **1.3.3** Create chip components for status display and filtering
  - **Existing**: Use box_model.rs with rounded borders
  - **Note**: ChipStyle enum with 6 variants, ChipSize with 3 sizes
  - **Note**: ChipGroup supports single/multi-select modes
  - **Note**: No rounded corners implemented yet (uses SizedPoly::none())
- [x] **1.3.4** Create form components
  - **Development status**: Completed
  - Text input with placeholder ✓
  - Button with hover states ✓
  - Toggle switch ✓
  - Dropdown/select ✓
  - Slider component ✓
  - Color picker (placeholder) ✓
  - Form validation helpers ✓
  - File picker for SSH keys and future document uploads (placeholder) ✓
  - **Implementation**: Created `sidebar/components/forms.rs` with all components
  - **Note**: Uses Element-based rendering (not termwiz widgets)

### 1.4 Animation Framework
- [x] **1.4.1** Extend ColorEase for sidebar animations
  - **Development status**: Completed
  - Created `sidebar/animation/animations.rs` with:
    - SidebarAnimation using ColorEase for timing ✓
    - SidebarPositionAnimation for slide effects ✓
    - SidebarOpacityAnimation for fade effects ✓
    - SidebarSlideAndFadeAnimation for combined effects ✓
  - Integrated with SidebarState and rendering pipeline ✓
  - Sidebars now use smooth position-based animations with easing ✓
  - **Implementation**: Updated paint_left_sidebar() and paint_right_sidebar() to use animation offsets
- [x] **1.4.2** Create animation coordinator
  - **Development status**: Completed
  - Created `sidebar/animation/coordinator.rs` with:
    - AnimationCoordinator for managing multiple animations ✓
    - Queue and priority system ✓
    - Animation interruption handling ✓
    - Performance optimization with frame budgets ✓
    - Support for callbacks on completion ✓
  - **Features**: Max concurrent animations, animation states, target-based management

**Phase 1 Summary**: Foundation complete. All shared UI components (cards, chips, scrollable containers, form components) and animation framework (ColorEase integration, animation coordinator) are implemented and ready for use.

---

## Phase 2: AI Sidebar UI Implementation

### 2.1 AI Sidebar Structure
- [x] **2.1.1** Create `AiSidebar` struct (`sidebar/ai_sidebar.rs`)
  - **Development status**: Completed
  - Implement Sidebar trait ✓
  - Initialize with default state ✓
  - Set up component hierarchy ✓
  - **Implementation**: Full AiSidebar implementation exists in ai_sidebar.rs
  - Implements all required UI components ✓
  - Uses Element-based rendering ✓
  - Includes populate_mock_data() for testing ✓
  - **Integration**: setup_ai_sidebar() is called in termwindow/mod.rs:844 ✓
  - **Note**: No Send+Sync issues - compiles successfully
- [x] **2.1.2** Implement sidebar header
  - **Development status**: Completed
  - "CLiBuddy AI" title ✓
  - Click on the AI icon button to close/open ✓
  - **Implementation**: render_header() method in ai_sidebar.rs
  - Shows "CLiBuddy AI" title with proper styling ✓
- [x] **2.1.3** Create activity log filtering system
  - **Development status**: Completed
  - Filters: All, Commands, Chat, Suggestions ✓
  - **Implementation**: ActivityFilter enum with all variants
  - Filter chips rendered with selection state ✓
  - Activity items filtered based on selection ✓

### 2.2 Status and Goal Components
- [x] **2.2.1** Implement status chip
  - **Development status**: Completed
  - States: Idle, Thinking, Gathering Data, Needs Approval ✓
  - Color coding and icons ✓
  - **Implementation**: AgentMode enum with all states
  - render_status_chip() with icons (○ ◐ ◑ ⚠) ✓
  - Color coding via ChipStyle variants ✓
- [x] **2.2.2** Create Current Goal card
  - **Development status**: Completed
  - Display AI-inferred or user-set goals ✓
  - Confirmation (thumbs up) for user confirmation of AI-inferred goal ✓
  - Edit mode for user modification or creation ✓
  - **Implementation**: CurrentGoal struct with all fields
  - render_current_goal() with edit mode UI ✓
  - Confirmation/edit buttons implemented ✓
  - Methods: handle_goal_confirm(), handle_goal_edit_toggle(), handle_goal_save() ✓

### 2.3 Activity Components
- [x] **2.3.1** Create Current Suggestion card
  - **Development status**: Completed
  - Display AI suggestions ✓
  - Action buttons to be displayed when AI requires (Run, Dismiss) ✓
  - Syntax formatting for e.g. commands ✓
  - **Implementation**: CurrentSuggestion struct
  - render_current_suggestion() with action buttons ✓
  - Run/Dismiss button handlers implemented ✓
- [x] **2.3.2** Implement activity log
  - **Development status**: Completed
  - Timeline view with timestamps ✓
  - Expandable command entries ✓
  - Chat history entries ✓
  - Prior suggestions entries ✓
  - Prior goals entries ✓
  - Virtual scrolling for performance ✓
  - **Implementation**: ActivityItem enum with Command/Chat/Suggestion/Goal variants
  - render_activity_log() with ScrollableContainer ✓
  - Command items show status icons and can expand to show output ✓
  - Chat messages styled differently for user vs AI ✓
- [x] **2.3.3** Create command execution component for display within timeline
  - **Development status**: Completed (part of 2.3.2)
  - Show command text with syntax highlighting ✓
  - Display execution status (success/failure) ✓
  - Display which pane ran in (if more than 1 visible) ✓
  - Expand to show output ✓

### 2.4 Chat Interface
- [ ] **2.4.1** Create chat input component
  - **Development status**: Partially complete
  - Multi-line text input ❌ (single line implemented)
  - Send button ✓
  - Keyboard shortcuts (Enter to send, Shift+Enter for newline) ✓ (Enter to send only)
  - Future version to support drag-n-drop upload of various files (images, PDFs, text files, docx) ❌
  - **Implementation**: Basic chat input in render_chat_input()
  - handle_chat_input() and handle_chat_send() methods ✓
- [ ] **2.4.2** Implement chat message display within the activity log
  - **Development status**: Partially complete
  - User vs AI message styling ✓
  - Markdown rendering support ❌ (plain text only)
  - Code block syntax highlighting ❌
  - **Implementation**: Chat messages rendered in activity log with different styling
- [x] **2.4.3** Add chat history scrolling
  - **Development status**: Completed (part of activity log scrolling)
  - Auto-scroll to bottom on new messages
  - Maintain scroll position when reviewing history

### 2.5 Config System Integration
- [ ] **2.5.1** Integrate our components into the Wezterm config system
  - Identify the proper config flow for items like colors, activation booleans, and other preferences
  - Set up the ./clibuddy/wezterm.lua file with relevant new config items, and remove the global export
  - Integrate with the wezterm config processing system

**Phase 2 Summary**: Core AI sidebar UI is mostly complete. The sidebar renders with all major components (header, status, goals, suggestions, activity log, chat). Missing features: multi-line chat input, markdown rendering, and code syntax highlighting in chat. No work done on the config system integration yet. The sidebar is integrated and can be toggled via the button.

---

## Phase 3: Terminal Integration

### 3.1 Capture Pipeline
- [ ] **3.1.1** Create PTY stream interceptor (`capture/stream_interceptor.rs`)
  - Hook into WezTerm's existing PTY handling
  - Create non-blocking stream processor
  - Handle multiple panes per tab
  - **Hook point**: Intercept in `read_from_pane_pty` in mux/src/lib.rs
- [ ] **3.1.2** Implement secret masker (`capture/secret_masker.rs`)
  - Regex patterns for common secrets (API keys, passwords)
  - Entropy-based detection
  - User-configurable patterns in settings TOML file (and via future GUI in left sidebar)
  - Performance optimization for real-time processing
  - **Library**: Consider `secrecy` crate for secure string handling
  - **No existing**: WezTerm has no secret detection currently
- [ ] **3.1.3** Create block builder (`capture/block_builder.rs`)
  - Aggregate command + output + metadata
  - Handle multi-line commands
  - Track execution timing
  - Associate with pane IDs

### 3.2 Block Detection and State Tracking
- [ ] **3.2.1** Implement shell integration detection
  - Detect existing shell integration (iTerm2, etc.)
  - Auto-inject PS0/PS1 markers if not present
  - Parse command start/end markers
  - **Existing**: Shell integration OSC sequences already supported. Need to get auto-injection of shell integration working (so users don't have to do any prep steps).
- [ ] **3.2.2** Create PTY input detector with environment tracking
  - Monitor for Enter key events
  - Detect common prompt patterns with regex
  - Handle different shell types (bash, zsh, fish)
  - Detect `cd` commands and track working directory
  - Capture `export` and environment changes
  - Monitor `source`, `module load`, `conda activate`
- [ ] **3.2.3** Implement heuristic detector
  - Timing-based detection (2+ second gaps)
  - Command completion patterns
  - Cursor position tracking
  - Fallback for edge cases
- [ ] **3.2.4** Create environment state serialization
  - Convert tracked state to JSON
  - Include in block metadata
  - Track per-pane state
  - Prepare for backend transmission

### 3.3 Data Filtering
- [ ] **3.3.1** Create junk output detector
  - Detect binary output (high non-printable ratio)
  - Identify excessive repetition
  - Flag overly long lines
  - Configurable thresholds in settings TOML file and GUI
  - **Library**: Consider `encoding_rs` for character encoding detection
- [ ] **3.3.2** Implement output summarizer
  - Filter repeating outputs intelligently
  - Preserve important error messages

### 3.4 Command Execution in User Panes
- [ ] **3.4.1** Create command injection system
  - Inject commands into user's visible terminal pane
  - Show AI commands with lower contrast/styling
  - Handle command queueing and synchronization
- [ ] **3.4.2** Implement approval prompts in terminal
  - Replace normal prompt with approval UI when needed
  - Display command to be executed
  - Show accept (Enter) / reject (Ctrl-C) options
  - Handle user response
- [ ] **3.4.3** Create execution status tracking
  - Track which commands were AI-initiated
  - Monitor execution results
  - Update AI sidebar with results

---

## Phase 4: Backend Communication

### 4.1 WebSocket Client
- [ ] **4.1.1** Create WebSocket client module (`backend/websocket_client.rs`)
  - Use tokio-tungstenite or similar
  - Handle TLS connections
  - Implement reconnection logic
  - Message queuing during disconnection
  - **Library**: `tokio-tungstenite` for async WebSocket
  - **No existing**: WezTerm has no WebSocket client
- [ ] **4.1.2** Implement authentication
  - API key management
  - Secure storage in OS keychain
  - Authentication handshake
  - Session management
  - **Library**: `keyring` crate for cross-platform credential storage

### 4.2 Message Protocol
- [ ] **4.2.1** Define message types
  - Block submission
  - Command execution requests
  - Chat messages
  - Status updates
  - Environment sync
- [ ] **4.2.2** Create serialization layer
  - JSON message formatting
  - Protocol versioning
  - Compression for large blocks
  - **Library**: `serde_json` for JSON; `flate2` for compression
- [ ] **4.2.3** Implement message handlers
  - Route incoming messages to appropriate components
  - Update UI state based on backend responses
  - Error handling and retries

### 4.3 State Management
- [ ] **4.3.1** Create AI state store (`state/ai_store.rs`)
  - Current goal state
  - Active suggestions
  - Command history
  - Chat history
  - Per-tab isolation
  - **Pattern**: Use Arc<Mutex<>> like mux does for shared state
- [ ] **4.3.2** Implement state persistence
  - Save state to disk periodically
  - Restore on reboot
  - Handle migrations
- [ ] **4.3.3** Create state synchronization
  - Keep UI in sync with backend state
  - Handle concurrent updates
  - Conflict resolution

---

## Phase 5: AI Features

### 5.1 Execution Channels
- [ ] **5.1.1** Create local shell executor (`execution/local_executor.rs`)
  - Spawn invisible PTY with same environment
  - Execute read-only commands
  - Handle timeouts and cancellation
  - Capture output for backend
  - **Existing**: Use portable_pty crate already in WezTerm
- [ ] **5.1.2** Implement SSH executor (`execution/ssh_executor.rs`)
  - Reuse WezTerm's SSH infrastructure
  - Create background SSH connections
  - Multiple exec channels per connection
  - Keep-alive handling
  - **Existing**: Use wezterm-ssh crate and SshDomain
- [ ] **5.1.3** Create execution coordinator
  - Route commands to appropriate executor
  - Handle fallbacks
  - Manage concurrent executions
  - Track execution history

### 5.2 Environment Synchronization
- [ ] **5.2.1** Implement environment replay
  - Replay captured state on new connections
  - Handle complex environments (conda, modules)
  - Error recovery
  - **Existing**: Pane trait provides environment access
- [ ] **5.2.2** Create invisible verification
  - Use OSC escape sequences for invisible commands
  - Strip from display buffer
  - Periodic state verification
  - **Existing**: OSC sequence support in escape parser
- [ ] **5.2.3** Implement drift detection
  - Compare expected vs actual state
  - Trigger re-synchronization
  - User notifications for sync issues

### 5.3 Approval System
- [ ] **5.3.1** Create approval dialog component
  - Prompt overlay for dangerous commands (in place of normal command prompt)
  - Show command details and risks
  - Accept/Reject buttons with short keys (enter, ctrl-c)
  - Remember decisions option
- [ ] **5.3.2** Implement approval rules
  - Read-only command allowlist
  - Dangerous command list
  - User-configurable
- [ ] **5.3.3** Create inline execution UI
  - Show AI commands in terminal with lower contrast
  - Indicate AI vs user commands

---

## Phase 6: Left Sidebar (Lower Priority)

### 6.1 Settings Panel
- [ ] **6.1.1** Create settings sidebar structure
  - Reuse shared sidebar infrastructure
  - Implement/use settings categories
  - Add sidebar-specific configuration (width, animation duration)
  - **Existing**: Config system with dynamic reloading. Our config changes will write to the user-facing TOML settings file.
- [ ] **6.1.2** Implement theme settings
  - Theme selector dropdown
  - Color pickers
  - Opacity slider
  - Live preview via saving to file every 500ms
  - **Existing**: These settings already in config, just adding a GUI to control them
- [ ] **6.1.3** Add AI configuration options
  - Secret masking patterns
  - Allow / high-risk lists as noted above
  - UI preferences with user-configurable secret masking (regex)

### 6.2 SSH Host Management
- [ ] **6.2.1** Create SSH host data model
  - Define `SshHostEntry` struct with id, name, hostname or IP, port, username, identity_file, order, group, icon
  - Implement persistence with TOML (or JSON if better) storage at location appropriate for each target
  - Add migration from existing ssh_config entries
- [ ] **6.2.2** Create SSH host list component
  - Display saved hosts with groups (collapsible)
  - Search/filter functionality at top of sidebar
  - Drag-and-drop reordering with visual feedback
    - DragState struct: dragging_id, start_pos, current_pos, placeholder_index
  - Status indicators (connected/disconnected) with buttons to jump to connected or open new connection in new tab
  - **Pattern**: Reuse launcher's list patterns
- [ ] **6.2.3** Implement host editor modal
  - Add/edit host dialog using overlay pattern
  - Form validation for hostname/IP and port
  - SSH key file picker
  - Buttons at bottom of sidebar plus context menu (right-click) with: Edit, Delete, Duplicate
  - **Existing**: Use termwiz widgets for forms
- [ ] **6.2.4** Create quick connect functionality for one-click opening of connection in a new tab
  - One-click connection converts `SshHostEntry` to `SshDomain`
  - Hook into existing SSH connection flow
  - Show connection progress in sidebar
  - Keyboard shortcuts: e.g. Enter to connect, Delete to remove, Ctrl+N for new
- [ ] **6.2.5** Add Lua event integration
  - `ssh-sidebar-connect` event for customization
  - `ssh-sidebar-before-save` event for validation
  - Allow lua-based customizization of connection behavior

---

## Phase 7: Polish & UX

### 7.1 Performance Optimization
- [ ] **7.1.1** Profile and optimize rendering
  - Minimize redraws
  - Implement dirty region tracking?
  - GPU acceleration where possible
  - **Library**: `pprof` for profiling; `tracy` for detailed performance analysis
- [ ] **7.1.2** Optimize capture pipeline
  - Reduce regex compilation overhead
  - Batch process updates
  - Memory usage optimization
  - **Library**: `regex` with `lazy_static` for compiled regex caching
- [ ] **7.1.3** Optimize message handling
  - Debounce UI updates
  - Efficient data structures
  - Lazy loading for history

### 7.2 User Experience
- [ ] **7.2.1** Add keyboard shortcuts
  - Toggle sidebars
  - Focus chat input
  - Navigate activity log
  - Approve/reject commands
  - **Existing**: Key binding system in config; add new KeyAssignment variants
- [ ] **7.2.2** Implement tooltips
  - Hover tooltips for buttons
  - Status explanations
  - Command details
  - **Pattern**: Use box_model hover state for tooltip triggers

### 7.3 Error Handling
- [ ] **7.3.1** Implement comprehensive error handling
  - Connection failures
  - Backend errors
  - Execution failures
  - User-friendly error messages
- [ ] **7.3.2** Add retry logic
  - Automatic reconnection
  - Command retry with backoff
  - Queue failed messages
- [ ] **7.3.3** Create error UI
  - Error notifications
  - Recovery actions
  - Debug information (when enabled)

---

## Testing Strategy

### Unit Tests
- [ ] Test secret masking with various input patterns
- [ ] Test block detection accuracy
- [ ] Test message serialization/deserialization
- [ ] Test state management operations

### Integration Tests
- [ ] Test sidebar show/hide animations
- [ ] Test capture pipeline end-to-end
- [ ] Test WebSocket connection handling
- [ ] Test command execution flows

### Performance Tests
- [ ] Benchmark capture pipeline with high throughput
- [ ] Test UI responsiveness with large activity logs
- [ ] Memory usage profiling
- [ ] WebSocket message handling under load

### Manual Testing Checklist
- [ ] Test with different shell types (bash, zsh, fish)
- [ ] Test with various terminal sizes
- [ ] Test keyboard navigation
- [ ] Test error recovery scenarios
- [ ] Test with slow/unreliable connections

---

## Success Metrics

1. **Performance**
   - Capture pipeline adds <5ms latency
   - UI remains responsive with 1000+ activity items
   - Memory usage <200MB for typical session

2. **Reliability**
   - 99.9% uptime for WebSocket connection
   - Graceful handling of all error scenarios
   - No data loss during disconnections

3. **User Experience**
   - Sidebar animations complete in <200ms
   - Commands execute within 100ms of accepting
   - Intuitive UI requiring minimal documentation

---

## Notes for Contributors

1. **Code Organization**
   - Follow WezTerm's existing patterns
   - Use Rust's type system for safety
   - Write comprehensive doc comments
   - Include examples in documentation

2. **UI Development**
   - Leverage WezTerm's existing widget system
   - Reuse components wherever possible
   - Test on all supported platforms
   - Consider accessibility

3. **Security Considerations**
   - Never log sensitive information
   - Validate all backend responses
   - Use secure storage for credentials
   - Follow other best practices

4. **Performance Guidelines**
   - Profile before optimizing
   - Use async/await appropriately
   - Minimize allocations in hot paths
   - Batch UI updates

---

## Backlog (Future Features)

### Agent Mode
- [ ] **B.1** Add Agent Mode toggle to AI sidebar header
- [ ] **B.2** Implement agent mode logic
  - Goal-driven execution
  - Step-by-step progress
  - Automatic command execution (with safety checks)
  - Pause/resume functionality
- [ ] **B.3** Create agent mode UI
  - Task list with current step display (checkmarks)
  - Stop button
  - High-risk mode toggle
- [ ] **B.4** Agent mode safety features
  - Enhanced approval rules for agent mode
  - Rollback capabilities
  - Execution limits and timeouts

This task list serves as the primary reference for all development work on CLiBuddy Terminal. Tasks should be completed in order within each phase, with Phase 1 establishing the foundation for all subsequent work. Refer to ../SPEC.md for more details (when available)