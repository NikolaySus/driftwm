//! Frame-independent wallpaper time, including interruptible speed transitions.

use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    #[default]
    EaseInOut,
}

impl Easing {
    fn value(self, x: f64) -> f64 {
        match self {
            Self::Linear => x,
            Self::EaseIn => x * x,
            Self::EaseOut => x * (2.0 - x),
            Self::EaseInOut if x < 0.5 => 2.0 * x * x,
            Self::EaseInOut => 1.0 - 2.0 * (1.0 - x).powi(2),
        }
    }

    fn integral(self, x: f64) -> f64 {
        match self {
            Self::Linear => x * x / 2.0,
            Self::EaseIn => x.powi(3) / 3.0,
            Self::EaseOut => x * x - x.powi(3) / 3.0,
            Self::EaseInOut if x < 0.5 => 2.0 * x.powi(3) / 3.0,
            Self::EaseInOut => x - 0.5 + 2.0 * (1.0 - x).powi(3) / 3.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationSettings {
    pub speed: f64,
    pub lock_speed: f64,
    pub duration_ms: u32,
    pub easing: Easing,
}

impl Default for AnimationSettings {
    fn default() -> Self {
        Self {
            speed: 1.0,
            lock_speed: 1.0,
            duration_ms: 1000,
            easing: Easing::default(),
        }
    }
}

#[derive(Debug)]
pub struct AnimationClock {
    epoch: Instant,
    time: f64,
    from_speed: f64,
    target_speed: f64,
    settings: AnimationSettings,
    locked: bool,
    last_lock_event: Option<Instant>,
}

impl AnimationClock {
    pub fn new(now: Instant, settings: AnimationSettings) -> Self {
        Self {
            epoch: now,
            time: 0.0,
            from_speed: settings.speed,
            target_speed: settings.speed,
            settings,
            locked: false,
            last_lock_event: None,
        }
    }

    /// Sampling is read-only: extra monitors and screenshots cannot advance time.
    pub fn sample(&self, now: Instant) -> (f64, f64) {
        let elapsed = now.saturating_duration_since(self.epoch).as_secs_f64();
        let duration = f64::from(self.settings.duration_ms) / 1000.0;
        if duration == 0.0 {
            return (self.time + elapsed * self.target_speed, self.target_speed);
        }
        let transition = elapsed.min(duration);
        let x = transition / duration;
        // Weighted endpoint speeds avoid cancellation for deceleration to zero.
        let weight = duration * self.settings.easing.integral(x);
        let time = self.time
            + (transition - weight) * self.from_speed
            + weight * self.target_speed
            + (elapsed - transition) * self.target_speed;
        let blend = self.settings.easing.value(x);
        (
            time,
            (1.0 - blend) * self.from_speed + blend * self.target_speed,
        )
    }

    pub fn configure(&mut self, now: Instant, locked: bool, settings: AnimationSettings) {
        if self.locked == locked && self.settings == settings {
            return;
        }
        let (time, speed) = self.sample(now);
        if self.locked != locked {
            self.last_lock_event = Some(now);
        }
        self.epoch = now;
        self.time = time;
        self.from_speed = speed;
        self.target_speed = if locked {
            settings.lock_speed
        } else {
            settings.speed
        };
        self.settings = settings;
        self.locked = locked;
    }

    pub fn last_lock_event(&self) -> Option<Instant> {
        self.last_lock_event
    }

    /// Real time, independent of wallpaper speed and renderer lifetime.
    pub fn lock_signals(&self, now: Instant) -> (f32, f32) {
        let age = self.last_lock_event.map_or(-1.0, |event| {
            now.saturating_duration_since(event).as_secs_f32()
        });
        (if self.locked { 1.0 } else { 0.0 }, age)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn curves_and_integrals() {
        for (curve, quarter, total) in [
            (Easing::Linear, 0.25, 0.5),
            (Easing::EaseIn, 0.0625, 1.0 / 3.0),
            (Easing::EaseOut, 0.4375, 2.0 / 3.0),
            (Easing::EaseInOut, 0.125, 0.5),
        ] {
            close(curve.value(0.0), 0.0);
            close(curve.value(0.25), quarter);
            close(curve.value(1.0), 1.0);
            close(curve.integral(0.0), 0.0);
            close(curve.integral(1.0), total);
            for i in 1..100 {
                let x = f64::from(i) / 100.0;
                close(
                    (curve.integral(x + 1e-6) - curve.integral(x - 1e-6)) / 2e-6,
                    curve.value(x),
                );
            }
        }
    }

    #[test]
    fn lock_signals_use_real_time_and_only_state_changes_retrigger() {
        let now = Instant::now();
        let settings = AnimationSettings {
            speed: 0.0,
            lock_speed: 0.0,
            ..AnimationSettings::default()
        };
        let mut clock = AnimationClock::new(now, settings);
        assert_eq!(clock.lock_signals(now), (0.0, -1.0));
        clock.configure(now, true, settings);
        let later = now + Duration::from_millis(400);
        assert_eq!(clock.lock_signals(later), (1.0, 0.4));
        assert_eq!(clock.sample(later).0, 0.0);
        clock.configure(later, true, AnimationSettings::default());
        assert_eq!(clock.lock_signals(later), (1.0, 0.4));
        assert_eq!(clock.last_lock_event(), Some(now));
        for _ in 0..3 {
            assert_eq!(clock.lock_signals(later), (1.0, 0.4));
        }
        clock.configure(later, false, settings);
        assert_eq!(clock.lock_signals(later), (0.0, 0.0));
        clock.configure(later + Duration::from_millis(50), true, settings);
        assert_eq!(
            clock.lock_signals(later + Duration::from_secs(2)),
            (1.0, 1.95)
        );
    }

    #[test]
    fn clock_is_independent_of_frames_and_outputs() {
        let now = Instant::now();
        let mut c = AnimationClock::new(now, AnimationSettings::default());
        close(c.sample(now + Duration::from_secs(10)).0, 10.0);
        let settings = AnimationSettings {
            lock_speed: 0.5,
            ..AnimationSettings::default()
        };
        c.configure(now + Duration::from_secs(10), true, settings);
        let later = now + Duration::from_secs(3611);
        let expected = 10.0 + 0.75 + 1800.0;
        for _ in 0..4 {
            close(c.sample(later).0, expected);
        }
        close(c.sample(later).1, 0.5);
    }

    #[test]
    fn interruption_and_reload_preserve_time_and_speed() {
        let now = Instant::now();
        let settings = AnimationSettings {
            lock_speed: 0.0,
            ..AnimationSettings::default()
        };
        let mut c = AnimationClock::new(now, settings);
        c.configure(now, true, settings);
        let middle = now + Duration::from_millis(500);
        let before = c.sample(middle);
        c.configure(middle, true, settings);
        close(c.sample(now + Duration::from_secs(1)).1, 0.0);
        c.configure(middle, false, settings);
        assert_eq!(c.sample(middle), before);
        close(c.sample(middle + Duration::from_secs(1)).1, 1.0);
        let changed = AnimationSettings {
            speed: 2.0,
            ..settings
        };
        c.configure(middle, false, changed);
        assert_eq!(c.sample(middle), before);
        close(c.sample(middle + Duration::from_secs(1)).1, 2.0);
    }

    #[test]
    fn immediate_and_eased_freeze_do_not_jump_time() {
        let now = Instant::now();
        for duration_ms in [0, 1000] {
            let settings = AnimationSettings {
                lock_speed: 0.0,
                duration_ms,
                ..AnimationSettings::default()
            };
            let mut c = AnimationClock::new(now, settings);
            c.configure(now + Duration::from_secs(2), true, settings);
            close(c.sample(now + Duration::from_secs(2)).0, 2.0);
            let settled = c.sample(now + Duration::from_secs(3));
            close(settled.0, if duration_ms == 0 { 2.0 } else { 2.5 });
            assert_eq!(settled, c.sample(now + Duration::from_secs(1000)));
        }
    }
}
