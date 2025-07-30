# Text Selection Implementation Plan v2

## Overview

This document outlines a new (hopefully correct) implementation approach for text selection in WezTerm's AI sidebar activity log, learning from previous attempts and building on sound UI principles.

## Core Principles

### 1. Virtual Scrolling Architecture
- Activity log items are rendered with a buffer zone above/below the viewport
- Content shifts using negative margins for smooth scrolling
- Items dynamically enter/leave the render buffer
- **Key insight**: Item positions are viewport-relative and change during scrolling

### 2. Coordinate Systems
We have (or should have) THREE coordinate systems:

1. **Window Coordinates**: Absolute position from window origin (0,0)
2. **Item-Relative Coordinates**: Position relative to an activity item's origin
3. **Viewport Coordinates**: Position relative to the visible sidebar activity log area, which is a rectangle that a scissor rect clips scrolling content when it scrolls outside of.

**Note**: "Bounds" in our codebase are generally viewport-relative positions that change as items scroll, though we should avoid using the term "bounds" since it is vague, and re-factor away from it when it is encountered.

### 3. Text Rendering Paths
- **WrappedText**: Simple text, generates GlyphWithCluster cells (working for goal/chat)
- **StyledWrappedText**: Text with style spans, currently generates only Glyph cells
- **MultilineText**: Result of wrapping, contains lines of cells
- **Children**: Markdown rendering creates nested child elements

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

## Correct Implementation Approach

### Phase 1: Fix Position Extraction for Activity Items

The current `extract_positions_recursive` approach is flawed because:
1. It flattens a hierarchical structure
2. It loses child element bounds information
3. It can't handle variable line heights

**New Approach**: Hierarchical position tracking

```rust
#[derive(Debug, Clone)]
pub struct ElementPositionInfo {
    /// Bounds of this element relative to activity item origin
    pub bounds: Rect,
    /// For MultilineText elements, position info for each line
    pub lines: Vec<LinePositionInfo>,
    /// For Children elements, nested position info
    pub children: Vec<ElementPositionInfo>,
}

impl ElementPositionInfo {
    /// Find the line at a given Y coordinate (item-relative)
    pub fn find_line_at_y(&self, y: f32) -> Option<(usize, &LinePositionInfo)> {
        // Check if Y is within our bounds
        if y < self.bounds.origin.y || y >= self.bounds.origin.y + self.bounds.size.height {
            return None;
        }
        
        // For MultilineText, find the line
        if !self.lines.is_empty() {
            let relative_y = y - self.bounds.origin.y;
            for (idx, line) in self.lines.iter().enumerate() {
                if relative_y >= line.y_position && relative_y < line.y_position + line.height {
                    return Some((idx, line));
                }
            }
        }
        
        // For Children, recurse
        for child in &self.children {
            if let Some(result) = child.find_line_at_y(y) {
                return Some(result);
            }
        }
        
        None
    }
}
```

### Phase 2: Proper Mouse Hit Testing

