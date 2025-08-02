# Text Selection Implementation Plan v3

## Overview

This document outlines the implementation approach for pixel-perfect text selection in WezTerm's AI sidebar activity log. This plan addresses architectural issues identified in previous attempts and implements industry-standard UI patterns while maintaining performance isolation from the terminal.

## Goals

Users should be able to interact in the sidebar activity log similar to how they would in Slack. They should be able to scroll up and down to see a historical log of chat messages, commands that have been run, changes to the goal, and suggestions that have been made. They should also be able to select and copy text out of the log. The current focus is to get selection and copying working right.
* Be able to select single and multi-line text in all items within the log (selecting within a single item)
* Be able to select across any format of text (plain text, markdown bold/italic, code blocks, etc)
* Be able to click-and-drag or click, shift, click to select long sections of text (even beyond the viewport height, using scrolling)
* Cmd-C (or the windows/Linux equivelant in Wezterm) should copy the selection
* Re-selecting should replace the current selection (just like how the goal card selection works)
* single-clicking anywhere in the sidebar should de-select (just like how the goal card selection works)

## Core Principles

### 1. Virtual Scrolling Architecture
- Activity log items are rendered with a buffer zone above/below the viewport
- Content shifts using negative margins for smooth scrolling
- Items dynamically enter/leave the render buffer
- **Key insight**: Item positions are viewport-relative and change during scrolling

### 2. Coordinate Systems

~~We actually have FIVE coordinate systems that must be carefully managed:~~

~~1. **Window Coordinates**: Absolute position from window origin (0,0)~~
~~2. **Viewport Coordinates**: Position relative to the visible sidebar activity log area~~
~~3. **Item Viewport Position**: Where the item appears in the viewport (changes with scroll)~~
~~4. **Item-Relative Coordinates**: Position relative to an activity item's origin (stable)~~
~~5. **Element-Relative Coordinates**: Position within nested markdown elements~~

**UPDATED (Session 12)**: After deep analysis, we're simplifying to 4 coordinate systems that match implementation reality:

1. **Window Coordinates**: Absolute position from window origin (0,0)
2. **Viewport Coordinates**: Position relative to the visible sidebar activity log area
3. **Item Coordinates**: Position relative to an activity item's origin (stable, accounts for scroll via viewport_y offset)
4. **Content Coordinates**: Position where text actually renders (inside padding/borders of elements)

**Key Changes**:
- "Item Viewport Position" is not a coordinate space but a transformation parameter (viewport_y offset)
- "Element-Relative" becomes "Content" - text renders at content_rect, not element bounds
- This fixes the root cause: positions were stored at element origin (0,0) but text renders at content position

**Why not Document Coordinates**: The activity log's dynamic nature (items added, virtual scrolling height changes) makes true document-relative coordinates impractical. Instead, we use item indices + item-relative positions for stable references.

