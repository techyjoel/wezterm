# Refactoring Guide for WezTerm AI Sidebar

This document provides a comprehensive refactoring plan for the WezTerm AI sidebar codebase. It supersedes LOG_SELECTION_3.md and incorporates all relevant information from that document.

## Overview

The AI sidebar implementation has grown organically during development, resulting in several files and functions that exceed maintainability thresholds. This guide categorizes refactoring tasks by priority and provides detailed implementation plans.

## Refactoring Categories

### MUST Refactor (Critical - Blocking Maintainability)

These items significantly impede code understanding, debugging, and testing. They should be addressed immediately.

### SHOULD Refactor (High Priority - Impacting Quality)

These items make the code harder to work with and should be addressed soon after critical items.

### COULD Refactor (Nice to Have - Minor Improvements)

These are optional improvements that would enhance code quality but aren't blocking issues.

---

## MUST Refactor - Critical Items

### 1. Split `ai_sidebar.rs` (5906 lines → ~5 modules)

**Current State**: Single monolithic file containing all sidebar logic including rendering, event handling, text selection, chat input, modals, and activity log management.

**Target State**: Modular architecture with clear separation of concerns.

#### Implementation Plan

Create the following module structure:
```
wezterm-gui/src/sidebar/
├── ai_sidebar.rs (core logic, ~1500 lines)
├── text_selection.rs (selection system, ~800 lines)
├── activity_log_renderer.rs (activity rendering, ~1200 lines)
├── chat_input.rs (input handling, ~700 lines)
├── modal_manager.rs (modal coordination, ~400 lines)
└── sidebar_state.rs (shared state, ~300 lines)
```

#### Module 1: `text_selection.rs`

**Extract from ai_sidebar.rs**:
- Lines 3335-3501: `hit_test_text_positions` and related hit testing
- Lines 3726-3959: Selection rendering logic
- Lines 145-273: `get_selected_text_with_positions`
- Lines 3087-3334: Position management functions
- Selection state fields from struct

**New Module Structure**:
```rust
// Note: As a core sidebar file (not component), use crate:: imports
use crate::color::LinearRgba;
use window::PixelUnit;
use crate::sidebar::position_cache::{
    CoordinateTransform, HitResult, ItemPositionData, 
    SelectionPosition, SelectionState, WindowCoord, 
    ViewportCoord, ItemCoord
};
use std::collections::HashMap;
use euclid::{Point2D, Rect};

pub struct TextSelectionManager {
    selection_state: SelectionState,
    // Store position data directly, not via Arc (position data is Send+Sync safe)
    item_positions: HashMap<usize, ItemPositionData>,
    transform: CoordinateTransform,
    // Cache bounds for hit testing without storing font references
    cached_bounds: HashMap<usize, Rect<f32, PixelUnit>>,
}

// TextSelectionManager is automatically Send+Sync since all fields are
// No unsafe impl needed

impl TextSelectionManager {
    pub fn new() -> Self { ... }
    pub fn start_selection(&mut self, position: SelectionPosition) { ... }
    pub fn update_selection(&mut self, position: SelectionPosition) { ... }
    pub fn clear_selection(&mut self) { ... }
    pub fn hit_test(&self, window_point: Point2D<f32, PixelUnit>) -> Option<HitResult> { ... }
    pub fn calculate_selection_rects(&self) -> Vec<Rect<f32, PixelUnit>> { ... }
    pub fn get_selected_text(&self, activity_log: &[crate::sidebar::ActivityItem]) -> Option<String> { ... }
}
```

**Integration Points**:
- AiSidebar will hold a `TextSelectionManager` instance
- Delegate all selection-related calls through the manager
- Share position cache via Arc for thread safety

#### Module 2: `activity_log_renderer.rs`

**Extract from ai_sidebar.rs**:
- Lines 2144-2688: Entire `render_activity_log` function
- Lines 1892-2143: Helper functions for activity rendering
- Lines 1499-1891: Item rendering functions

