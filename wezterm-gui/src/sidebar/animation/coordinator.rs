//! Animation coordinator for managing multiple concurrent animations
//! Provides queueing, interruption handling, and performance optimization

use super::animations::{
    SidebarAnimation, SidebarOpacityAnimation, SidebarPositionAnimation,
    SidebarSlideAndFadeAnimation,
};
use std::collections::HashMap;
use std::time::Instant;

/// Unique identifier for animations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnimationId(u64);

impl AnimationId {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

/// Priority levels for animations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnimationPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Animation state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimationState {
    Queued,
    Running,
    Paused,
    Completed,
    Interrupted,
}

/// Wrapper for different animation types
#[derive(Debug, Clone)]
pub enum AnimationType {
    Position(SidebarPositionAnimation),
    Opacity(SidebarOpacityAnimation),
    SlideAndFade(SidebarSlideAndFadeAnimation),
    Generic(SidebarAnimation),
}

impl AnimationType {
    /// Check if the animation is currently running
    pub fn is_animating(&self) -> bool {
        match self {
            Self::Position(anim) => anim.is_animating(),
            Self::Opacity(anim) => anim.is_fading(),
            Self::SlideAndFade(anim) => anim.is_animating(),
            Self::Generic(anim) => anim.is_animating(),
        }
    }

    /// Get the next frame time for this animation
    pub fn get_next_frame_time(&mut self) -> Option<Instant> {
        match self {
            Self::Position(anim) => anim.get_next_frame_time(),
            Self::Opacity(anim) => anim.animation.get_next_frame_time(),
            Self::SlideAndFade(anim) => anim.get_next_frame_time(),
            Self::Generic(anim) => anim.get_next_frame_time(),
        }
    }
}

/// Managed animation entry
struct ManagedAnimation {
    id: AnimationId,
    animation: AnimationType,
    priority: AnimationPriority,
    state: AnimationState,
    target: AnimationTarget,
    interruptible: bool,
    on_complete: Option<Box<dyn FnOnce() + Send>>,
}

/// Target for animations (what they're animating)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AnimationTarget {
    LeftSidebar,
    RightSidebar,
    Component(String),
    Custom(String),
}

/// Animation coordinator for managing multiple animations
pub struct AnimationCoordinator {
    animations: HashMap<AnimationId, ManagedAnimation>,
    queue: Vec<AnimationId>,
    max_concurrent: usize,
    frame_budget_ms: u64,
}

impl AnimationCoordinator {
    /// Create a new animation coordinator
    pub fn new() -> Self {
        Self {
            animations: HashMap::new(),
            queue: Vec::new(),
            max_concurrent: 5,
            frame_budget_ms: 16, // Target 60fps
        }
    }

    /// Set the maximum number of concurrent animations
    pub fn set_max_concurrent(&mut self, max: usize) {
        self.max_concurrent = max;
    }

    /// Set the frame budget in milliseconds
    pub fn set_frame_budget_ms(&mut self, budget_ms: u64) {
        self.frame_budget_ms = budget_ms;
    }

    /// Queue a new animation
    pub fn queue_animation(
        &mut self,
        animation: AnimationType,
        target: AnimationTarget,
        priority: AnimationPriority,
        interruptible: bool,
    ) -> AnimationId {
        let id = AnimationId::new();

        let managed = ManagedAnimation {
            id,
            animation,
            priority,
            state: AnimationState::Queued,
            target,
            interruptible,
            on_complete: None,
        };

        self.animations.insert(id, managed);
        self.queue.push(id);

        // Sort queue by priority (highest first)
        let mut priorities: Vec<(AnimationId, AnimationPriority)> = self.queue
            .iter()
            .filter_map(|&id| {
                self.animations.get(&id).map(|a| (id, a.priority))
            })
            .collect();
        priorities.sort_by_key(|(_, p)| std::cmp::Reverse(*p));
        self.queue = priorities.into_iter().map(|(id, _)| id).collect();

        // Try to start queued animations
        self.process_queue();

        id
    }

    /// Queue an animation with a completion callback
    pub fn queue_animation_with_callback<F>(
        &mut self,
        animation: AnimationType,
        target: AnimationTarget,
        priority: AnimationPriority,
        interruptible: bool,
        on_complete: F,
    ) -> AnimationId
    where
        F: FnOnce() + Send + 'static,
    {
        let id = self.queue_animation(animation, target, priority, interruptible);

        if let Some(managed) = self.animations.get_mut(&id) {
            managed.on_complete = Some(Box::new(on_complete));
        }

        id
    }

