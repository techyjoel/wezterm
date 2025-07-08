//! Shared scrollbar state management
//!
//! This module provides a unified scrollbar state that can be used by different
//! components (ScrollableContainer, ModalManager) while allowing them to keep
//! their own rendering approaches. This addresses the code duplication between
//! components while respecting WezTerm's architectural constraints.

use std::time::{Duration, Instant};

use super::scrollbar_helpers::MIN_THUMB_SIZE;

/// Shared scrollbar state that can be used by different components
#[derive(Clone, Debug)]
pub struct ScrollbarState {
    // Core scroll state
    pub scroll_offset: f32,
    pub content_height: f32,
    pub viewport_height: f32,

    // Interaction state
    pub is_hovering: bool,
    pub is_dragging: bool,
    pub drag_start_y: f32,
    pub drag_start_offset: f32,

    // Auto-hide state
    pub last_interaction: Option<Instant>,
    pub current_opacity: f32,
    pub fade_animation: Option<FadeAnimation>,
}

impl ScrollbarState {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0.0,
            content_height: 0.0,
            viewport_height: 0.0,
            is_hovering: false,
            is_dragging: false,
            drag_start_y: 0.0,
            drag_start_offset: 0.0,
            last_interaction: None,
            current_opacity: 0.0,
            fade_animation: None,
        }
    }

    /// Update dimensions
    pub fn set_dimensions(&mut self, content_height: f32, viewport_height: f32) {
        self.content_height = content_height;
        self.viewport_height = viewport_height;
    }

    /// Set scroll position (clamped to valid range)
    pub fn set_scroll_offset(&mut self, offset: f32) {
        self.scroll_offset = offset.max(0.0).min(self.max_scroll());
        self.record_interaction();
    }

    /// Get maximum scroll offset
    pub fn max_scroll(&self) -> f32 {
        (self.content_height - self.viewport_height).max(0.0)
    }

    /// Check if scrollbar is needed
    pub fn is_needed(&self) -> bool {
        self.content_height > self.viewport_height
    }

    /// Handle mouse wheel with configurable sensitivity
    pub fn handle_wheel(&mut self, delta: f32, lines_per_notch: f32) -> bool {
        if !self.is_needed() {
            return false;
        }

        const MIN_LINE_HEIGHT: f32 = 1.0;
        let line_height = (self.viewport_height / 30.0).max(MIN_LINE_HEIGHT); // Approximate visible lines
        let scroll_amount = delta * line_height * lines_per_notch;

        let old_offset = self.scroll_offset;
        self.set_scroll_offset(self.scroll_offset - scroll_amount);

        old_offset != self.scroll_offset
    }

    /// Start dragging at given position
    pub fn start_drag(&mut self, y_position: f32) {
        self.is_dragging = true;
        self.drag_start_y = y_position;
        self.drag_start_offset = self.scroll_offset;
        self.record_interaction();
    }

    /// Update drag to new position
    pub fn update_drag(&mut self, y_position: f32, scrollbar_height: f32) {
        if !self.is_dragging {
            return;
        }

        let thumb_info = self.calculate_thumb_geometry(scrollbar_height);
        let drag_delta = y_position - self.drag_start_y;
        let scrollable_track = scrollbar_height - thumb_info.height;

        if scrollable_track > 0.0 {
            let scroll_ratio = drag_delta / scrollable_track;
            let new_offset = self.drag_start_offset + (scroll_ratio * self.max_scroll());
            self.set_scroll_offset(new_offset);
        }
    }

    /// End dragging
    pub fn end_drag(&mut self) {
        self.is_dragging = false;
    }

    /// Handle click on scrollbar track (not thumb)
    pub fn handle_track_click(
        &mut self,
        click_y: f32,
        scrollbar_y: f32,
        scrollbar_height: f32,
    ) -> bool {
        if !self.is_needed() {
            return false;
        }

        let thumb_info = self.calculate_thumb_geometry(scrollbar_height);
        let thumb_top = scrollbar_y + thumb_info.y_offset;
        let thumb_bottom = thumb_top + thumb_info.height;

        // Check if click is on track (not thumb)
        if click_y < thumb_top {
            // Click above thumb - page up
            let old_offset = self.scroll_offset;
            self.set_scroll_offset(self.scroll_offset - self.viewport_height * 0.9);
            return old_offset != self.scroll_offset;
        } else if click_y > thumb_bottom {
            // Click below thumb - page down
            let old_offset = self.scroll_offset;
            self.set_scroll_offset(self.scroll_offset + self.viewport_height * 0.9);
            return old_offset != self.scroll_offset;
        }

        // Click was on thumb, not track
        false
    }

    /// Update hover state
    pub fn set_hovering(&mut self, hovering: bool) {
        if hovering && !self.is_hovering {
            self.record_interaction();
        }
        self.is_hovering = hovering;
    }

    /// Record interaction for auto-hide
    pub fn record_interaction(&mut self) {
        self.last_interaction = Some(Instant::now());
        // If we're invisible, start fade in
        if self.current_opacity < 1.0 && self.fade_animation.is_none() {
            self.fade_animation = Some(FadeAnimation::new(
                FadeDirection::In,
                Duration::from_millis(200),
            ));
        }
    }

    /// Calculate thumb geometry for rendering
    pub fn calculate_thumb_geometry(&self, scrollbar_height: f32) -> ThumbGeometry {
        let ratio = self.viewport_height / self.content_height;
        let thumb_height = (scrollbar_height * ratio)
            .max(MIN_THUMB_SIZE) // Min thumb height
            .min(scrollbar_height);

        // The scrollable track is the area where the thumb can move
        // It's the scrollbar height minus the thumb height
        let scrollable_track = scrollbar_height - thumb_height;
        let scroll_ratio = if self.max_scroll() > 0.0 {
            self.scroll_offset / self.max_scroll()
        } else {
            0.0
        };

        // Calculate thumb position, ensuring it doesn't extend past the bottom
        let thumb_y_offset = (scrollable_track * scroll_ratio).min(scrollbar_height - thumb_height); // Ensure thumb stays within bounds

        ThumbGeometry {
            y_offset: thumb_y_offset,
            height: thumb_height,
        }
    }

    /// Update animation state (returns true if redraw needed)
    pub fn update_animation(&mut self, config: &ScrollbarConfig) -> bool {
        // Start fade out if needed
        if config.auto_hide {
            if let Some(last_interaction) = self.last_interaction {
                let should_start_fade = last_interaction.elapsed() > config.auto_hide_delay
                    && self.fade_animation.is_none()
                    && self.current_opacity > 0.0
                    && !self.is_hovering
                    && !self.is_dragging;

                if should_start_fade {
                    self.fade_animation =
                        Some(FadeAnimation::new(FadeDirection::Out, config.fade_duration));
                }
            }
        }

        // Update ongoing animation
        if let Some(animation) = &mut self.fade_animation {
            self.current_opacity = animation.calculate_opacity();

            if animation.is_complete() {
                // Store direction before clearing animation
                let direction = animation.direction;
                self.fade_animation = None;
                // Ensure we end at target opacity
                self.current_opacity = match direction {
                    FadeDirection::In => 1.0,
                    FadeDirection::Out => 0.0,
                };
            }

            true // Need redraw
        } else {
            false
        }
    }

    /// Get current opacity for rendering
    pub fn get_opacity(&self, config: &ScrollbarConfig) -> f32 {
        if !self.is_needed() {
            return 0.0; // Never show scrollbar if not needed
        }

        if !config.auto_hide {
            return 1.0; // Always show if auto-hide is disabled
        }

        self.current_opacity // Use animated opacity when auto-hide is enabled
    }

    /// Trigger fade in animation
    pub fn fade_in(&mut self, config: &ScrollbarConfig) {
        if config.auto_hide && self.current_opacity < 1.0 {
            self.fade_animation = Some(FadeAnimation::new(FadeDirection::In, config.fade_duration));
        }
    }
}