**Refactor `render_activity_log` into smaller functions**:
```rust
pub struct ActivityLogRenderer {
    viewport_height: f32,
    scroll_offset: f32,
    cached_heights: HashMap<usize, f32>,
}

impl ActivityLogRenderer {
    // Main entry point - orchestrates rendering
    pub fn render(&mut self, 
        activity_log: &[ActivityItem],
        fonts: &SidebarFonts,
        colors: &SidebarColors,
    ) -> Element {
        let visible_range = self.calculate_visible_range(activity_log);
        let filtered_items = self.filter_items(activity_log, visible_range);
        let rendered_items = self.render_items(filtered_items, fonts, colors);
        self.wrap_in_viewport(rendered_items)
    }

    // Broken down from original mega-function
    fn calculate_visible_range(&self, items: &[ActivityItem]) -> Range<usize> { 
        // Lines 2180-2230 of original
    }
    
    fn filter_items(&self, items: &[ActivityItem], range: Range<usize>) -> Vec<&ActivityItem> {
        // Lines 2231-2280 of original
    }
    
    fn render_items(&mut self, items: Vec<&ActivityItem>, fonts: &SidebarFonts, colors: &SidebarColors) -> Vec<Element> {
        // Lines 2281-2500 of original
        // Further break down into:
        // - render_single_item()
        // - apply_item_styling()
        // - calculate_item_position()
    }
    
    fn calculate_item_height(&mut self, item: &ActivityItem, index: usize) -> f32 {
        // Lines 2501-2550 of original
        // Includes caching logic
    }
    
    fn wrap_in_viewport(&self, items: Vec<Element>) -> Element {
        // Lines 2551-2688 of original
    }
}
```

#### Module 3: `chat_input.rs`

**Extract from ai_sidebar.rs**:
- Lines 2693-2777: `render_chat_input_text`
- Lines 2778-2841: `render_chat_input`
- Lines 2842-3048: Chat input helper functions
- Lines 4991-5141: `handle_chat_input_click_with_positions`
- Chat input state fields

**New Module Structure**:
```rust
pub struct ChatInputHandler {
    input_text: String,
    cursor_position: usize,
    selection_start: Option<usize>,
    selection_end: Option<usize>,
    glyph_positions: Vec<Vec<(f32, f32, usize)>>,
    input_scroll_offset: f32,
}

impl ChatInputHandler {
    pub fn new() -> Self { ... }
    
    pub fn render(&mut self, fonts: &SidebarFonts) -> Element {
        let wrapped_lines = self.wrap_input_text(fonts);
        let cursor_element = self.create_cursor_element();
        let selection_element = self.create_selection_element();
        self.combine_elements(wrapped_lines, cursor_element, selection_element)
    }
    
    // Break down the original 447-line function
    fn wrap_input_text(&self, fonts: &SidebarFonts) -> Vec<Element> { ... }
    fn create_cursor_element(&self) -> Option<Element> { ... }
    fn create_selection_element(&self) -> Option<Element> { ... }
    fn handle_click(&mut self, x: f32, y: f32) -> bool { ... }
    fn insert_text(&mut self, text: &str) { ... }
    fn delete_selection(&mut self) { ... }
}
```

### 2. Refactor `extract_positions_recursively_with_text_and_wraps` (502 lines → 6 functions)

**File**: `wezterm-gui/src/termwindow/render/activity_log_positions.rs`

**Current State**: Single recursive function handling all element types with complex branching.

#### Implementation Plan

