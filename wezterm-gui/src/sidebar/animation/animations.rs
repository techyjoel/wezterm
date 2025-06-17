//! Animation support for sidebars
//! Provides smooth slide and fade animations using ColorEase

use crate::colorease::ColorEase;
use config::EasingFunction;
use std::time::Instant;

/// Animation state for sidebars with proper easing
#[derive(Debug, Clone)]
pub struct SidebarAnimation {
    /// The ColorEase instance handling the animation timing
    color_ease: ColorEase,
    /// Whether we're animating in (true) or out (false)
    animating_in: bool,
    /// The animation start time
    start_time: Option<Instant>,
}

impl SidebarAnimation {
    /// Create a new sidebar animation
    pub fn new(duration_ms: u64) -> Self {
        Self {
            color_ease: ColorEase::new(
                duration_ms,
                EasingFunction::EaseOut,
                duration_ms,
                EasingFunction::EaseIn,
                None,
            ),
            animating_in: true,
            start_time: None,
        }
    }

    /// Create with custom easing functions
    pub fn with_easing(
        in_duration_ms: u64,
        in_function: EasingFunction,
        out_duration_ms: u64,
        out_function: EasingFunction,
    ) -> Self {
        Self {
            color_ease: ColorEase::new(
                in_duration_ms,
                in_function,
                out_duration_ms,
                out_function,
                None,
            ),
            animating_in: true,
            start_time: None,
        }
    }

    /// Start an animation in the specified direction
    pub fn start(&mut self, animating_in: bool) {
        let was_animating = self.is_animating();
        let was_direction = self.animating_in;
        self.animating_in = animating_in;
        let now = Instant::now();
        self.start_time = Some(now);
        self.color_ease.update_start(now);
        log::info!(
            "SidebarAnimation::start: animating_in={}, was_animating={}, was_direction={}",
            animating_in,
            was_animating,
            was_direction
        );
    }

    /// Get the current animation progress (0.0 to 1.0)
    /// Returns None if animation is complete
    pub fn get_progress(&mut self) -> Option<f32> {
        if self.start_time.is_none() {
            return None;
        }

        match self.color_ease.intensity_one_shot() {
            Some((intensity, _next_frame)) => {
                // ColorEase gives us intensity for fade animations
                // For slide animations, we want position progress
                if self.animating_in {
                    Some(intensity)
                } else {
                    Some(1.0 - intensity)
                }
            }
            None => {
                // Animation complete
                self.start_time = None;
                None
            }
        }
    }

    /// Check if animation is currently running
    pub fn is_animating(&self) -> bool {
        self.start_time.is_some()
    }

    /// Force stop the animation
    pub fn stop(&mut self) {
        self.start_time = None;
    }

    /// Get the next frame time for smooth animation
    pub fn get_next_frame_time(&mut self) -> Option<Instant> {
        if self.start_time.is_none() {
            return None;
        }

        match self.color_ease.intensity_one_shot() {
            Some((_intensity, next_frame)) => Some(next_frame),
            None => None,
        }
    }
}

/// Position-based animation for sidebar sliding
#[derive(Debug, Clone)]
pub struct SidebarPositionAnimation {
    pub(super) animation: SidebarAnimation,
    start_position: f32,
    end_position: f32,
}

impl SidebarPositionAnimation {
    /// Create a new position animation
    pub fn new(duration_ms: u64, start_pos: f32, end_pos: f32) -> Self {
        Self {
            animation: SidebarAnimation::new(duration_ms),
            start_position: start_pos,
            end_position: end_pos,
        }
    }

    /// Create with custom easing
    pub fn with_easing(
        in_duration_ms: u64,
        in_function: EasingFunction,
        out_duration_ms: u64,
        out_function: EasingFunction,
        start_pos: f32,
        end_pos: f32,
    ) -> Self {
        Self {
            animation: SidebarAnimation::with_easing(
                in_duration_ms,
                in_function,
                out_duration_ms,
                out_function,
            ),
            start_position: start_pos,
            end_position: end_pos,
        }
    }

    /// Start the animation
    pub fn start(&mut self, forward: bool) {
        log::info!(
            "SidebarPositionAnimation::start: forward={}, start_pos={}, end_pos={}",
            forward,
            self.start_position,
            self.end_position
        );
        self.animation.start(forward);
    }
    