#[derive(Debug, Clone)]
pub struct ThumbGeometry {
    pub y_offset: f32, // Offset from top of scrollbar
    pub height: f32,   // Height of thumb
}

#[derive(Debug, Clone)]
pub struct FadeAnimation {
    start_time: Instant,
    duration: Duration,
    direction: FadeDirection,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FadeDirection {
    In,
    Out,
}

impl FadeAnimation {
    pub fn new(direction: FadeDirection, duration: Duration) -> Self {
        Self {
            start_time: Instant::now(),
            duration,
            direction,
        }
    }

    pub fn calculate_opacity(&self) -> f32 {
        let elapsed = self.start_time.elapsed().as_secs_f32();
        let progress = (elapsed / self.duration.as_secs_f32()).min(1.0).max(0.0);

        match self.direction {
            FadeDirection::In => progress,
            FadeDirection::Out => 1.0 - progress,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.start_time.elapsed() >= self.duration
    }
}

/// Configuration for scrollbar behavior
#[derive(Debug, Clone)]
pub struct ScrollbarConfig {
    pub width: f32,
    pub auto_hide: bool,
    pub auto_hide_delay: Duration,
    pub fade_duration: Duration,
}

impl Default for ScrollbarConfig {
    fn default() -> Self {
        Self {
            width: 10.0,
            auto_hide: true,
            auto_hide_delay: Duration::from_millis(1500),
            fade_duration: Duration::from_millis(200),
        }
    }
}