**Break into type-specific handlers**:
```rust
// Main dispatcher - 50 lines max
pub fn extract_positions_recursively_with_text_and_wraps(
    computed: &ComputedElement,
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    fonts: &crate::sidebar::SidebarFonts,  // Correct type from actual code
    cumulative_byte_offset: &mut usize,
    rendered_text: &mut String,
    wrap_newlines: &mut HashSet<usize>,
    global_line_index: &mut usize,
) {
    match &computed.content {
        ComputedElementContent::Text(text) => {
            extract_text_element_positions(
                text, computed, builder, offset,
                cumulative_byte_offset, rendered_text,
                wrap_newlines, global_line_index
            );
        }
        ComputedElementContent::MultilineText { lines, .. } => {
            extract_multiline_element_positions(
                lines, computed, builder, offset,
                cumulative_byte_offset, rendered_text,
                wrap_newlines, global_line_index
            );
        }
        ComputedElementContent::Children(children) => {
            extract_children_element_positions(
                children, computed, builder, offset, fonts,
                cumulative_byte_offset, rendered_text,
                wrap_newlines, global_line_index
            );
        }
        _ => {}
    }
}

// Text element handler - 80 lines
fn extract_text_element_positions(
    text: &str,
    computed: &ComputedElement,
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    fonts: &crate::sidebar::SidebarFonts,  // Add missing parameter
    cumulative_byte_offset: &mut usize,
    rendered_text: &mut String,
    wrap_newlines: &mut HashSet<usize>,
    global_line_index: &mut usize,
) {
    // Extract lines 380-460 from original function
    // Handle single text elements with proper offset tracking
}

// Multiline text handler - 120 lines
fn extract_multiline_element_positions(
    lines: &[Vec<ElementCell>],
    computed: &ComputedElement,
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    cumulative_byte_offset: &mut usize,
    rendered_text: &mut String,
    wrap_newlines: &mut HashSet<usize>,
    global_line_index: &mut usize,
) {
    // Extract lines 461-580 from original function
    // Process wrapped lines with cluster tracking
    for (local_line_index, line) in lines.iter().enumerate() {
        extract_line_positions(
            line, *global_line_index, offset,
            cumulative_byte_offset, rendered_text
        );
        *global_line_index += 1;
    }
}

// Children handler with semantic type dispatch - 150 lines
fn extract_children_element_positions(
    children: &[ComputedElement],
    computed: &ComputedElement,
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    fonts: &FontConfiguration,
    cumulative_byte_offset: &mut usize,
    rendered_text: &mut String,
    wrap_newlines: &mut HashSet<usize>,
    global_line_index: &mut usize,
) {
    // Check for semantic types and handle specially
    if let Some(semantic_type) = &computed.semantic_type {
        handle_semantic_element(
            semantic_type, children, builder, offset,
            fonts, cumulative_byte_offset, rendered_text,
            wrap_newlines, global_line_index
        );
    } else {
        // Regular children processing
        for child in children {
            extract_positions_recursively_with_text_and_wraps(
                child, builder, child_offset, fonts,
                cumulative_byte_offset, rendered_text,
                wrap_newlines, global_line_index
            );
        }
    }
}

// Semantic element handler - 100 lines
fn handle_semantic_element(
    semantic_type: &SemanticType,
    children: &[ComputedElement],
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    fonts: &FontConfiguration,
    cumulative_byte_offset: &mut usize,
    rendered_text: &mut String,
    wrap_newlines: &mut HashSet<usize>,
    global_line_index: &mut usize,
) {
    match semantic_type {
        SemanticType::Heading(level) => {
            handle_heading_element(*level, children, ...);
        }
        SemanticType::CodeBlock { .. } => {
            handle_code_block_element(children, ...);
        }
        SemanticType::ListItem { .. } => {
            handle_list_item_element(children, ...);
        }
        _ => {
            // Default semantic handling
        }
    }
}

// Helper for code block offset adjustments - 30 lines
fn apply_code_block_offset_adjustment(
    offset: Point2D<f32, PixelUnit>
) -> Point2D<f32, PixelUnit> {
    // Existing helper from lines 291-298
    Point2D::new(
        offset.x + CODE_BLOCK_PADDING,
        offset.y + CODE_BLOCK_PADDING + CODE_BLOCK_TOP_MARGIN
    )
}
```

### 3. Refactor `render_markdown` (477 lines → 5 functions)

**File**: `wezterm-gui/src/sidebar/components/markdown.rs`

**Current State**: Monolithic function combining parsing, syntax highlighting, and element building.

#### Implementation Plan

