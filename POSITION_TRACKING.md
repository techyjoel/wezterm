# Exact Glyph Position Tracking Implementation Plan

## Status: Infrastructure Complete (Phases 1-4)

This document outlines the implementation plan for capturing exact glyph positions during text shaping and preserving them through the rendering pipeline to enable pixel-perfect text interaction (cursor placement and text selection) in the Wezterm sidebar.

## Completed Work

### ✅ Phase 1: Extended CachedGlyph with Cluster Information
- Added `cluster: Option<u32>` field to CachedGlyph struct
- Updated all CachedGlyph instantiations throughout the codebase
- Added `track_cluster` boolean parameter to `cached_glyph()` method
- Cluster tracking only enabled for sidebar text (None for terminal glyphs)

### ✅ Phase 1.4: Added Render Source Context
- Created `RenderSource` enum with variants: Terminal, Sidebar, TabBar
- Added `source: RenderSource` field to LayoutContext
- Updated all LayoutContext instantiations to specify their rendering source

### ✅ Phase 2: Created Position Extraction Infrastructure
- Implemented `GlyphPositionMap` struct with position tracking
- Added `from_cells()` method to extract positions from ElementCells with cluster info
- Added `hit_test()` method for converting x-coordinates to byte offsets

### ✅ Phase 3: Enhanced Line Information Tracking
- Enhanced `WrappedLine` struct with:
  - `shaped_text: String` - actual text sent to shaper
  - `shaped_offset: usize` - byte offset of shaped_text within line
- Updated `cluster_to_byte_offset()` to properly convert cluster positions to document offsets
- Updated all WrappedLine instantiations to track shaped text information

### ✅ Phase 4: Prepared Element Computation
- Added infrastructure for tracking clusters based on RenderSource
- Updated `shape_text_to_cells()` to accept track_cluster parameter
- Modified `wrap_text_with_info()` to return both cells and WrappedLine information
- All text shaping methods now conditionally track clusters based on context

## Overview (Original)

## Decision History

### Why This Approach Was Chosen

After extensive review and consideration of alternatives, we've chosen to modify `CachedGlyph` to include cluster information. This decision was made because:

1. **User Requirement**: The user explicitly wants pixel-perfect positioning, not "good enough" approximations
2. **Technical Necessity**: Without cluster information, we cannot map glyphs back to text positions
3. **Architectural Integrity**: Storing cluster data in CachedGlyph is the architecturally correct solution
4. **Industry Standard**: This is how professional text editors achieve accurate text interaction

### Alternatives Considered and Rejected

1. **Character Width Estimation** (Current approach)
   - **Why Rejected**: Inherently inaccurate for proportional fonts, leads to poor UX
   - **Problems**: Cursor drift, unreliable click positioning, impossible text selection

2. **Threading Cluster Data Alongside** 
   - **Why Rejected**: Architecturally messy, requires passing data through multiple layers
   - **Problems**: No clear ownership, complex data flow, maintenance nightmare

3. **Post-Shaping Position Extraction**
   - **Why Rejected**: By the time we can extract positions, cluster data is already lost
   - **Problems**: Cannot map glyphs back to text without cluster information

### The Fundamental Constraint

The core issue identified through multiple reviews:
- **HarfBuzz provides cluster information** during shaping (byte offset in original text)
- **CachedGlyph doesn't store it**, so this critical data is lost
- **Without cluster data**, accurate position mapping is impossible

## Implementation Plan

### Phase 1: Extend CachedGlyph with Cluster Information (Sidebar Only)

#### 1.4 Add Render Source Context

To explicitly track where rendering is happening:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RenderSource {
    Terminal,
    Sidebar,
    TabBar,
}

pub struct LayoutContext {
    // ... existing fields ...
    pub source: RenderSource,  // NEW: Explicit source tracking
}
```

This ensures we always know the rendering context and can make explicit decisions about cluster tracking.

#### 1.1 Modify CachedGlyph Structure with Conditional Cluster Storage

```rust
#[derive(Clone, Debug)]
pub struct CachedGlyph {
    pub texture: Option<Sprite>,
    pub x_offset: PixelLength,
    pub y_offset: PixelLength,
    pub bearing_x: PixelLength,
    pub bearing_y: PixelLength,
    pub scale: f64,
    pub has_color: bool,
    pub x_advance: PixelLength,
    pub y_advance: PixelLength,
    
