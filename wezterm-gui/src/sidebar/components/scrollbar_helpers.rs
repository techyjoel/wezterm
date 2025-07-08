//! Shared scrollbar calculation utilities
//!
//! This module provides common calculations and utilities for scrollbar implementations,
//! ensuring consistent behavior across different scrollbar patterns.

/// Minimum thumb size in pixels to ensure it's always grabbable
pub const MIN_THUMB_SIZE: f32 = 20.0;

/// Information needed to render a scrollbar externally
#[derive(Debug, Clone)]
pub struct ScrollbarInfo {
    /// Whether scrollbar should be shown
    pub should_show: bool,
    /// Thumb position as a fraction (0.0 = top, 1.0 = bottom)
    pub thumb_position: f32,
    /// Thumb size as a fraction of total height (0.0 to 1.0)
    pub thumb_size: f32,
    /// Total content height in pixels
    pub content_height: f32,
    /// Visible viewport height in pixels  
    pub viewport_height: f32,
    /// Current scroll offset in pixels
    pub scroll_offset: f32,
    /// DEPRECATED: Total scrollable items (kept for compatibility)
    pub total_items: usize,
    /// DEPRECATED: Visible viewport items (kept for compatibility)
    pub viewport_items: usize,
}

/// Metrics for calculating scrollbar geometry and behavior
#[derive(Debug, Clone)]
pub struct ScrollMetrics {
    /// Total height of the scrollable content
    pub content_height: f32,
    /// Height of the visible viewport
    pub viewport_height: f32,
    /// Current scroll offset from top
    pub scroll_offset: f32,
}

impl ScrollMetrics {
    /// Create new scroll metrics
    pub fn new(content_height: f32, viewport_height: f32, scroll_offset: f32) -> Self {
        Self {
            content_height,
            viewport_height,
            scroll_offset: scroll_offset
                .max(0.0)
                .min(Self::max_scroll_internal(content_height, viewport_height)),
        }
    }

    /// Calculate the thumb size based on content ratio
    pub fn thumb_size(&self, track_height: f32) -> f32 {
        if self.content_height <= 0.0 || self.viewport_height <= 0.0 {
            return 0.0;
        }

        let ratio = self.viewport_height / self.content_height;
        (track_height * ratio).max(MIN_THUMB_SIZE).min(track_height)
    }

    /// Calculate the thumb position within the track
    pub fn thumb_position(&self, track_height: f32) -> f32 {
        if self.content_height <= self.viewport_height {
            return 0.0;
        }

        let thumb_size = self.thumb_size(track_height);
        let scrollable_track = track_height - thumb_size;

        if scrollable_track <= 0.0 {
            return 0.0;
        }

        let max_scroll = self.content_height - self.viewport_height;
        let scroll_ratio = if max_scroll > 0.0 {
            self.scroll_offset / max_scroll
        } else {
            0.0
        };

        scrollable_track * scroll_ratio
    }

    /// Handle mouse wheel event
    pub fn handle_wheel(&mut self, delta: f32, lines_per_notch: f32) -> bool {
        if !self.is_scrollable() {
            return false;
        }

        // Estimate line height as a fraction of viewport with minimum threshold
        const MIN_LINE_HEIGHT: f32 = 1.0;
        let line_height = (self.viewport_height / 30.0).max(MIN_LINE_HEIGHT);
        let scroll_amount = delta * line_height * lines_per_notch;

        let old_offset = self.scroll_offset;
        self.set_scroll_offset(self.scroll_offset - scroll_amount);

        old_offset != self.scroll_offset
    }

    /// Handle scrollbar drag
    pub fn handle_drag(&mut self, drag_delta_y: f32, track_height: f32) -> bool {
        if !self.is_scrollable() {
            return false;
        }

        let thumb_size = self.thumb_size(track_height);
        let scrollable_track = track_height - thumb_size;

        if scrollable_track <= 0.0 {
            return false;
        }

        let max_scroll = self.content_height - self.viewport_height;
        let scroll_per_pixel = max_scroll / scrollable_track;

        let old_offset = self.scroll_offset;
        self.set_scroll_offset(self.scroll_offset + (drag_delta_y * scroll_per_pixel));

        old_offset != self.scroll_offset
    }

    /// Handle direct click on scrollbar track
    pub fn handle_track_click(&mut self, click_y: f32, track_height: f32) -> bool {
        if !self.is_scrollable() {
            return false;
        }

        let thumb_pos = self.thumb_position(track_height);
        let thumb_size = self.thumb_size(track_height);

        // Determine if click is above or below thumb
        if click_y < thumb_pos {
            // Page up
            self.page_up()
        } else if click_y > thumb_pos + thumb_size {
            // Page down
            self.page_down()
        } else {
            false
        }
    }

    /// Scroll up by one page
    pub fn page_up(&mut self) -> bool {
        let old_offset = self.scroll_offset;
        self.set_scroll_offset(self.scroll_offset - self.viewport_height * 0.9);
        old_offset != self.scroll_offset
    }

    /// Scroll down by one page
    pub fn page_down(&mut self) -> bool {
        let old_offset = self.scroll_offset;
        self.set_scroll_offset(self.scroll_offset + self.viewport_height * 0.9);
        old_offset != self.scroll_offset
    }

    /// Set scroll offset with bounds checking
    pub fn set_scroll_offset(&mut self, offset: f32) {
        self.scroll_offset = offset.max(0.0).min(self.max_scroll());
    }

