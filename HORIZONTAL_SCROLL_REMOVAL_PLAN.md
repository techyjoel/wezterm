# Horizontal Scrolling Removal and Line Wrapping Implementation Plan

## Executive Summary

This plan outlines the systematic removal of all horizontal scrolling code and implementation of line wrapping for code blocks in the CLiBuddy Terminal sidebar. The goal is to restore stability, fix visual overflow issues, and prepare for text selection implementation.

**Background**: After extensive attempts to implement horizontal scrolling (including GPU scissor rects, manual clipping, and explicit clip bounds), fundamental architectural limitations in WezTerm's batched rendering pipeline make proper visual clipping impossible without major refactoring. Line wrapping is the pragmatic solution that works within existing architecture.

## Current State Analysis

### Working Features to Preserve
- ✅ Modal system (overlay framework)
- ✅ Text wrapping for non-code content (paragraphs, chat messages)
- ✅ Vertical scrolling in activity log and modals
- ✅ Syntax highlighting in code blocks
- ✅ Copy button functionality for code blocks

### Broken/Problematic Features to Remove
- ❌ Horizontal scrolling mechanics (scrollbar, shift+wheel)
- ❌ Manual clipping attempts in render_element
- ❌ Horizontal scroll state management
- ❌ Code block viewport containers with negative margins
- ❌ Clip bounds infrastructure

### Potential Bugs Introduced
1. Manual clipping code may skip rendering valid content
2. Clip bounds calculations could affect element sizing
3. Horizontal scroll containers may cause layout issues
4. Z-index assignments (50-69) for code blocks may interfere with other UI

## Phase 1: Code Removal and Cleanup

### 1.1 Remove Horizontal Scrolling Module
**File**: `wezterm-gui/src/sidebar/components/horizontal_scroll.rs`
- **Action**: Delete entire file
- **Impact**: Removes HorizontalScrollContainer and all scrolling mechanics

### 1.2 Clean Up Markdown Renderer
**File**: `wezterm-gui/src/sidebar/components/markdown.rs`

**Remove**:
- `CodeBlockContainer` struct
- `code_block_registry` and `temp_code_registry` 
- `HorizontalScrollContainer` usage in `render_code_block`
- Horizontal scroll width calculations
- Content width measurement logic

**Restore**:
- Simple code block rendering without viewport
- Direct rendering of syntax-highlighted lines

### 1.3 Remove Manual Clipping from render_element
**File**: `wezterm-gui/src/termwindow/box_model.rs`

**Remove** (lines ~1308-1409):
- Clip bounds checking for sprites and glyphs
- Partial clipping calculations
- Texture coordinate adjustments
- All references to `element.clip_bounds` in rendering

**Keep**:
- Basic sprite and glyph rendering logic
- Original positioning calculations

### 1.4 Remove Clip Bounds Infrastructure
**Files**: Multiple

**Remove**:
- `clip_bounds` field from `Element` and `ComputedElement`
- `ClipBounds` enum if no longer used
- `with_clip_bounds()` builder method
- Clip bounds transformation in `compute_element`

### 1.5 Clean Up Activity Log
**File**: `wezterm-gui/src/sidebar/components/activity_log.rs`

**Remove**:
- References to code block registry
- Horizontal scroll state management
- Any horizontal scroll-related imports

### 1.6 Clean Up Modal System
**File**: `wezterm-gui/src/sidebar/components/modal/suggestion_modal.rs`

**Remove**:
- Code block registry references
- Horizontal scroll container usage
- Simplify markdown rendering

### 1.7 Reset Z-Index Assignments
**Action**: Search for z-index values 50-69 and restore to default layering

### 1.8 Search for Missed References
**Action**: Global search and cleanup
- Search for "horizontal_scroll" across entire codebase
- Search for "code_block_registry" 
- Search for "clip_bounds" usage
- Search for "HorizontalScrollContainer" imports
- Update or remove any related tests
- Check for stale imports
- Remove any shift+wheel event handlers specific to horizontal scrolling