    // NEW FIELD: Preserve cluster information ONLY for sidebar text
    // None for terminal glyphs to avoid memory overhead
    pub cluster: Option<u32>,  // Byte offset in shaped text segment
}
```

**Memory Optimization**: 
- Terminal glyphs: `cluster = None` (no memory overhead)
- Sidebar glyphs: `cluster = Some(offset)` (only ~4 bytes per sidebar glyph)
- With typical usage, this affects <1000 glyphs instead of 100,000+

**Rationale**: 
- Minimal change (adds 4 bytes per glyph)
- Preserves essential mapping information
- Natural place to store glyph-specific data

#### 1.2 Update Glyph Cache Creation with Explicit Flag

```rust
pub fn cached_glyph(
    &mut self,
    info: &GlyphInfo,
    style: &TextStyle,
    followed_by_space: bool,
    font: &Rc<LoadedFont>,
    metrics: &RenderMetrics,
    num_cells: u8,
    track_cluster: bool,  // NEW: Explicit flag to enable cluster tracking
) -> anyhow::Result<Rc<CachedGlyph>> {
    // ... existing code ...
    
    let glyph = CachedGlyph {
        // ... existing fields ...
        // Only store cluster when explicitly requested
        cluster: if track_cluster { 
            Some(info.cluster) 
        } else { 
            None 
        },
    };
    
    // ... rest of function ...
}
```

#### 1.3 Thread Flag Through Call Chain

We need to pass the `track_cluster` flag through the shaping pipeline:

```rust
// In shape_text_to_cells
fn shape_text_to_cells(
    &self,
    text: &str,
    infos: &[GlyphInfo],
    font: &Rc<LoadedFont>,
    context: &LayoutContext,
    style: &config::TextStyle,
    track_cluster: bool,  // NEW: Pass through to cached_glyph
) -> anyhow::Result<Vec<ElementCell>> {
    // ... existing code ...
    
    let glyph = glyph_cache.cached_glyph(
        info,
        style,
        followed_by_space,
        font,
        context.metrics,
        num_cells,
        track_cluster,  // Pass flag to glyph cache
    )?;
    
    // ... rest of function ...
}
```

**Update Call Sites**: 
- Terminal rendering: `track_cluster = false`
- Sidebar rendering: `track_cluster = true`

### Phase 2: Create Position Extraction Infrastructure

#### 2.1 Position Mapping Structure (wezterm-gui/src/termwindow/box_model.rs)

```rust
/// Maps text positions to screen coordinates for accurate hit testing
#[derive(Debug, Clone)]
pub struct GlyphPositionMap {
    /// For each glyph: (byte_offset, x_start, x_end)
    pub positions: Vec<(usize, f32, f32)>,
}

impl GlyphPositionMap {
    /// Create from shaped ElementCells with cluster information
    /// Note: clusters are relative to the shaped line, not the full document
    pub fn from_cells(
        cells: &[ElementCell], 
        wrapped_line: &WrappedLine,
    ) -> Self {
        let mut positions = Vec::new();
        let mut x_pos = 0.0;
        
        for cell in cells {
            match cell {
                ElementCell::Glyph(cached_glyph) => {
                    let x_start = x_pos;
                    let x_end = x_pos + cached_glyph.x_advance.get() as f32;
                    
                    // Only process glyphs with cluster information (sidebar text)
                    if let Some(cluster) = cached_glyph.cluster {
                        // Cluster is relative to shaped text, convert to document offset
                        let byte_offset = wrapped_line.cluster_to_byte_offset(cluster);
                        positions.push((byte_offset, x_start, x_end));
                    }
                    // Terminal glyphs (cluster = None) are skipped
                    
                    x_pos = x_end;
                }
                ElementCell::Sprite(sprite) => {
                    // Block drawing characters don't have text positions
                    x_pos += sprite.pixel_rect.size.width as f32;
                }
            }
        }
        
        GlyphPositionMap { positions }
    }
    
