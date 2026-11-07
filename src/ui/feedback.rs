use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

struct Value {
    key: u64,
    from: f32,
    target: f32,
    start: Instant,
    duration: Duration,
}

struct Follower {
    key: u64,
    value: f32,
    velocity: f32,
    target: f32,
    updated: Instant,
}

impl Follower {
    fn advance(&mut self, now: Instant, ms: u64) {
        let dt = now.saturating_duration_since(self.updated).as_secs_f32();
        let omega = 9.0 / (ms.max(1) as f32 * 0.001);
        let offset = self.value - self.target;
        let tangent = self.velocity + omega * offset;
        let decay = (-omega * dt).exp();
        self.value = self.target + (offset + tangent * dt) * decay;
        self.velocity = (self.velocity - omega * tangent * dt) * decay;
        self.updated = now;
        if !self.active() {
            self.value = self.target;
            self.velocity = 0.0;
        }
    }

    fn active(&self) -> bool {
        (self.value - self.target).abs() > 0.001 || self.velocity.abs() > 0.01
    }
}

impl Value {
    fn at(&self, now: Instant) -> f32 {
        let t = (now.saturating_duration_since(self.start).as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0);
        self.from + (self.target - self.from) * (1.0 - (1.0 - t).powi(3))
    }
}

pub struct Feedback {
    enabled: Cell<bool>,
    values: RefCell<Vec<Value>>,
    followers: RefCell<Vec<Follower>>,
    limit: usize,
}

impl Default for Feedback {
    fn default() -> Self {
        Self::with_capacity(256)
    }
}

impl Feedback {
    pub fn with_capacity(limit: usize) -> Self {
        Self {
            enabled: Cell::new(true),
            values: RefCell::new(Vec::new()),
            followers: RefCell::new(Vec::new()),
            limit: limit.clamp(1, 1024),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.set(enabled);
        if !enabled {
            self.values.borrow_mut().clear();
            self.followers.borrow_mut().clear();
        }
    }

    pub fn sample(&self, key: u64, target: f32, initial: f32, ms: u64) -> f32 {
        self.sample_at(key, target, initial, ms, Instant::now())
    }

    pub fn follow(&self, key: u64, target: f32, initial: f32, ms: u64) -> f32 {
        self.follow_at(key, target, initial, ms, Instant::now())
    }

    fn follow_at(&self, key: u64, target: f32, initial: f32, ms: u64, now: Instant) -> f32 {
        if !self.enabled.get() {
            return target;
        }
        let mut values = self.followers.borrow_mut();
        if let Some(value) = values.iter_mut().find(|v| v.key == key) {
            value.advance(now, ms);
            value.target = target;
            value.value
        } else {
            if values.len() >= self.limit {
                values.retain(|v| now.saturating_duration_since(v.updated) < Duration::from_secs(2));
                if values.len() >= self.limit {
                    values.remove(0);
                }
            }
            values.push(Follower { key, value: initial, velocity: 0.0, target, updated: now });
            initial
        }
    }

    fn sample_at(&self, key: u64, target: f32, initial: f32, ms: u64, now: Instant) -> f32 {
        if !self.enabled.get() {
            return target;
        }
        let mut values = self.values.borrow_mut();
        if let Some(value) = values.iter_mut().find(|v| v.key == key) {
            if value.target != target {
                value.from = value.at(now);
                value.target = target;
                value.start = now;
                value.duration = Duration::from_millis(ms.max(1));
            }
            value.at(now)
        } else {
            if values.len() >= self.limit {
                values.retain(|v| now.saturating_duration_since(v.start) < Duration::from_secs(2));
                if values.len() >= self.limit {
                    values.remove(0);
                }
            }
            values.push(Value { key, from: initial, target, start: now, duration: Duration::from_millis(ms.max(1)) });
            initial
        }
    }

    pub fn active(&self) -> bool {
        let now = Instant::now();
        self.enabled.get()
            && (self
                .values
                .borrow()
                .iter()
                .any(|v| v.from != v.target && now.saturating_duration_since(v.start) < v.duration)
                || self
                    .followers
                    .borrow()
                    .iter()
                    .any(|v| now.saturating_duration_since(v.updated) < Duration::from_secs(1) && v.active()))
    }
}

pub fn motion_allowed() -> bool {
    thread_local! {
        static CACHED: Cell<Option<(Instant, bool)>> = const { Cell::new(None) };
    }
    if let Some((at, enabled)) = CACHED.get()
        && at.elapsed() < Duration::from_millis(500)
    {
        return enabled;
    }
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    let mut enabled = windows::core::BOOL(1);
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&raw mut enabled).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    let enabled = enabled.as_bool();
    CACHED.set(Some((Instant::now(), enabled)));
    enabled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn following_is_frame_rate_independent_and_keeps_velocity_on_retarget() {
        let now = Instant::now();
        let mut results = Vec::new();
        for hz in [30, 60, 120, 144, 240, 1000] {
            let feedback = Feedback::default();
            feedback.follow_at(1, 1.0, 0.0, 180, now);
            for frame in 1..=hz {
                feedback.follow_at(1, 1.0, 0.0, 180, now + Duration::from_secs_f64(frame as f64 / hz as f64));
            }
            results.push(feedback.followers.borrow()[0].value);
            assert!(!feedback.active());
            let at = now + Duration::from_secs(2);
            feedback.follow_at(1, 0.0, 0.0, 180, at);
            feedback.follow_at(1, 0.0, 0.0, 180, at + Duration::from_millis(40));
            let before = {
                let values = feedback.followers.borrow();
                (values[0].value, values[0].velocity)
            };
            feedback.follow_at(1, 1.0, 0.0, 180, at + Duration::from_millis(40));
            let values = feedback.followers.borrow();
            assert_eq!((values[0].value, values[0].velocity), before);
        }
        assert!(results.iter().all(|v| *v == 1.0));
    }

    #[test]
    fn dense_dock_animation_channels_settle_without_restarting() {
        let feedback = Feedback::with_capacity(512);
        let now = Instant::now();
        for key in 0..384 {
            assert_eq!(feedback.sample_at(key, 1.0, 0.0, 180, now), 0.0);
        }
        for key in 0..384 {
            assert_eq!(feedback.sample_at(key, 1.0, 0.0, 180, now + Duration::from_secs(3)), 1.0);
        }
        assert_eq!(feedback.values.borrow().len(), 384);
    }

    #[test]
    fn reversing_midway_continues_from_the_visible_value_and_stops() {
        let values = Feedback::default();
        let start = Instant::now();
        assert_eq!(values.sample_at(1, 1.0, 0.0, 160, start), 0.0);
        let halfway = start + Duration::from_millis(80);
        let before = values.sample_at(1, 1.0, 0.0, 160, halfway);
        assert_eq!(values.sample_at(1, 0.0, 0.0, 160, halfway), before);
        assert_eq!(values.sample_at(1, 0.0, 0.0, 160, halfway + Duration::from_millis(160)), 0.0);
        values.set_enabled(false);
        assert_eq!(values.sample(1, 1.0, 0.0, 160), 1.0);
        assert!(!values.active());
    }
}
