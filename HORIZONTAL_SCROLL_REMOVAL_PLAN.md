# Horizontal Scrolling Removal and Line Wrapping Implementation Plan

## Implementation Status: ✅ REMOVAL COMPLETE, 🔧 WRAPPING NEEDS FIXES (2025-06-30)

**Summary**: Horizontal scrolling has been successfully removed and basic line wrapping implemented. However, several issues remain with code block rendering that need to be addressed.

## Current Issues to Fix

### 1. **Code Block Indentation Lost** ✅ FIXED
**Problem**: Leading whitespace/indentation was being stripped from code blocks
**Symptom**: Python code showed `def`, `try:`, `with`, etc. all left-aligned
**Root Cause**: 
- The `wrap_text` function in `box_model.rs` was skipping consecutive spaces (lines 785-787)
- This stripped leading indentation when processing text for wrapping
**Fix Applied**: 
- Modified `wrap_text` to preserve leading spaces by counting them separately
- Added logic to prepend indentation to each line before processing words
- Lines 772-843 in `wezterm-gui/src/termwindow/box_model.rs`

### 2. **Syntax Coloring Simplified** ✅ FIXED
**Problem**: Code blocks were only using a single color instead of full syntax highlighting
**Current State**: Full syntax highlighting now preserved across wrapped lines
**Solution Implemented**: 
- Changed from `WrappedText` to `Element::Children` with inline display
- Each syntax segment is rendered as a separate inline element that wraps naturally
- Lines 445-467 in `wezterm-gui/src/sidebar/components/markdown.rs`
**How It Works**:
- Syntax segments maintain their original colors
- Inline display allows segments to wrap at word boundaries
- Parent block element handles line height and margin settings

### 3. **Modal Clipping Issues** 🟡 MEDIUM - ARCHITECTURAL LIMITATION
**Problem**: Modal content renders outside the modal background bounds
**Symptom**: Text appears above/below the modal's visual container
**Root Cause**: 
- WezTerm's Element system doesn't support true clipping/scissor rectangles
- The GPU-based rendering happens in a later pass, preventing parent-based clipping
- Elements are positioned absolutely and can render anywhere
**Current Workaround**:
- Using "cut-a-hole" pattern with higher z-index frame sections
- Modal background at z-index 22, content at z-index 21
- This only hides content behind the frame, doesn't truly clip
**Note**: This is a known architectural limitation that would require significant changes to fix properly

### 4. **Text Styling Partially Implemented** 🟢 LOW
**Problem**: Bold and italic text formatting not fully implemented
**Current State**:
- Bold headings ARE working (using separate heading font)
- Bold/italic in paragraphs NOT working (TODOs in code)
- Lists structure working but no bullet/number rendering
- Inline code shows with backticks but no styling
**Implementation Path**:
- WezTerm supports FontWeight and FontStyle (see `config/src/font.rs`)
- SidebarFonts struct only has heading/body/code fonts currently
- Would need to load bold/italic variants and apply based on emphasis stack
**Key Files**: 
- `wezterm-gui/src/sidebar/components/markdown.rs` (lines 298-312)
- Currently has TODOs for applying font variants

## What's Working Well
- ✅ Line wrapping for long lines
- ✅ Newline preservation between logical lines  
- ✅ Configurable line height and margin
- ✅ Copy button functionality
- ✅ Vertical scrolling and scrollbar dragging
- ✅ Basic markdown structure (headings, paragraphs, code blocks)
- ✅ Tab handling in indentation (converts to 4 spaces)

## Recent Fixes (Latest Session)
- **Dead Code Cleanup**: Removed ColoredWrappedText implementation and all related code
- **Indentation Bug Fix**: Fixed double-processing of leading spaces that could cause wrapped lines to have incorrect indentation
- **Tab Support**: Added proper tab-to-space conversion in indentation handling

## Implementation Details for Completed Fixes

