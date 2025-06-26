use std::time::Instant;

pub struct ModalAnimation {
    pub opacity: f32,
    pub scale: f32,
    pub position_y: f32,
    pub start_time: Option<Instant>,
    pub duration_ms: u64,
}

impl ModalAnimation {
    pub fn new() -> Self {
        Self {
            opacity: 0.0,
            scale: 0.95,
            position_y: 20.0,
            start_time: None,
            duration_ms: 20, // 20ms animations
        }
    }

    pub fn start_open(&mut self) {
        self.start_time = Some(Instant::now());
    }

    pub fn start_close(&mut self) {
        self.start_time = Some(Instant::now());
    }

    pub fn is_complete(&self) -> bool {
        if let Some(start) = self.start_time {
            let elapsed = start.elapsed().as_millis() as u64;
            elapsed >= self.duration_ms
        } else {
            true
        }
    }

    pub fn get_transform(&self) -> (f32, f32, f32) {
        if let Some(start) = self.start_time {
            let elapsed = start.elapsed().as_millis() as f32;
            let progress = (elapsed / self.duration_ms as f32).min(1.0);
            
            // Simple linear interpolation for now
            let opacity = progress;
            let scale = 0.95 + (0.05 * progress);
            let position_y = 20.0 * (1.0 - progress);
            
            (opacity, scale, position_y)
        } else {
            (self.opacity, self.scale, self.position_y)
        }
    }
}