## Phase 2: Implement Line Wrapping for Code Blocks

### 2.1 Extend WrappedText for Code Preservation
**Challenge**: Code blocks need to preserve exact spacing and indentation

**Solution**: Since WRAPPING.md shows WrappedText is already implemented and working for other content, we should reuse it with modifications for code blocks. Instead of creating a new WrappedCode variant, enhance the existing wrap_text implementation to support a "preserve whitespace" mode:
```rust
fn wrap_text_preserve_whitespace(
    &self,
    text: &str,
    font: &Rc<LoadedFont>,
    max_width: f32,
    context: &LayoutContext,
    style: &config::TextStyle,
) -> anyhow::Result<Vec<Vec<ElementCell>>> {
    // Split by newlines first to preserve line structure
    let lines: Vec<&str> = text.lines().collect();
    let mut wrapped_lines = Vec::new();
    
    for line in lines {
        if line.is_empty() {
            // Preserve empty lines
            wrapped_lines.push(Vec::new());
            continue;
        }
        
        // Measure line width
        let line_width = self.calculate_line_width(line, font, context, style)?;
        
        if line_width <= max_width {
            // Line fits, render as-is
            let cells = self.shape_line_to_cells(line, font, context, style)?;
            wrapped_lines.push(cells);
        } else {
            // Line too long, wrap at character boundaries
            // preserving leading whitespace
            let wrapped = self.wrap_long_code_line(line, font, max_width, context, style)?;
            wrapped_lines.extend(wrapped);
        }
    }
    
    Ok(wrapped_lines)
}
```

### 2.2 Create Specialized Code Block Element
**File**: `wezterm-gui/src/sidebar/components/markdown.rs`

```rust
fn render_wrapped_code_block(
    &self,
    code: &str,
    syntax: Option<&str>,
    fonts: &SidebarFonts,
    max_width: f32,
) -> Vec<Element> {
    let mut elements = vec![];
    
    // Add copy button (already working)
    elements.push(self.create_copy_button(code));
    
    // Create container with proper background
    let container = Element::new()
        .with_background(self.code_background_color())
        .with_padding(8.0)
        .with_content(ElementContent::Children(vec![
            // Use new wrapped code element
            Element::new(
                &fonts.code,
                ElementContent::WrappedCode {
                    text: code.to_string(),
                    syntax: syntax.map(String::from),
                    preserve_whitespace: true,
                }
            )
        ]));
    
    elements.push(container);
    elements
}
```

### 2.3 Consider Element Structure Options
**File**: `wezterm-gui/src/termwindow/box_model.rs`

**Option A**: Reuse existing WrappedText with special handling:
```rust
// Use existing WrappedText but with code font and special wrapping logic
Element::new(&fonts.code, ElementContent::WrappedText(code))
```

**Option B**: Add parameters to existing WrappedText (if needed):
```rust
// Only if we need to distinguish code from regular wrapped text
// Consider if the font alone is sufficient differentiation
```

**Decision**: Start with Option A. The code font and context should be sufficient to handle code-specific wrapping needs without modifying the ElementContent enum.

### 2.4 Handle Syntax Highlighting with Wrapping
**Challenge**: Maintain syntax colors across wrapped lines

**Solution**: 
1. Apply syntax highlighting first to get styled runs
2. Wrap styled runs while preserving style information
3. Render with appropriate colors

## Phase 3: Testing and Bug Fixes

### 3.1 Visual Testing Checklist
- [ ] Code blocks wrap long lines without overflow
- [ ] Indentation preserved in wrapped code
- [ ] Syntax highlighting maintained across wraps
- [ ] Copy button still copies full unwrapped text
- [ ] No visual artifacts from removed clipping code
- [ ] Modal code blocks wrap properly
- [ ] Activity log renders correctly without horizontal scroll

