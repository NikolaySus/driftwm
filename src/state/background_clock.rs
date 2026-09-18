use std::time::Instant;

use super::DriftWm;

impl DriftWm {
    pub(crate) fn sync_background_clock(&mut self) {
        self.background_clock.configure(
            Instant::now(),
            self.session_lock.is_locked(),
            self.config.background.animation,
        );
    }

    pub(crate) fn background_time(&self) -> f32 {
        self.background_clock
            .sample(Instant::now())
            .0
            .min(f64::from(f32::MAX)) as f32
    }
}
