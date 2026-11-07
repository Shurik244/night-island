use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PetPerformance {
    Still,
    Eco,
    #[default]
    Balanced,
    Smooth,
}

impl PetPerformance {
    pub const ALL: [Self; 4] = [Self::Still, Self::Eco, Self::Balanced, Self::Smooth];

    pub fn name(self) -> &'static str {
        match self {
            Self::Still => "Pet: reduced motion",
            Self::Eco => "Pet: economical · up to 20 FPS",
            Self::Balanced => "Pet: balanced · up to 30 FPS",
            Self::Smooth => "Pet: smooth · up to 60 FPS",
        }
    }

    pub fn step(self, forward: bool) -> Self {
        let index = Self::ALL.iter().position(|v| *v == self).unwrap_or(2);
        Self::ALL[(index + if forward { 1 } else { 3 }) % 4]
    }

    pub fn interval(self, active: bool, sleeping: bool, detached: bool) -> u32 {
        let fps = match self {
            Self::Still => return 100,
            Self::Eco => {
                if sleeping {
                    4
                } else if active {
                    20
                } else {
                    10
                }
            }
            Self::Balanced => {
                if sleeping {
                    8
                } else if active {
                    30
                } else if detached {
                    12
                } else {
                    15
                }
            }
            Self::Smooth => {
                if sleeping {
                    12
                } else if active {
                    60
                } else if detached {
                    18
                } else {
                    20
                }
            }
        };
        (1000_u32).div_ceil(fps)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Motion {
    pub behavior: super::pet_behavior::BehaviorPose,
    pub mouth: f32,
    pub lean: f32,
    pub stretch: f32,
    pub warmth: f32,
    pub lid: f32,
    pub still: bool,
}

#[derive(Default)]
pub struct PetMotion {
    pub look: (f32, f32),
    pub motion: Motion,
    held: bool,
    near: bool,
    release: f32,
    greeting: f32,
    stroke: f32,
    previous_look: (f32, f32),
    interest: f32,
    interest_look: (f32, f32),
}

impl PetMotion {
    pub fn interest_gaze(
        &mut self,
        normal: (f32, f32),
        target: Option<(f32, f32)>,
        dt: f32,
        still: bool,
    ) -> ((f32, f32), f32) {
        let blend = 1.0 - (-dt.clamp(0.0, 0.25) * 10.0).exp();
        if let Some(target) = target {
            self.interest_look.0 += (target.0.clamp(-0.85, 0.85) - self.interest_look.0) * blend;
            self.interest_look.1 += (target.1.clamp(-0.85, 0.85) - self.interest_look.1) * blend;
        }
        self.interest += (f32::from(target.is_some() && !still) - self.interest) * blend;
        if self.interest < 0.002 {
            self.interest = 0.0;
        }
        if still {
            self.interest = 0.0;
        }
        (
            (
                normal.0 + (self.interest_look.0 - normal.0) * self.interest,
                normal.1 + (self.interest_look.1 - normal.1) * self.interest,
            ),
            self.interest,
        )
    }

    pub fn advance(&mut self, target: (f32, f32), dt: f32, near: bool, held: bool, still: bool) -> Motion {
        let dt = dt.clamp(0.0, 0.25);
        let smoothing = 1.0 - (-dt * 16.0).exp();
        let speed = ((target.0 - self.previous_look.0).abs() + (target.1 - self.previous_look.1).abs()) / dt.max(0.001);
        self.previous_look = target;
        if near && !self.near {
            self.greeting = 1.0;
        }
        if !held && self.held {
            self.release = 1.0;
        }
        self.held = held;
        self.near = near;
        self.greeting = (self.greeting - dt * 0.75).max(0.0);
        self.release = (self.release - dt * 1.6).max(0.0);
        self.stroke = (self.stroke + if near && speed > 0.12 && !held { dt * 1.8 } else { -dt * 0.7 }).clamp(0.0, 1.0);
        self.look.0 += (target.0 - self.look.0) * smoothing;
        self.look.1 += (target.1 - self.look.1) * smoothing;
        let lean = self.look.0
            * if held {
                0.15
            } else if near {
                0.07
            } else {
                0.025
            };
        self.motion.lean += (lean - self.motion.lean) * (1.0 - (-dt * 9.0).exp());
        let stretch =
            if held { 0.10 } else { -(self.release * std::f32::consts::PI * 3.0).sin() * self.release * 0.10 };
        self.motion.stretch += (stretch - self.motion.stretch) * smoothing;
        let warmth = (self.greeting * 0.42).max(self.stroke);
        self.motion.warmth += (warmth - self.motion.warmth) * (1.0 - (-dt * 9.0).exp());
        if warmth == 0.0 && self.motion.warmth < 0.005 {
            self.motion.warmth = 0.0;
        }
        self.motion.still = still;
        if still {
            self.motion.lean = 0.0;
            self.motion.stretch = 0.0;
            self.motion.warmth = 0.0;
            self.look = (0.0, 0.0);
        }
        self.motion
    }
}

pub fn pointer_gaze(offset: (f32, f32)) -> (f32, f32) {
    let soften = |value: f32| value / value.hypot(1.0);
    (soften(offset.0), soften(offset.1))
}

pub fn eyelid(remaining: f32) -> f32 {
    let progress = (1.0 - remaining / 0.18).clamp(0.0, 1.0);
    (std::f32::consts::PI * progress).sin().powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interest_enters_and_leaves_smoothly_without_a_pointer_greeting() {
        for fps in [15, 20, 30, 60, 144] {
            let mut pet = PetMotion::default();
            let dt = 1.0 / fps as f32;
            let mut previous = 0.0;
            for _ in 0..fps {
                let (gaze, weight) = pet.interest_gaze((0.0, 0.0), Some((2.0, 0.0)), dt, false);
                assert!(weight >= previous && gaze.0 <= 0.85);
                pet.advance(gaze, dt, false, false, false);
                assert_eq!(pet.greeting, 0.0);
                previous = weight;
            }
            assert!(previous > 0.99);
            let (gaze, weight) = pet.interest_gaze((0.0, 0.0), None, dt, false);
            assert!(weight > 0.4 && weight < previous && gaze.0 > 0.0);
            for _ in 0..fps {
                pet.interest_gaze((0.0, 0.0), None, dt, false);
            }
            assert_eq!(pet.interest, 0.0);
            assert_eq!(pet.interest_gaze((0.0, 0.0), Some((1.0, 0.0)), dt, true), ((0.0, 0.0), 0.0));
        }
    }

    #[test]
    fn pointer_tracking_continues_beyond_the_old_radius_and_across_monitors() {
        let mut previous = 0.0;
        for distance in [0.0, 100.0, 247.0, 260.0, 500.0, 1920.0, 7680.0, 20_000.0] {
            let gaze = pointer_gaze((distance / 260.0, -distance / 200.0));
            assert!(gaze.0 >= previous && gaze.0 < 1.0);
            assert!(gaze.1 <= 0.0 && gaze.1 > -1.0);
            let opposite = pointer_gaze((-distance / 260.0, distance / 200.0));
            assert_eq!(opposite, (-gaze.0, -gaze.1));
            previous = gaze.0;
        }
        for fps in [10, 30, 60] {
            let mut motion = PetMotion::default();
            for _ in 0..fps {
                motion.advance(pointer_gaze((2.0, -2.0)), 1.0 / fps as f32, false, false, false);
            }
            let first = motion.look;
            for _ in 0..fps {
                motion.advance(pointer_gaze((6.0, -6.0)), 1.0 / fps as f32, false, false, false);
            }
            assert!(motion.look.0 > first.0 && motion.look.1 < first.1);
            for _ in 0..fps {
                motion.advance(pointer_gaze((-6.0, 6.0)), 1.0 / fps as f32, false, false, false);
            }
            assert!(motion.look.0 < -0.98 && motion.look.1 > 0.98);
        }
    }

    #[test]
    fn gaze_converges_without_fps_dependent_steps_or_overshoot() {
        let mut results = Vec::new();
        for fps in [10, 20, 30, 60] {
            let mut pet = PetMotion::default();
            for _ in 0..fps {
                pet.advance((1.0, -0.7), 1.0 / fps as f32, true, false, false);
                assert!((0.0..=1.0).contains(&pet.look.0));
            }
            results.push(pet.look);
        }
        assert!(results.iter().all(|look| (look.0 - 1.0).abs() < 0.001 && (look.1 + 0.7).abs() < 0.001));
    }

    #[test]
    fn carrying_petting_and_landing_settle_without_frame_dependent_jumps() {
        for fps in [10, 30, 60] {
            let mut pet = PetMotion::default();
            let dt = 1.0 / fps as f32;
            for _ in 0..fps {
                pet.advance((0.8, 0.3), dt, true, true, false);
            }
            assert!(pet.motion.stretch > 0.09 && pet.motion.lean > 0.1);
            for i in 0..fps * 4 {
                let motion = pet.advance((0.0, 0.0), dt, false, false, false);
                assert!(motion.stretch.abs() < 0.11 && motion.lean.abs() < 0.16);
                if i == fps * 4 - 1 {
                    assert!(motion.stretch.abs() < 0.001 && motion.lean.abs() < 0.001);
                }
            }
            for i in 0..fps {
                pet.advance(((i as f32 * dt * 8.0).sin() * 0.2, 0.0), dt, true, false, false);
            }
            assert!(pet.motion.warmth > 0.5);
        }
    }

    #[test]
    fn performance_caps_and_reduced_motion_are_independent_of_island_fps() {
        for preset in PetPerformance::ALL {
            assert!(preset.interval(false, true, true) >= preset.interval(true, false, true));
            assert!(preset.interval(true, false, true) >= 17);
        }
        let mut pet = PetMotion::default();
        let pose = pet.advance((1.0, 1.0), 0.1, true, true, true);
        assert_eq!(pet.look, (0.0, 0.0));
        assert_eq!(pose.lean, 0.0);
        assert_eq!(pose.stretch, 0.0);
        assert!(eyelid(0.09) > 0.99 && eyelid(0.18) < 0.01 && eyelid(0.0) < 0.01);
    }
}
