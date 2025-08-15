//! Mouse event handling for the terminal window
//!
//! This module implements mouse interaction logic for all UI elements in WezTerm:
//! - Terminal pane interactions (selection, scrolling, clicking links)
//! - Tab bar interactions (switching tabs, closing tabs)
//! - Sidebar interactions (buttons, scrollbars, content)
//! - Split pane resizing
//! - Context menu triggering
//!
//! The module uses the UIItem system to track interactive elements and their bounds,
//! routing events to the appropriate handlers based on hit testing.

use crate::sidebar::ai_sidebar::SelectionTarget;
use crate::tabbar::TabBarItem;
use crate::termwindow::{
    FocusArea, GuiWin, MouseCapture, PositionedSplit, ScrollHit, TermWindowNotif, UIItem,
    UIItemType, TMB,
};
use ::window::{
    MouseButtons, MouseButtons as WMB, MouseCursor, MouseEvent, MouseEventKind as WMEK, MousePress,
    WindowDecorations, WindowOps, WindowState,
};
use config::keyassignment::{KeyAssignment, MouseEventTrigger, SpawnTabDomain};
use config::MouseEventAltScreen;
use mux::pane::{Pane, WithPaneLines};
use mux::tab::SplitDirection;
use mux::Mux;
use mux_lua::MuxPane;
use std::convert::TryInto;
use std::ops::Sub;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use termwiz::hyperlink::Hyperlink;
use termwiz::surface::Line;
use wezterm_dynamic::ToDynamic;
use wezterm_term::input::{MouseButton, MouseEventKind as TMEK};
use wezterm_term::{ClickPosition, LastMouseClick, StableRowIndex};

/// Horizontal scroll speed multiplier for mouse wheel events
const HORIZONTAL_SCROLL_SPEED: f32 = 30.0;

/// Find the byte offset in text from an x coordinate using pre-calculated character positions
fn find_byte_offset_from_x(x: f32, char_positions: &[(f32, f32, usize)]) -> usize {
    // If no positions, return 0
    if char_positions.is_empty() {
        return 0;
    }

    // Find the character that contains this x position
    for (x_start, x_end, byte_offset) in char_positions {
        if x >= *x_start && x <= *x_end {
            // Check if we're closer to the start or end of this character
            let mid = (x_start + x_end) / 2.0;
            if x < mid {
                return *byte_offset;
            } else {
                // Return the next character's offset if available
                if let Some(next) = char_positions
                    .iter()
                    .find(|(_, _, offset)| *offset > *byte_offset)
                {
                    return next.2;
                }
                // Otherwise, we're at the end of the text
                return *byte_offset + 1; // Assume single-byte char for simplicity
            }
        }
    }

    // If x is before the first character, return 0
    if let Some(first) = char_positions.first() {
        if x < first.0 {
            return 0;
        }
    }

    // If x is after the last character, return the end position
    if let Some(last) = char_positions.last() {
        return last.2 + 1; // Assume single-byte char for simplicity
    }

    0
}

/// Helper to access AI sidebar with less nesting
fn with_ai_sidebar<F, R>(
    sidebar_manager: &std::cell::RefCell<crate::sidebar::SidebarManager>,
    f: F,
) -> Option<R>
where
    F: FnOnce(&mut crate::sidebar::ai_sidebar::AiSidebar) -> R,
{
    let manager = sidebar_manager.try_borrow_mut().ok()?;
    let sidebar = manager.get_right_sidebar()?;
    let mut sidebar = sidebar.lock().ok()?;
    let ai_sidebar = sidebar
        .as_any_mut()
        .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>()?;
    Some(f(ai_sidebar))
}

impl super::TermWindow {
    fn resolve_ui_item(&self, event: &MouseEvent) -> Option<UIItem> {
        let x = event.coords.x;
        let y = event.coords.y;

        // Debug log during drag to see what UI items are being resolved
        if matches!(event.kind, WMEK::Move) && !event.mouse_buttons.is_empty() {
            log::debug!("resolve_ui_item during drag at ({}, {})", x, y);
        }

        // Debug logging for goal area clicks
        let sidebar_manager = self.sidebar_manager.borrow();
        let sidebar_width = sidebar_manager.get_window_expansion() as f32;
        drop(sidebar_manager);
        let window_width = self.dimensions.pixel_width as f32;
        let sidebar_x = window_width - sidebar_width;

        // Check if click is in sidebar area where goal might be
        if x >= sidebar_x as isize && y >= 100 && y <= 400 {
            log::debug!(
                "UIITEM DEBUG: Mouse click in potential goal area - x={}, y={}, checking {} UIItems",
                x, y, self.ui_items.len()
            );

            // Log mouse position and UI items
            log::debug!(
                "Mouse event at ({}, {}), checking {} UI items",
                x,
                y,
                self.ui_items.len()
            );

            // Log all UIItems that could match this position
            for (idx, item) in self.ui_items.iter().enumerate().rev() {
                if item.hit_test(x, y) {
                    log::debug!(
                        "UIITEM DEBUG: UIItem {} matches click - type={:?}, bounds=({},{},{},{})",
                        idx,
                        item.item_type,
                        item.x,
                        item.y,
                        item.width,
                        item.height
                    );
                }
            }
        }

        self.ui_items
            .iter()
            .rev()
            .find(|item| item.hit_test(x, y))
            .cloned()
    }

    fn leave_ui_item(&mut self, item: &UIItem) {
        match item.item_type {
            UIItemType::TabBar(_) => {
                self.update_title_post_status();
            }
            UIItemType::CloseTab(_)
            | UIItemType::AboveScrollThumb
            | UIItemType::BelowScrollThumb
            | UIItemType::ScrollThumb
            | UIItemType::Split(_)
            | UIItemType::SidebarButton(_)
            | UIItemType::Sidebar(_)
            | UIItemType::SidebarFilterChip(_)
            | UIItemType::ShowMoreButton(_)
            | UIItemType::SuggestionRunButton
            | UIItemType::SuggestionDismissButton
            | UIItemType::CodeBlockContent(_)
            | UIItemType::CodeBlockCopyButton(_)
            | UIItemType::ModalCloseButton
            | UIItemType::ChatInput { .. }
            | UIItemType::ActivityItemText { .. }
            | UIItemType::SuggestionText { .. }
            | UIItemType::GoalText { .. }
            | UIItemType::ActivityLogBackground => {}
        }
    }

    fn enter_ui_item(&mut self, item: &UIItem) {
        match item.item_type {
            UIItemType::TabBar(_) => {}
            UIItemType::CloseTab(_)
            | UIItemType::AboveScrollThumb
            | UIItemType::BelowScrollThumb
            | UIItemType::ScrollThumb
            | UIItemType::Split(_)
            | UIItemType::SidebarButton(_)
            | UIItemType::Sidebar(_)
            | UIItemType::SidebarFilterChip(_)
            | UIItemType::ShowMoreButton(_)
            | UIItemType::SuggestionRunButton
            | UIItemType::SuggestionDismissButton
            | UIItemType::CodeBlockContent(_)
            | UIItemType::CodeBlockCopyButton(_)
            | UIItemType::ModalCloseButton
            | UIItemType::ChatInput { .. }
            | UIItemType::ActivityItemText { .. }
            | UIItemType::SuggestionText { .. }
            | UIItemType::GoalText { .. }
            | UIItemType::ActivityLogBackground => {}
        }
    }