Current hit testing is broken because it assumes:
1. All activity items are MultilineText (they're actually Children with nested elements)
2. Line positions are activity-relative (they're element-relative)
3. Character positions are accurate (they're estimates)

**New Approach**: Two-stage hit testing

```rust
impl AiSidebar {
    /// Convert mouse position to text position within an activity item
    pub fn hit_test_activity_item(
        &self,
        index: usize,
        window_x: f32,
        window_y: f32,
    ) -> Option<TextPosition> {
        // Stage 1: Convert window → item coordinates
        let item_viewport_pos = self.item_viewport_positions.get(&index)?;
        let item_x = window_x - self.sidebar_x - item_viewport_pos.x;
        let item_y = window_y - item_viewport_pos.y;
        
        // Stage 2: Find element and line containing this position
        let position_info = self.activity_item_positions.get(&index)?;
        let (line_index, line_info) = position_info.find_line_at_y(item_y)?;
        
        // Stage 3: Find byte offset within line
        let line_relative_x = item_x - line_info.x_position;
        let byte_offset = if let Some(positions) = &line_info.glyph_positions {
            // Use exact positions if available
            find_byte_offset_from_positions(line_relative_x, positions)
        } else {
            // Fall back to estimation
            estimate_byte_offset(line_relative_x, line_info.text.as_str())
        };
        
        Some(TextPosition { line_index, byte_offset })
    }
}
```

### Phase 3: Selection State Management

Current selection state is confused about what coordinates it's storing.

**New Approach**: Store document-relative positions

```rust
#[derive(Debug, Clone)]
pub struct SelectionState {
    /// Which item is being selected
    pub target: Option<SelectionTarget>,
    /// Selection anchor and current positions
    pub anchor: Option<TextPosition>,
    pub current: Option<TextPosition>,
    /// Pre-computed selection rectangles (viewport coordinates)
    pub rectangles: Vec<SelectionRect>,
}

#[derive(Debug, Clone)]
pub struct SelectionRect {
    /// Rectangle in viewport coordinates
    pub rect: Rect,
    /// Which line this rectangle is for
    pub line_index: usize,
}

impl SelectionState {
    /// Update selection rectangles after any change
    pub fn update_rectangles(&mut self, sidebar: &AiSidebar) {
        self.rectangles.clear();
        
        let (target, anchor, current) = match (&self.target, &self.anchor, &self.current) {
            (Some(t), Some(a), Some(c)) => (t, a, c),
            _ => return,
        };
        
        // Calculate rectangles for each line in selection
        match target {
            SelectionTarget::ActivityItem { index, .. } => {
                if let Some(rects) = sidebar.calculate_activity_selection_rects(*index, anchor, current) {
                    self.rectangles = rects;
                }
            }
            SelectionTarget::Goal { .. } => {
                if let Some(rect) = sidebar.calculate_goal_selection_rect(anchor, current) {
                    self.rectangles = vec![SelectionRect { rect, line_index: 0 }];
                }
            }
            // ... other targets
        }
    }
}
```

### Phase 4: Incremental StyledWrappedText Fix

Instead of modifying cluster values, we need to track segment information:

```rust
/// Information needed to map styled text segments back to original text
#[derive(Debug, Clone)]
pub struct StyledTextMapping {
    /// For each segment: (start_byte_in_original, segment_text)
    pub segments: Vec<(usize, String)>,
}

impl TermWindow {
    /// Shape styled text preserving position information
    fn shape_styled_line_with_mapping(
        &self,
        line: &WrappedLine,
        style_spans: &[ElementStyleSpan],
        // ... other params
    ) -> (Vec<ElementCell>, StyledTextMapping) {
        let mut cells = Vec::new();
        let mut mapping = StyledTextMapping { segments: Vec::new() };
        
        for span in style_spans {
            let segment_start = span.start;
            let segment_text = &line.text[span.start..span.end];
            
            // Track this segment's position
            mapping.segments.push((segment_start, segment_text.to_string()));
            
            // Shape the segment (clusters will be segment-relative)
            let segment_cells = self.shape_text_to_cells(
                segment_text,
                // ... params
                track_cluster: true,
            )?;
            
            cells.extend(segment_cells);
        }
        
        (cells, mapping)
    }
}
```

Then during position extraction:

```rust
/// Extract positions from styled text cells using mapping
fn extract_styled_positions(
    cells: &[ElementCell],
    mapping: &StyledTextMapping,
) -> Vec<(usize, f32, f32)> {
    let mut positions = Vec::new();
    let mut x = 0.0;
    let mut cell_idx = 0;
    
    for (segment_start, segment_text) in &mapping.segments {
        let segment_len = segment_text.len();
        
        // Process cells for this segment
        while cell_idx < cells.len() {
            if let ElementCell::GlyphWithCluster { glyph, cluster } = &cells[cell_idx] {
                // Convert segment-relative cluster to document position
                let doc_offset = segment_start + *cluster as usize;
                positions.push((doc_offset, x, x + glyph.x_advance.get()));
                x += glyph.x_advance.get();
                cell_idx += 1;
                
                // Stop if we've processed all of this segment
                if *cluster as usize >= segment_len {
                    break;
                }
            }
        }
    }
    
    positions
}
```

## Implementation Order

### Step 1: Clean Up Current Code (1 hour)
1. Remove duplicate copy handling in keyevent.rs
2. Move key logging to trace level
3. Keep the `needs_invalidate` pattern for all selections
4. Revert problematic cluster offset adjustments in box_model.rs

### Step 2: Fix Coordinate Systems (2 hours)
1. Rename `activity_item_bounds` to `item_viewport_positions`
2. Document coordinate spaces clearly at each transformation
3. Add debug assertions to verify coordinate ranges

### Step 3: Implement Hierarchical Position Tracking (3 hours)
1. Create `ElementPositionInfo` structure
2. Update position extraction to preserve hierarchy
3. Implement `find_line_at_y` for nested elements

### Step 4: Fix Mouse Hit Testing (2 hours)
1. Implement two-stage hit testing
2. Handle click-outside for deselection
3. Test with nested markdown content

### Step 5: Fix Selection Rendering (2 hours)
1. Pre-compute selection rectangles on state change
2. Ensure rectangles use viewport coordinates
3. Handle scrolling updates

### Step 6: Enable Multi-line Selection (2 hours)
1. Track line ranges in selection
2. Calculate rectangles for line spans
3. Handle line wrapping edge cases

### Step 7: Fix StyledWrappedText (3 hours)
1. Implement segment mapping
2. Update position extraction
3. Test with activity log messages

## Success Criteria

1. **Click Accuracy**: Clicking on any character selects exactly that position
2. **Multi-line Selection**: Can select across multiple lines and paragraphs
3. **Deselection**: Single click deselects (when not dragging)
4. **Scrolling**: Selection rectangles stay aligned during scroll, including when items enter/leave the virtual scrolling buffer.
5. **Copy**: Selected text copies correctly with Cmd-C/Ctrl-C
6. **Performance**: No noticeable lag during selection

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