# Windows Border Implementation Troubleshooting Notes

## Current Status
We have implemented OS window border support for Windows but are encountering `BorrowMutError` panics during runtime. The border functionality itself works (logs show "Successfully set Windows 11 DWM border color") but there are reentrancy issues.

## Latest Changes Made
1. **Fixed compilation errors**: Changed `LoadLibraryA` from Result pattern matching to null pointer checks
2. **Fixed method name errors**: Changed calls from non-existent `inner.set_window_border()` to `inner.apply_windows_border()`
3. **Implemented state storage**: Added `pending_border: Option<OsBorderStyle>` to `WindowInner`
4. **Added deferred application**: Using `PostMessageW(WM_USER + 1000)` to defer border application to safe message handler

## Current Issue
Still getting `BorrowMutError` panics with stack trace showing:
```
window::os::windows::window::WindowInner::apply_pending_borders
-> NtUserSetWindowPos 
-> DWM APIs 
-> window messages 
-> IME callbacks 
-> borrowing conflicts
```

## Files Modified
- `window/src/os/windows/window.rs`: Main implementation
- Added `pending_border` field to `WindowInner` struct
- Modified `set_window_border` and `update_window_border` to use PostMessage
- Added `WM_USER + 1000` message handler
- Modified window creation to use PostMessage instead of direct call

## Implementation Details

### Border State Storage
```rust
// In WindowInner struct
pending_border: Option<crate::os::parameters::OsBorderStyle>,

// In WindowInner initialization
pending_border: None,
```

### Deferred Application Pattern
```rust
fn set_window_border(&self, border: Option<&crate::os::parameters::OsBorderStyle>) {
    let border_clone = border.cloned();
    Connection::with_window_inner(self.0, move |inner| {
        inner.pending_border = border_clone;
        Ok(())
    });
    
    // Trigger safe application via message queue
    unsafe {
        PostMessageW(self.0 .0, WM_USER + 1000, 0, 0);
    }
}
```

### Message Handler
```rust
WM_USER + 1000 => {
    // Custom message to safely apply pending borders
    if let Some(inner) = rc_from_hwnd(hwnd) {
        inner.borrow_mut().apply_pending_borders();
    }
    Some(0)
}
```

## Next Steps for Windows Testing

1. **Build and test current implementation**:
   ```bash
   cargo build --release
   ./target/release/wezterm.exe
   ```

2. **Check if PostMessage approach resolves reentrancy**:
   - Look for `BorrowMutError` panics
   - Verify borders are applied (check logs for "Successfully set Windows 11 DWM border color")

3. **If still getting reentrancy issues**, consider these alternatives:
   
   **Option A: Move to apply_theme function**
   ```rust
   // Add border application directly in apply_theme() function
   // This follows the exact same pattern as other DWM operations
   ```
   
   **Option B: Use spawn queue instead of PostMessage**
   ```rust
   // Use wezterm's existing spawn queue mechanism
   crate::spawn::SPAWN_QUEUE.spawn(...);
   ```
   
   **Option C: Delay border application until window is fully initialized**
   ```rust
   // Only apply borders after window creation is completely finished
   // Maybe trigger from first paint or resize event
   ```

4. **Test border functionality**:
   - Create test config with visible border:
     ```lua
     config.window_frame = {
       os_window_border_enabled = true,
       os_window_border = {
         width = "6px",
         color = "#ff0000", -- Red for visibility
         radius = "12px"    -- Windows 11 only
       }
     }
     ```
   - Test on both Windows 10 and Windows 11 if possible
   - Verify DWM extended frame fallback on Windows 10

5. **Debug approach if issues persist**:
   - Use `RUST_BACKTRACE=full` for detailed stack traces
   - Add more detailed logging around DWM API calls
   - Check if issue is specific to certain DWM operations (color vs extended frame)

## Related Files to Check
- `wezterm-gui/src/termwindow/mod.rs`: Where border methods are called from
- `config/src/color.rs`: Border configuration structures
- `window/src/os/parameters.rs`: OsBorderStyle definition

## Expected Behavior When Working
- Windows 11: Custom colored border via `DWMWA_BORDER_COLOR` API
- Windows 10: Extended frame border via `DwmExtendFrameIntoClientArea`
- No runtime panics or borrowing conflicts
- Border should resize with window and follow theme changes

## Key Learnings from macOS Implementation
- Store state, apply during proper lifecycle events
- Avoid calling platform APIs during arbitrary operations
- Use existing platform patterns for timing (like apply_theme)
- Integration with window creation, resize, and theme change events