    /// Main mouse event handler that routes events to appropriate subsystems
    ///
    /// This function determines which UI element the mouse event targets and
    /// dispatches to the appropriate handler. It checks in order:
    /// 1. Sidebar modal overlays
    /// 2. Captured mouse state (dragging)
    /// 3. UI items (via hit testing)
    /// 4. Terminal pane content
    pub fn mouse_event_impl(&mut self, event: MouseEvent, context: &dyn WindowOps) {
        log::debug!(
            "mouse_event_impl: event.kind={:?}, mouse_buttons={:?}, text_selection_drag_active={}",
            event.kind,
            event.mouse_buttons,
            self.text_selection_drag_active
        );
        log::trace!("{:?}", event);
        let pane = match self.get_active_pane_or_overlay() {
            Some(pane) => pane,
            None => return,
        };

        self.current_mouse_event.replace(event.clone());

        let border = self.get_os_border();

        let first_line_offset = if self.show_tab_bar && !self.config.tab_bar_at_bottom {
            self.tab_bar_pixel_height().unwrap_or(0.) as isize
        } else {
            0
        } + border.top.get() as isize;

        let (padding_left, padding_top) = self.padding_left_top();

        let y = (event
            .coords
            .y
            .sub(padding_top as isize)
            .sub(first_line_offset)
            .max(0)
            / self.render_metrics.cell_size.height) as i64;

        let x = (event
            .coords
            .x
            .sub((padding_left + border.left.get() as f32) as isize)
            .max(0) as f32)
            / self.render_metrics.cell_size.width as f32;
        let x = if !pane.is_mouse_grabbed() {
            // Round the x coordinate so that we're a bit more forgiving of
            // the horizontal position when selecting cells
            x.round()
        } else {
            x
        }
        .trunc() as usize;

        let mut y_pixel_offset = event
            .coords
            .y
            .sub(padding_top as isize)
            .sub(first_line_offset);
        if y > 0 {
            y_pixel_offset = y_pixel_offset.max(0) % self.render_metrics.cell_size.height;
        }

        let mut x_pixel_offset = event
            .coords
            .x
            .sub((padding_left + border.left.get() as f32) as isize);
        if x > 0 {
            x_pixel_offset = x_pixel_offset.max(0) % self.render_metrics.cell_size.width;
        }

        self.last_mouse_coords = (x, y);

        let mut capture_mouse = false;

        match event.kind {
            WMEK::Release(ref press) => {
                self.current_mouse_capture = None;
                // Don't clear text_selection_drag_active here - let the specific handlers do it
                self.current_mouse_buttons.retain(|p| p != press);
                if press == &MousePress::Left && self.window_drag_position.take().is_some() {
                    // Completed a window drag
                    return;
                }
                if press == &MousePress::Left && self.dragging.take().is_some() {
                    // Completed a drag
                    return;
                }

                // Forward mouse release to sidebars that might have active drag operations
                // This ensures scrollbar drag states are properly cleared even when the
                // mouse is released outside the sidebar bounds
                if press == &MousePress::Left {
                    let mut sidebar_manager = self.sidebar_manager.borrow_mut();

                    // Check right sidebar
                    if let Some(sidebar) = sidebar_manager.get_right_sidebar() {
                        let mut sidebar_locked = sidebar.lock().unwrap();
                        let _ = sidebar_locked.handle_mouse_event(&event);
                    }

                    // Check left sidebar
                    if let Some(sidebar) = sidebar_manager.get_left_sidebar() {
                        let mut sidebar_locked = sidebar.lock().unwrap();
                        let _ = sidebar_locked.handle_mouse_event(&event);
                    }

                    drop(sidebar_manager);
                }
            }

            WMEK::Press(ref press) => {
                capture_mouse = true;

                // Perform click counting
                let button = mouse_press_to_tmb(press);

                let click_position = ClickPosition {
                    column: x,
                    row: y,
                    x_pixel_offset,
                    y_pixel_offset,
                };

                let click = match self.last_mouse_click.take() {
                    None => LastMouseClick::new(button, click_position),
                    Some(click) => click.add(button, click_position),
                };
                self.last_mouse_click = Some(click);
                self.current_mouse_buttons.retain(|p| p != press);
                self.current_mouse_buttons.push(*press);
            }

            WMEK::Move => {
                if let Some(start) = self.window_drag_position.as_ref() {
                    // Dragging the window
                    // Compute the distance since the initial event
                    let delta_x = start.screen_coords.x - event.screen_coords.x;
                    let delta_y = start.screen_coords.y - event.screen_coords.y;

                    // Now compute a new window position.
                    // We don't have a direct way to get the position,
                    // but we can infer it by comparing the mouse coords
                    // with the screen coords in the initial event.
                    // This computes the original top_left position,
                    // and applies the total drag delta to it.
                    let top_left = ::window::ScreenPoint::new(
                        (start.screen_coords.x - start.coords.x) - delta_x,
                        (start.screen_coords.y - start.coords.y) - delta_y,
                    );
                    // and now tell the window to go there
                    context.set_window_position(top_left);
                    return;
                }

                if let Some((item, start_event)) = self.dragging.take() {
                    self.drag_ui_item(item, start_event, x, y, event, context);
                    return;
                }

                // Forward mouse move events to sidebars that might have active drag operations
                // This ensures scrollbar dragging continues to work even when the mouse
                // moves outside the sidebar bounds
                let mut sidebar_handled = false;
                {
                    let mut sidebar_manager = self.sidebar_manager.borrow_mut();

                    // Check right sidebar for active drag
                    if let Some(sidebar) = sidebar_manager.get_right_sidebar() {
                        let mut sidebar_locked = sidebar.lock().unwrap();
                        // Check if this sidebar might be dragging (we'll let it decide)
                        if let Ok(handled) = sidebar_locked.handle_mouse_event(&event) {
                            if handled {
                                sidebar_handled = true;
                                context.invalidate();
                            }
                        }
                    }

                    // Check left sidebar for active drag if right didn't handle it
                    if !sidebar_handled {
                        if let Some(sidebar) = sidebar_manager.get_left_sidebar() {
                            let mut sidebar_locked = sidebar.lock().unwrap();
                            if let Ok(handled) = sidebar_locked.handle_mouse_event(&event) {
                                if handled {
                                    sidebar_handled = true;
                                    context.invalidate();
                                }
                            }
                        }
                    }
                }

                // If a sidebar handled the event (e.g., scrollbar dragging), don't process it further
                if sidebar_handled {
                    return;
                }
            }
            _ => {}
        }

        let prior_ui_item = self.last_ui_item.clone();

        let ui_item = if matches!(
            self.current_mouse_capture,
            None | Some(MouseCapture::UI) | Some(MouseCapture::TextSelection)
        ) {
            log::debug!("About to resolve_ui_item - event.kind: {:?}, mouse_buttons: {:?}, text_selection_drag: {}", 
                      event.kind, event.mouse_buttons, self.text_selection_drag_active);
            let ui_item = self.resolve_ui_item(&event);
            log::debug!(
                "resolve_ui_item returned: {:?}",
                ui_item.as_ref().map(|item| &item.item_type)
            );

            match (self.last_ui_item.take(), &ui_item) {
                (Some(prior), Some(item)) => {
                    if prior != *item || !self.config.use_fancy_tab_bar {
                        self.leave_ui_item(&prior);
                        self.enter_ui_item(item);
                        context.invalidate();
                    }
                }
                (Some(prior), None) => {
                    self.leave_ui_item(&prior);
                    context.invalidate();
                }
                (None, Some(item)) => {
                    self.enter_ui_item(item);
                    context.invalidate();
                }
                (None, None) => {}
            }

            ui_item
        } else {
            None
        };

        if let Some(item) = ui_item.clone() {
            // Check if this is a vertical scroll on a code block or copy button without Shift
            // If so, forward it to the sidebar's scroll handler
            let is_code_block_vertical_scroll =
                matches!(
                    (&item.item_type, &event.kind),
                    (UIItemType::CodeBlockContent(_), WMEK::VertWheel(_))
                        | (UIItemType::CodeBlockCopyButton(_), WMEK::VertWheel(_))
                ) && !event.modifiers.contains(::window::Modifiers::SHIFT);

            if is_code_block_vertical_scroll {
                // Forward vertical scroll to the sidebar that contains this code block
                // Code blocks are always in the right sidebar (AiSidebar)
                let mut sidebar_manager = self.sidebar_manager.borrow_mut();
                if let Some(sidebar) = sidebar_manager.get_right_sidebar() {
                    let mut sidebar_locked = sidebar.lock().unwrap();
                    if let Ok(handled) = sidebar_locked.handle_mouse_event(&event) {
                        if handled {
                            context.invalidate();
                        }
                    }
                }
            } else {
                if capture_mouse {
                    self.current_mouse_capture = Some(MouseCapture::UI);
                }
                self.mouse_event_ui_item(item, pane, y, event, context);
            }
        } else if self.text_selection_drag_active && matches!(event.kind, WMEK::Move) {
            // Special handling for text selection drag when UI item can't be resolved
            // This happens when UI items are rebuilt during paint
            log::debug!("SPECIAL HANDLING: text selection drag without UI item");

            let window_point = euclid::Point2D::new(event.coords.x as f32, event.coords.y as f32);

            // Use hierarchical hit testing directly on the sidebar
            let hit_result = with_ai_sidebar(&self.sidebar_manager, |ai_sidebar| {
                ai_sidebar.hit_test_activity_log(window_point)
            })
            .flatten();

            if let Some(hit) = hit_result {
                log::debug!(
                    "Text selection drag hit test found item {} at byte {}",
                    hit.item_index,
                    hit.position_in_item.byte_offset
                );
                with_ai_sidebar(&self.sidebar_manager, |ai_sidebar| {
                    // Ensure selection is active
                    if !ai_sidebar.is_selecting() {
                        ai_sidebar.activate_prepared_selection();
                    }
                    // Update selection with new position
                    eprintln!("🎯 HIT TEST: item={}, byte_offset={}", 
                        hit.item_index, 
                        hit.position_in_item.byte_offset
                    );
                    ai_sidebar.update_activity_log_selection_drag(
                        hit.item_index,
                        hit.position_in_item.byte_offset,
                    );
                });
                context.invalidate();
            }
        } else if matches!(
            self.current_mouse_capture,
            None | Some(MouseCapture::TerminalPane(_))
        ) {
            self.mouse_event_terminal(
                pane,
                ClickPosition {
                    column: x,
                    row: y,
                    x_pixel_offset,
                    y_pixel_offset,
                },
                event,
                context,
                capture_mouse,
            );
        }

        if prior_ui_item != ui_item {
            self.update_title_post_status();
        }
    }

    pub fn mouse_leave_impl(&mut self, context: &dyn WindowOps) {
        self.current_mouse_event = None;
        self.update_title();
        context.set_cursor(Some(MouseCursor::Arrow));
        context.invalidate();
    }

    fn drag_split(
        &mut self,
        mut item: UIItem,
        split: PositionedSplit,
        start_event: MouseEvent,
        x: usize,
        y: i64,
        context: &dyn WindowOps,
    ) {
        let mux = Mux::get();
        let tab = match mux.get_active_tab_for_window(self.mux_window_id) {
            Some(tab) => tab,
            None => return,
        };
        let delta = match split.direction {
            SplitDirection::Horizontal => (x as isize).saturating_sub(split.left as isize),
            SplitDirection::Vertical => (y as isize).saturating_sub(split.top as isize),
        };

        if delta != 0 {
            tab.resize_split_by(split.index, delta);
            if let Some(split) = tab.iter_splits().into_iter().nth(split.index) {
                item.item_type = UIItemType::Split(split);
                context.invalidate();
            }
        }
        self.dragging.replace((item, start_event));
    }

