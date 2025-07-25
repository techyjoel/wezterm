# Multi-line Chat Input & Text Selection Implementation

## Project Goals
1. **Focus Management**: Focus defaults to terminal. Only moves to chat input when clicked. Modals steal focus and return it when closed.
2. **Text Selection**: Enable click-and-drag per-character text selection with visual feedback (blue background, white text) in activity log, suggestion card, suggestion "view more" modal, and goal text.
3. **Multi-line Chat Input**: Full editing capabilities with click-to-position cursor, scrolling, Enter to send, Shift+Enter for newline.

## Latest Status (After Session 14 - Goal Selection Complete)

### ✅ Working Features
1. **Multi-line Chat Input**:
   - Click-to-position cursor works perfectly with wrapped text
   - Shift+Enter inserts newlines, Enter sends
   - Scrollbar appears and functions for long text
   - Focus management works (Escape returns to terminal)

2. **Goal Text Selection** (COMPLETE):
   - Click-and-drag selection works for entire text including last characters
   - Selection uses actual glyph positions for pixel-perfect accuracy
   - Visual feedback with blue background
   - Position tracking infrastructure fully integrated

### 🔧 Issues to Fix
1. **Deselection Behavior**:
   - Deselection requires mouse movement after click (should be immediate)
   - Clicking outside goal card doesn't deselect

2. **Clipboard Integration**:
   - Command-C doesn't copy selected text yet
   - Need to implement clipboard handling for sidebar selections

### ❌ Other Selection Areas - Not Yet Implemented

1. **Activity Log**: Selection broken (needs position tracking integration)
2. **Suggestion Card**: Selection not implemented
3. **Show More Modal**: Selection not implemented  
4. **Chat Input**: Selection partially implemented but needs fixes

## Key Technical Learnings

### Position Tracking for Text Selection
1. **Glyph Position Extraction**: The `GlyphPositionMap` infrastructure extracts exact character positions after text rendering
2. **Selection Rectangle Calculation**: MUST use actual glyph positions, not estimated character widths
3. **Coordinate Systems**: Always account for padding/margins when transforming between absolute and relative coordinates

### Critical Implementation Details
- **Pattern Matching**: Use proper `match` expressions instead of complex `matches!` with pattern guards
- **Debug Logging**: Keep minimal, strategic logging only - excessive logging impacts performance
- **Selection State**: Sidebar maintains independent selection state from terminal
- **Event Handling**: Drag events must be handled at sidebar level when mouse moves outside UIItem bounds

## Next Implementation Steps

1. **Fix Deselection Behavior**:
   - Add `process_event_with_context()` call after selection state changes
   - Implement global click handler in sidebar for click-outside-to-deselect

2. **Clipboard Integration**:
   - Add keyboard event handler for Cmd+C when sidebar has focus
   - Extract selected text using byte offsets
   - Use existing WezTerm clipboard API

3. **Apply to Other Selection Areas**:
   - Activity Log: Extract positions for each message after rendering
   - Suggestion Cards: Add position tracking to suggestion text
   - Chat Input: Complete existing partial implementation


## Important Implementation Context

### Working Infrastructure
1. **Position Tracking**: `GlyphPositionMap` extracts positions after rendering
2. **Coordinate Transformation**: Properly converts absolute to relative  
3. **Selection Rendering**: Works at appropriate z-index with sub-layers

### Key Files
- `sidebar_render.rs`: Position extraction and selection overlay rendering
- `ai_sidebar.rs`: Selection state management and rectangle calculation
- `mouseevent.rs`: Event routing and coordinate transformation
- `box_model.rs`: Text shaping with cluster tracking for sidebar text
