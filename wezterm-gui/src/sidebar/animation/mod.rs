//! Animation support for sidebars
//! Provides smooth animations and a coordinator for managing multiple concurrent animations

mod animations;
mod coordinator;

// Re-export the main animation types
pub use animations::{
    SidebarAnimation, SidebarOpacityAnimation, SidebarPositionAnimation,
    SidebarSlideAndFadeAnimation,
};

// Export the coordinator
pub use coordinator::{
    AnimationCoordinator, AnimationId, AnimationPriority, AnimationState, AnimationStats,
    AnimationTarget, AnimationType,
};