    fn drag_scroll_thumb(
        &mut self,
        item: UIItem,
        start_event: MouseEvent,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        let pane = match self.get_active_pane_or_overlay() {
            Some(pane) => pane,
            None => return,
        };

        let dims = pane.get_dimensions();
        let current_viewport = self.get_viewport(pane.pane_id());

        let tab_bar_height = if self.show_tab_bar {
            self.tab_bar_pixel_height().unwrap_or(0.)
        } else {
            0.
        };
        let (top_bar_height, bottom_bar_height) = if self.config.tab_bar_at_bottom {
            (0.0, tab_bar_height)
        } else {
            (tab_bar_height, 0.0)
        };

        let border = self.get_os_border();
        let y_offset = top_bar_height + border.top.get() as f32;

        let from_top = start_event.coords.y.saturating_sub(item.y as isize);
        let effective_thumb_top = event
            .coords
            .y
            .saturating_sub(y_offset as isize + from_top)
            .max(0) as usize;

        // Convert thumb top into a row index by reversing the math
        // in ScrollHit::thumb
        let row = ScrollHit::thumb_top_to_scroll_top(
            effective_thumb_top,
            &*pane,
            current_viewport,
            self.dimensions.pixel_height.saturating_sub(
                y_offset as usize + border.bottom.get() + bottom_bar_height as usize,
            ),
            self.min_scroll_bar_height() as usize,
        );
        self.set_viewport(pane.pane_id(), Some(row), dims);
        context.invalidate();
        self.dragging.replace((item, start_event));
    }

    fn drag_ui_item(
        &mut self,
        item: UIItem,
        start_event: MouseEvent,
        x: usize,
        y: i64,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        match item.item_type {
            UIItemType::Split(split) => {
                self.drag_split(item, split, start_event, x, y, context);
            }
            UIItemType::ScrollThumb => {
                self.drag_scroll_thumb(item, start_event, event, context);
            }
            _ => {
                log::error!("drag not implemented for {:?}", item);
            }
        }
    }

    fn mouse_event_ui_item(
        &mut self,
        item: UIItem,
        pane: Arc<dyn Pane>,
        _y: i64,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        // Debug log mouse event details for activity and goal items
        match &item.item_type {
            UIItemType::ActivityItemText { .. } => {
                log::debug!(
                    "mouse_event_ui_item ActivityItemText - event.mouse_buttons: {:?}",
                    event.mouse_buttons
                );
            }
            UIItemType::GoalText { .. } => {
                log::debug!(
                    "mouse_event_ui_item GoalText - event.mouse_buttons: {:?}",
                    event.mouse_buttons
                );
            }
            _ => {}
        }

        self.last_ui_item.replace(item.clone());

        // Debug logging for goal text
        if matches!(&item.item_type, UIItemType::GoalText { .. }) {
            log::debug!(
                "GOAL EVENT DEBUG: mouse_event_ui_item called with GoalText - event kind={:?}, bounds=({},{},{},{})",
                event.kind, item.x, item.y, item.width, item.height
            );
        }

        match &item.item_type {
            UIItemType::TabBar(tab_item) => {
                self.mouse_event_tab_bar(tab_item.clone(), event, context);
            }
            UIItemType::AboveScrollThumb => {
                self.mouse_event_above_scroll_thumb(item.clone(), pane, event, context);
            }
            UIItemType::ScrollThumb => {
                self.mouse_event_scroll_thumb(item.clone(), pane, event, context);
            }
            UIItemType::BelowScrollThumb => {
                self.mouse_event_below_scroll_thumb(item.clone(), pane, event, context);
            }
            UIItemType::Split(split) => {
                self.mouse_event_split(item.clone(), split.clone(), event, context);
            }
            UIItemType::CloseTab(idx) => {
                self.mouse_event_close_tab(*idx, event, context);
            }
            UIItemType::SidebarButton(position) => {
                self.mouse_event_sidebar_button(*position, event, context);
            }
            UIItemType::Sidebar(position) => {
                self.mouse_event_sidebar(*position, event, context);
            }
            UIItemType::SidebarFilterChip(filter) => {
                self.mouse_event_sidebar_filter_chip(*filter, event, context);
            }
            UIItemType::ShowMoreButton(suggestion_id) => {
                self.mouse_event_show_more_button(suggestion_id.clone(), event, context);
            }
            UIItemType::SuggestionRunButton => {
                self.mouse_event_suggestion_run_button(event, context);
            }
            UIItemType::SuggestionDismissButton => {
                self.mouse_event_suggestion_dismiss_button(event, context);
            }
            UIItemType::CodeBlockContent(block_id) => {
                self.mouse_event_code_block_content(block_id.clone(), event, context);
            }
            UIItemType::CodeBlockCopyButton(block_id) => {
                self.mouse_event_code_block_copy_button(block_id.clone(), event, context);
            }
            UIItemType::ModalCloseButton => {
                self.mouse_event_modal_close_button(event, context);
            }
            UIItemType::ChatInput { line_positions } => {
                self.mouse_event_chat_input(item.clone(), line_positions, event, context);
            }
            UIItemType::ActivityItemText {
                index,
                char_positions,
            } => {
                self.mouse_event_activity_item_text(*index, char_positions, event, context);
            }
            UIItemType::SuggestionText { char_positions } => {
                self.mouse_event_suggestion_text(char_positions, event, context);
            }
            UIItemType::GoalText { char_positions } => {
                self.mouse_event_goal_text(char_positions, event, context);
            }
            UIItemType::ActivityLogBackground => {
                self.mouse_event_activity_log_background(event, context);
            }
        }
    }

    pub fn mouse_event_sidebar_button(
        &mut self,
        position: crate::sidebar::SidebarPosition,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        // Set cursor to arrow for sidebar buttons
        context.set_cursor(Some(MouseCursor::Arrow));

        match event.kind {
            WMEK::Press(MousePress::Left) => {
                log::info!("Toggle sidebar {:?}", position);
                let mut sidebar_manager = self.sidebar_manager.borrow_mut();
                let was_expansion_width = sidebar_manager.get_window_expansion();
                log::info!("Current expansion width: {}", was_expansion_width);

                match position {
                    crate::sidebar::SidebarPosition::Left => {
                        sidebar_manager.toggle_left_sidebar();
                    }
                    crate::sidebar::SidebarPosition::Right => {
                        sidebar_manager.toggle_right_sidebar();
                    }
                }

                let new_expansion_width = sidebar_manager.get_window_expansion();
                log::info!("New expansion width: {}", new_expansion_width);
                drop(sidebar_manager);

                // If expansion state changed, we need to resize the window
                if was_expansion_width != new_expansion_width {
                    log::info!(
                        "Expansion changed from {} to {}",
                        was_expansion_width,
                        new_expansion_width
                    );

                    // For window resize, we need to work with the actual window dimensions
                    // The key insight: when hiding the sidebar, we want to shrink the window
                    // by the sidebar width. When showing it, we want to expand by the sidebar width.

                    let new_width = if new_expansion_width > was_expansion_width {
                        // Showing sidebar - expand window
                        self.dimensions.pixel_width
                            + (new_expansion_width - was_expansion_width) as usize
                    } else {
                        // Hiding sidebar - shrink window
                        self.dimensions
                            .pixel_width
                            .saturating_sub((was_expansion_width - new_expansion_width) as usize)
                    };

                    log::info!(
                        "Current window width: {}, new window width: {}",
                        self.dimensions.pixel_width,
                        new_width
                    );

                    // Trigger a resize to account for sidebar visibility change
                    if let Some(window) = self.window.as_ref() {
                        let window = window.clone();
                        // Use the TermWindow's set_inner_size which handles resizes_pending
                        self.set_inner_size(&window, new_width, self.dimensions.pixel_height);
                    }
                } else {
                    log::info!("No expansion change, just visibility toggle");
                }

                // Force immediate repaint to avoid transparent areas
                context.invalidate();
                if let Some(window) = self.window.as_ref() {
                    window.invalidate();
                }
            }
            _ => {}
        }
    }

    pub fn mouse_event_sidebar(
        &mut self,
        position: crate::sidebar::SidebarPosition,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        // Set cursor to arrow for sidebar
        context.set_cursor(Some(MouseCursor::Arrow));

        // Handle clicks on empty sidebar space to clear selection
        if let WMEK::Press(MousePress::Left) = event.kind {
            // Check if this is the right sidebar where selections happen
            if position == crate::sidebar::SidebarPosition::Right {
                let sidebar_manager = self.sidebar_manager.borrow();
                if let Some(sidebar) = sidebar_manager.get_right_sidebar() {
                    if let Ok(mut sidebar) = sidebar.lock() {
                        if let Some(ai_sidebar) = sidebar
                            .as_any_mut()
                            .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>(
                        ) {
                            // Clear any active selection when clicking on empty space
                            if ai_sidebar.clear_selection() {
                                context.invalidate();
                            }
                        }
                    }
                }
                drop(sidebar_manager);
            }
        }

        // Forward mouse events to the sidebar
        let mut sidebar_manager = self.sidebar_manager.borrow_mut();
        let sidebar = match position {
            crate::sidebar::SidebarPosition::Left => sidebar_manager.get_left_sidebar(),
            crate::sidebar::SidebarPosition::Right => sidebar_manager.get_right_sidebar(),
        };

        if let Some(sidebar) = sidebar {
            let mut sidebar_locked = sidebar.lock().unwrap();
            if let Ok(handled) = sidebar_locked.handle_mouse_event(&event) {
                if handled {
                    context.invalidate();
                }
            }
        }
    }

    pub fn mouse_event_close_tab(
        &mut self,
        idx: usize,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        match event.kind {
            WMEK::Press(MousePress::Left) => {
                log::debug!("Should close tab {}", idx);
                self.close_specific_tab(idx, true);
            }
            _ => {}
        }
        context.set_cursor(Some(MouseCursor::Arrow));
    }

