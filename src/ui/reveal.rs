use std::time::{Duration, Instant};

use crate::data::settings::AnimCurve;

#[derive(Clone)]
pub(crate) struct Reveal {
    from: f32,
    target: f32,
    velocity: f32,
    start: Instant,
    duration: Duration,
    curve: AnimCurve,
}

impl Reveal {
    pub fn new(value: f32) -> Self {
        Self {
            from: value,
            target: value,
            velocity: 0.0,
            start: Instant::now(),
            duration: Duration::ZERO,
            curve: AnimCurve::Smooth,
        }
    }

    pub fn retarget(&mut self, shown: bool, ms: u64, curve: AnimCurve, now: Instant) {
        let moving = self.active(now);
        let (from, velocity) = self.sample(now);
        self.from = from;
        self.target = f32::from(shown);
        self.velocity = velocity;
        self.curve = curve;
        self.start = now;
        self.duration = if ms <= 1 {
            Duration::ZERO
        } else {
            let mut seconds = ms as f32 * 0.001 * (self.target - from).abs().sqrt();
            if velocity > 0.0 {
                seconds = seconds.min(3.0 * (1.0 - from) / velocity);
            } else if velocity < 0.0 {
                seconds = seconds.min(-3.0 * from / velocity);
            }
            Duration::from_secs_f32(seconds.max(0.0))
        };
        if !moving && !self.duration.is_zero() {
            let slope = match curve {
                AnimCurve::Out => 3.0,
                AnimCurve::Linear => 1.0,
                _ => 0.0,
            };
            self.velocity = (self.target - from) * slope / self.duration.as_secs_f32();
        }
    }

    pub fn duration(&self) -> Duration {
        self.duration
    }

    pub fn started(&self) -> Instant {
        self.start
    }

    pub fn remaining(&self, now: Instant) -> Duration {
        self.duration.saturating_sub(now.saturating_duration_since(self.start))
    }

    pub fn active(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) < self.duration
    }

    pub fn value(&self, now: Instant) -> f32 {
        self.sample(now).0
    }

    pub fn frame(&self, fraction: f32) -> f32 {
        if self.duration.is_zero() { self.target } else { self.position(fraction).clamp(0.0, 1.0) }
    }

    fn position(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let distance = self.target - self.from;
        let (eased, initial_slope) = match self.curve {
            AnimCurve::Spring => (1.0 - (1.0 - t).powi(4) * (1.0 + 4.0 * t), 0.0),
            AnimCurve::Smooth => (t * t * (3.0 - 2.0 * t), 0.0),
            AnimCurve::Out => (1.0 - (1.0 - t).powi(3), 3.0),
            AnimCurve::Linear => (t, 1.0),
        };
        let tangent = self.velocity * self.duration.as_secs_f32() - distance * initial_slope;
        self.from + distance * eased + tangent * t * (1.0 - t).powi(2)
    }

    fn sample(&self, now: Instant) -> (f32, f32) {
        if !self.active(now) {
            return (self.target, 0.0);
        }
        let span = self.duration.as_secs_f32();
        let t = now.saturating_duration_since(self.start).as_secs_f32() / span;
        if t == 0.0 {
            return (self.from, self.velocity);
        }
        let value = self.position(t);
        let before = (t - 0.0005).max(0.0);
        let after = (t + 0.0005).min(1.0);
        let velocity = (self.position(after) - self.position(before)) / ((after - before) * span);
        (value.clamp(0.0, 1.0), if value > 0.0 && value < 1.0 { velocity } else { 0.0 })
    }
}

pub(crate) fn smooth_range(value: f32, start: f32, end: f32) -> f32 {
    let t = ((value - start) / (end - start)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_reveals_keep_position_and_velocity_at_every_frame_rate() {
        for curve in [AnimCurve::Spring, AnimCurve::Smooth, AnimCurve::Out, AnimCurve::Linear] {
            for hz in [30, 60, 120, 144, 240] {
                let start = Instant::now();
                let mut reveal = Reveal::new(0.0);
                reveal.retarget(true, 420, curve, start);
                for frame in 1..120 {
                    let now = start + Duration::from_secs_f64(frame as f64 / hz as f64);
                    let before = reveal.sample(now);
                    if frame % 7 == 0 {
                        let moving = reveal.active(now);
                        reveal.retarget(frame % 14 == 0, 320, curve, now);
                        assert_eq!(reveal.sample(now).0, before.0);
                        if moving {
                            assert_eq!(reveal.sample(now).1, before.1);
                        }
                    }
                    assert!((0.0..=1.0).contains(&reveal.value(now)));
                    assert!(reveal.sample(now).1.is_finite());
                }
                let now = start + Duration::from_secs(5);
                reveal.retarget(false, 320, curve, now);
                assert_eq!(reveal.sample(now + Duration::from_secs(1)), (0.0, 0.0));
                reveal.retarget(true, 1, curve, now);
                assert_eq!(reveal.sample(now), (1.0, 0.0));
            }
        }
    }

    #[test]
    fn opening_is_monotonic_and_content_waits_for_room() {
        let mut reveal = Reveal::new(0.0);
        reveal.retarget(true, 420, AnimCurve::Spring, Instant::now());
        let mut previous = 0.0;
        for frame in 0..=1000 {
            let value = reveal.frame(frame as f32 / 1000.0);
            assert!(value >= previous && value <= 1.0);
            previous = value;
        }
        assert_eq!(smooth_range(0.7, 0.72, 0.97), 0.0);
        assert_eq!(smooth_range(1.0, 0.72, 0.97), 1.0);
        assert!(reveal.frame(0.5) > 0.8);
        let mut linear = Reveal::new(0.0);
        linear.retarget(true, 420, AnimCurve::Linear, Instant::now());
        assert!((linear.frame(0.25) - 0.25).abs() < 0.0001);
    }
}