### 3.2 Functional Testing
- [ ] Verify no panics or crashes
- [ ] Check memory usage (no leaks from removed state)
- [ ] Ensure scrollbar calculations correct
- [ ] Test with various code block sizes
- [ ] Test window resizing behavior

### 3.3 Expected Bug Fixes
1. **Visual Overflow**: Should be completely resolved
2. **RefCell Panics**: Should no longer occur
3. **Z-Index Conflicts**: Restored to standard layering
4. **Layout Issues**: Simplified structure should be more stable

## Phase 4: Preparation for Text Selection

### 4.1 Design Considerations
With line wrapping implemented, text selection becomes simpler:
- Each wrapped line has known bounds
- No horizontal scrolling complexity
- Can build character position map during wrapping

### 4.2 Selection Mapping Strategy
```rust
struct SelectionMap {
    lines: Vec<LineSelectionInfo>,
}

struct LineSelectionInfo {
    screen_bounds: RectF,
    text_start_idx: usize,
    text_end_idx: usize,
    glyph_positions: Vec<GlyphPosition>,
}

// Build during wrap_text operation
```

### 4.3 Integration Points
- Add selection state to sidebar components
- Track mouse drag events for selection
- Render selection highlights behind text
- Implement copy-to-clipboard for selections

## Implementation Order

1. **Create backup branch** (for reference)
2. **Implement code wrapping** (Phase 2) - FIRST to ensure functionality
3. **Test new wrapping implementation** - Verify it works before removing old code
4. **Remove horizontal scrolling code** (Phase 1)
5. **Run `cargo check` and fix compilation errors**
6. **Run full test suite** (Phase 3)
7. **Document changes** for text selection work
8. **Update CLAUDE.md** with new patterns

**Note**: Implementing the replacement before removal ensures we maintain functionality throughout the refactoring process.

## Risk Mitigation

### Potential Risks
1. **Regression in code display**: Mitigate by careful testing
2. **Performance impact**: Line wrapping is already implemented and performant
3. **User confusion**: Document the change clearly
4. **Performance with large code blocks**: Very long code blocks might cause performance issues with line wrapping
   - Mitigation: Add maximum line limit with "Show more" functionality
   - Consider virtual scrolling for extremely long blocks
   - Profile performance with realistic code samples

### Rollback Plan
- Keep horizontal scrolling code in git history
- Document attempt in HORIZONTAL_SCROLL_ATTEMPTS.md
- Can reference for future implementation if needed

## Success Criteria

1. **No Visual Overflow**: Code blocks stay within boundaries
2. **Readable Code**: All code visible without horizontal scrolling
3. **Preserved Functionality**: Copy buttons, syntax highlighting work
4. **Stable Rendering**: No crashes or visual artifacts
5. **Performance**: No noticeable slowdown
6. **Clean Codebase**: All horizontal scroll code removed
7. **Future-Ready**: Clear path to text selection implementation

## Timeline Estimate

- Phase 1 (Removal): 2-3 hours
- Phase 2 (Wrapping): 3-4 hours  
- Phase 3 (Testing): 2-3 hours
- Phase 4 (Documentation): 1 hour

Total: ~8-11 hours of focused work

## Next Steps After Implementation

1. Implement text selection using parallel text mapping
2. Add keyboard navigation for selection
3. Integrate with system clipboard
4. Consider future enhancements (find in sidebar, etc.)
5. Consider render-to-texture approach for true horizontal scrolling if needed in future (see "Future Implementation Options" in HORIZONTAL_SCROLL_ATTEMPTS.md for detailed technical approach)

## Conclusion

Removing horizontal scrolling in favor of line wrapping is the pragmatic choice that:
- Solves immediate visual bugs
- Simplifies the codebase
- Enables text selection implementation
- Maintains all current functionality
- Provides better user experience for reading code

The architectural challenges discovered during horizontal scrolling implementation make this the correct technical decision for the project's goals.