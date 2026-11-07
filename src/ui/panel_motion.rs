use std::time::{Duration, Instant};

pub(super) struct Travel {
    from: f32,
    target: f32,
    started: Instant,
    duration: Duration,
    velocity: f32,
}

impl Default for Travel {
    fn default() -> Self {
        Self { from: 0.0, target: 0.0, started: Instant::now(), duration: Duration::ZERO, velocity: 0.0 }
    }
}

impl Travel {
    #[cfg(test)]
    pub fn seek(&mut self, milliseconds: u64) {
        self.started = Instant::now() - Duration::from_millis(milliseconds);
    }

    pub fn value(&self, now: Instant) -> f32 {
        self.sample(now).0
    }

    fn sample(&self, now: Instant) -> (f32, f32) {
        if self.duration.is_zero() {
            return (self.target, 0.0);
        }
        let t = (now.saturating_duration_since(self.started).as_secs_f32() / self.duration.as_secs_f32()).min(1.0);
        let span = self.duration.as_secs_f32();
        let distance = self.target - self.from;
        let value = self.from + distance * t * t * (3.0 - 2.0 * t) + self.velocity * span * t * (1.0 - t).powi(2);
        let velocity = distance * 6.0 * t * (1.0 - t) / span + self.velocity * (1.0 - 4.0 * t + 3.0 * t * t);
        (value.clamp(0.0, 1.0), velocity)
    }

    pub fn show(&mut self, show: bool, ms: u32, now: Instant) {
        let target = if show { 1.0 } else { 0.0 };
        if target != self.target || ms == 0 {
            (self.from, self.velocity) = self.sample(now);
            self.target = target;
            self.started = now;
            let mut seconds = ms as f32 / 1000.0 * (target - self.from).abs().sqrt();
            if self.velocity > 0.0 {
                seconds = seconds.min(3.0 * (1.0 - self.from) / self.velocity);
            } else if self.velocity < 0.0 {
                seconds = seconds.min(-3.0 * self.from / self.velocity);
            }
            self.duration = Duration::from_micros((seconds.max(0.0) * 1_000_000.0).round() as u64);
        }
    }

    pub fn remaining(&self, now: Instant) -> Duration {
        self.duration.saturating_sub(now.saturating_duration_since(self.started))
    }

    pub fn active(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started) < self.duration
    }

    pub fn shown(&self) -> bool {
        self.target == 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_accelerates_softly_and_repeated_updates_do_not_restart_it() {
        let at = Instant::now();
        let mut travel = Travel::default();
        travel.show(true, 360, at);
        assert_eq!(travel.sample(at).1, 0.0);
        assert!(travel.value(at + Duration::from_millis(16)) < 0.01);
        for ms in (16..=352).step_by(16) {
            let now = at + Duration::from_millis(ms);
            let before = travel.sample(now);
            travel.show(true, 360, now);
            assert_eq!(travel.sample(now), before);
        }
        assert_eq!(travel.sample(at + Duration::from_millis(360)), (1.0, 0.0));
        assert!(!travel.active(at + Duration::from_millis(360)));
    }

    #[test]
    fn rapid_reversals_keep_velocity_and_stay_in_bounds() {
        let at = Instant::now();
        let mut travel = Travel::default();
        travel.show(true, 360, at);
        for (index, ms) in [80, 110, 145, 190, 240, 260, 300].into_iter().enumerate() {
            let now = at + Duration::from_millis(ms);
            let before = travel.sample(now);
            travel.show(index % 2 != 0, 360, now);
            assert_eq!(travel.sample(now), before);
            for elapsed in 0..400 {
                let (value, velocity) = travel.sample(now + Duration::from_millis(elapsed));
                assert!((0.0..=1.0).contains(&value) && velocity.is_finite());
            }
        }
        assert_eq!(travel.sample(at + Duration::from_secs(2)), (0.0, 0.0));
    }

    #[test]
    fn reversing_reveal_preserves_position_and_reduced_motion_finishes_immediately() {
        let at = Instant::now();
        let mut travel = Travel::default();
        travel.show(true, 360, at);
        let halfway = at + Duration::from_millis(100);
        let before = travel.value(halfway);
        travel.show(false, 360, halfway);
        assert_eq!(before, travel.value(halfway));
        assert!(travel.value(halfway + Duration::from_millis(80)) < before);
        assert_eq!(travel.value(halfway + Duration::from_secs(1)), 0.0);
        travel.show(true, 0, halfway);
        assert_eq!(travel.value(halfway), 1.0);
        assert!(!travel.active(halfway));
    }
}