```rust
// Main orchestrator - 80 lines
fn render_markdown(
    text: &str,
    font: &Rc<LoadedFont>,
    code_font: Option<&Rc<LoadedFont>>,
    max_width: Option<f32>,
    syntax_registry: Option<&SyntaxRegistry>,
    syntax_theme_name: Option<&str>,
    markdown_context: Option<&MarkdownContext>,
    palette: Option<&ColorPalette>,
) -> Element {
    // Parse markdown into events
    let events = parse_markdown_events(text);
    
    // Build element tree from events
    let elements = build_markdown_elements(
        events, font, code_font, max_width,
        syntax_registry, syntax_theme_name,
        markdown_context, palette
    );
    
    // Wrap in container with proper styling
    wrap_markdown_container(elements)
}

// Markdown parsing - 60 lines
fn parse_markdown_events(text: &str) -> Vec<pulldown_cmark::Event> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    
    let parser = Parser::new_ext(text, options);
    parser.collect()
}

// Element building from events - 200 lines
fn build_markdown_elements(
    events: Vec<pulldown_cmark::Event>,
    font: &Rc<LoadedFont>,
    code_font: Option<&Rc<LoadedFont>>,
    max_width: Option<f32>,
    syntax_registry: Option<&SyntaxRegistry>,
    syntax_theme_name: Option<&str>,
    markdown_context: Option<&MarkdownContext>,
    palette: Option<&ColorPalette>,
) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut current_paragraph = Vec::new();
    let mut in_code_block = false;
    let mut code_block_content = String::new();
    
    for event in events {
        match event {
            Event::Start(tag) => {
                handle_tag_start(tag, &mut current_paragraph, &mut in_code_block);
            }
            Event::End(tag) => {
                handle_tag_end(
                    tag, &mut elements, &mut current_paragraph,
                    &mut in_code_block, &mut code_block_content,
                    font, code_font, syntax_registry, palette
                );
            }
            Event::Text(text) => {
                handle_text_event(text, &mut current_paragraph, &mut code_block_content, in_code_block);
            }
            Event::Code(code) => {
                handle_inline_code(code, &mut current_paragraph, code_font);
            }
            // ... other events
        }
    }
    
    elements
}

// Syntax highlighting for code blocks - 100 lines
fn apply_syntax_highlighting(
    code: &str,
    language: Option<&str>,
    syntax_registry: Option<&SyntaxRegistry>,
    theme_name: Option<&str>,
    palette: Option<&ColorPalette>,
) -> Vec<StyleSpan> {
    // Extract from current highlight_code_block function
    // Lines 961-1060 of original
}

// Style span building - 80 lines
fn build_style_spans(
    text: &str,
    inline_styles: &[InlineStyle],
    font: &Rc<LoadedFont>,
    palette: Option<&ColorPalette>,
) -> Vec<StyleSpan> {
    // Build style spans from inline markdown styles
    // Handle bold, italic, code, links
}
```

---

## SHOULD Refactor - High Priority Items

### 1. Split `mouse_event_terminal` (418 lines → 5 functions)

**File**: `wezterm-gui/src/termwindow/mouseevent.rs`

**Current State**: All terminal mouse event handling in one function with deep nesting.

#### Implementation Plan

```rust
// Main dispatcher - 80 lines
fn mouse_event_terminal(
    &mut self,
    mut event: MouseEvent,
    context: &dyn WindowOps,
) -> Result<(), Error> {
    // Determine event type and dispatch
    match event.kind {
        MouseEventKind::Press(button) => {
            self.handle_mouse_press(event, button, context)
        }
        MouseEventKind::Release(button) => {
            self.handle_mouse_release(event, button, context)
        }
        MouseEventKind::Move => {
            self.handle_mouse_move(event, context)
        }
        MouseEventKind::VertWheel(amount) => {
            self.handle_mouse_wheel(event, amount, context)
        }
        MouseEventKind::HorzWheel(amount) => {
            self.handle_horizontal_wheel(event, amount, context)
        }
    }
}

// Mouse press handler - 100 lines
fn handle_mouse_press(
    &mut self,
    event: MouseEvent,
    button: MouseButton,
    context: &dyn WindowOps,
) -> Result<(), Error> {
    // Start selection, handle clicks, context menu
}

// Mouse move handler - 80 lines
fn handle_mouse_move(
    &mut self,
    event: MouseEvent,
    context: &dyn WindowOps,
) -> Result<(), Error> {
    // Update selection, handle dragging
}

// Mouse wheel handler - 80 lines
fn handle_mouse_wheel(
    &mut self,
    event: MouseEvent,
    amount: i16,
    context: &dyn WindowOps,
) -> Result<(), Error> {
    // Scrolling logic, viewport updates
}

// Selection management - 80 lines
fn update_terminal_selection(
    &mut self,
    start: Point,
    end: Point,
    selection_mode: SelectionMode,
) -> Result<(), Error> {
    // Selection rectangle calculation and updates
}
```