    fn do_new_tab_button_click(&mut self, button: MousePress) {
        let pane = match self.get_active_pane_or_overlay() {
            Some(pane) => pane,
            None => return,
        };
        let action = match button {
            MousePress::Left => Some(KeyAssignment::SpawnTab(SpawnTabDomain::CurrentPaneDomain)),
            MousePress::Right => Some(KeyAssignment::ShowLauncher),
            MousePress::Middle => None,
        };

        async fn dispatch_new_tab_button(
            lua: Option<Rc<mlua::Lua>>,
            window: GuiWin,
            pane: MuxPane,
            button: MousePress,
            action: Option<KeyAssignment>,
        ) -> anyhow::Result<()> {
            let default_action = match lua {
                Some(lua) => {
                    let args = lua.pack_multi((
                        window.clone(),
                        pane,
                        format!("{button:?}"),
                        action.clone(),
                    ))?;
                    config::lua::emit_event(&lua, ("new-tab-button-click".to_string(), args))
                        .await
                        .map_err(|e| {
                            log::error!("while processing new-tab-button-click event: {:#}", e);
                            e
                        })?
                }
                None => true,
            };
            if let (true, Some(assignment)) = (default_action, action) {
                window.window.notify(TermWindowNotif::PerformAssignment {
                    pane_id: pane.0,
                    assignment,
                    tx: None,
                });
            }
            Ok(())
        }
        let window = GuiWin::new(self);
        let pane = MuxPane(pane.pane_id());
        promise::spawn::spawn(config::with_lua_config_on_main_thread(move |lua| {
            dispatch_new_tab_button(lua, window, pane, button, action)
        }))
        .detach();
    }

    pub fn mouse_event_tab_bar(
        &mut self,
        item: TabBarItem,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        match event.kind {
            WMEK::Press(MousePress::Left) => match item {
                TabBarItem::Tab { tab_idx, .. } => {
                    self.activate_tab(tab_idx as isize).ok();
                }
                TabBarItem::NewTabButton { .. } => {
                    self.do_new_tab_button_click(MousePress::Left);
                }
                TabBarItem::None | TabBarItem::LeftStatus | TabBarItem::RightStatus => {
                    let maximized = self
                        .window_state
                        .intersects(WindowState::MAXIMIZED | WindowState::FULL_SCREEN);
                    if let Some(ref window) = self.window {
                        if self.config.window_decorations
                            == WindowDecorations::INTEGRATED_BUTTONS | WindowDecorations::RESIZE
                        {
                            if self.last_mouse_click.as_ref().map(|c| c.streak) == Some(2) {
                                if maximized {
                                    window.restore();
                                } else {
                                    window.maximize();
                                }
                            }
                        }
                    }
                    // Potentially starting a drag by the tab bar
                    if !maximized {
                        self.window_drag_position.replace(event.clone());
                    }
                    context.request_drag_move();
                }
                TabBarItem::WindowButton(button) => {
                    use window::IntegratedTitleButton as Button;
                    if let Some(ref window) = self.window {
                        match button {
                            Button::Hide => window.hide(),
                            Button::Maximize => {
                                let maximized = self
                                    .window_state
                                    .intersects(WindowState::MAXIMIZED | WindowState::FULL_SCREEN);
                                if maximized {
                                    window.restore();
                                } else {
                                    window.maximize();
                                }
                            }
                            Button::Close => self.close_requested(&window.clone()),
                        }
                    }
                }
            },
            WMEK::Press(MousePress::Middle) => match item {
                TabBarItem::Tab { tab_idx, .. } => {
                    self.close_specific_tab(tab_idx, true);
                }
                TabBarItem::NewTabButton { .. } => {
                    self.do_new_tab_button_click(MousePress::Middle);
                }
                TabBarItem::None
                | TabBarItem::LeftStatus
                | TabBarItem::RightStatus
                | TabBarItem::WindowButton(_) => {}
            },
            WMEK::Press(MousePress::Right) => match item {
                TabBarItem::Tab { .. } => {
                    self.show_tab_navigator();
                }
                TabBarItem::NewTabButton { .. } => {
                    self.do_new_tab_button_click(MousePress::Right);
                }
                TabBarItem::None
                | TabBarItem::LeftStatus
                | TabBarItem::RightStatus
                | TabBarItem::WindowButton(_) => {}
            },
            WMEK::Move => match item {
                TabBarItem::None | TabBarItem::LeftStatus | TabBarItem::RightStatus => {
                    context.set_window_drag_position(event.screen_coords);
                }
                TabBarItem::WindowButton(window::IntegratedTitleButton::Maximize) => {
                    let item = self.last_ui_item.clone().unwrap();
                    let bounds: ::window::ScreenRect = euclid::rect(
                        item.x as isize - (event.coords.x as f32 as isize - event.screen_coords.x),
                        item.y as isize - (event.coords.y as isize - event.screen_coords.y),
                        item.width as isize,
                        item.height as isize,
                    );
                    context.set_maximize_button_position(bounds);
                }
                TabBarItem::WindowButton(_)
                | TabBarItem::Tab { .. }
                | TabBarItem::NewTabButton { .. } => {}
            },
            WMEK::VertWheel(n) => {
                if self.config.mouse_wheel_scrolls_tabs {
                    self.activate_tab_relative(if n < 1 { 1 } else { -1 }, true)
                        .ok();
                }
            }
            _ => {}
        }
        context.set_cursor(Some(MouseCursor::Arrow));
    }

    pub fn mouse_event_above_scroll_thumb(
        &mut self,
        _item: UIItem,
        pane: Arc<dyn Pane>,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        if let WMEK::Press(MousePress::Left) = event.kind {
            let dims = pane.get_dimensions();
            let current_viewport = self.get_viewport(pane.pane_id());
            // Page up
            self.set_viewport(
                pane.pane_id(),
                Some(
                    current_viewport
                        .unwrap_or(dims.physical_top)
                        .saturating_sub(self.terminal_size.rows.try_into().unwrap()),
                ),
                dims,
            );
            context.invalidate();
        }
        context.set_cursor(Some(MouseCursor::Arrow));
    }

    pub fn mouse_event_below_scroll_thumb(
        &mut self,
        _item: UIItem,
        pane: Arc<dyn Pane>,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        if let WMEK::Press(MousePress::Left) = event.kind {
            let dims = pane.get_dimensions();
            let current_viewport = self.get_viewport(pane.pane_id());
            // Page down
            self.set_viewport(
                pane.pane_id(),
                Some(
                    current_viewport
                        .unwrap_or(dims.physical_top)
                        .saturating_add(self.terminal_size.rows.try_into().unwrap()),
                ),
                dims,
            );
            context.invalidate();
        }
        context.set_cursor(Some(MouseCursor::Arrow));
    }

    pub fn mouse_event_scroll_thumb(
        &mut self,
        item: UIItem,
        _pane: Arc<dyn Pane>,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        if let WMEK::Press(MousePress::Left) = event.kind {
            // Start a scroll drag
            // self.scroll_drag_start = Some(from_top);
            self.dragging = Some((item, event));
        }
        context.set_cursor(Some(MouseCursor::Arrow));
    }

    pub fn mouse_event_split(
        &mut self,
        item: UIItem,
        split: PositionedSplit,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        context.set_cursor(Some(match &split.direction {
            SplitDirection::Horizontal => MouseCursor::SizeLeftRight,
            SplitDirection::Vertical => MouseCursor::SizeUpDown,
        }));

        if event.kind == WMEK::Press(MousePress::Left) {
            self.dragging.replace((item, event));
        }
    }