    /// Start the animation from the current position
    pub fn start_from_current(&mut self, forward: bool) {
        // Get the current position before starting
        let current_pos = self.get_position();
        
        // Update start and end positions based on direction
        if forward {
            // Animating in (showing): go from current position to visible position (0)
            self.start_position = current_pos;
            self.end_position = 0.0;
        } else {
            // Animating out (hiding): go from current position to hidden position
            self.start_position = current_pos;
            self.end_position = current_pos.abs().max(300.0); // Use the original off-screen position
        }
        
        log::info!(
            "SidebarPositionAnimation::start_from_current: forward={}, current_pos={}, new_start={}, new_end={}",
            forward,
            current_pos,
            self.start_position,
            self.end_position
        );
        
        self.animation.start(forward);
    }

    /// Get the current position
    pub fn get_position(&mut self) -> f32 {
        match self.animation.get_progress() {
            Some(progress) => {
                // Now that start/end are updated dynamically, just interpolate between them
                let delta = self.end_position - self.start_position;
                let position = self.start_position + (delta * progress);
                log::trace!(
                    "get_position: progress={}, start={}, end={}, delta={}, position={}, animating_in={}",
                    progress,
                    self.start_position,
                    self.end_position,
                    delta,
                    position,
                    self.animation.animating_in
                );
                position
            }
            None => {
                // Animation complete, return end position
                let position = self.end_position;
                log::trace!(
                    "get_position: animation complete, returning end_position={}",
                    position
                );
                position
            }
        }
    }

    /// Check if animation is running
    pub fn is_animating(&self) -> bool {
        self.animation.is_animating()
    }

    /// Get next frame time
    pub fn get_next_frame_time(&mut self) -> Option<Instant> {
        self.animation.get_next_frame_time()
    }

    /// Get the current animation progress (0.0 to 1.0)
    pub fn get_progress(&mut self) -> Option<f32> {
        self.animation.get_progress()
    }
}

/// Opacity animation for fade effects
#[derive(Debug, Clone)]
pub struct SidebarOpacityAnimation {
    pub(super) animation: SidebarAnimation,
}

impl SidebarOpacityAnimation {
    /// Create a new opacity animation
    pub fn new(fade_duration_ms: u64) -> Self {
        Self {
            animation: SidebarAnimation::new(fade_duration_ms),
        }
    }

    /// Start fading in or out
    pub fn start_fade(&mut self, fade_in: bool) {
        self.animation.start(fade_in);
    }

    /// Get current opacity (0.0 to 1.0)
    pub fn get_opacity(&mut self) -> f32 {
        self.animation.get_progress().unwrap_or(1.0)
    }

    /// Check if fading
    pub fn is_fading(&self) -> bool {
        self.animation.is_animating()
    }
}

/// Combined slide and fade animation
#[derive(Debug, Clone)]
pub struct SidebarSlideAndFadeAnimation {
    pub(super) position_anim: SidebarPositionAnimation,
    pub(super) opacity_anim: SidebarOpacityAnimation,
}

impl SidebarSlideAndFadeAnimation {
    /// Create a new slide and fade animation
    pub fn new(
        slide_duration_ms: u64,
        fade_duration_ms: u64,
        start_pos: f32,
        end_pos: f32,
    ) -> Self {
        Self {
            position_anim: SidebarPositionAnimation::new(slide_duration_ms, start_pos, end_pos),
            opacity_anim: SidebarOpacityAnimation::new(fade_duration_ms),
        }
    }

    /// Start the animation
    pub fn start(&mut self, showing: bool) {
        self.position_anim.start(showing);
        self.opacity_anim.start_fade(showing);
    }

    /// Get current position and opacity
    pub fn get_state(&mut self) -> (f32, f32) {
        (
            self.position_anim.get_position(),
            self.opacity_anim.get_opacity(),
        )
    }

    /// Check if any animation is running
    pub fn is_animating(&self) -> bool {
        self.position_anim.is_animating() || self.opacity_anim.is_fading()
    }

    /// Get the next frame time (earliest of both animations)
    pub fn get_next_frame_time(&mut self) -> Option<Instant> {
        let pos_time = self.position_anim.get_next_frame_time();
        let fade_time = self.opacity_anim.animation.get_next_frame_time();

        match (pos_time, fade_time) {
            (Some(p), Some(f)) => Some(p.min(f)),
            (Some(p), None) => Some(p),
            (None, Some(f)) => Some(f),
            (None, None) => None,
        }
    }
}