### 2. Modularize `box_model.rs` (3800 lines → 3 modules)

**Current State**: Mixed responsibilities - layout calculation, text wrapping, and rendering. 
**Critical Note**: The `shape_line_with_styles` function alone is ~1310 lines (2168-3478), not 358 as initially estimated.

#### Implementation Plan

Create module structure:
```
wezterm-gui/src/termwindow/box_model/
├── mod.rs (core types and traits, ~800 lines)
├── layout.rs (layout calculation, ~1200 lines)
├── wrapping.rs (text wrapping logic, ~1000 lines)
└── shaping.rs (text shaping, ~800 lines)
```

**Module: wrapping.rs**
- Move `wrap_text_into_lines` (288 lines)
- Move `wrap_text_with_estimates` (200 lines)
- Move `wrap_styled_text` (150 lines)
- Extract wrapping helper functions

**Module: shaping.rs**
- Move `shape_line_with_styles` (~1310 lines!) and break it down into multiple functions:
  ```rust
  pub fn shape_line_with_styles(...) -> Result<Vec<ElementCell>, Error> {
      // This needs aggressive refactoring - currently 1310 lines!
      let font_variants = resolve_font_variants(...);  // ~300 lines
      let style_spans = resolve_style_spans(...);       // ~200 lines
      let shaped_clusters = shape_text_clusters(...);   // ~400 lines
      let wrapped_cells = apply_wrapping_rules(...);    // ~200 lines
      let final_cells = apply_style_adjustments(...);   // ~210 lines
      Ok(final_cells)
  }
  ```

**Module: layout.rs**
- Move `compute_element` functions
- Move dimension calculation logic
- Move position calculation helpers

### 3. Split `termwindow/mod.rs` (3948 lines → 4 modules)

**Current State**: Core window module has become a catch-all for various functionalities.

#### Implementation Plan

```
wezterm-gui/src/termwindow/
├── mod.rs (core TermWindow struct, ~1000 lines)
├── window_events.rs (event handling, ~1000 lines)
├── window_state.rs (state management, ~1000 lines)
└── window_render.rs (rendering coordination, ~1000 lines)
```

### 4. Extract Performance Optimizations

**Based on LOG_SELECTION_3.md Future Work**

#### Dirty Tracking Implementation

```rust
// In ai_sidebar.rs or new sidebar_state.rs
pub struct PerformanceOptimizer {
    positions_dirty: bool,
    last_viewport_height: f32,
    last_scroll_offset: f32,
    cached_item_heights: HashMap<usize, f32>,
    position_cache_generation: u64,
}

impl PerformanceOptimizer {
    pub fn mark_dirty(&mut self) {
        self.positions_dirty = true;
        self.position_cache_generation += 1;
    }
    
    pub fn needs_update(&self, viewport_height: f32, scroll_offset: f32) -> bool {
        self.positions_dirty ||
        (viewport_height - self.last_viewport_height).abs() > 0.1 ||
        (scroll_offset - self.last_scroll_offset).abs() > 0.1
    }
    
    pub fn update_complete(&mut self, viewport_height: f32, scroll_offset: f32) {
        self.positions_dirty = false;
        self.last_viewport_height = viewport_height;
        self.last_scroll_offset = scroll_offset;
    }
}
```

### 5. Consolidate Constants

**Remove all magic numbers as identified in LOG_SELECTION_3.md**