**Note on Item Indexing**: In the implementation, activity log items are appended to a vector, meaning:
- Index 0 = oldest item
- New items get higher indices  
- This provides natural stability (existing indices don't change when new items are added)

**Critical**: Clear separation between these coordinate spaces is essential for correct hit testing and selection rendering. ~~The current codebase confuses these (at the time of writing this plan), leading to selection bugs.~~ **Session 12 Fix**: The coordinate confusion was identified as storing positions at element origin instead of content position where text actually renders.

### 3. Text Rendering Paths
- **WrappedText**: Simple text, generates GlyphWithCluster cells (working for goal/chat)
- **StyledWrappedText**: Text with style spans, currently generates only Glyph cells
- **MultilineText**: Result of wrapping, contains lines of cells
- **Children**: Markdown rendering creates nested child elements

## Architectural Analysis

### Why Current Implementation Fails

1. **Information Loss**: We use character width estimation (`calculate_char_positions`) instead of actual glyph positions from text shaping
2. **Coordinate Confusion**: Mixing viewport-relative and document-relative positions
3. **Incomplete Mouse Handling**: Selection drag breaks when cursor leaves UIItem bounds
4. **No Markdown Support**: Complex content loses position information during rendering

### UI Best Practices We're Implementing

1. **Pixel-Perfect Accuracy**: Professional text editors use exact glyph positions from text shaping
2. **Hierarchical Hit Testing**: Complex layouts require tree-based hit testing (like browsers)
3. **Mouse Capture**: Proper drag handling requires capturing mouse events
4. **Separation of Concerns**: Document model, layout model, and view model must be separate

## Why Previous Attempts Failed

### Session 27-28: Cluster Tracking
**What went wrong**: 
- Tried to adjust cluster values by adding segment offsets
- Assumed clusters should be document-absolute
- Actually, clusters are segment-relative and that's correct
- The adjustment broke the cluster→text mapping

**Lesson**: Don't modify cluster values - use them as-is from HarfBuzz

### Session 21-26: Coordinate Confusion
**What went wrong**:
- Mixed coordinate systems (viewport vs item-relative)
- Double-transformed coordinates 
- Incorrect Y position calculations
- Assumed uniform line heights (they vary due to different fonts, font sizes, and padding)

**Lesson**: Be explicit about coordinate spaces at every step and don't assume line heights nor assume they will be consistent

### Core Architectural Constraints

After thorough analysis, we've identified key constraints in WezTerm's architecture:

1. **Two-Phase Rendering**: Elements are computed first, then positions extracted after rendering
2. **Virtual Scrolling**: Items constantly enter/leave the render buffer, making positions unstable
3. **Thread Safety**: Sidebars use `Arc<Mutex<dyn Sidebar>>` requiring careful synchronization
4. **Performance Critical**: Rendering runs 60+ FPS, any overhead impacts responsiveness

## Understanding the Activity Log's Dynamic Nature

### Why Document Coordinates Don't Work

The activity log presents unique challenges for coordinate systems:

1. **Dynamic Growth**: New items are continuously added at the top
2. **Variable Heights**: Items have different heights based on content
3. **Virtual Scrolling**: Only visible items (+buffer) are rendered
4. **Height Estimation**: Total scroll height is estimated until items render

Traditional "document coordinates" assume a stable document with fixed positions. The activity log violates all these assumptions.

### The Solution: Item-Relative Positioning

Instead of trying to maintain absolute document positions, we use a two-level reference system:

1. **Item Index**: Identifies which activity log item (stable)
2. **Position in Item**: Coordinates relative to that item's origin (stable)

This approach provides stability because:
- Item indices don't change (newest = 0, older = higher indices)
- Internal item layout is stable once rendered
- Positions remain valid as new items are added
- Virtual scrolling doesn't affect item-relative positions

```rust
// Instead of absolute document position
struct DocumentPosition {
    y: f32,  // Becomes invalid as items are added!
}

// We use stable item-relative position
struct StablePosition {
    item_index: usize,       // Which item (0 = oldest)
    offset_in_item: Point2D<f32>, // Where in that item
}
```

## Final Implementation Plan

### Design Philosophy

1. **Work WITH the Architecture**: Instead of fighting WezTerm's two-phase rendering, leverage it
2. **Performance Isolation**: Minimize or prevent impact on terminal rendering performance
3. **UI Best Practices**: Implement proper mouse capture, hierarchical hit testing, and pixel-perfect accuracy
4. **Clear Separation of Concerns**: Document model, layout model, view model, and interaction model are separate

### Key Architectural Decisions

#### 1. Sidebar-Local Position Tracking
Store all position data in the sidebar to avoid impacting terminal performance:

```rust
pub struct AiSidebar {
    // Sidebar-specific position cache
    position_cache: TextPositionCache,
    // Item positions (item-relative, stable)
    item_positions: HashMap<usize, ItemPositionData>,
    // View-specific state (changes with scroll)
    viewport: ViewportInfo,
}

pub struct ItemPositionData {
    // Positions relative to item origin (0,0) - stable
    position_tree: PositionTree,
    // Current viewport Y position (changes with scroll)
    viewport_y: Option<f32>,
}
```

#### 2. Scroll-Aware Position Cache
Handle virtual scrolling properly with cache invalidation:

```rust
pub struct TextPositionCache {
    // Key by item, not by scroll state
    positions: HashMap<PositionCacheKey, Rc<PositionTree>>,
    // LRU eviction for bounded memory
    lru: LruCache<PositionCacheKey, ()>,
    max_entries: usize,
}

#[derive(Hash, Eq, PartialEq)]
pub struct PositionCacheKey {
    item_index: usize,
    content_hash: u64,  // To invalidate when content changes
    // Note: No viewport state - positions are item-relative
}
```

#### 3. Frame-Perfect Position Updates
Extract positions BEFORE rendering for accuracy:

```rust
impl AiSidebar {
    pub fn prepare_render_frame(&mut self, viewport_height: f32) {
        let visible_range = self.calculate_visible_range(viewport_height);
        
        // Pre-compute positions for visible items
        for index in visible_range.0..=visible_range.1 {
            if let Some(item) = self.activity_log.get(index) {
                self.prepare_item_positions(index, item);
            }
        }
    }
}
```

#### 4. Proper Mouse Capture
Fix the drag-outside-bounds issue:

```rust
impl TermWindow {
    selection_capture: Option<SelectionCapture>,
    
    fn start_text_selection(&mut self, target: SelectionTarget) {
        self.selection_capture = Some(SelectionCapture {
            mode: CaptureMode::TextSelection,
            initial_target: target,
        });
        self.window.capture_mouse();
    }
}
```

#### 5. Hierarchical Hit Testing
Implement proper tree-based hit testing after UIItem dispatch:

```rust
impl AiSidebar {
    fn hit_test(&self, window_point: Point2D<f32>) -> Option<HitResult> {
        // 1. Window → Viewport transformation
        let viewport_point = self.transform.window_to_viewport(WindowCoord(window_point));
        
        // 2. Find which item was hit
        for (index, item_data) in &self.item_positions {
            if let Some(item_viewport_y) = item_data.viewport_y {
                // 3. Viewport → Item transformation
                let item_point = self.transform.viewport_to_item(viewport_point, item_viewport_y);
                
                // 4. Hit test within item (item-relative coordinates)
                if let Some(position) = self.hit_test_item(&item_data.position_tree, item_point) {
                    return Some(HitResult {
                        item_index: *index,
                        position_in_item: position,
                    });
                }
            }
        }
        None
    }
}
```

#### 6. Clear Coordinate Transformations
Explicit coordinate space types and transformations:

```rust
pub struct WindowCoord(Point2D<f32>);
pub struct ViewportCoord(Point2D<f32>);
pub struct ItemCoord(Point2D<f32>);
pub struct ContentCoord(Point2D<f32>);  // UPDATED: Renamed from ElementCoord

// Selection positions use stable references
pub struct SelectionPosition {
    item_index: usize,              // Which activity item
    position_in_item: ItemPosition, // Position within that item
}

pub struct ItemPosition {
    byte_offset: usize,    // Byte offset in item's text
    affinity: TextAffinity,
}

// Selection state using stable positions
pub struct SelectionState {
    anchor: Option<SelectionPosition>,
    current: Option<SelectionPosition>,
    is_dragging: bool,
}

impl CoordinateTransform {
    fn window_to_viewport(&self, w: WindowCoord) -> ViewportCoord {
        ViewportCoord(w.0 - Vector2D::new(self.sidebar_x, self.sidebar_y))
    }
    
    fn viewport_to_item(&self, v: ViewportCoord, item_viewport_y: f32) -> ItemCoord {
        ItemCoord(v.0 - Vector2D::new(0.0, item_viewport_y))
    }
    
    fn item_to_content(&self, i: ItemCoord, content_offset: Vector2D<f32>) -> ContentCoord {
        // UPDATED: Transform to where text actually renders (content_rect)
        ContentCoord(i.0 - content_offset)
    }
}
```

## Implementation Phases

### Phase 1: Position Infrastructure (Week 1)

**Goal**: Build sidebar-local position tracking system

1. **Implement TextPositionCache** with item-based keys and LRU eviction
2. **Add PositionTree structure** for hierarchical position data
3. **Create coordinate transformation types** (WindowCoord, ViewportCoord, etc.)
4. **Test infrastructure** for position calculations

**Why**: Positions must be cached efficiently without impacting terminal performance. Item-relative positions remain stable regardless of scrolling.

### Phase 2: Position Extraction with Full Markdown Support (Week 2)

**Goal**: Extract real glyph positions during rendering for all markdown elements

1. **Conditional extraction** based on RenderSource::Sidebar
2. **Build position trees** that mirror markdown structure:
   ```rust
   pub enum ElementType {
       Paragraph { line_height: f32, margin: f32 },
       Heading { level: u8, font_size: f32, margin: f32 },
       CodeBlock { font: FontRef, padding: f32, bg_color: Color },
       ListItem { 
           indent: f32, 
           marker_width: f32,
           depth: usize,
           is_ordered: bool,
       },
       InlineText { 
           style: TextStyle,  // Regular, Bold, Italic, BoldItalic
           is_link: Option<String>,
       },
       InlineCode {
           font: FontRef,
           bg_color: Color,
           padding: f32,
       },
   }
   ```
3. **Handle all markdown elements**:
   - Paragraphs with proper spacing
   - Headings (1-6) with different fonts and margins
   - Code blocks with monospace font and padding
   - Lists (ordered/unordered) with nesting and indentation
   - Inline code with background and padding
   - Bold/italic/combined emphasis
   - Links (parsed but not visually distinct)
   - Soft/hard line breaks
4. **Unicode support** including emoji and RTL text

**Why**: Full markdown support is required from day one. The hierarchical structure naturally handles the complexity of nested elements, different fonts, and varying spacing.

### Phase 2.5: Semantic Tagging Implementation (NEW - Addresses Phase 2 Limitations)

**Goal**: Replace fragile visual property detection with proper semantic tagging

**Background**: Phase 2 was completed with visual property detection as a workaround. This phase will implement the originally intended semantic tagging approach.

1. **Add Semantic Type Enum** to `box_model.rs`:
   ```rust
   #[derive(Debug, Clone, PartialEq)]
   pub enum SemanticType {
       Heading(HeadingLevel),
       Paragraph,
       CodeBlock { language: Option<String> },
       ListItem { ordered: bool, depth: usize },
       InlineCode,
       Bold,
       Italic,
       Link { url: String },
   }
   ```

2. **Extend Core Structures**:
   ```rust
   pub struct Element {
       pub item_type: Option<UIItemType>,
       pub semantic_type: Option<SemanticType>,  // NEW
       // ... existing fields
   }
   
   pub struct ComputedElement {
       pub item_type: Option<UIItemType>,
       pub semantic_type: Option<SemanticType>,  // NEW
       // ... existing fields
   }
   ```

3. **Update Markdown Renderer** to assign semantic types:
   ```rust
   Element::new(&fonts.heading, ElementContent::WrappedText(text))
       .semantic_type(SemanticType::Heading(level))
       .colors(colors)
   ```

4. **Replace Visual Detection** in `determine_element_type()`:
   ```rust
   // Instead of checking colors and sizes:
   match computed.semantic_type {
       Some(SemanticType::CodeBlock { .. }) => {
           return Some(ElementType::CodeBlock { ... });
       }
       Some(SemanticType::Heading(level)) => {
           return Some(ElementType::Heading { level, ... });
       }
       // etc.
   }
   ```

**Why**: The current visual detection is fragile and breaks with theme/font changes. Semantic tagging is the architecturally correct solution that preserves content type information through the rendering pipeline.

**Implementation complexity**: Low - follows existing `item_type` pattern, only 4 files need modification.

### Phase 3: Mouse Event Architecture (Week 3)

**Goal**: Implement proper mouse capture and hierarchical hit testing

1. **Selection capture state** in TermWindow for drag handling
2. **Fast path** for terminal mouse events
3. **Hierarchical hit testing** with markdown awareness:
   ```rust
   fn hit_test_markdown_item(
       &self,
       position_tree: &PositionTree,
       point: ItemCoord,
   ) -> Option<ItemPosition> {
       match position_tree.element_type {
           ElementType::CodeBlock { padding, .. } => {
               // Adjust for code block padding
               let adjusted = point - Vector2D::new(padding, padding);
               self.hit_test_text(&position_tree.text_positions, adjusted)
           }
           ElementType::ListItem { indent, marker_width, .. } => {
               // Account for list indentation and marker
               let adjusted = point - Vector2D::new(indent + marker_width, 0.0);
               // Recurse into list item content
               for child in &position_tree.children {
                   if let Some(hit) = self.hit_test_markdown_item(child, adjusted) {
                       return Some(hit);
                   }
               }
           }
           ElementType::InlineCode { padding, .. } => {
               // Handle inline code padding
               let adjusted = point - Vector2D::new(padding, 0.0);
               self.hit_test_text(&position_tree.text_positions, adjusted)
           }
           // ... other element types
       }
   }
   ```
4. **Thread-safe lock protocol** to prevent deadlocks

**Why**: Markdown elements have different layouts (padding, indentation, fonts) that must be handled during hit testing. The hierarchical approach naturally handles nested structures.

### Phase 4: Selection Rendering (Week 4)

**Goal**: Render selection with proper clipping and markdown element handling

1. **Calculate selection rectangles** for markdown elements:
   ```rust
   fn calculate_markdown_selection_rects(
       &self,
       position_tree: &PositionTree,
       start: &ItemPosition,
       end: &ItemPosition,
       parent_offset: Vector2D<f32>,
   ) -> Vec<SelectionRect> {
       let mut rects = Vec::new();
       let element_offset = parent_offset + position_tree.bounds.origin;
       
       match position_tree.element_type {
           ElementType::Paragraph { line_height, .. } => {
               self.add_text_selection_rects(
                   &position_tree.text_positions,
                   start, end, element_offset, line_height,
                   &mut rects
               );
           }
           ElementType::CodeBlock { ref font, padding, bg_color } => {
               // Code blocks need special handling for background
               let block_offset = element_offset + Vector2D::new(padding, padding);
               self.add_code_block_selection_rects(
                   &position_tree.text_positions,
                   start, end, block_offset,
                   font.metrics().line_height,
                   bg_color,
                   &mut rects
               );
           }
           ElementType::InlineCode { padding, bg_color, .. } => {
               // Inline code selection includes background
               self.add_inline_code_selection_rect(
                   &position_tree.text_positions,
                   start, end, element_offset,
                   padding, bg_color,
                   &mut rects
               );
           }
           // ... handle other element types
       }
       
       // Recurse into children
       for child in &position_tree.children {
           rects.extend(self.calculate_markdown_selection_rects(
               child, start, end, element_offset
           ));
       }
       
       rects
   }
   ```
2. **Handle multi-line selection** across different markdown elements
3. **Integrate with scissor rect** clipping
4. **Respect element-specific rendering**:
   - Code blocks: include background in selection
   - Inline code: highlight with background
   - Lists: handle indentation properly
   - Headings: use correct font metrics

**Why**: Different markdown elements require different selection rendering approaches. Code needs background highlighting, lists need proper indentation handling, and headings use different font sizes.

## Critical Implementation Details

### Selection During Virtual Scrolling

When items scroll in/out of the render buffer:

```rust
impl AiSidebar {
    fn update_selection_rendering(&mut self) {
        if let Some(selection) = &self.selection_state {
            // Only render selection for visible items
            let visible_range = self.calculate_visible_range();
            
            // Check if selected item is visible
            if selection.anchor.item_index >= visible_range.0 
                && selection.anchor.item_index <= visible_range.1 {
                // Transform item-relative to viewport coordinates
                if let Some(item_data) = self.item_positions.get(&selection.anchor.item_index) {
                    if let Some(viewport_y) = item_data.viewport_y {
                        // Now we can render the selection
                        self.render_selection_rect(selection, viewport_y);
                    }
                }
            }
        }
    }
}
```

### Markdown Position Extraction

Integrate position tracking into the markdown rendering pipeline:

```rust
impl MarkdownRenderer {
    pub fn render_with_positions(
        content: &str,
        fonts: &SidebarFonts,
        max_width: f32,
    ) -> (Element, PositionTree) {
        let mut position_builder = PositionTreeBuilder::new();
        let mut element_stack = Vec::new();
        
        for event in Parser::new(content) {
            match event {
                Event::Start(tag) => match tag {
                    Tag::Heading(level) => {
                        position_builder.start_element(ElementType::Heading {
                            level,
                            font_size: fonts.heading_size(level),
                            margin: HEADING_MARGIN,
                        });
                    }
                    Tag::CodeBlock(_) => {
                        position_builder.start_element(ElementType::CodeBlock {
                            font: fonts.code.clone(),
                            padding: CODE_BLOCK_PADDING,
                            bg_color: CODE_BLOCK_BG,
                        });
                    }
                    Tag::List(start_num) => {
                        let depth = position_builder.current_list_depth();
                        position_builder.start_element(ElementType::ListItem {
                            indent: depth * LIST_INDENT,
                            marker_width: MARKER_WIDTH,
                            depth,
                            is_ordered: start_num.is_some(),
                        });
                    }
                    // ... other tags
                }
                Event::Text(text) => {
                    // Track text positions with current style
                    let style = position_builder.current_text_style();
                    position_builder.add_text(text, style);
                }
                Event::Code(code) => {
                    position_builder.add_inline_code(code, &fonts.code);
                }
                // ... other events
            }
        }
        
        (element, position_builder.build())
    }
}
```

### Thread Safety and Lock Protocol

To prevent deadlocks with `Arc<Mutex<dyn Sidebar>>`:

```rust
// Lock ordering: Always acquire in this order
// 1. sidebar_manager
// 2. specific sidebar
// 3. Never re-acquire while holding another

impl TermWindow {
    fn render_sidebar_safe(&mut self) -> Result<()> {
        let sidebar_data = {
            let manager = self.sidebar_manager.borrow();
            let sidebar = manager.right_sidebar.lock().unwrap();
            sidebar.prepare_render_data()
        }; // All locks released
        
        self.render_with_data(sidebar_data)
    }
}
```

### Handling Grapheme Clusters

For correct Unicode handling:

```rust
pub struct GraphemeBoundary {
    pub byte_offset: usize,
    pub is_line_break: bool,
    pub grapheme: String,
}

impl AiSidebar {
    fn hit_test_with_graphemes(
        &self,
        line: &VisualLine,
        x: f32,
    ) -> Option<ItemPosition> {
        // Find grapheme boundary, not byte boundary
        let boundary = self.find_grapheme_boundary(glyph.byte_offset);
        Some(ItemPosition {
            byte_offset: boundary.byte_offset,
            affinity: TextAffinity::Leading,
        })
    }
}
```

### Performance Optimizations

1. **Early exit for terminal**: Check coordinates before any sidebar logic
2. **Bounded cache**: LRU eviction prevents memory growth
3. **Lazy position extraction**: Only for visible items
4. **Binary search**: For glyph hit testing in long lines

## Why This Architecture

### Following UI Best Practices

1. **Pixel-Perfect Accuracy**: Using real glyph positions from text shaping, not estimates
2. **Proper Event Handling**: Mouse capture ensures selection works even when dragging outside bounds
3. **Hierarchical Hit Testing**: Handles complex nested layouts like markdown correctly
4. **Clear Separation of Concerns**: Document, layout, view, and interaction models are separate

### Working with WezTerm's Architecture

1. **Two-Phase Rendering**: We work with it by caching positions after computation
2. **Virtual Scrolling**: Item-relative positions remain stable as items scroll in/out of view
3. **UIItem System**: We enhance it with position references rather than replacing it
4. **Performance Critical Path**: All changes are isolated to sidebar code
5. **Activity Log Dynamics**: Using item indices + relative positions handles the log's growth
6. **Markdown Complexity**: Hierarchical position tracking handles all markdown elements

### Fixing Core Issues

1. **Information Preservation**: Glyph positions flow through the entire pipeline
2. **Coordinate Clarity**: Five-tier system with explicit transformations at each level
3. **Mouse Capture**: Fixes the fundamental drag-outside-bounds bug
4. **Memory Bounded**: LRU cache prevents unbounded growth

## Success Criteria

1. **Click Accuracy**: Clicking on any character selects exactly that position ❌ (4 char offset, improved from 5-6)
2. **Multi-line Selection**: Can select across multiple lines and paragraphs ⚠️ (works but with offset)
3. **Deselection**: Single click deselects (when not dragging) ✅
4. **Scrolling**: Selection rectangles stay aligned during scroll, including when items enter/leave the virtual scrolling buffer. ⚠️ (untested with offset issues)
5. **Copy**: Selected text copies correctly with Cmd-C/Ctrl-C ⚠️ (copies offset text)
6. **Performance**: No noticeable lag during selection ✅
7. **Vertical Alignment**: Selection appears on correct line ✅ (FIXED in Session 12)

**Current Status (Session 12):**
- 3/7 criteria fully met (deselection, performance, vertical alignment)
- Selection system largely functional with reduced horizontal offset
- User messages: 4 char offset (improved from 5-6)
- AI messages: Selection/rendering disagree by 7 chars total
- Coordinate system needs final consistency fix

## Testing Strategy

### Manual Testing
1. Test selection in each content type:
   - Goal text (single element)
   - User messages (simple text)
   - AI messages (complex markdown)
   - Code blocks within markdown
   
2. Test edge cases:
   - Select across paragraph boundaries
   - Select starting from middle of word
   - Select backwards (right to left)
   - Select while scrolling

### Debug Helpers
```rust
// Add conditional debug rendering
if cfg!(debug_assertions) && std::env::var("WEZTERM_DEBUG_SELECTION").is_ok() {
    // Render element bounds in red
    // Render line bounds in green  
    // Render click positions as dots
}
```

## Key Insights

1. **Don't fight the architecture**: Virtual scrolling means positions change and items will jump into/out of the buffer above the visible content, changing the negative margin
2. **Trust HarfBuzz clusters**: They're segment-relative and that's correct
3. **Explicit coordinates**: Always know which space you're in
4. **Incremental progress**: Get single-line working before multi-line

## Common Pitfalls to Avoid

1. **Don't assume uniform line heights** - Markdown has variable spacing
2. **Don't modify cluster values** - Use them as-is with proper context
3. **Don't mix coordinate spaces** - Transform explicitly at boundaries
4. **Don't use character counts** - Use byte offsets for Unicode safety

## Implementation Status

### Key Learnings (Session 12)

1. **Coordinate System Simplification Works**: The 4-space model (Window→Viewport→Item→Content) is cleaner and more maintainable than the original 5-space design.

2. **Content Offset Is Critical**: The difference between `content_rect` and `bounds` (padding/border) must be handled consistently throughout the pipeline.

3. **Vertical Offset Was Text Positioning**: The 0.5-line offset was fixed by properly calculating content offset, suggesting it was related to how text was positioned within its container.

4. **Coordinate Space Consistency Is Essential**: Storing positions in one space (content) but comparing in another (item) creates systematic offsets.

5. **Markdown Nesting Complexity**: Nested markdown elements require careful handling to avoid accumulating padding offsets at each level.

6. **Double Transformations Are Common**: Many offset bugs come from applying the same transformation twice in the pipeline.

### Current Issues (As of Session 12)

After implementing Phases 1-4 and extensive troubleshooting, we've made significant progress. The markdown duplication issue is mostly fixed, but coordinate system inconsistencies cause selection offsets:

#### ✅ FIXED: Text wrapping for command items
- **Issue**: Command items didn't wrap and flowed off the side
- **Root cause**: `ElementContent::WrappedText` requires explicit `max_width` constraint
- **Fix applied**: Added width calculation and `.max_width(Some(Dimension::Pixels(content_width)))` to command items
- **Status**: Working correctly

#### ✅ FIXED: Click to deselect
- **Progress**: Clicking in activity log now deselects goal card selection
- **Fix applied**: Added `ActivityLogBackground` UIItemType with click handler
- **Status**: Deselection works correctly for all items

#### ✅ FIXED: Half-width rendering (First user message only) - RESOLVED in session 5
- **Issue**: First user message renders at half width, all other items render correctly
- **Historical context**: 
  - This was caused by the prior attempt at implementing text selection in the activity log
  - Old selection code switched from `MarkdownRenderer` to `StyledWrappedText` without width constraints
- **Investigation findings**:
  - Current code DOES have `max_width` on `StyledWrappedText`, so that's not the issue
  - Width calculation appears correct: sidebar_width - margins - padding - borders - scrollbar
  - Debug logs confirm correct values: `content_width=342`, `max_width=342`, `content_rect: width=342`
  - Issue appears to be visual only, not computational
  - Box model's complex constraint logic (box_model.rs:2361-2370) may be applying unexpected limits
- **Root cause discovered (session 5)**:
  - `StyledWrappedText` calculated its content_rect as `euclid::rect(0., 0., max_width, pixel_height)`
  - `WrappedText` calculated actual line widths: `euclid::rect(0., 0., max_line_width.max(min_width), pixel_height)`
  - This forced StyledWrappedText elements to be exactly max_width wide regardless of actual content
- **Fix applied**:
  1. Updated `StyledWrappedText` content rect calculation to match `WrappedText` behavior
  2. Removed `StyledWrappedText` usage from activity log entirely:
     - User messages now use `WrappedText` 
     - AI messages continue using `MarkdownRenderer`
     - Selection will be rendered as overlays per the plan, not inline styles
  3. Removed `create_selection_spans()` function from ai_sidebar.rs
  4. Cleaned up all debug logging related to this issue
- **Status**: ✅ FULLY RESOLVED - All messages now render at proper width

#### ✅ FIXED: Position extraction for all items - RESOLVED in session 7
- **Issue**: Items 2 and 4 (AI messages with complex markdown) failed position extraction
- **Root cause**: Unbalanced start/end element calls in `extract_positions_recursively()`
  - We called `start_element()` only for Heading and CodeBlock semantic types
  - But called `end_element()` for ALL semantic types
  - This caused the root element to be lost after processing many children
- **Fix applied**: Track `started_element` flag, only call `end_element()` if we actually started one
- **Result**: ALL items now extract positions successfully:
  - Item 0 (Command): 42 positions
  - Item 1 (User chat): 218 positions  
  - Item 2 (AI chat): 998 positions ✅
  - Item 3 (User chat): 76 positions
  - Item 4 (AI chat): 291 positions ✅

#### ✅ FIXED: Coordinate transform configuration - RESOLVED in session 7
- **Issue**: Coordinate transform used `0.0` for sidebar_y instead of `activity_bounds.origin.y`
- **Impact**: Window to viewport transformation was incorrect
- **Fix applied**: Updated `update_coordinate_transform()` call to use `activity_bounds.origin.y`
- **Result**: Viewport coordinates now properly relative to activity log viewport

#### ⚠️ PARTIALLY WORKING: Selection functionality

**Current state (Session 12)**:
- ✅ Coordinate system simplified to 4 spaces (Window→Viewport→Item→Content)
- ✅ **Vertical offset FIXED**: Selection and rendering on correct line
- ⚠️ Horizontal offset reduced but not eliminated:
  - **User messages**: Selection/rendering aligned, both 4 chars right
  - **AI messages**: Selection 4 chars left, rendering 3 chars right (7 char disagreement)
- **Root cause**: Positions stored in content coordinates but compared with item coordinates

**Historical attempts and learnings**:

**Early attempts** (pre-Session 6):
- **Coordinate translation attempt**: Added UI item coordinate translation in sidebar_render.rs
  - Tried: `ui_item.x += (sidebar_x + activity_bounds.origin.x)` and y translation
  - Result: Deselection started working, but was double-translating coordinates
  - Status: Removed after discovering activity_bounds already includes sidebar_x
- **Hit testing order investigation**: Confirmed UI items checked in reverse order (last added = first checked)
  - This is correct behavior and not the issue
- **Sidebar UI item blocking hypothesis**:
  - Initially suspected Sidebar UI items were blocking activity log clicks
  - Tried moving Sidebar UI item to end of list → made it worse (blocked everything)
  - Tried removing Sidebar UI item entirely → broke scrolling
  - **Conclusion**: Sidebar items are at bottom of stack, not the issue
- **calculate_char_positions removal**: Replaced old character estimation with position extraction
  - Good change aligned with plan, but exposed the Card wrapper issue
- **Virtual scrolling negative margin hypothesis**: Suspected activity log's negative top margin wasn't accounted for

**Session 6-7**: Position extraction failures
- Fixed `find_activity_item_element()` to check current element for UIItemType first (was only checking children)
- Enhanced `extract_positions_recursively()` to process ALL children, not just semantic types
- Added child offset calculation using actual bounds for positioning
- Fixed unbalanced element start/end calls causing root element loss (called `start_element()` only for Heading/CodeBlock but `end_element()` for ALL semantic types)
- Removed incorrect `builder.end_element()` call that was discarding root element
- Fixed coordinate transform to use `activity_bounds.origin.y` instead of `0.0`
- Added `get_activity_item_bounds()` and passed bounds to `extract_activity_item_positions()`
- **Result**: All items now extract positions successfully, `in_range=true` for valid clicks

**Session 8**: Hit testing and mouse capture
- Fixed position tree bounds by passing actual item bounds to extraction
- Implemented zero-width selections: Modified `calculate_line_selection_rect()` to show 2px cursor for debugging
- Discovered mouse event routing issue: Activity log drag events bypass normal UI resolution
- Found selection rendering asymmetry: Works for simple text (item 1) but not markdown (item 2)
- Attempted mouse capture fix: Added early `MouseCapture::UI` - NO EFFECT
- **Key insight**: Issue was NOT in sidebar code but in event routing
- **Result**: `in_range=true` for all valid clicks

**Session 9**: Mouse event routing fixes
- **Original issue**: Activity log Move events had `mouse_buttons: NONE`
- **First root cause**: UI items cleared/rebuilt on paint, losing state
- **Second root cause**: Sidebar claimed all Move events when `is_dragging`
- Fixed mouse button state persistence: Added `text_selection_drag_active` flag to maintain state across UI rebuilds
- Added `MouseCapture::TextSelection` variant for proper event routing
- Modified sidebar `handle_mouse_event` to check selection type and return false for ActivityItem drags
- Discovered rendering offset: Selection appears ~6 chars right of actual position
- Found timing issue: Selection only updates on mouse out, not during drag
- Confirmed copy works: Despite visual offset, copy gets (offset) text
- **Result**: Move events properly reach handlers with correct button state

**Session 10**: 5-coordinate system investigation
- Found we had a "4.5" system - ElementCoord exists but used inconsistently
- Attempted to complete 5-coordinate system:
  - Subtracted padding during position extraction → Made offset WORSE (increased offset)
  - Added padding back during selection rendering → No improvement
  - Added content offset (border + padding) in calculate_selection_rectangles → No change
  - Reverted these changes as they were based on incorrect assumptions
- Properly exported constants: Moved CHAT_ITEM_PADDING etc to sidebar_constants.rs
- Added content offset calculation but no visible improvement
- **Session 10 state**: ~5 char horizontal offset, ~0.5 line vertical offset (NEW)
- **Key insight**: Coordinate system implementation had drifted from original plan

**Session 11**: Double padding discovery and fixes
- **Root cause identified**: Double application of padding offset (13px × 2 = 26px offset)
  - Positions extracted at (13, 13) thinking that's where text renders
  - Selection rendering adds padding again, creating double offset
- Coordinate system redesign attempt:
  - Changed position extraction to Element-relative (0,0)
  - Added proper Element→Window transformation with padding during rendering
- Markdown duplication fixes:
  - Added deduplication in PositionTreeBuilder → Partial improvement only
  - Prevented double processing in extract_positions_from_content → No effect
  - **Final fix**: Added logic to detect when Children directly contain text, skip recursive processing
  - Only process text elements once at leaf level
- **State after fixes**:
  - Item 1 (plain text): Selection/rendering aligned but BOTH 5-6 chars right + 0.5 lines down
  - Item 2 (markdown): Selection 4-6 chars LEFT of click, rendering 4 chars RIGHT + 0.5 lines down
  - Duplicate rectangles eliminated (only one set appears)
- **Key insights**:
  - 5-level coordinate system not cleanly implemented
  - Element vs Text coordinate distinction conflated
  - Different behavior for markdown suggests different code paths
  - Same byte_offset appeared with different child indices (Child 0, Child 1, etc.) in logs

**Session 12**: 4-coordinate system implementation
- Simplified from 5 to 4 coordinate spaces:
  - Merged "Item Viewport Position" into Item transformation (just viewport_y offset)
  - Renamed ElementCoord → ContentCoord to reflect actual purpose
- Fixed position extraction:
  - Calculate content_offset as `content_rect.origin - bounds.origin`
  - Pass content_offset to extract_positions_recursively instead of (0,0)
  - Store positions where text actually renders (content coordinates)
- Removed double content offset in selection rendering:
  - Positions already include offset from extraction
  - Removed redundant `content_offset` addition in `calculate_selection_rectangles`
- Fixed nested markdown padding accumulation:
  - Child positions calculated relative to parent's content_rect, not bounds
  - Prevents accumulation of padding through nested elements
  - Added text_offset calculation for Text and MultilineText content
- **Result**: Vertical offset FIXED (text on correct line), horizontal reduced from 5-6 to 4 chars





- **Session 12 critical findings and fixes**:
  1. **Simplified to 4-coordinate system** ✅:
     - Merged "Item Viewport Position" into Item transformation
     - Renamed ElementCoord → ContentCoord to reflect actual purpose
     - Updated LOG_SELECTION_2.md documentation
  2. **Fixed position extraction** ✅:
     - Positions now extracted at content_rect position where text renders
     - Added content_offset to PositionTree structure
     - Handles nested markdown elements properly
  3. **Removed double content offset** ✅:
     - Selection rendering no longer adds content_offset twice
     - Positions already include offset from extraction
  4. **Fixed nested markdown padding** ✅:
     - Child positions calculated relative to parent's content_rect
     - Prevents accumulation of padding through nested elements
  5. **Fixed vertical offset** ✅:
     - Text now renders at correct Y position
     - Likely fixed by proper content offset calculation
  6. **Remaining issue**:
     - Coordinate space inconsistency between storage and usage
     - Positions stored in content space but compared with item space
     - Causes 4-char offset in user messages, disagreement in AI messages


#### Technical Deep Dive: Current Coordinate System Status

**Position Extraction System Status** (Session 12):
- ✅ Cluster tracking enabled for sidebar (`RenderSource::Sidebar`)
- ✅ Text shaping creates `GlyphWithCluster` cells with position data
- ✅ Position extraction handles all markdown elements correctly
- ✅ Positions extracted at content_rect position (where text renders)
- ✅ Content offset properly calculated and stored in PositionTree
- ⚠️ Positions stored in content space but compared with item space

**Coordinate System Implementation** (Session 12 - 4-space model):
- ✅ Simplified to 4 coordinate types (Window→Viewport→Item→Content)
- ✅ `CoordinateTransform` struct with transformation methods
- ✅ Window to viewport transformation correct
- ✅ Viewport to item transformation uses viewport_y offset
- ❌ Item to content transformation not properly used in hit testing
- ⚠️ Coordinate space mismatch between storage and comparison

**Current Selection Issues** (Session 12):
- **Item 1 (user)**: Selection/rendering aligned, both 4 chars right
- **Item 2 (AI)**: Selection 4 chars left, rendering 3 chars right (7 char disagreement)
- **Vertical offset**: FIXED - proper content offset calculation resolved this
- **Root cause**: Positions stored in content coordinates but hit testing compares with item coordinates

#### Implementation History and Lessons Learned

**Phase 1-4 Implementation (Completed)**:
- ✅ Position infrastructure with all coordinate types and transformations
- ✅ Glyph position extraction with cluster tracking
- ✅ Semantic tagging for markdown elements
- ✅ Mouse event architecture (fully working as of Session 9)
- ⚠️ Selection rendering infrastructure (functional but with offset issues)

**Key Fixes Applied Successfully**:
1. **Text wrapping fix**: Added `max_width` constraints to command items ✅
2. **ActivityLogBackground UIItemType**: Added for deselection functionality ✅
3. **Mouse button state persistence**: Added `text_selection_drag_active` flag ✅
4. **Mouse event routing**: Fixed sidebar claiming all drag events ✅
5. **Markdown duplication**: Added logic to prevent processing same text multiple times ✅
6. **Constants centralization**: Moved all padding/margin values to sidebar_constants.rs ✅
5. **Coordinate translation attempt**: Added then removed due to double-translation issue
6. **Changed command output from Text to WrappedText**: Fixed text wrapping for command output
7. **Half-width rendering fix (session 5)**: 
   - Fixed `StyledWrappedText` content rect calculation to use actual line widths
   - Removed `StyledWrappedText` usage from activity log user messages
   - All messages now use `WrappedText` or `MarkdownRenderer` for consistent rendering

**Critical Discoveries**:
1. **Position extraction receives wrong element**: Gets entire activity log instead of individual items
2. **Card wrapper blocks position access**: UIItemType on wrapper, cluster data in nested content
3. **Markdown works differently**: Flatter structure allows position extraction to succeed
4. **Coordinate system mismatch**: UI items use relative coords, mouse events use absolute
5. **StyledWrappedText width bug (session 5)**: Content rect forced to max_width instead of actual text width

**What We've Learned**:
- The plan's architecture is sound, but implementation details matter
- Card wrapper structure wasn't anticipated in the original plan
- Position extraction needs to handle nested element structures
- Debug logging is essential for understanding complex rendering pipelines
- Content type rendering consistency is crucial - mixing StyledWrappedText with WrappedText caused width issues
- The overlay-based selection approach is correct - inline style modifications create maintenance problems


### Phase 1: Position Infrastructure ✅ COMPLETED

**What was built:**
- `position_cache.rs` created with all core data structures exactly as designed
- `TextPositionCache` with HashMap storage and Vec-based LRU tracking
  - **Performance Note**: Uses Vec with O(n) scanning for LRU, documented as acceptable for 100-entry limit
  - Added performance documentation suggesting `lru` crate or `IndexMap` for larger caches
- `PositionTree` hierarchical structure matching the plan exactly
- All 5 coordinate types implemented: `WindowCoord`, `ViewportCoord`, `ItemCoord`, `ElementCoord`, `SelectionPosition`
- `CoordinateTransform` with all transformation methods as designed
- Element types for all markdown elements (paragraphs, headings, code blocks, lists, inline elements)
- `SelectionState` structure using stable item-relative positions as planned

**Deviations from plan:**
- Removed `Rc<LoadedFont>` references from element types to satisfy `Send + Sync` requirements
- Used `Arc` instead of `Rc` for position trees throughout for thread safety
- Added `#[must_use]` annotation to `ItemPosition::validate()` for better error handling

### Phase 2: Position Extraction ✅ COMPLETED

**What was built:**
- `activity_log_positions.rs` created with position extraction framework
- `extract_activity_item_positions()` function walks computed elements as designed
- Integration point added in `sidebar_render.rs` calling extraction during rendering
- Full `PositionTreeBuilder` implementation with hierarchical structure support
- **Line height extraction**: Extracts actual line heights from `ComputedElementContent::MultilineText`
- **Position extraction**: Uses `GlyphWithCluster` data with `WrappedLine` info exactly as planned
- **Nested structure handling**: Recursively processes children elements maintaining hierarchy

**Implementation aligned with plan:**
- ✅ Conditional extraction based on `RenderSource::Sidebar` 
- ✅ Building position trees that mirror markdown structure
- ✅ Using actual line positions from `line_positions` Vec when available
- ✅ Converting cluster positions to document byte offsets via `WrappedLine::cluster_to_byte_offset()`
- ✅ Full integration with glyph position tracking from POSITION_TRACKING.md

**Font Size Improvements:**
- Heading font sizes now use multipliers relative to user's configured sidebar font size
- Constants moved to `sidebar_constants.rs` with multipliers like `H1_FONT_SIZE_MULTIPLIER: 2.0`
- All hardcoded values replaced with constants for maintainability

### Phase 2.5: Semantic Tagging ✅ COMPLETED

**What was built exactly as planned:**
- `SemanticType` enum added to `box_model.rs` with all markdown element types
- Extended `Element` and `ComputedElement` structs with `semantic_type: Option<SemanticType>`
- Added `semantic_type()` builder method to Element
- Updated markdown renderer to assign semantic types for all element types
- Modified `determine_element_type()` to check semantic type FIRST

**Additional improvements beyond plan:**
- Created `sidebar_constants.rs` to centralize ALL rendering constants
- Visual detection kept as fallback for backwards compatibility
- Debug logging added to track when fallback is used

### Unicode Safety ✅ COMPLETED

**Implemented exactly as designed:**
- `WrappedLine::cluster_to_byte_offset()` enhanced with Unicode safety documentation
- `WrappedLine::is_char_boundary()` helper method added
- `ItemPosition` enhanced with `validate()` and `to_char_index()` methods
- Validation integrated into hit testing with proper error logging
- All cluster-to-byte conversions proven safe

### Phase 3: Mouse Event Architecture ✅ COMPLETED (with clarifications)

**What was built:**
- ✅ Hierarchical hit testing in `hit_test_activity_log()` - exactly as designed
- ✅ Full 5-tier coordinate transformation system implemented correctly
- ✅ Hit testing for all element types with padding/indentation handling
- ✅ Mouse event integration in `termwindow/mouseevent.rs`
- ✅ Proper selection state management in `AiSidebar` with `SelectionState` field
- ✅ Extended `SelectionTarget::ActivityItem` to support multi-item selection with separate anchor/current indices

**Multi-item selection implementation:**
- Modified `SelectionTarget::ActivityItem` to have:
  - `anchor_index` and `anchor_byte` for selection start
  - `current_index` and `current_byte` for selection end
- Implemented `update_activity_log_selection_drag()` to handle crossing item boundaries
- `get_selected_text()` properly handles multi-item selection with text concatenation

**Mouse Capture Clarification:**
- WezTerm uses `current_mouse_capture: Option<MouseCapture>` for internal state tracking
- This is NOT OS-level mouse capture but sufficient for the use case
- The existing `MouseCapture::UI` variant properly handles sidebar selection
- No additional `SelectionCapture` struct needed - the plan's requirement is met differently

**Deviations/Clarifications from plan:**
- Mouse capture implemented via existing `MouseCapture` enum rather than new `SelectionCapture`
- This approach is simpler and consistent with WezTerm's existing patterns
- Drag-outside-bounds works correctly with the current implementation

### Phase 4: Selection Rendering ✅ COMPLETED (with limitations)

**What was built:**
- ✅ `calculate_selection_rectangles()` now uses actual glyph positions from `PositionTree`
- ✅ Added `get_item_positions()` method to access cached position data
- ✅ Position-based rectangle calculation fully implemented with coordinate transformations
- ✅ Proper integration with rendering pipeline via `render_sidebar_selection_overlays()`
- ✅ Z-index layering working correctly (z-index 14, sub-layer 0 for selection behind text)
- ✅ Visual feedback now renders correctly for single-item selection
- ✅ Fallback to character approximation when position data unavailable

**Implementation aligned with plan:**
- ✅ Uses cached `PositionTree` data exactly as designed
- ✅ Transforms item-relative coordinates to absolute screen coordinates
- ✅ Integrates with existing scissor rect clipping at z-index 14
- ✅ Selection rectangles render behind text (sub-layer 0) as intended

**Deviations from plan:**
- Added validation for selection range (start_byte > end_byte check)
- Simplified glyph boundary logic - removed buggy partial glyph detection
- Added `get_line_height()` helper method to eliminate code duplication

**Still needed:**
- ❌ Multi-item selection rectangle calculation (explicitly TODO in code)
- ⚠️ Element-specific rendering (code blocks with background, etc.) not implemented
- ⚠️ Performance optimization for position extraction (runs every frame)

**Current limitations:**
1. **Multi-item selection**: Shows debug log but no visual feedback when selecting across items
   ```rust
   } else {
       // TODO: Implement multi-item selection rendering
       log::debug!("Multi-item selection not yet implemented: {} to {}", 
                   anchor_index, current_index);
   }
   ```

2. **Unicode edge cases**: Simplified boundary detection may miss some complex Unicode scenarios

3. **Performance**: Position extraction happens on every render frame without caching

### 📝 Remaining TODOs:

1. **Multi-item selection** (Phase 4 requirement not met):
   - Need to iterate through all items from anchor_index to current_index
   - Calculate rectangles for partial selections at start/end items
   - Handle full selection for middle items

2. **Performance optimization**:
   - Position extraction runs on every render frame
   - Should implement incremental updates or smarter caching
   - Consider caching rendered selection rectangles

3. **Element-specific selection rendering**:
   - Code blocks should include background in selection
   - Inline code needs special handling
   - Lists need proper indentation handling

## Next Steps

### Critical Bug Fixes Required

1. ~~**Fix text wrapping in activity log**~~ ✅ COMPLETED
2. ~~**Fix half-width rendering issue**~~ ✅ COMPLETED (Session 5)
3. ~~**Fix position extraction failures**~~ ✅ COMPLETED (Session 7)
4. ~~**Fix position tree bounds height calculation**~~ ✅ COMPLETED (Session 8)
5. ~~**Fix mouse button state during drag**~~ ✅ COMPLETED (Session 9 Part 1)
6. ~~**Fix mouse event routing for activity log drag**~~ ✅ COMPLETED (Session 9 Part 2)

7. **Fix coordinate system to use 4-space model** (Session 12)
   - **Status**: PARTIALLY COMPLETE
   - **Completed**:
     - [x] Simplified to 4-coordinate system (Window→Viewport→Item→Content)
     - [x] Fixed position extraction to use content coordinates
     - [x] Added content_offset to PositionTree
     - [x] Updated coordinate type names (ElementCoord → ContentCoord)
     - [x] Removed double content offset in selection rendering
     - [x] Fixed nested markdown padding accumulation
     - [x] **FIXED vertical offset** - selection and rendering now on correct line
   - **Remaining Issues**:
     - [ ] Coordinate space mismatch: positions stored in content space but compared with item space
     - [ ] User messages: 4 characters right offset (selection and rendering aligned)
     - [ ] AI messages: Selection 4 left, rendering 3 right (7 char disagreement)
   - **Next Steps**:
     - Fix coordinate space consistency - either store in item space OR properly transform
     - Add proper content coordinate transformation in hit testing
     - Verify all coordinate transformations are consistent

8. **Fix remaining horizontal offset** (Session 13 TODO)
   - **Status**: Selection rectangles appear but with complex offset issues:
     - **Item 1 (simple text/user message)** (Session 11 state):
       - Selection and rendering are aligned with each other
       - BUT both are 5-6 characters right + 0.5 lines down from click position
       - This suggests consistent coordinate space error
     - **Item 2 (markdown/AI message)** (Session 11 state):
       - Selection is 4-6 characters LEFT of click position
       - Rendering is ~4 characters RIGHT + 0.5 lines down from click position
       - Selection and rendering DISAGREE - suggests different code paths
       - Duplicate rectangles mostly fixed (only one set now)
   - **Attempted fixes that FAILED**:
     - Session 10:
       - ❌ Subtracting padding during position extraction (made it worse)
       - ❌ Adding padding back during selection rendering
       - ❌ Adding content offset calculation
       - ❌ Deduplication in PositionTreeBuilder (partially worked)
     - Session 11:
       - ✅ Changed to Element-relative (0,0) extraction
       - ✅ Added proper Element→Window transformation
       - ⚠️ BUT offsets persist with different characteristics
   - **Root cause hypothesis (Session 11)**:
     - The 5-level coordinate system is not properly implemented
     - Element vs Text coordinates are conflated
     - ComputedElement bounds may be border-box but used as content-box
     - Markdown has different rendering pipeline than plain text
   - **Next steps**:
     - Complete audit of coordinate transformation pipeline
     - Verify what coordinate space ComputedElement bounds represent
     - Debug why markdown has different selection vs rendering offsets
     - Add debug visualization for coordinate boundaries
     - Consider architectural simplification to 4-level system
     - Investigate if text baseline vs line-box-top causes vertical offset

8. ~~**Fix markdown duplicate selection rectangles**~~ ✅ MOSTLY FIXED (Session 11)
   - **Status**: Duplicate rectangles eliminated, only one set appears now
   - **Root cause**: Markdown's nested structure caused same text to be processed multiple times
   - **Fix applied**: Added logic to detect when Children directly contain text, skip recursive processing
   - **Remaining issue**: Selection/rendering offsets differ (see item 7)
   - **Attempted fixes (Session 10)**:
     - Added deduplication based on byte_offset and line_index → Partial improvement only
     - Removed duplicate Children processing → No effect
   - **Evidence**: Logs show same byte_offset in multiple Child elements (Child 0, 1, 2...)
   - **Next steps**:
     - Track which elements have been processed to prevent re-extraction
     - Consider extracting positions only from leaf elements with actual text
     - May need to restructure how markdown elements are traversed

9. **Selection in command items**:
   - **Status**: No selection rectangles for command items in the activity log (e.g. item 0)
   - ** Next steps**:
     - Begin troubleshooting


### Original Phase 4 Completion (Postponed)
1. **Implement multi-item selection rendering**
   - This is blocked until single-item selection works
   - Architecture is in place, just needs the logic in the else branch
   - Will require iterating through items and handling partial selections

### Future Enhancements (Postponed)
1. **Performance optimization**
   - Cache position data to avoid recalculation every frame
   - Implement dirty tracking for position updates
   - Consider pre-computing selection rectangles

2. **Element-specific rendering**
   - Enhance selection appearance for code blocks
   - Handle inline code with proper background
   - Improve list item selection with indentation

3. **Extended features**
   - Triple-click to select entire item
   - Shift+click for range selection
   - Keyboard navigation of selection