    /// Start an animation immediately, potentially interrupting others
    pub fn start_immediate(
        &mut self,
        animation: AnimationType,
        target: AnimationTarget,
        priority: AnimationPriority,
    ) -> AnimationId {
        // Check if we need to interrupt existing animations for this target
        let to_interrupt: Vec<_> = self
            .animations
            .iter()
            .filter(|(_, a)| {
                a.target == target
                    && a.state == AnimationState::Running
                    && a.interruptible
                    && a.priority < priority
            })
            .map(|(id, _)| *id)
            .collect();

        for id in to_interrupt {
            self.interrupt_animation(id);
        }

        let id = AnimationId::new();

        let mut managed = ManagedAnimation {
            id,
            animation,
            priority,
            state: AnimationState::Running,
            target,
            interruptible: false,
            on_complete: None,
        };

        // Start the animation based on its type
        match &mut managed.animation {
            AnimationType::Position(anim) => anim.start(true),
            AnimationType::Opacity(anim) => anim.start_fade(true),
            AnimationType::SlideAndFade(anim) => anim.start(true),
            AnimationType::Generic(anim) => anim.start(true),
        }

        self.animations.insert(id, managed);
        id
    }

    /// Interrupt an animation
    pub fn interrupt_animation(&mut self, id: AnimationId) -> bool {
        if let Some(anim) = self.animations.get_mut(&id) {
            if anim.interruptible && anim.state == AnimationState::Running {
                anim.state = AnimationState::Interrupted;

                // Stop the underlying animation
                match &mut anim.animation {
                    AnimationType::Generic(a) => a.stop(),
                    AnimationType::Position(a) => a.animation.stop(),
                    AnimationType::Opacity(a) => a.animation.stop(),
                    AnimationType::SlideAndFade(a) => {
                        a.position_anim.animation.stop();
                        a.opacity_anim.animation.stop();
                    }
                }

                return true;
            }
        }
        false
    }

    /// Pause an animation
    pub fn pause_animation(&mut self, id: AnimationId) -> bool {
        if let Some(anim) = self.animations.get_mut(&id) {
            if anim.state == AnimationState::Running {
                anim.state = AnimationState::Paused;
                return true;
            }
        }
        false
    }

    /// Resume a paused animation
    pub fn resume_animation(&mut self, id: AnimationId) -> bool {
        if let Some(anim) = self.animations.get_mut(&id) {
            if anim.state == AnimationState::Paused {
                anim.state = AnimationState::Running;
                return true;
            }
        }
        false
    }

    /// Cancel an animation (remove it completely)
    pub fn cancel_animation(&mut self, id: AnimationId) -> bool {
        self.queue.retain(|&queue_id| queue_id != id);
        self.animations.remove(&id).is_some()
    }

    /// Process the animation queue
    fn process_queue(&mut self) {
        let running_count = self
            .animations
            .values()
            .filter(|a| a.state == AnimationState::Running)
            .count();

        if running_count >= self.max_concurrent {
            return;
        }

        // Find next queued animation to start
        let next_id = self
            .queue
            .iter()
            .find(|&&id| {
                self.animations
                    .get(&id)
                    .map(|a| a.state == AnimationState::Queued)
                    .unwrap_or(false)
            })
            .copied();

        if let Some(id) = next_id {
            if let Some(anim) = self.animations.get_mut(&id) {
                anim.state = AnimationState::Running;

                // Start the animation based on its type
                match &mut anim.animation {
                    AnimationType::Position(a) => a.start(true),
                    AnimationType::Opacity(a) => a.start_fade(true),
                    AnimationType::SlideAndFade(a) => a.start(true),
                    AnimationType::Generic(a) => a.start(true),
                }
            }

            // Remove from queue since it's now running
            self.queue.retain(|&queue_id| queue_id != id);
        }
    }

    /// Update all animations and return if any need redraw
    pub fn update(&mut self) -> bool {
        let mut needs_redraw = false;
        let mut completed = Vec::new();

        let frame_start = Instant::now();

        // Update all running animations
        for (id, anim) in self.animations.iter_mut() {
            if anim.state != AnimationState::Running {
                continue;
            }

            // Check frame budget
            if frame_start.elapsed().as_millis() as u64 > self.frame_budget_ms {
                // We've exceeded our frame budget, defer remaining updates
                needs_redraw = true;
                break;
            }

            // Update animation
            if !anim.animation.is_animating() {
                completed.push(*id);
            } else {
                needs_redraw = true;
            }
        }

        // Handle completed animations
        for id in completed {
            if let Some(mut anim) = self.animations.remove(&id) {
                anim.state = AnimationState::Completed;

                // Call completion callback if present
                if let Some(callback) = anim.on_complete.take() {
                    callback();
                }
            }
        }

        // Process queue for any new animations
        self.process_queue();

        needs_redraw
    }