Extend `sidebar_constants.rs`:
```rust
// Text rendering
pub const LINE_SPACING_MULTIPLIER: f32 = 1.1;
pub const DEFAULT_LINE_HEIGHT: f32 = 20.0;
pub const MIN_SELECTION_WIDTH: f32 = 2.0;

// Z-index layers (from rendering-pipeline.md)
pub const BACKGROUND_Z_INDEX: u8 = 0;
pub const SELECTION_Z_INDEX: u8 = 13;
pub const ACTIVITY_LOG_Z_INDEX: u8 = 14;
pub const MODAL_BACKDROP_Z_INDEX: u8 = 20;
pub const MODAL_CONTENT_Z_INDEX: u8 = 21;

// Colors
pub const SELECTION_COLOR: LinearRgba = LinearRgba { 
    r: 0.3, g: 0.5, b: 0.8, a: 0.3 
};

// Spacing (already exist, verify complete)
pub const SIDEBAR_PADDING: f32 = 16.0;
pub const ITEM_SPACING: f32 = 12.0;
pub const CODE_BLOCK_PADDING: f32 = 12.0;
pub const CODE_BLOCK_TOP_MARGIN: f32 = 8.0;
```

---

## COULD Refactor - Optional Improvements

### 1. Complete Text Selection System Architecture

From LOG_SELECTION_3.md, these are working but could be improved:

#### Position Tree Caching
```rust
// Optional: Cache position trees for unchanged content
struct PositionTreeCache {
    cache: HashMap<ItemId, (u64, Arc<PositionTree>)>,
    generation: u64,
}

impl PositionTreeCache {
    pub fn get(&self, id: ItemId) -> Option<Arc<PositionTree>> {
        self.cache.get(&id)
            .filter(|(gen, _)| *gen == self.generation)
            .map(|(_, tree)| Arc::clone(tree))
    }
}
```

#### Lazy Position Evaluation
- Only calculate positions for visible items until selection starts
- Defer position extraction for off-screen content

### 2. Add Comprehensive Testing

**Unit Tests Needed**:
- Line selection edge cases
- Partial line selection
- Multi-element selection
- Artificial newline filtering
- Coordinate transformations

**Integration Tests Needed**:
- Selection across different markdown elements
- Performance benchmarks with large documents

### 3. Improve Type Safety

```rust
// Consider stronger typing for offsets
#[derive(Debug, Clone, Copy)]
struct ByteOffset(usize);

#[derive(Debug, Clone, Copy)]
struct CharacterIndex(usize);

#[derive(Debug, Clone, Copy)]
struct LineIndex(usize);

// Prevent mixing different offset types at compile time
impl ByteOffset {
    pub fn to_char_index(&self, text: &str) -> CharacterIndex {
        // Proper UTF-8 aware conversion
    }
}
```

### 4. Extract Smaller Components

**From forms.rs (2069 lines)**:
- TextInput component (~400 lines)
- Dropdown component (~300 lines)
- Checkbox component (~200 lines)
- Button component (~200 lines)
- Form validation logic (~300 lines)

### 5. Improve Error Handling

Replace panics with proper error propagation:
```rust
// Instead of:
let position = self.positions.get(&index).unwrap();

// Use:
let position = self.positions.get(&index)
    .ok_or_else(|| Error::PositionNotFound(index))?;
```

### 6. Document Internal APIs

Add rustdoc comments for complex internal functions:
```rust
/// Extracts text positions from a computed element tree.
/// 
/// This function recursively traverses the element tree and builds
/// a position tree that maps byte offsets to screen coordinates.
/// 
/// # Arguments
/// * `computed` - The root computed element
/// * `builder` - Position tree builder for accumulating results
/// * `offset` - Current rendering offset in screen coordinates
/// 
/// # Position Tracking
/// Uses global line indexing to ensure unique line numbers across
/// all elements (fixes multi-element selection bug from Session 31).
pub fn extract_positions_recursively_with_text_and_wraps(...) {
    // Implementation
}
```

---

## Performance Considerations

### Position Extraction Hot Path

**Critical**: The position extraction runs during every render frame for visible items. When refactoring `extract_positions_recursively_with_text_and_wraps`:

1. **Minimize function call overhead** - Consider inlining small helper functions
2. **Preserve single-pass extraction** - Don't add multiple traversals
3. **Benchmark before/after** - Ensure no performance regression
4. **Keep mutable reference passing** - Avoid cloning large data structures

### Virtual Scrolling Height Caching

**Critical**: The height caching prevents viewport-sized jumps. When splitting activity log renderer:

```rust
// MUST preserve this pattern from sidebar-patterns.md
if item_bottom > 0.0 && item_top < viewport_height {
    // Cache ANY visible item, not just fully visible
    self.cached_item_heights.insert(index, height);
}
```

### Rendering Pipeline Constraints

**Z-Index Sub-layers**: The rendering pipeline has hard constraints:
- Only sub-layers 0, 1, 2 are valid
- Using sub-layer > 2 causes panic in HeapQuadAllocator
- Modules must coordinate z-index usage to avoid conflicts

## Migration Strategy

**Important**: Consider incremental refactoring for the most critical functions. The `shape_line_with_styles` function at 1310 lines and `mouse_event_terminal` at 418 lines may benefit from gradual extraction of helper functions rather than aggressive splitting.

### Phase 1: Critical Refactoring (1-2 weeks)
1. Split `ai_sidebar.rs` into modules
2. Refactor `extract_positions_recursively_with_text_and_wraps`
3. Break down `render_markdown`

### Phase 2: High Priority (1 week)
1. Split `mouse_event_terminal`
2. Modularize `box_model.rs`
3. Add performance optimizations

### Phase 3: Code Quality (1 week)
1. Consolidate constants
2. Add unit tests for refactored code
3. Document internal APIs

### Phase 4: Optional Improvements (ongoing)
1. Implement position caching
2. Improve type safety
3. Extract remaining small components

## Testing Strategy

For each refactored module:
1. **Preserve existing behavior**: Ensure all existing tests pass
2. **Add unit tests**: Cover edge cases in newly extracted functions
3. **Performance testing**: Verify no performance regressions
4. **Integration testing**: Ensure modules work together correctly

## Success Criteria

1. **No file exceeds 1500 lines** (currently 6 files over 1300 lines)
2. **No function exceeds 150 lines** (currently 7 functions over 300 lines)
3. **All magic numbers replaced with constants**
4. **Core functionality extracted into testable modules**
5. **Performance maintained or improved**
6. **Code coverage increased by at least 20%**

## Notes from Previous Implementation (LOG_SELECTION_3.md)

### Current Working Features
The text selection system is **fully functional** as of Session 31:
- Single element selection: ✅ Perfect visual and copy
- Multi-element selection within item: ✅ Fixed with global line indexing
- Multi-item selection: ✅ Works across different activity items
- Wrapped text: ✅ No artificial newlines in copied text
- Syntax highlighting: ✅ Consistent colors
- Code block multi-line selection: ✅ Fixed

### Key Technical Achievements
1. **Global Line Index Tracking**: Fixed multi-element selection by maintaining line indices across all elements
2. **Artificial Newline Filtering**: Track and skip wrap newlines when copying text
3. **Proper Coordinate Systems**: 3-tier system (Window → Viewport → Item) with correct transformations
4. **Code Block Offset Handling**: Account for 12px padding + 8px top margin

### Important Implementation Details

When refactoring, preserve these critical fixes:

1. **Global Line Indexing** (Session 31 fix):
```rust
// Each visual line must get a unique index across ALL elements
let mut global_line_index = 0usize;
for element in elements {
    for line in element.lines {
        process_line(line, global_line_index);
        global_line_index += 1;  // Never reset!
    }
}
```

2. **Wrap Newline Tracking** (Session 30 fix):
```rust
// Track artificial newlines added for wrapping
wrap_newlines.insert(byte_offset);
// Skip them when extracting selected text
if !wrap_newlines.contains(&byte_offset) {
    result.push(char);
}
```

3. **Code Block Offset Adjustment**:
```rust
// Code blocks need special offset handling
offset.x += CODE_BLOCK_PADDING;  // 12px
offset.y += CODE_BLOCK_PADDING + CODE_BLOCK_TOP_MARGIN;  // 12px + 8px
```

## References

- **Architecture Documentation**: `dev-docs/architecture.md`
- **Rendering Pipeline**: `dev-docs/rendering-pipeline.md`
- **Sidebar Patterns**: `dev-docs/sidebar-patterns.md`
- **Text Layout**: `dev-docs/text-layout.md`
- **Previous Implementation Log**: `LOG_SELECTION_2.md` and `LOG_SELECTION_3.md` (now deprecated)