    /// Find byte offset for a given x coordinate
    pub fn hit_test(&self, x: f32) -> Option<usize> {
        // Handle click before first character
        if x < 0.0 {
            return Some(0);
        }
        
        // Find the glyph containing this x position
        for &(byte_offset, x_start, x_end) in &self.positions {
            if x >= x_start && x < x_end {
                // Determine if click is closer to start or end of glyph
                let mid = (x_start + x_end) / 2.0;
                if x < mid {
                    return Some(byte_offset);
                } else {
                    // Return position after this character
                    // (Need to handle multi-byte characters properly)
                    return self.positions.iter()
                        .find(|(offset, _, _)| *offset > byte_offset)
                        .map(|(offset, _, _)| *offset)
                        .or(Some(byte_offset + 1)); // Approximate for last char
                }
            }
        }
        
        // Click after last character
        self.positions.last().map(|(offset, _, _)| *offset + 1)
    }
}
```

### Phase 3: Track Line Information During Wrapping

#### 3.1 Enhanced Line Tracking (wezterm-gui/src/termwindow/box_model.rs)

Since wrapping happens before shaping, we need to track where each line came from:

```rust
/// Track byte offset information through text wrapping
#[derive(Debug, Clone)]
pub struct WrappedLine {
    pub text: String,
    pub byte_offset: usize,      // Where this line starts in original text
    pub shaped_text: String,     // Text actually sent to shaper (may differ)
    pub shaped_offset: usize,    // Byte offset of shaped_text within line text
    pub skipped_spaces: usize,   // Bytes skipped at line start
}

impl WrappedLine {
    /// Convert a cluster position (relative to shaped text) to document byte offset
    pub fn cluster_to_byte_offset(&self, cluster: u32) -> usize {
        // Cluster is relative to shaped_text, not original document
        // Account for: line offset + skipped spaces + shaped offset + cluster
        self.byte_offset + self.skipped_spaces + self.shaped_offset + cluster as usize
    }
}
```

### Phase 4: Extract Positions During Element Computation

#### 4.1 Modify compute_element for WrappedText (wezterm-gui/src/termwindow/box_model.rs)

```rust
ElementContent::WrappedText(text) => {
    // Wrap text into lines with byte offset tracking
    let wrapped_lines = self.wrap_text_with_offsets(text, &element.font, max_width)?;
    
    // Determine if we should track clusters based on context
    let track_cluster = match context.source {
        RenderSource::Sidebar => true,
        RenderSource::Terminal => false,
        RenderSource::TabBar => false,
    };
    
    let mut lines = Vec::new();
    let mut position_maps = Vec::new();
    
    // Shape each line and extract positions
    for wrapped_line in &wrapped_lines {
        let shaped_cells = self.shape_text_to_cells(
            &wrapped_line.text,
            &element.font,
            &style,
            track_cluster,  // Pass explicit flag
        )?;
        
        // Only extract positions for interactive sidebar elements
        if track_cluster && element.item_type.is_some() {
            let position_map = GlyphPositionMap::from_cells(
                &shaped_cells,
                &wrapped_line,
            );
            position_maps.push(position_map);
        }
        
        lines.push(shaped_cells);
    }
    
    // Store positions in UIItemType if this is chat input
    if let Some(UIItemType::ChatInput { ref mut line_positions }) = element.item_type {
        *line_positions = position_maps.iter().map(|map| {
            map.positions.iter().map(|&(offset, start, end)| {
                (start, end, offset)
            }).collect()
        }).collect();
    }
    
    Ok(ComputedElement {
        // ... existing fields ...
        content: ComputedElementContent::MultilineText {
            lines,
            line_height,
            // We don't need to store position_maps in ComputedElement
            // since we've already transferred them to UIItemType
        },
    })
}
```

### Phase 5: Update Click Handling (PENDING)

**Status**: Ready to implement. Infrastructure is in place.

#### 5.1 Use Exact Positions in Event Handler (wezterm-gui/src/sidebar/ai_sidebar.rs)

```rust
pub fn handle_chat_input_click_with_positions(
    &mut self,
    relative_x: f32,
    relative_y: f32,
    line_positions: &Vec<Vec<(f32, f32, usize)>>,
) {
    // Focus the input
    self.focus_chat_input();
    
    // Account for padding
    let text_padding = 4.0;
    let container_padding_top = 8.0;
    let adjusted_x = relative_x - text_padding;
    let adjusted_y = relative_y - container_padding_top - 2.0;
    
    if adjusted_x >= 0.0 && adjusted_y >= 0.0 {
        let line_height_with_spacing = 20.0 * 1.1; // From logs
        
        // Calculate which line was clicked
        let clicked_line = ((adjusted_y + self.chat_input.scroll_pixel_offset) 
            / line_height_with_spacing) as usize;
        
        if clicked_line < self.chat_input.lines.len() 
            && clicked_line < line_positions.len() {
            
            // Find character position using exact glyph positions
            let line_positions = &line_positions[clicked_line];
            
            // Convert byte offset to character index
            let byte_offset = find_closest_position(adjusted_x, line_positions);
            let char_index = byte_offset_to_char_index(
                &self.chat_input.lines[clicked_line], 
                byte_offset
            );
            
            // Update cursor position
            self.chat_input.cursor_line = clicked_line;
            self.chat_input.cursor_col = char_index;
        }
    }
}
```

### Phase 6: Enable Text Selection (FUTURE)

**Status**: Pending Phase 5 completion.

With exact positions, we can implement pixel-perfect text selection:

```rust
/// Track selection state with byte offsets
pub struct SelectionState {
    pub start_line: usize,
    pub start_byte: usize,
    pub end_line: usize,
    pub end_byte: usize,
    pub is_selecting: bool,
}