    /// Get the next frame time (earliest of all animations)
    pub fn get_next_frame_time(&mut self) -> Option<Instant> {
        let mut earliest: Option<Instant> = None;

        for anim in self.animations.values_mut() {
            if anim.state != AnimationState::Running {
                continue;
            }

            if let Some(next) = anim.animation.get_next_frame_time() {
                earliest = Some(match earliest {
                    Some(e) => e.min(next),
                    None => next,
                });
            }
        }

        earliest
    }

    /// Check if a specific animation is running
    pub fn is_animation_running(&self, id: AnimationId) -> bool {
        self.animations
            .get(&id)
            .map(|a| a.state == AnimationState::Running)
            .unwrap_or(false)
    }

    /// Check if any animations are running for a target
    pub fn has_animations_for_target(&self, target: &AnimationTarget) -> bool {
        self.animations
            .values()
            .any(|a| &a.target == target && a.state == AnimationState::Running)
    }

    /// Get animation state
    pub fn get_animation_state(&self, id: AnimationId) -> Option<AnimationState> {
        self.animations.get(&id).map(|a| a.state)
    }

    /// Clear all completed animations
    pub fn clear_completed(&mut self) {
        self.animations
            .retain(|_, a| a.state != AnimationState::Completed);
    }

    /// Get statistics about current animations
    pub fn get_stats(&self) -> AnimationStats {
        let mut stats = AnimationStats::default();

        for anim in self.animations.values() {
            match anim.state {
                AnimationState::Queued => stats.queued += 1,
                AnimationState::Running => stats.running += 1,
                AnimationState::Paused => stats.paused += 1,
                AnimationState::Completed => stats.completed += 1,
                AnimationState::Interrupted => stats.interrupted += 1,
            }
        }

        stats.total = self.animations.len();
        stats
    }
}

/// Statistics about animations
#[derive(Debug, Default, Clone)]
pub struct AnimationStats {
    pub total: usize,
    pub queued: usize,
    pub running: usize,
    pub paused: usize,
    pub completed: usize,
    pub interrupted: usize,
}

impl Default for AnimationCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::EasingFunction;

    #[test]
    fn test_animation_queueing() {
        let mut coordinator = AnimationCoordinator::new();
        coordinator.set_max_concurrent(2);

        // Queue 3 animations
        let id1 = coordinator.queue_animation(
            AnimationType::Generic(SidebarAnimation::new(100)),
            AnimationTarget::LeftSidebar,
            AnimationPriority::Normal,
            true,
        );

        let id2 = coordinator.queue_animation(
            AnimationType::Generic(SidebarAnimation::new(100)),
            AnimationTarget::RightSidebar,
            AnimationPriority::High,
            true,
        );

        let id3 = coordinator.queue_animation(
            AnimationType::Generic(SidebarAnimation::new(100)),
            AnimationTarget::Custom("test".to_string()),
            AnimationPriority::Low,
            true,
        );

        // High priority should run first
        assert_eq!(
            coordinator.get_animation_state(id2),
            Some(AnimationState::Running)
        );
        assert_eq!(
            coordinator.get_animation_state(id1),
            Some(AnimationState::Running)
        );
        assert_eq!(
            coordinator.get_animation_state(id3),
            Some(AnimationState::Queued)
        );
    }

    #[test]
    fn test_animation_interruption() {
        let mut coordinator = AnimationCoordinator::new();

        // Start an interruptible animation
        let id1 = coordinator.queue_animation(
            AnimationType::Generic(SidebarAnimation::new(1000)),
            AnimationTarget::LeftSidebar,
            AnimationPriority::Low,
            true,
        );

        // Start a high priority animation for the same target
        let id2 = coordinator.start_immediate(
            AnimationType::Generic(SidebarAnimation::new(100)),
            AnimationTarget::LeftSidebar,
            AnimationPriority::High,
        );

        // First animation should be interrupted
        assert_eq!(
            coordinator.get_animation_state(id1),
            Some(AnimationState::Interrupted)
        );
        assert_eq!(
            coordinator.get_animation_state(id2),
            Some(AnimationState::Running)
        );
    }
}
