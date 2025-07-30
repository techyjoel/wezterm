# Goal Card Selection Issues and Fixes

## Issues Identified:
1. Can't re-select when a current selection exists
2. Have to move mouse before deselection shows
3. Can only deselect by clicking on goal text, not anywhere in sidebar

## Root Causes:

### Issue 1: Can't Re-select
In `ai_sidebar.rs::prepare_selection()`:
```rust
if should_clear {
    self.selection_state.clear();
    return;  // <-- This prevents new selection from starting!
}
```

### Issue 2: Mouse Movement Required
Selection state changes don't trigger UI invalidation.

### Issue 3: Click Anywhere to Deselect
No handler for clicks on empty sidebar space.

## Implemented Fixes:

### 1. Fixed prepare_selection in ai_sidebar.rs
- ✅ Removed early return after clearing
- ✅ Added bool return to indicate if invalidation needed
- ✅ Always prepare new selection after clearing

### 2. Added invalidation handling
- ✅ Made `prepare_selection()` return bool
- ✅ Added `clear_selection()` method that returns bool
- ✅ Updated all mouse event handlers to check return and invalidate

### 3. Added sidebar background click handling
- ✅ In `mouse_event_sidebar`, added check for left clicks
- ✅ Clear any active selection on background click
- ✅ Trigger invalidation if selection was cleared

## Summary of Changes:

1. **ai_sidebar.rs**:
   - Modified `prepare_selection()` to return bool and not early-return
   - Added `clear_selection()` public method

2. **mouseevent.rs**:
   - Updated goal, activity, and suggestion handlers to use return values
   - Added click-anywhere-to-deselect in `mouse_event_sidebar`
   - Ensured invalidation happens whenever selection state changes

The fixes ensure:
- Re-selection works by clearing old and starting new selection
- UI updates immediately on deselection (no mouse movement needed)
- Clicking anywhere in sidebar clears selection