impl SelectionState {
    /// Get selected text using byte offsets
    pub fn get_selected_text(&self, lines: &[String]) -> Option<String> {
        if !self.is_selecting {
            return None;
        }
        
        // Extract text between byte offsets
        // Handle single-line and multi-line selection
        // ... implementation ...
    }
}
```

## Testing Strategy

1. **Unit Tests**
   - Test position extraction with various Unicode strings
   - Verify cluster-to-byte-offset mapping
   - Test hit testing edge cases
   - **Critical**: Test RTL and bidirectional text
   - **Critical**: Test emoji with ZWJ sequences
   - **Critical**: Test ligatures (fi, fl, etc.)

2. **Integration Tests**
   - Test with different fonts (monospace vs proportional)
   - Test with ligatures and complex scripts
   - Test with emoji and combining characters
   - Test with wrapped text where shaped != original

3. **Performance Tests**
   - Measure memory overhead of storing cluster data
   - Verify no regression in text rendering performance
   - Benchmark with large text blocks (10K+ characters)

## Success Metrics

1. **Cursor Positioning**: Click anywhere in text and cursor appears exactly at that position
2. **Multi-line Support**: Clicking on any line works correctly
3. **Text Selection**: Drag to select with pixel-perfect boundaries
4. **Unicode Support**: Works correctly with all Unicode text
5. **Performance**: Less than 5% overhead on text rendering

## Implementation Timeline

### Completed (Phases 1-4): ~10 hours
- ✅ Phase 1-2: Modified CachedGlyph and created infrastructure
- ✅ Phase 3-4: Implemented position tracking through pipeline

### Remaining Work:
- Phase 5: Update event handlers (2-3 hours)
- Phase 6: Implement text selection (3-4 hours)
- Testing and refinement: (4-5 hours)

Remaining Total: 9-12 hours

## Implementation Order

1. **Start with comprehensive tests** for Unicode edge cases
2. **Add performance benchmarks** before making changes
3. **Implement for ChatInput only** initially (already sidebar-only)
4. **Extend to other sidebar interactive text** after validation
5. **Never extend to terminal text** to maintain performance

## Why This Will Work

1. **Preserves Essential Data**: Cluster information is the key to accurate positioning
2. **Minimal Core Changes**: Only modifies CachedGlyph, everything else builds on top
3. **Works with Architecture**: Flows naturally through existing rendering pipeline
4. **Proven Approach**: This is how text editors achieve pixel-perfect interaction
5. **Future Proof**: Enables not just cursor positioning but also text selection and accessibility

## Known Limitations

1. **Memory Impact**: Minimal - only affects sidebar glyphs (~1000) not terminal glyphs (~100,000+)
2. **RTL Complexity**: Bidirectional text will need additional handling
3. **Performance**: Extra processing during glyph creation and position extraction (sidebar only)

These are acceptable trade-offs for achieving pixel-perfect text interaction.

## Memory Optimization Strategy

By limiting cluster tracking to sidebar text only:
- **Terminal rendering**: Unchanged, no memory overhead
- **Sidebar text**: ~4-8 bytes per glyph (Option<u32> with padding)
- **Typical impact**: <8KB for entire sidebar vs >800KB if applied to terminal
- **Result**: 100x reduction in memory overhead

## Conclusion

This approach requires modifying a core structure (CachedGlyph), but it's the right engineering decision. The alternative approaches all hit the same fundamental blocker - without cluster information, accurate positioning is impossible. By preserving this data where it naturally belongs, we enable pixel-perfect text interaction throughout the application.