    fn mouse_event_terminal(
        &mut self,
        mut pane: Arc<dyn Pane>,
        position: ClickPosition,
        event: MouseEvent,
        context: &dyn WindowOps,
        capture_mouse: bool,
    ) {
        // Clear any sidebar focus when clicking on terminal
        if matches!(event.kind, WMEK::Press(_)) {
            // Set focus back to terminal for copy operations
            self.focus_area = FocusArea::Terminal;
            if let Ok(mut sidebar_manager) = self.sidebar_manager.try_borrow_mut() {
                // Clear focus from left sidebar if it exists
                if let Some(sidebar) = sidebar_manager.get_left_sidebar() {
                    if let Ok(mut sidebar) = sidebar.lock() {
                        sidebar.clear_focus();
                    }
                }
                // Clear focus from right sidebar if it exists
                if let Some(sidebar) = sidebar_manager.get_right_sidebar() {
                    if let Ok(mut sidebar) = sidebar.lock() {
                        sidebar.clear_focus();
                    }
                }
            }
        }

        let mut is_click_to_focus_pane = false;

        let ClickPosition {
            mut column,
            mut row,
            mut x_pixel_offset,
            mut y_pixel_offset,
        } = position;

        let is_already_captured = matches!(
            self.current_mouse_capture,
            Some(MouseCapture::TerminalPane(_))
        );

        for pos in self.get_panes_to_render() {
            if !is_already_captured
                && row >= pos.top as i64
                && row <= (pos.top + pos.height) as i64
                && column >= pos.left
                && column <= pos.left + pos.width
            {
                if pane.pane_id() != pos.pane.pane_id() {
                    // We're over a pane that isn't active
                    match &event.kind {
                        WMEK::Press(_) => {
                            let mux = Mux::get();
                            mux.get_active_tab_for_window(self.mux_window_id)
                                .map(|tab| tab.set_active_idx(pos.index));

                            pane = Arc::clone(&pos.pane);
                            is_click_to_focus_pane = true;
                        }
                        WMEK::Move => {
                            if self.config.pane_focus_follows_mouse {
                                let mux = Mux::get();
                                mux.get_active_tab_for_window(self.mux_window_id)
                                    .map(|tab| tab.set_active_idx(pos.index));

                                pane = Arc::clone(&pos.pane);
                                context.invalidate();
                            }
                        }
                        WMEK::Release(_) | WMEK::HorzWheel(_) => {}
                        WMEK::VertWheel(_) => {
                            // Let wheel events route to the hovered pane,
                            // even if it doesn't have focus
                            pane = Arc::clone(&pos.pane);
                            context.invalidate();
                        }
                    }
                }
                column = column.saturating_sub(pos.left);
                row = row.saturating_sub(pos.top as i64);
                break;
            } else if is_already_captured && pane.pane_id() == pos.pane.pane_id() {
                column = column.saturating_sub(pos.left);
                row = row.saturating_sub(pos.top as i64).max(0);

                if position.column < pos.left {
                    x_pixel_offset -= self.render_metrics.cell_size.width
                        * (pos.left as isize - position.column as isize);
                }
                if position.row < pos.top as i64 {
                    y_pixel_offset -= self.render_metrics.cell_size.height
                        * (pos.top as isize - position.row as isize);
                }

                break;
            }
        }

        if capture_mouse {
            self.current_mouse_capture = Some(MouseCapture::TerminalPane(pane.pane_id()));
        }

        let is_focused = if let Some(focused) = self.focused.as_ref() {
            !self.config.swallow_mouse_click_on_window_focus
                || (focused.elapsed() > Duration::from_millis(200))
        } else {
            false
        };

        if self.focused.is_some() && !is_focused {
            if matches!(&event.kind, WMEK::Press(_))
                && self.config.swallow_mouse_click_on_window_focus
            {
                // Entering click to focus state
                self.is_click_to_focus_window = true;
                context.invalidate();
                log::trace!("enter click to focus");
                return;
            }
        }
        if self.is_click_to_focus_window && matches!(&event.kind, WMEK::Release(_)) {
            // Exiting click to focus state
            self.is_click_to_focus_window = false;
            context.invalidate();
            log::trace!("exit click to focus");
            return;
        }

        let allow_action = if self.is_click_to_focus_window || !is_focused {
            matches!(&event.kind, WMEK::VertWheel(_) | WMEK::HorzWheel(_))
        } else {
            true
        };

        log::trace!(
            "is_focused={} allow_action={} event={:?}",
            is_focused,
            allow_action,
            event
        );

        let dims = pane.get_dimensions();
        let stable_row = self
            .get_viewport(pane.pane_id())
            .unwrap_or(dims.physical_top)
            + row as StableRowIndex;

        self.pane_state(pane.pane_id())
            .mouse_terminal_coords
            .replace((
                ClickPosition {
                    column,
                    row,
                    x_pixel_offset,
                    y_pixel_offset,
                },
                stable_row,
            ));

        pane.apply_hyperlinks(stable_row..stable_row + 1, &self.config.hyperlink_rules);

        struct FindCurrentLink {
            current: Option<Arc<Hyperlink>>,
            stable_row: StableRowIndex,
            column: usize,
        }

        impl WithPaneLines for FindCurrentLink {
            fn with_lines_mut(&mut self, stable_top: StableRowIndex, lines: &mut [&mut Line]) {
                if stable_top == self.stable_row {
                    if let Some(line) = lines.get(0) {
                        if let Some(cell) = line.get_cell(self.column) {
                            self.current = cell.attrs().hyperlink().cloned();
                        }
                    }
                }
            }
        }

        let mut find_link = FindCurrentLink {
            current: None,
            stable_row,
            column,
        };
        pane.with_lines_mut(stable_row..stable_row + 1, &mut find_link);
        let new_highlight = find_link.current;

        match (self.current_highlight.as_ref(), new_highlight) {
            (Some(old_link), Some(new_link)) if Arc::ptr_eq(&old_link, &new_link) => {
                // Unchanged
            }
            (None, None) => {
                // Unchanged
            }
            (_, rhs) => {
                // We're hovering over a different URL, so invalidate and repaint
                // so that we render the underline correctly
                self.current_highlight = rhs;
                context.invalidate();
            }
        };

        let outside_window = event.coords.x < 0
            || event.coords.x as usize > self.dimensions.pixel_width
            || event.coords.y < 0
            || event.coords.y as usize > self.dimensions.pixel_height;

        context.set_cursor(Some(if self.current_highlight.is_some() {
            // When hovering over a hyperlink, show an appropriate
            // mouse cursor to give the cue that it is clickable
            MouseCursor::Hand
        } else if pane.is_mouse_grabbed() || outside_window {
            MouseCursor::Arrow
        } else {
            MouseCursor::Text
        }));

        let event_trigger_type = match &event.kind {
            WMEK::Press(press) => {
                let press = mouse_press_to_tmb(press);
                match self.last_mouse_click.as_ref() {
                    Some(LastMouseClick { streak, button, .. }) if *button == press => {
                        Some(MouseEventTrigger::Down {
                            streak: *streak,
                            button: press,
                        })
                    }
                    _ => None,
                }
            }
            WMEK::Release(press) => {
                let press = mouse_press_to_tmb(press);
                match self.last_mouse_click.as_ref() {
                    Some(LastMouseClick { streak, button, .. }) if *button == press => {
                        Some(MouseEventTrigger::Up {
                            streak: *streak,
                            button: press,
                        })
                    }
                    _ => None,
                }
            }
            WMEK::Move => {
                if !self.current_mouse_buttons.is_empty() {
                    if let Some(LastMouseClick { streak, button, .. }) =
                        self.last_mouse_click.as_ref()
                    {
                        if Some(*button)
                            == self.current_mouse_buttons.last().map(mouse_press_to_tmb)
                        {
                            Some(MouseEventTrigger::Drag {
                                streak: *streak,
                                button: *button,
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            WMEK::VertWheel(amount) => Some(match *amount {
                0 => return,
                1.. => MouseEventTrigger::Down {
                    streak: 1,
                    button: MouseButton::WheelUp(*amount as usize),
                },
                _ => MouseEventTrigger::Down {
                    streak: 1,
                    button: MouseButton::WheelDown(-amount as usize),
                },
            }),
            WMEK::HorzWheel(amount) => Some(match *amount {
                0 => return,
                1.. => MouseEventTrigger::Down {
                    streak: 1,
                    button: MouseButton::WheelLeft(*amount as usize),
                },
                _ => MouseEventTrigger::Down {
                    streak: 1,
                    button: MouseButton::WheelRight(-amount as usize),
                },
            }),
        };

        if allow_action {
            if let Some(mut event_trigger_type) = event_trigger_type {
                self.current_event = Some(event_trigger_type.to_dynamic());
                let mut modifiers = event.modifiers;

                // Since we use shift to force assessing the mouse bindings, pretend
                // that shift is not one of the mods when the mouse is grabbed.
                let mut mouse_reporting = pane.is_mouse_grabbed();
                if mouse_reporting {
                    if modifiers.contains(self.config.bypass_mouse_reporting_modifiers) {
                        modifiers.remove(self.config.bypass_mouse_reporting_modifiers);
                        mouse_reporting = false;
                    }
                }

                if mouse_reporting {
                    // If they were scrolled back prior to launching an
                    // application that captures the mouse, then mouse based
                    // scrolling assignments won't have any effect.
                    // Ensure that we scroll to the bottom if they try to
                    // use the mouse so that things are less surprising
                    self.scroll_to_bottom(&pane);
                }

                // normalize delta and streak to make mouse assignment
                // easier to wrangle
                match event_trigger_type {
                    MouseEventTrigger::Down {
                        ref mut streak,
                        button:
                            MouseButton::WheelUp(ref mut delta)
                            | MouseButton::WheelDown(ref mut delta)
                            | MouseButton::WheelLeft(ref mut delta)
                            | MouseButton::WheelRight(ref mut delta),
                    }
                    | MouseEventTrigger::Up {
                        ref mut streak,
                        button:
                            MouseButton::WheelUp(ref mut delta)
                            | MouseButton::WheelDown(ref mut delta)
                            | MouseButton::WheelLeft(ref mut delta)
                            | MouseButton::WheelRight(ref mut delta),
                    }
                    | MouseEventTrigger::Drag {
                        ref mut streak,
                        button:
                            MouseButton::WheelUp(ref mut delta)
                            | MouseButton::WheelDown(ref mut delta)
                            | MouseButton::WheelLeft(ref mut delta)
                            | MouseButton::WheelRight(ref mut delta),
                    } => {
                        *streak = 1;
                        *delta = 1;
                    }
                    _ => {}
                };

                let mouse_mods = config::MouseEventTriggerMods {
                    mods: modifiers,
                    mouse_reporting,
                    alt_screen: if pane.is_alt_screen_active() {
                        MouseEventAltScreen::True
                    } else {
                        MouseEventAltScreen::False
                    },
                };

                if let Some(action) = self.input_map.lookup_mouse(event_trigger_type, mouse_mods) {
                    self.perform_key_assignment(&pane, &action).ok();
                    return;
                }
            }
        }

        let mouse_event = wezterm_term::MouseEvent {
            kind: match event.kind {
                WMEK::Move => TMEK::Move,
                WMEK::VertWheel(_) | WMEK::HorzWheel(_) | WMEK::Press(_) => TMEK::Press,
                WMEK::Release(_) => TMEK::Release,
            },
            button: match event.kind {
                WMEK::Release(ref press) | WMEK::Press(ref press) => mouse_press_to_tmb(press),
                WMEK::Move => {
                    if event.mouse_buttons == WMB::LEFT {
                        TMB::Left
                    } else if event.mouse_buttons == WMB::RIGHT {
                        TMB::Right
                    } else if event.mouse_buttons == WMB::MIDDLE {
                        TMB::Middle
                    } else {
                        TMB::None
                    }
                }
                WMEK::VertWheel(amount) => {
                    if amount > 0 {
                        TMB::WheelUp(amount as usize)
                    } else {
                        TMB::WheelDown((-amount) as usize)
                    }
                }
                WMEK::HorzWheel(amount) => {
                    if amount > 0 {
                        TMB::WheelLeft(amount as usize)
                    } else {
                        TMB::WheelRight((-amount) as usize)
                    }
                }
            },
            x: column,
            y: row,
            x_pixel_offset,
            y_pixel_offset,
            modifiers: event.modifiers,
        };

        if allow_action
            && !(self.config.swallow_mouse_click_on_pane_focus && is_click_to_focus_pane)
        {
            pane.mouse_event(mouse_event).ok();
        }

        match event.kind {
            WMEK::Move => {}
            _ => {
                context.invalidate();
            }
        }
    }

    pub fn mouse_event_sidebar_filter_chip(
        &mut self,
        filter: crate::sidebar::ai_sidebar::ActivityFilter,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        if let WMEK::Press(MousePress::Left) = event.kind {
            log::info!("Filter chip clicked via UIItem: {:?}", filter);

            // Get the sidebar manager and update the filter
            let sidebar_manager = self.sidebar_manager.borrow();
            if let Some(sidebar) = sidebar_manager.get_right_sidebar() {
                let mut sidebar_locked = sidebar.lock().unwrap();
                if let Some(ai_sidebar) = sidebar_locked
                    .as_any_mut()
                    .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>()
                {
                    ai_sidebar.activity_filter = filter;
                    context.invalidate();
                }
            }
        }
    }

    pub fn mouse_event_show_more_button(
        &mut self,
        suggestion_id: String,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        if let WMEK::Press(MousePress::Left) = event.kind {
            log::info!(
                "Show more button clicked via UIItem for suggestion: {}",
                suggestion_id
            );

            // Get the sidebar manager and show the modal
            let sidebar_manager = self.sidebar_manager.borrow();
            if let Some(sidebar) = sidebar_manager.get_right_sidebar() {
                let mut sidebar_locked = sidebar.lock().unwrap();
                if let Some(ai_sidebar) = sidebar_locked
                    .as_any_mut()
                    .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>()
                {
                    // Get the current suggestion
                    if let Some(suggestion) = ai_sidebar.get_current_suggestion() {
                        ai_sidebar.show_suggestion_modal(suggestion.clone());
                        context.invalidate();
                    }
                }
            }
        }
    }

    pub fn mouse_event_suggestion_run_button(
        &mut self,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        if let WMEK::Press(MousePress::Left) = event.kind {
            log::info!("Run button clicked via UIItem");
            // TODO: Implement actual run functionality
            // For now, just close the modal
            let sidebar_manager = self.sidebar_manager.borrow();
            if let Some(sidebar) = sidebar_manager.get_right_sidebar() {
                let mut sidebar_locked = sidebar.lock().unwrap();
                if let Some(ai_sidebar) = sidebar_locked
                    .as_any_mut()
                    .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>()
                {
                    ai_sidebar.close_modal();
                    context.invalidate();
                }
            }
        }
    }

    pub fn mouse_event_suggestion_dismiss_button(
        &mut self,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        if let WMEK::Press(MousePress::Left) = event.kind {
            log::info!("Dismiss button clicked via UIItem");
            // TODO: Implement actual dismiss functionality (remove from suggestion list)
            // For now, just close the modal
            let sidebar_manager = self.sidebar_manager.borrow();
            if let Some(sidebar) = sidebar_manager.get_right_sidebar() {
                let mut sidebar_locked = sidebar.lock().unwrap();
                if let Some(ai_sidebar) = sidebar_locked
                    .as_any_mut()
                    .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>()
                {
                    ai_sidebar.close_modal();
                    context.invalidate();
                }
            }
        }
    }

    pub fn mouse_event_code_block_content(
        &mut self,
        _block_id: String,
        _event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        // Code blocks now use line wrapping - no special interaction needed
        context.set_cursor(Some(MouseCursor::Text));
    }

    pub fn mouse_event_code_block_copy_button(
        &mut self,
        block_id: String,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        context.set_cursor(Some(MouseCursor::Arrow));

        match event.kind {
            WMEK::Press(MousePress::Left) => {
                log::info!("Copy code block: {}", block_id);

                // Get the sidebar to access the code content
                let sidebar_manager = self.sidebar_manager.borrow();
                if let Some(sidebar) = sidebar_manager.get_right_sidebar() {
                    let sidebar_locked = sidebar.lock().unwrap();
                    if let Some(ai_sidebar) = sidebar_locked
                        .as_any()
                        .downcast_ref::<crate::sidebar::ai_sidebar::AiSidebar>(
                    ) {
                        // Get the code block registry
                        if let Some(ref registry) = ai_sidebar.code_block_registry {
                            if let Ok(mut reg) = registry.lock() {
                                if let Some(container) = reg.get_mut(&block_id) {
                                    // Copy the raw code content
                                    self.copy_to_clipboard(
                                        config::keyassignment::ClipboardCopyDestination::ClipboardAndPrimarySelection,
                                        container.raw_code.clone()
                                    );

                                    // Set copy success time for visual feedback
                                    container.copy_success_time = Some(std::time::Instant::now());

                                    // Log success with language info
                                    let lang_info =
                                        container.language.as_deref().unwrap_or("plain text");
                                    log::info!("Copied {} code block to clipboard", lang_info);

                                    context.invalidate();
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub fn mouse_event_modal_close_button(&mut self, event: MouseEvent, context: &dyn WindowOps) {
        context.set_cursor(Some(MouseCursor::Arrow));

        match event.kind {
            WMEK::Press(MousePress::Left) => {
                log::debug!("Modal close button clicked");

                // Close modal through sidebar manager
                if let Ok(mut mgr) = self.sidebar_manager.try_borrow_mut() {
                    if let Some(sidebar) = mgr.get_right_sidebar() {
                        if let Ok(mut sidebar) = sidebar.lock() {
                            // Direct access to modal manager
                            if let Some(ai_sidebar) = sidebar
                                .as_any_mut()
                                .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>(
                            ) {
                                ai_sidebar.modal_manager.close();
                                context.invalidate();
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub fn mouse_event_chat_input(
        &mut self,
        item: UIItem,
        line_positions: &Vec<Vec<(f32, f32, usize)>>,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        context.set_cursor(Some(MouseCursor::Text));

        match event.kind {
            WMEK::Press(MousePress::Left) => {
                // Set focus to sidebar for copy operations
                self.focus_area = FocusArea::Sidebar;
                log::debug!(
                    "Chat input clicked at ({}, {})",
                    event.coords.x,
                    event.coords.y
                );

                // Get the AI sidebar and handle click with position
                if let Ok(mut mgr) = self.sidebar_manager.try_borrow_mut() {
                    if let Some(sidebar) = mgr.get_right_sidebar() {
                        if let Ok(mut sidebar) = sidebar.lock() {
                            if let Some(ai_sidebar) = sidebar
                                .as_any_mut()
                                .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>(
                            ) {
                                // Clear any existing non-chat-input selection when clicking in chat input
                                if let Some(selection) =
                                    &ai_sidebar.selection_state.active_selection
                                {
                                    match selection {
                                        SelectionTarget::ChatInput { .. } => {
                                            // Don't clear chat input selections - let the handler manage them
                                        }
                                        _ => {
                                            // Clear other types of selections (goal, activity, suggestion)
                                            ai_sidebar.clear_selection();
                                        }
                                    }
                                }

                                // Get the bounds of the chat input from the UI item
                                let bounds = euclid::rect::<f32, euclid::UnknownUnit>(
                                    item.x as f32,
                                    item.y as f32,
                                    item.width as f32,
                                    item.height as f32,
                                );

                                // Handle click with pre-calculated character positions
                                let relative_x = event.coords.x as f32 - bounds.origin.x;
                                let relative_y = event.coords.y as f32 - bounds.origin.y;

                                // Get the exact glyph positions from the sidebar
                                // (UIItemType line_positions are empty placeholders)
                                let glyph_positions =
                                    ai_sidebar.get_chat_input_glyph_positions().clone();
                                ai_sidebar.handle_chat_input_click_with_positions(
                                    relative_x,
                                    relative_y,
                                    &glyph_positions,
                                    false, // Not a drag
                                    event.modifiers.contains(window::Modifiers::SHIFT),
                                );
                                context.invalidate(); // Trigger repaint to show cursor position
                            }
                        }
                    }
                }
            }
            WMEK::Move => {
                // Handle drag selection if left button is held
                if event.mouse_buttons.contains(MouseButtons::LEFT) {
                    log::debug!(
                        "Chat input drag at ({}, {})",
                        event.coords.x,
                        event.coords.y
                    );

                    if let Ok(mut mgr) = self.sidebar_manager.try_borrow_mut() {
                        if let Some(sidebar) = mgr.get_right_sidebar() {
                            if let Ok(mut sidebar) = sidebar.lock() {
                                if let Some(ai_sidebar) = sidebar
                                    .as_any_mut()
                                    .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>(
                                ) {
                                    // First check if we have a prepared selection to activate
                                    if !ai_sidebar.is_selecting() {
                                        if let Some(prepared) =
                                            ai_sidebar.selection_state.prepared_selection.clone()
                                        {
                                            ai_sidebar.selection_state.active_selection =
                                                Some(prepared);
                                            ai_sidebar.selection_state.is_dragging = true;
                                        }
                                    }

                                    // Get the bounds of the chat input
                                    let bounds = euclid::rect::<f32, euclid::UnknownUnit>(
                                        item.x as f32,
                                        item.y as f32,
                                        item.width as f32,
                                        item.height as f32,
                                    );

                                    let relative_x = event.coords.x as f32 - bounds.origin.x;
                                    let relative_y = event.coords.y as f32 - bounds.origin.y;

                                    // Get the exact glyph positions from the sidebar
                                    let glyph_positions =
                                        ai_sidebar.get_chat_input_glyph_positions().clone();
                                    ai_sidebar.handle_chat_input_click_with_positions(
                                        relative_x,
                                        relative_y,
                                        &glyph_positions,
                                        true,  // Is a drag
                                        false, // Shift not relevant during drag
                                    );
                                    context.invalidate();
                                }
                            }
                        }
                    }
                }
            }
            WMEK::Release(MousePress::Left) => {
                // End drag selection
                if let Ok(mut mgr) = self.sidebar_manager.try_borrow_mut() {
                    if let Some(sidebar) = mgr.get_right_sidebar() {
                        if let Ok(mut sidebar) = sidebar.lock() {
                            if let Some(ai_sidebar) = sidebar
                                .as_any_mut()
                                .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>(
                            ) {
                                ai_sidebar.selection_state.is_dragging = false;
                                ai_sidebar.selection_state.prepared_selection = None;
                            }
                        }
                    }
                }
            }
            WMEK::VertWheel(amount) => {
                // Handle scroll wheel events for chat input
                log::debug!("Chat input scroll wheel event: amount={}", amount);

                // Get the AI sidebar and forward the scroll event
                if let Ok(mut mgr) = self.sidebar_manager.try_borrow_mut() {
                    if let Some(sidebar) = mgr.get_right_sidebar() {
                        if let Ok(mut sidebar) = sidebar.lock() {
                            if let Some(ai_sidebar) = sidebar
                                .as_any_mut()
                                .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>(
                            ) {
                                // Forward scroll event to chat input (works even without focus)
                                // Handle wheel event and invalidate if something changed
                                if ai_sidebar.handle_chat_input_wheel_simple(amount) {
                                    context.invalidate();
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub fn mouse_event_activity_item_text(
        &mut self,
        index: usize,
        char_positions: &[(f32, f32, usize)],
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        log::debug!(
            "mouse_event_activity_item_text called with event.kind: {:?}",
            event.kind
        );
        context.set_cursor(Some(MouseCursor::Text));

        // Set mouse capture on press to ensure drag events work properly
        if matches!(event.kind, WMEK::Press(MousePress::Left)) {
            log::debug!("Setting text_selection_drag_active = true");
            self.current_mouse_capture = Some(MouseCapture::TextSelection);
            self.text_selection_drag_active = true;
        }

        match event.kind {
            WMEK::Press(MousePress::Left) => {
                // Set focus to sidebar for copy operations
                self.focus_area = FocusArea::Sidebar;

                let window_point =
                    euclid::Point2D::new(event.coords.x as f32, event.coords.y as f32);
                log::debug!(
                    "Activity item {} text clicked at ({}, {})",
                    index,
                    window_point.x,
                    window_point.y
                );

                // Use hierarchical hit testing to find exact text position
                let hit_result = with_ai_sidebar(&self.sidebar_manager, |ai_sidebar| {
                    log::debug!(
                        "Calling hit_test_activity_log for window point {:?}",
                        window_point
                    );
                    let result = ai_sidebar.hit_test_activity_log(window_point);
                    log::debug!("Hit test result: {:?}", result);
                    result
                })
                .flatten();

                if let Some(ref hit) = hit_result {
                    log::debug!(
                        "Hit result: item_index={}, byte_offset={}",
                        hit.item_index,
                        hit.position_in_item.byte_offset
                    );
                    if hit.item_index == index {
                        // Start selection at the hit position
                        log::debug!(
                            "Starting selection at item {} byte offset {}",
                            hit.item_index,
                            hit.position_in_item.byte_offset
                        );
                        with_ai_sidebar(&self.sidebar_manager, |ai_sidebar| {
                            ai_sidebar.start_activity_log_selection(
                                hit.item_index,
                                hit.position_in_item.byte_offset,
                            );
                            Some(())
                        });
                        context.invalidate();
                        return;
                    }
                } else {
                    log::debug!("Hit test returned None");
                }

                // Fallback to character positions if hierarchical hit testing fails
                // TODO: Remove this fallback once position extraction is complete
                let byte_offset = if let Some(ref hit) = hit_result {
                    if hit.item_index == index {
                        log::debug!(
                            "Hit test found byte_offset = {} for item {}",
                            hit.position_in_item.byte_offset,
                            index
                        );
                        hit.position_in_item.byte_offset
                    } else {
                        log::warn!(
                            "Hit test returned different item index: {} vs expected {}",
                            hit.item_index,
                            index
                        );
                        // Don't fall back - this indicates a coordinate system issue
                        log::error!(
                            "Hit test returned wrong item index - coordinate system mismatch"
                        );
                        return self.window.as_ref().unwrap().clone().invalidate();
                    }
                } else {
                    log::debug!("Hit test returned None - position data may not be ready");
                    // Don't fall back to imprecise character positions
                    // The position data may not be extracted yet for this item
                    return self.window.as_ref().unwrap().clone().invalidate();
                };

                log::debug!("Final byte_offset = {}", byte_offset);

                // Store the potential selection start but don't activate selection yet
                // Selection will only start when dragging begins
                log::debug!(
                    "SELECTION_DEBUG: Preparing selection for item {} at byte {}",
                    index,
                    byte_offset
                );
                let needs_invalidate = with_ai_sidebar(&self.sidebar_manager, |ai_sidebar| {
                    ai_sidebar.prepare_selection(SelectionTarget::ActivityItem {
                        anchor_index: index,
                        anchor_byte: byte_offset,
                        current_index: index,
                        current_byte: byte_offset,
                    })
                })
                .unwrap_or(false);

                if needs_invalidate {
                    context.invalidate();
                }
            }
            WMEK::Move => {
                log::debug!("ActivityItem WMEK::Move event - event.mouse_buttons: {:?}, current_mouse_buttons: {:?}, text_selection_drag: {}", 
                          event.mouse_buttons, self.current_mouse_buttons, self.text_selection_drag_active);
                // Check our text selection drag state instead of relying on event.mouse_buttons
                // which can be lost when UI items are rebuilt
                if self.text_selection_drag_active {
                    log::debug!(
                        "ActivityItem drag detected at ({}, {})",
                        event.coords.x,
                        event.coords.y
                    );
                    let window_point =
                        euclid::Point2D::new(event.coords.x as f32, event.coords.y as f32);

                    // Use hierarchical hit testing for drag position
                    let hit_result = with_ai_sidebar(&self.sidebar_manager, |ai_sidebar| {
                        ai_sidebar.hit_test_activity_log(window_point)
                    })
                    .flatten();

                    if let Some(hit) = hit_result {
                        log::debug!(
                            "SELECTION_DEBUG: Hit test during drag found item {} at byte {}",
                            hit.item_index,
                            hit.position_in_item.byte_offset
                        );
                        with_ai_sidebar(&self.sidebar_manager, |ai_sidebar| {
                            // Activate selection if not already active
                            if !ai_sidebar.is_selecting() {
                                log::debug!("SELECTION_DEBUG: Selection not active, activating prepared selection");
                                ai_sidebar.activate_prepared_selection();
                            }
                            // Update selection with new hit position
                            // This handles crossing item boundaries
                            ai_sidebar.update_activity_log_selection_drag(
                                hit.item_index,
                                hit.position_in_item.byte_offset,
                            );
                        });
                        context.invalidate(); // Ensure UI updates during drag
                    } else {
                        // If hit testing fails during drag, we're likely outside the activity log
                        // Continue with the last valid position
                        log::debug!(
                            "Hit test failed during drag - mouse likely outside activity log"
                        );
                    }
                }
            }
            WMEK::Release(MousePress::Left) => {
                // End selection
                log::debug!(
                    "ActivityItem Release: clearing text_selection_drag_active (was {})",
                    self.text_selection_drag_active
                );
                self.text_selection_drag_active = false;
                with_ai_sidebar(&self.sidebar_manager, |ai_sidebar| {
                    ai_sidebar.end_selection();
                });
            }
            WMEK::VertWheel(_) => {
                // Forward scroll events to the sidebar handler
                self.mouse_event_sidebar(crate::sidebar::SidebarPosition::Right, event, context);
            }
            _ => {}
        }
    }

    pub fn mouse_event_suggestion_text(
        &mut self,
        char_positions: &[(f32, f32, usize)],
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        context.set_cursor(Some(MouseCursor::Text));

        match event.kind {
            WMEK::Press(MousePress::Left) => {
                let x = event.coords.x as f32;
                log::debug!("Suggestion text clicked at x={}", x);

                // Find the byte offset from the character positions
                let byte_offset = find_byte_offset_from_x(x, char_positions);

                // Start text selection for suggestion
                let needs_invalidate = if let Ok(mut mgr) = self.sidebar_manager.try_borrow_mut() {
                    if let Some(sidebar) = mgr.get_right_sidebar() {
                        if let Ok(mut sidebar) = sidebar.lock() {
                            if let Some(ai_sidebar) = sidebar
                                .as_any_mut()
                                .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>(
                            ) {
                                ai_sidebar.prepare_selection(SelectionTarget::Suggestion {
                                    anchor_byte: byte_offset,
                                    current_byte: byte_offset,
                                })
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                if needs_invalidate {
                    context.invalidate();
                }
            }
            WMEK::Move => {
                // Only start selection if mouse button is pressed (dragging)
                if event.mouse_buttons.contains(MouseButtons::LEFT) {
                    if let Ok(mut mgr) = self.sidebar_manager.try_borrow_mut() {
                        if let Some(sidebar) = mgr.get_right_sidebar() {
                            if let Ok(mut sidebar) = sidebar.lock() {
                                if let Some(ai_sidebar) = sidebar
                                    .as_any_mut()
                                    .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>(
                                ) {
                                    // Activate selection if not already active
                                    if !ai_sidebar.is_selecting() {
                                        ai_sidebar.activate_prepared_selection();
                                    }
                                    let x = event.coords.x as f32;

                                    // Transform to relative coordinates using goal bounds
                                    let relative_x = if let Some(bounds) =
                                        ai_sidebar.get_goal_bounds()
                                    {
                                        let rel_x = x - bounds.origin.x - 8.0; // Subtract padding (matches GOAL_CARD_PADDING in ai_sidebar.rs)
                                        log::debug!("GOAL EVENT DEBUG (drag): Relative x={} (absolute {} - bounds.x {} - padding 8)", 
                                            rel_x, x, bounds.origin.x);
                                        rel_x
                                    } else {
                                        log::debug!("GOAL EVENT DEBUG (drag): No goal bounds, using absolute x={}", x);
                                        x
                                    };

                                    let byte_offset =
                                        find_byte_offset_from_x(relative_x, char_positions);
                                    log::debug!("GOAL EVENT DEBUG (drag): Goal drag - byte_offset={} using relative_x={}", 
                                        byte_offset, relative_x);
                                    ai_sidebar.update_selection_drag(byte_offset);
                                }
                            }
                        }
                    }
                }
            }
            WMEK::Release(MousePress::Left) => {
                if let Ok(mut mgr) = self.sidebar_manager.try_borrow_mut() {
                    if let Some(sidebar) = mgr.get_right_sidebar() {
                        if let Ok(mut sidebar) = sidebar.lock() {
                            if let Some(ai_sidebar) = sidebar
                                .as_any_mut()
                                .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>(
                            ) {
                                ai_sidebar.end_selection();
                            }
                        }
                    }
                }
            }
            WMEK::VertWheel(_) => {
                // Forward scroll events to the sidebar handler
                self.mouse_event_sidebar(crate::sidebar::SidebarPosition::Right, event, context);
            }
            _ => {}
        }
    }

    pub fn mouse_event_goal_text(
        &mut self,
        char_positions: &[(f32, f32, usize)],
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        log::debug!(
            "GOAL EVENT DEBUG: mouse_event_goal_text called - event kind={:?}",
            event.kind
        );
        context.set_cursor(Some(MouseCursor::Text));

        // Set mouse capture on press to ensure drag events work properly
        if matches!(event.kind, WMEK::Press(MousePress::Left)) {
            self.current_mouse_capture = Some(MouseCapture::TextSelection);
            self.text_selection_drag_active = true;
        }

        match event.kind {
            WMEK::Press(MousePress::Left) => {
                // Set focus to sidebar for copy operations
                self.focus_area = FocusArea::Sidebar;

                let x = event.coords.x as f32;
                log::debug!("GOAL EVENT DEBUG: Goal text clicked at absolute x={}", x);
                log::debug!(
                    "GOAL EVENT DEBUG: Goal char_positions count: {}",
                    char_positions.len()
                );

                // Get goal bounds to transform coordinates
                let relative_x = if let Ok(mgr) = self.sidebar_manager.try_borrow() {
                    if let Some(sidebar) = mgr.get_right_sidebar() {
                        if let Ok(sidebar) = sidebar.lock() {
                            if let Some(ai_sidebar) = sidebar
                                .as_any()
                                .downcast_ref::<crate::sidebar::ai_sidebar::AiSidebar>(
                            ) {
                                if let Some(bounds) = ai_sidebar.get_goal_bounds() {
                                    let rel_x = x - bounds.origin.x - 8.0; // Subtract padding
                                    log::debug!("GOAL EVENT DEBUG: Goal bounds: origin=({}, {}), size=({}, {})",
                                        bounds.origin.x, bounds.origin.y, bounds.size.width, bounds.size.height);
                                    log::debug!("GOAL EVENT DEBUG: Relative x={} (absolute {} - bounds.x {} - padding 8)", 
                                        rel_x, x, bounds.origin.x);
                                    rel_x
                                } else {
                                    log::debug!("GOAL EVENT DEBUG: No goal bounds available!");
                                    x
                                }
                            } else {
                                x
                            }
                        } else {
                            x
                        }
                    } else {
                        x
                    }
                } else {
                    x
                };

                // Debug: Log the first few and last few char positions to understand coordinate space
                if !char_positions.is_empty() {
                    log::debug!(
                        "GOAL EVENT DEBUG: First char position: {:?}",
                        char_positions.first()
                    );
                    log::debug!(
                        "GOAL EVENT DEBUG: Last char position: {:?}",
                        char_positions.last()
                    );
                    if char_positions.len() > 2 {
                        log::debug!(
                            "GOAL EVENT DEBUG: Second char position: {:?}",
                            char_positions.get(1)
                        );
                    }
                }

                // Get real positions from sidebar instead of using pre-calculated ones
                let byte_offset = if let Ok(mgr) = self.sidebar_manager.try_borrow() {
                    if let Some(sidebar) = mgr.get_right_sidebar() {
                        if let Ok(sidebar) = sidebar.lock() {
                            if let Some(ai_sidebar) = sidebar
                                .as_any()
                                .downcast_ref::<crate::sidebar::ai_sidebar::AiSidebar>(
                            ) {
                                // Use real positions if available, fall back to pre-calculated
                                if let Some(real_positions) = ai_sidebar.get_goal_positions() {
                                    log::debug!(
                                        "GOAL EVENT DEBUG: Using {} real character positions",
                                        real_positions.len()
                                    );
                                    find_byte_offset_from_x(relative_x, real_positions)
                                } else {
                                    log::debug!("GOAL EVENT DEBUG: No real positions, using {} pre-calculated positions", char_positions.len());
                                    find_byte_offset_from_x(relative_x, char_positions)
                                }
                            } else {
                                find_byte_offset_from_x(relative_x, char_positions)
                            }
                        } else {
                            find_byte_offset_from_x(relative_x, char_positions)
                        }
                    } else {
                        find_byte_offset_from_x(relative_x, char_positions)
                    }
                } else {
                    find_byte_offset_from_x(relative_x, char_positions)
                };
                log::debug!("GOAL EVENT DEBUG: Goal text clicked - calculated byte_offset: {} using relative_x={}", 
                    byte_offset, relative_x);

                // Start text selection for goal
                let needs_invalidate = if let Ok(mut mgr) = self.sidebar_manager.try_borrow_mut() {
                    if let Some(sidebar) = mgr.get_right_sidebar() {
                        if let Ok(mut sidebar) = sidebar.lock() {
                            if let Some(ai_sidebar) = sidebar
                                .as_any_mut()
                                .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>(
                            ) {
                                ai_sidebar.prepare_selection(SelectionTarget::Goal {
                                    anchor_byte: byte_offset,
                                    current_byte: byte_offset,
                                })
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                // Always invalidate on goal text click to ensure UI updates
                // This is needed because prepare_selection might clear an existing selection
                context.invalidate();
            }
            WMEK::Move => {
                // Only start selection if mouse button is pressed (dragging)
                if event.mouse_buttons.contains(MouseButtons::LEFT) {
                    if let Ok(mut mgr) = self.sidebar_manager.try_borrow_mut() {
                        if let Some(sidebar) = mgr.get_right_sidebar() {
                            if let Ok(mut sidebar) = sidebar.lock() {
                                if let Some(ai_sidebar) = sidebar
                                    .as_any_mut()
                                    .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>(
                                ) {
                                    // Activate selection if not already active
                                    if !ai_sidebar.is_selecting() {
                                        ai_sidebar.activate_prepared_selection();
                                    }
                                    let x = event.coords.x as f32;

                                    // Transform to relative coordinates using goal bounds
                                    let relative_x = if let Some(bounds) =
                                        ai_sidebar.get_goal_bounds()
                                    {
                                        let rel_x = x - bounds.origin.x - 8.0; // Subtract padding (matches GOAL_CARD_PADDING in ai_sidebar.rs)
                                        log::debug!("GOAL EVENT DEBUG (drag): Relative x={} (absolute {} - bounds.x {} - padding 8)", 
                                            rel_x, x, bounds.origin.x);
                                        rel_x
                                    } else {
                                        log::debug!("GOAL EVENT DEBUG (drag): No goal bounds, using absolute x={}", x);
                                        x
                                    };

                                    let byte_offset =
                                        find_byte_offset_from_x(relative_x, char_positions);
                                    log::debug!("GOAL EVENT DEBUG (drag): Goal drag - byte_offset={} using relative_x={}", 
                                        byte_offset, relative_x);
                                    ai_sidebar.update_selection_drag(byte_offset);
                                }
                            }
                        }
                    }
                }
            }
            WMEK::Release(MousePress::Left) => {
                if let Ok(mut mgr) = self.sidebar_manager.try_borrow_mut() {
                    if let Some(sidebar) = mgr.get_right_sidebar() {
                        if let Ok(mut sidebar) = sidebar.lock() {
                            if let Some(ai_sidebar) = sidebar
                                .as_any_mut()
                                .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>(
                            ) {
                                ai_sidebar.end_selection();
                            }
                        }
                    }
                }
            }
            WMEK::VertWheel(_) => {
                // Forward scroll events to the sidebar handler
                self.mouse_event_sidebar(crate::sidebar::SidebarPosition::Right, event, context);
            }
            _ => {}
        }
    }

    pub fn mouse_event_activity_log_background(
        &mut self,
        event: MouseEvent,
        context: &dyn WindowOps,
    ) {
        context.set_cursor(Some(MouseCursor::Arrow));

        match event.kind {
            WMEK::Press(MousePress::Left) => {
                // Clear any existing selection when clicking on the activity log background
                self.focus_area = FocusArea::Sidebar;
                log::debug!("Activity log background clicked - clearing selection");

                // Clear any active selection
                if let Ok(mut mgr) = self.sidebar_manager.try_borrow_mut() {
                    if let Some(sidebar) = mgr.get_right_sidebar() {
                        if let Ok(mut sidebar) = sidebar.lock() {
                            if let Some(ai_sidebar) = sidebar
                                .as_any_mut()
                                .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>(
                            ) {
                                ai_sidebar.clear_selection();
                            }
                        }
                    }
                }

                // Request window invalidation
                context.invalidate();
            }
            WMEK::VertWheel(_) => {
                // Forward scroll events to the sidebar handler
                self.mouse_event_sidebar(crate::sidebar::SidebarPosition::Right, event, context);
            }
            _ => {}
        }
    }
}

fn mouse_press_to_tmb(press: &MousePress) -> TMB {
    match press {
        MousePress::Left => TMB::Left,
        MousePress::Right => TMB::Right,
        MousePress::Middle => TMB::Middle,
    }
}