### Fix 1: Preserve Code Indentation ✅
**Implementation**:
- Added leading space detection: `line_text.len() - line_text.trim_start().len()`
- Store indentation separately: `indentation = " ".repeat(leading_spaces)`
- Prepend indentation to line before processing words
- Skip leading spaces during word parsing to avoid duplication

### Fix 2: Restore Syntax Coloring ✅
**Implementation**:
- Used Approach A - Multiple inline elements per line
- Each syntax segment becomes an inline Element with its color preserved
- Parent block element handles line spacing and margins
- Natural word wrapping preserved through inline display

### Implementation Approach for Remaining Issues

### Fix 3: Modal Clipping (Architectural Limitation)
**Possible Workarounds**:
1. Limit modal content height to prevent overflow
2. Add top/bottom fade gradients to indicate more content
3. Implement manual bounds checking in content rendering
4. Consider future architectural changes to support proper clipping

### Fix 4: Text Styling Enhancement
1. Load bold/italic font variants in sidebar font initialization
2. Modify markdown renderer to:
   - Check emphasis stack when creating text elements
   - Switch fonts based on Bold/Italic state
   - Apply appropriate font variant to each text segment
3. Add bullet/number rendering for lists
4. Style inline code with code font and background color

## Current Syntax Highlighting Approach
The segment-based WrappedText approach (commit 322f5b8a0) works as follows:
- Each syntax-highlighted segment becomes its own WrappedText element
- Segments are displayed inline within a block container
- This preserves colors but may wrap at segment boundaries rather than word boundaries
- Trade-off: Better than no syntax highlighting, but not optimal for readability

### Known Issues with Current Approach
1. **Suboptimal Wrapping**: Lines may break mid-word if a syntax segment boundary occurs there
2. **Performance**: Creating many small Elements has overhead
3. **Wrapped Line Indentation**: The screenshot shows wrapped lines sometimes have incorrect indentation - this may be due to segment boundaries

## Architecture Notes

### Z-Index Layering (Working)
- **Z-index 10**: Activity log content
- **Z-index 12**: Sidebar background with cut-out hole
- **Z-index 14**: Main sidebar content  
- **Z-index 16**: Scrollbars and buttons
- **Z-index 20-23**: Modals

### Key Files Reference
- **Markdown rendering**: `wezterm-gui/src/sidebar/components/markdown.rs`
- **Text wrapping**: `wezterm-gui/src/termwindow/box_model.rs` (wrap_text function)
- **Modal system**: `wezterm-gui/src/sidebar/components/modal/`
- **Activity log**: `wezterm-gui/src/sidebar/ai_sidebar.rs`

## Testing Required

### Visual Testing (Not Yet Complete)
- [ ] Verify code blocks wrap cleanly at sidebar edges
- [ ] Check that copy buttons remain properly positioned after wrapping
- [ ] Test with various programming languages (Python ✓, Rust, JavaScript, etc.)
- [ ] Ensure no visual artifacts from removed clipping code
- [ ] Verify markdown features:
  - [ ] Bold text rendering (`**text**`)
  - [ ] Italic text rendering (`*text*`)
  - [ ] Bulleted lists
  - [ ] Numbered lists
  - [ ] Inline code styling (`code`)
  - [ ] Link styling and hover effects
  - [ ] Heading styles (sizes and weights)
- [ ] Modal scrolling behavior and bounds clipping
- [ ] Filter chip functionality and click detection

### Functional Testing
- [ ] Test with extreme cases:
  - [ ] Very long lines (URLs, base64 strings)
  - [ ] Deeply indented code (5+ levels)
  - [ ] Mixed languages in same block
  - [ ] Empty code blocks
  - [ ] Code blocks with only whitespace
- [ ] Verify copy functionality provides exact original text
- [ ] Test line height/margin configuration changes
- [ ] Cross-platform testing (different fonts/DPI settings)

### Performance Testing  
- [ ] Verify smooth scrolling performance isn't degraded

### Edge Cases to Verify
- [ ] Unicode/emoji in code blocks
- [ ] RTL text handling
- [ ] Very narrow sidebar widths
- [ ] Dark/light theme compatibility