    /// Get maximum scroll offset
    pub fn max_scroll(&self) -> f32 {
        Self::max_scroll_internal(self.content_height, self.viewport_height)
    }

    /// Internal max scroll calculation
    fn max_scroll_internal(content_height: f32, viewport_height: f32) -> f32 {
        (content_height - viewport_height).max(0.0)
    }

    /// Check if content is scrollable
    pub fn is_scrollable(&self) -> bool {
        self.content_height > self.viewport_height
    }

    /// Get scroll progress as a percentage (0.0 to 1.0)
    pub fn scroll_progress(&self) -> f32 {
        let max_scroll = self.max_scroll();
        if max_scroll <= 0.0 {
            0.0
        } else {
            (self.scroll_offset / max_scroll).clamp(0.0, 1.0)
        }
    }

    /// Calculate visible item range for virtual scrolling
    pub fn visible_range(
        &self,
        item_heights: &[f32],
        item_spacing: f32,
        render_margin: f32,
    ) -> (usize, usize) {
        if item_heights.is_empty() {
            return (0, 0);
        }

        let viewport_start = (self.scroll_offset - render_margin).max(0.0);
        let viewport_end = self.scroll_offset + self.viewport_height + render_margin;

        let mut current_y = 0.0;
        let mut start_index = None;
        let mut end_index = item_heights.len();

        for (i, &height) in item_heights.iter().enumerate() {
            let item_bottom = current_y + height;

            // Check if item is in viewport
            if start_index.is_none() && item_bottom > viewport_start {
                start_index = Some(i);
            }

            if current_y > viewport_end {
                end_index = i;
                break;
            }

            current_y = item_bottom + item_spacing;
        }

        (start_index.unwrap_or(0), end_index)
    }
}

/// Helper for smooth scrollbar animations
#[derive(Debug, Clone)]
pub struct ScrollAnimation {
    pub start_offset: f32,
    pub target_offset: f32,
    pub start_time: std::time::Instant,
    pub duration: std::time::Duration,
}

impl ScrollAnimation {
    /// Create a new scroll animation
    pub fn new(start: f32, target: f32, duration: std::time::Duration) -> Self {
        Self {
            start_offset: start,
            target_offset: target,
            start_time: std::time::Instant::now(),
            duration,
        }
    }

    /// Get current offset based on animation progress
    pub fn current_offset(&self) -> f32 {
        let elapsed = self.start_time.elapsed();
        if elapsed >= self.duration {
            return self.target_offset;
        }

        let progress = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        let eased_progress = ease_out_cubic(progress);

        self.start_offset + (self.target_offset - self.start_offset) * eased_progress
    }

    /// Check if animation is complete
    pub fn is_complete(&self) -> bool {
        self.start_time.elapsed() >= self.duration
    }
}

/// Cubic ease-out function for smooth animations
fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thumb_calculations() {
        let metrics = ScrollMetrics::new(1000.0, 200.0, 100.0);

        // Track height of 100px
        let track_height = 100.0;
        let thumb_size = metrics.thumb_size(track_height);
        let thumb_pos = metrics.thumb_position(track_height);

        // Thumb should be 20% of track (viewport/content ratio)
        assert!((thumb_size - 20.0).abs() < 0.1);

        // Thumb position should reflect scroll progress
        let expected_pos = (100.0 / 800.0) * (track_height - thumb_size);
        assert!((thumb_pos - expected_pos).abs() < 0.1);
    }

    #[test]
    fn test_scroll_bounds() {
        let mut metrics = ScrollMetrics::new(1000.0, 200.0, 0.0);

        // Can't scroll negative
        metrics.set_scroll_offset(-100.0);
        assert_eq!(metrics.scroll_offset, 0.0);

        // Can't scroll past content
        metrics.set_scroll_offset(900.0);
        assert_eq!(metrics.scroll_offset, 800.0); // max scroll
    }

    #[test]
    fn test_non_scrollable_content() {
        let metrics = ScrollMetrics::new(100.0, 200.0, 0.0);

        assert!(!metrics.is_scrollable());
        assert_eq!(metrics.max_scroll(), 0.0);
        assert_eq!(metrics.thumb_size(100.0), 100.0); // Full track
    }

    #[test]
    fn test_visible_range() {
        let metrics = ScrollMetrics::new(1000.0, 200.0, 300.0);
        let item_heights = vec![50.0, 100.0, 75.0, 60.0, 80.0, 90.0, 70.0, 85.0];
        let spacing = 10.0;
        let margin = 50.0;

        let (start, end) = metrics.visible_range(&item_heights, spacing, margin);

        // Verify we get a reasonable range
        assert!(start < end);
        assert!(end <= item_heights.len());

        // With scroll offset 300 and viewport 200, we should see items around position 250-500
        // Item 0: 0-50, Item 1: 60-160, Item 2: 170-245, Item 3: 255-315, Item 4: 325-405
        // So we expect to see items 2-4 at minimum
        assert!(start <= 3); // May include item before visible range due to margin
        assert!(end >= 5); // Should include items 3,4 and maybe 5
    }

    #[test]
    fn test_ease_out_cubic() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);

        // Should ease out (start fast, end slow)
        assert!(ease_out_cubic(0.25) > 0.25);
        assert!(ease_out_cubic(0.75) > 0.75);
    }
}
