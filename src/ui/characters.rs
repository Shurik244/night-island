use serde::{Deserialize, Serialize};
use windows::Win32::Graphics::DirectWrite::{DWRITE_FONT_WEIGHT_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER};
use windows::core::Result;

use crate::ui::gfx::{Painter, rgba};

pub const BOX_W: f32 = 40.0;
pub const BOX_H: f32 = 30.0;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Character {
    None,
    Pixel,
    #[default]
    Spark,
    Eyes,
    Cat,
    Robot,
    Blob,
    Bear,
    Custom,
}

impl Character {
    pub const ALL: [Character; 9] = [
        Character::None,
        Character::Pixel,
        Character::Spark,
        Character::Eyes,
        Character::Cat,
        Character::Robot,
        Character::Blob,
        Character::Bear,
        Character::Custom,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Character::None => "No character",
            Character::Pixel => "Pixel",
            Character::Spark => "Spark",
            Character::Eyes => "Eyes",
            Character::Cat => "Cat",
            Character::Robot => "Robot",
            Character::Blob => "Blob",
            Character::Bear => "Bear",
            Character::Custom => "Custom pet",
        }
    }

    pub fn step(self, forward: bool) -> Character {
        let n = Self::ALL.len();
        let i = Self::ALL.iter().position(|c| *c == self).unwrap_or(0);
        Self::ALL[if forward { (i + 1) % n } else { (i + n - 1) % n }]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reaction {
    Idle,
    Music,
    Done,
    Attention,
    File,
    Sleep,
}

#[derive(Clone, Copy)]
pub struct Pose {
    pub motion: super::pet_motion::Motion,
    pub t: f32,
    pub reaction: Reaction,
    pub look: (f32, f32),
    pub blink: bool,
    pub accent: (f32, f32, f32),
    pub beat: f32,
}

impl Pose {
    pub fn lively(&self) -> bool {
        matches!(self.reaction, Reaction::Music | Reaction::Done | Reaction::Attention | Reaction::File)
    }

    fn bob(&self) -> f32 {
        let t = self.t;
        let gait = self.motion.behavior;
        if gait.walk > 0.05 {
            return -(gait.stride * 2.0).cos().abs() * 0.45 * gait.walk;
        }
        let bob = match self.reaction {
            Reaction::Idle => (t * 1.8).sin() * 0.15,
            Reaction::Music => -self.beat * 1.0 + (t * 9.0).sin() * 0.4,
            Reaction::Done => -(t * 5.0).sin().abs() * 1.1,
            Reaction::Attention => 0.0,
            Reaction::File => (t * 7.0).sin() * 1.0,
            Reaction::Sleep => (t * 1.0).sin() * 0.45,
        };
        bob * (1.0 - gait.sit * 0.7)
    }

    fn shake(&self) -> f32 {
        if self.reaction == Reaction::Attention { (self.t * 8.0).sin() * 0.5 } else { 0.0 }
    }

    fn mood(&self, calm: (f32, f32, f32)) -> (f32, f32, f32) {
        match self.reaction {
            Reaction::Idle => calm,
            Reaction::Music => self.accent,
            Reaction::Done => (0.47, 0.9, 0.55),
            Reaction::Attention => (1.0, 0.55, 0.28),
            Reaction::File => (0.5, 0.75, 1.0),
            Reaction::Sleep => (0.58, 0.63, 0.78),
        }
    }
}

type Rgb = (f32, f32, f32);

fn c(rgb: Rgb, a: f32) -> windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F {
    rgba(rgb.0, rgb.1, rgb.2, a)
}

const DARK: Rgb = (0.1, 0.11, 0.15);
const WHITE: Rgb = (0.96, 0.96, 0.98);

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
}

fn lit(p: &Painter, s: f32, oval: (f32, f32, f32, f32), color: Rgb) -> Result<()> {
    p.fill_ellipse_gradient(
        (oval.0 * s, oval.1 * s, oval.2 * s, oval.3 * s),
        c(mix(color, WHITE, 0.24), 1.0),
        c(mix(color, DARK, 0.24), 1.0),
    )
}

fn curve(p: &Painter, s: f32, points: [(f32, f32); 4], width: f32, color: Rgb, alpha: f32) -> Result<()> {
    p.bezier(points.map(|(x, y)| (x * s, y * s)), width * s, c(color, alpha))
}

fn shape(p: &Painter, s: f32, start: (f32, f32), curves: &[[(f32, f32); 3]], color: Rgb, alpha: f32) -> Result<()> {
    let curves: Vec<_> = curves.iter().map(|segment| segment.map(|(x, y)| (x * s, y * s))).collect();
    p.fill_curves((start.0 * s, start.1 * s), &curves, c(color, alpha))
}

fn smile(p: &Painter, s: f32, pose: &Pose, x: f32, y: f32, width: f32, color: Rgb) -> Result<()> {
    if pose.motion.mouth > 0.02 {
        return lit(p, s, (x, y + 0.35, width * 0.7, 0.35 + pose.motion.mouth * 1.25), color);
    }
    let joy = if pose.reaction == Reaction::Done { 1.0 } else { pose.motion.warmth };
    let depth = 0.45 + joy * 0.8;
    curve(
        p,
        s,
        [(x - width, y), (x - width * 0.6, y + depth), (x + width * 0.6, y + depth), (x + width, y)],
        0.65,
        color,
        0.85,
    )
}

#[cfg(test)]
pub fn draw(p: &Painter, character: Character, w: f32, pose: &Pose) -> Result<()> {
    draw_with_design(p, character, w, pose, None)
}

pub fn draw_attached(
    p: &Painter,
    character: Character,
    w: f32,
    pose: &Pose,
    design: Option<&super::pet_design::PetDesign>,
) -> Result<()> {
    let mut rest = *pose;
    rest.t = 0.0;
    rest.beat = 0.0;
    rest.look = (0.0, 0.0);
    rest.reaction = Reaction::Idle;
    rest.motion = Default::default();
    let (left, top, right, bottom) = match character {
        Character::Spark => (10.8, 5.8, 29.2, 24.2),
        Character::Blob => (9.0, 7.8, 31.0, 26.2),
        Character::Eyes => (9.0, 7.0, 31.0, 23.0),
        _ => drawing_bounds(character, &rest, design),
    };
    let k = (22.0 / (bottom - top)).min(28.0 / (right - left));
    let scale = w / BOX_W;
    let old = p.push_offset(0.0, 0.0);
    p.set_transform(
        &(windows_numerics::Matrix3x2 {
            M11: k,
            M22: k,
            M31: (20.0 - (left + right) * 0.5 * k) * scale,
            M32: (15.0 - (top + bottom) * 0.5 * k) * scale,
            ..Default::default()
        } * old),
    );
    let result = draw_with_design(p, character, w, pose, design);
    p.set_transform(&old);
    result
}

pub fn draw_with_design(
    p: &Painter,
    character: Character,
    w: f32,
    pose: &Pose,
    design: Option<&super::pet_design::PetDesign>,
) -> Result<()> {
    let s = w / BOX_W;
    let mut pose = *pose;
    if pose.motion.still {
        pose.t = 0.0;
        pose.beat = 0.0;
        pose.look = (0.0, 0.0);
        pose.blink = false;
        pose.motion = super::pet_motion::Motion {
            still: true,
            behavior: super::pet_behavior::BehaviorPose {
                sit: pose.motion.behavior.sit,
                sleep: pose.motion.behavior.sleep,
                ..Default::default()
            },
            ..Default::default()
        };
    }
    let gait = pose.motion.behavior;
    let breathing = if pose.motion.still { 0.0 } else { (pose.t * 1.65).sin() * 0.015 };
    let (sx, sy) = (
        1.0 + breathing - pose.motion.stretch * 0.45 + gait.sit * 0.035 + gait.sleep * 0.08,
        1.0 - breathing + pose.motion.stretch - gait.sit * 0.13 - gait.sleep * 0.16,
    );
    let angle = pose.motion.lean + gait.facing * gait.walk * 0.035 - gait.sleep * 0.08;
    let (sin, cos) = angle.sin_cos();
    let (cx, cy) = (20.0 * s, 24.0 * s);
    let mut transform = windows_numerics::Matrix3x2 {
        M11: sx * cos,
        M12: sx * sin,
        M21: -sy * sin,
        M22: sy * cos,
        M31: cx - cx * sx * cos + cy * sy * sin,
        M32: cy - cx * sx * sin - cy * sy * cos,
    };
    let (left, top, right, bottom) = drawing_bounds(character, &pose, design);
    let mut bounds = (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for (x, y) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
        let px = x * s * transform.M11 + y * s * transform.M21 + transform.M31;
        let py = x * s * transform.M12 + y * s * transform.M22 + transform.M32;
        bounds = (bounds.0.min(px), bounds.1.min(py), bounds.2.max(px), bounds.3.max(py));
    }
    let fit = (39.0 * s / (bounds.2 - bounds.0)).min(29.0 * s / (bounds.3 - bounds.1)).min(1.0);
    transform = transform * windows_numerics::Matrix3x2::scale(fit, fit);
    transform.M31 += (0.5 * s - bounds.0 * fit).max(0.0) + (39.5 * s - bounds.2 * fit).min(0.0);
    transform.M32 += (0.5 * s - bounds.1 * fit).max(0.0) + (29.5 * s - bounds.3 * fit).min(0.0);
    let old = p.push_offset(0.0, 0.0);
    p.set_transform(&(transform * old));
    let result = match character {
        Character::None => Ok(()),
        Character::Pixel => draw_pixel(p, s, &pose),
        Character::Spark => draw_spark(p, s, &pose),
        Character::Eyes => draw_eyes(p, s, &pose),
        Character::Cat => draw_cat(p, s, &pose),
        Character::Robot => draw_robot(p, s, &pose),
        Character::Blob => draw_blob(p, s, &pose),
        Character::Bear => draw_designed(p, s, &pose, super::pet_design::PetDesign::bear()),
        Character::Custom => draw_designed(p, s, &pose, design.unwrap_or_else(|| super::pet_design::PetDesign::bear())),
    };
    p.set_transform(&old);
    result?;
    if pose.motion.warmth > 0.4 && character != Character::None {
        let alpha = ((pose.motion.warmth - 0.4) / 0.6) * 0.9;
        let (x, y) = (32.0 * s, (3.5 + (pose.t * 2.0).sin() * 0.6) * s);
        p.fill_curves(
            (x, y + 0.7 * s),
            &[
                [(x - 1.1 * s, y - 1.5 * s), (x - 3.6 * s, y + 0.1 * s), (x, y + 3.2 * s)],
                [(x + 3.6 * s, y + 0.1 * s), (x + 1.1 * s, y - 1.5 * s), (x, y + 0.7 * s)],
            ],
            rgba(1.0, 0.59, 0.68, alpha),
        )?;
    }
    Ok(())
}

fn drawing_bounds(
    character: Character,
    pose: &Pose,
    design: Option<&super::pet_design::PetDesign>,
) -> (f32, f32, f32, f32) {
    use super::pet_design::PetBody;
    let (cx, cy) = (20.0 + pose.shake(), 17.0 + pose.bob());
    let mut bounds = match character {
        Character::Spark => {
            let radius = 11.6 + if pose.reaction == Reaction::Music { pose.beat * 0.9 } else { 0.0 };
            (cx - radius, cy - 2.0 - radius, cx + radius, cy - 2.0 + radius)
        }
        Character::Cat => (cx - 11.5, cy - 14.0, cx + 11.5, cy + 10.1 + pose.motion.behavior.walk * 1.4),
        Character::Robot => {
            let glow = if pose.reaction == Reaction::Music { 1.8 + pose.beat * 1.4 } else { 1.8 };
            (cx - 12.0, cy - 13.2 - glow, cx + 12.0, cy + 8.0)
        }
        Character::Bear | Character::Custom => {
            let default = super::pet_design::PetDesign::bear();
            let pet = if character == Character::Bear { default } else { design.unwrap_or(default) };
            let (cx, cy) = (20.0 + pose.look.0 * 0.4, 13.6 + pose.bob() * 0.5);
            let width = 9.4 * pet.width;
            let half = width.max(width * 0.76 + 5.2 * pet.ear_size);
            let top = match pet.body {
                PetBody::Bear => cy - 6.8 - 4.0 * pet.ear_size - 0.35,
                PetBody::Cat | PetBody::Fox => (cy - 6.8 - 4.5 * pet.ear_size - 0.35).max(0.8),
                PetBody::Rabbit => cy - 7.6 - 5.0 * pet.ear_size - 0.35,
                PetBody::Robot => cy - 10.6 - 0.35,
                PetBody::Ghost => cy - 7.9,
            };
            (
                cx - half,
                top,
                cx + half.max(if pet.tail { width + 5.9 } else { 0.0 }),
                cy + 14.1 + pose.motion.behavior.walk * 1.5,
            )
        }
        _ => (5.0, 3.0 + pose.bob(), 35.0, 28.0 + pose.bob()),
    };
    if pose.reaction == Reaction::Sleep {
        bounds = (bounds.0.min(28.0), bounds.1.min(-0.2), bounds.2.max(40.0), bounds.3.max(12.2));
    }
    bounds
}

#[allow(clippy::too_many_arguments)]
fn eye(p: &Painter, s: f32, pose: &Pose, x: f32, y: f32, rx: f32, ry: f32, color: Rgb) -> Result<()> {
    if matches!(pose.reaction, Reaction::Sleep | Reaction::Done) {
        let arch = if pose.reaction == Reaction::Done { -ry * 0.85 } else { ry * 0.3 };
        return curve(
            p,
            s,
            [(x - rx, y), (x - rx * 0.45, y + arch), (x + rx * 0.45, y + arch), (x + rx, y)],
            0.85,
            color,
            0.95,
        );
    }
    let openness = 1.0 - pose.motion.lid * 0.94;
    let (x, y) = (x + pose.look.0 * 1.15, y + pose.look.1 * 0.8);
    let height = if pose.blink { 0.25 } else { ry * openness * (1.0 - pose.motion.warmth * 0.12) };
    let surprise = if pose.reaction == Reaction::Attention { 1.15 } else { 1.0 };
    p.fill_ellipse_gradient(
        (x * s, y * s, rx * s * surprise, height.max(0.25) * s * surprise),
        c(mix(color, WHITE, 0.08), 1.0),
        c(color, 1.0),
    )?;
    if !pose.blink && height > 0.7 {
        p.fill_ellipse(
            (x - rx * 0.23) * s,
            (y - height * 0.33) * s,
            rx * 0.28 * s,
            height * 0.20 * s,
            c(WHITE, 0.88 * openness),
        )?;
    }
    Ok(())
}

fn zzz(p: &Painter, s: f32, pose: &Pose) -> Result<()> {
    if pose.reaction != Reaction::Sleep {
        return Ok(());
    }
    let drift = (pose.t * 0.9).sin() * 1.2;
    p.text(
        "z",
        (28.0 * s, (1.0 + drift) * s, 40.0 * s, (11.0 + drift) * s),
        9.0 * s,
        DWRITE_FONT_WEIGHT_BOLD,
        DWRITE_TEXT_ALIGNMENT_CENTER,
        c(WHITE, 0.7),
    )
}

fn draw_pixel(p: &Painter, s: f32, pose: &Pose) -> Result<()> {
    const BODY: [&str; 8] = [
        "..#.....#..",
        "...#...#...",
        "..#######..",
        ".##.###.##.",
        "###########",
        "#.#######.#",
        "#.#.....#.#",
        "...##.##...",
    ];
    const LEGS_B: &str = "..#.....#..";
    let color = match pose.reaction {
        Reaction::Attention => (0.9, 0.28, 0.24),
        Reaction::Done => (0.43, 0.82, 0.47),
        Reaction::Sleep => (0.55, 0.42, 0.37),
        _ => (0.91, 0.45, 0.32),
    };
    let cell = 2.0 * s;
    let (ox, oy) = ((BOX_W - 22.0) / 2.0 * s + pose.shake() * s, (BOX_H - 16.0) / 2.0 * s + pose.bob() * s);
    let frame_b = if pose.motion.behavior.walk > 0.05 {
        pose.motion.behavior.stride.sin() > 0.0
    } else {
        pose.lively() && ((pose.t * 4.0) as i32) % 2 == 1
    };
    for (row, line) in BODY.iter().enumerate() {
        let line = if row == 7 && frame_b { LEGS_B } else { line };
        for (col, ch) in line.chars().enumerate() {
            if ch == '#' {
                let (x, y) = (ox + col as f32 * cell, oy + row as f32 * cell);
                p.fill_rect(
                    (x, y, x + cell, y + cell),
                    c(
                        mix(
                            color,
                            if row < 3 { WHITE } else { DARK },
                            if row < 3 { 0.18 } else { (row as f32 - 2.0) * 0.025 },
                        ),
                        1.0,
                    ),
                )?;
            }
        }
    }
    for col in [3.0f32, 7.0] {
        let (x, y) = (ox + col * cell, oy + 3.0 * cell);
        if pose.blink || pose.reaction == Reaction::Sleep {
            p.fill_rect((x, y, x + cell, y + cell), c(color, 1.0))?;
        } else {
            let (px, py) = (x + cell * 0.25 + pose.look.0 * cell * 0.3, y + cell * 0.25 + pose.look.1 * cell * 0.3);
            p.fill_rect((px, py, px + cell * 0.55, py + cell * 0.55), c(WHITE, 1.0))?;
        }
    }
    if pose.motion.mouth > 0.02 {
        let (x, y) = (ox + 4.5 * cell, oy + 5.4 * cell);
        p.fill_rect((x, y, x + 2.0 * cell, y + (0.4 + pose.motion.mouth) * cell), c(DARK, 1.0))?;
    }
    zzz(p, s, pose)
}

fn draw_spark(p: &Painter, s: f32, pose: &Pose) -> Result<()> {
    let tint = pose.mood((0.72, 0.77, 0.95));
    let pulse = if pose.reaction == Reaction::Music { pose.beat * 0.7 } else { 0.0 };
    let r = 9.2 + pulse;
    let (cx, cy) = (20.0 + pose.shake(), 15.0 + pose.bob());
    p.fill_ellipse_gradient((cx * s, cy * s, (r + 2.3) * s, (r + 2.3) * s), c(tint, 0.24), c(tint, 0.0))?;
    p.fill_ellipse_gradient((cx * s, (cy + 10.0) * s, 6.0 * s, 1.0 * s), c(DARK, 0.24), c(DARK, 0.0))?;
    p.fill_ellipse_gradient(
        (cx * s, cy * s, r * s, r * s),
        c(WHITE, 1.0),
        c(mix(tint, (0.65, 0.68, 0.84), 0.45), 1.0),
    )?;
    p.fill_ellipse((cx - 3.5) * s, (cy - 5.2) * s, 3.0 * s, 1.35 * s, c(WHITE, 0.28))?;
    for sign in [-1.0, 1.0] {
        p.fill_ellipse(
            (cx + sign * 5.6) * s,
            (cy + 2.2) * s,
            1.5 * s,
            0.75 * s,
            c((0.94, 0.58, 0.65), 0.12 + pose.motion.warmth * 0.26),
        )?;
        eye(p, s, pose, cx + sign * 3.2, cy - 0.4, 1.5, 2.45, (0.14, 0.16, 0.24))?;
    }
    smile(p, s, pose, cx, cy + 3.2, 1.35, (0.32, 0.34, 0.46))?;
    zzz(p, s, pose)
}

fn draw_eyes(p: &Painter, s: f32, pose: &Pose) -> Result<()> {
    let (cy, dx) = (15.0 + pose.bob() * 0.35, pose.shake());
    for sign in [-1.0, 1.0] {
        let x = 20.0 + sign * 6.1 + dx;
        if pose.reaction == Reaction::Sleep || pose.blink {
            curve(p, s, [(x - 3.6, cy), (x - 1.5, cy + 1.5), (x + 1.5, cy + 1.5), (x + 3.6, cy)], 1.4, WHITE, 0.9)?;
        } else {
            let height = 6.4 * (1.0 - pose.motion.lid * 0.9 - pose.motion.mouth * 0.12);
            lit(p, s, (x, cy, 4.55, height), (0.94, 0.95, 0.99))?;
            let mut gaze = *pose;
            gaze.look.0 *= 1.25;
            eye(p, s, &gaze, x, cy + 0.4, 2.2, (height * 0.58).min(3.6), (0.12, 0.15, 0.22))?;
        }
    }
    zzz(p, s, pose)
}

fn draw_cat(p: &Painter, s: f32, pose: &Pose) -> Result<()> {
    let (cx, cy) = (20.0 + pose.shake(), 17.0 + pose.bob());
    let fur = (0.89, 0.91, 0.96);
    let pink = (0.92, 0.62, 0.67);
    let wiggle = (pose.t * 2.4).sin() * (0.18 + pose.motion.warmth * 0.4) + pose.look.0 * 0.22;
    for sign in [-1.0, 1.0] {
        let at = |x: f32, y: f32| (cx + sign * x, cy + y);
        shape(
            p,
            s,
            at(3.0, -7.0),
            &[
                [at(4.0, -10.2), at(7.0 + wiggle, -14.0), at(8.5 + wiggle, -13.1)],
                [at(10.0, -11.5), at(11.0, -5.6), at(10.3, -3.7)],
                [at(8.0, -3.7), at(5.0, -5.0), at(3.0, -7.0)],
            ],
            fur,
            1.0,
        )?;
        shape(
            p,
            s,
            at(5.1, -7.3),
            &[
                [at(6.0, -9.3), at(7.5 + wiggle, -11.3), at(8.0 + wiggle, -10.9)],
                [at(8.8, -9.2), at(9.0, -7.0), at(8.9, -5.8)],
                [at(7.0, -5.8), at(6.0, -6.0), at(5.1, -7.3)],
            ],
            pink,
            0.85,
        )?;
        let step = pose.motion.behavior.stride.sin() * pose.motion.behavior.walk * sign;
        lit(
            p,
            s,
            (cx + sign * (6.4 + pose.motion.behavior.sit * 0.4) + step * 0.8, cy + 8.4 + step * 1.4, 3.2, 1.6),
            fur,
        )?;
    }
    lit(p, s, (cx, cy, 11.5, 8.9), fur)?;
    lit(p, s, (cx, cy + 3.3, 5.9, 3.6), (0.98, 0.97, 0.97))?;
    for sign in [-1.0, 1.0] {
        eye(p, s, pose, cx + sign * 4.3, cy - 0.7, 1.65, 2.25, (0.16, 0.18, 0.24))?;
        p.fill_ellipse(
            (cx + sign * 7.7) * s,
            (cy + 2.0) * s,
            2.0 * s,
            0.85 * s,
            c(pink, 0.25 + pose.motion.warmth * 0.2),
        )?;
        for offset in [0.0, 1.5] {
            curve(
                p,
                s,
                [
                    (cx + sign * 6.1, cy + 2.8 + offset),
                    (cx + sign * 7.6, cy + 2.9 + offset),
                    (cx + sign * 9.1, cy + 2.0 + offset * 1.5),
                    (cx + sign * 10.7, cy + 2.1 + offset * 1.5),
                ],
                0.42,
                (0.46, 0.48, 0.56),
                0.66,
            )?;
        }
    }
    lit(p, s, (cx, cy + 2.1, 1.15, 0.75), pink)?;
    smile(p, s, pose, cx, cy + 3.8, 1.75, (0.39, 0.34, 0.42))?;
    zzz(p, s, pose)
}

fn draw_robot(p: &Painter, s: f32, pose: &Pose) -> Result<()> {
    let (cx, cy) = (20.0 + pose.shake(), 17.0 + pose.bob());
    let metal = (0.66, 0.69, 0.75);
    p.fill_round_gradient(
        ((cx - 11.0) * s, (cy - 8.0) * s, (cx + 11.0) * s, (cy + 8.0) * s),
        5.0 * s,
        c((0.94, 0.95, 0.98), 1.0),
        c((0.47, 0.52, 0.63), 1.0),
    )?;
    p.fill_round_rect(
        ((cx - 8.6) * s, (cy - 5.6) * s, (cx + 8.6) * s, (cy + 5.6) * s),
        3.0 * s,
        c((0.14, 0.16, 0.21), 1.0),
    )?;
    p.fill_round_gradient(
        ((cx - 8.0) * s, (cy - 5.2) * s, (cx + 8.0) * s, (cy - 1.0) * s),
        2.5 * s,
        c(WHITE, 0.12),
        c(WHITE, 0.0),
    )?;
    for sign in [-1.0f32, 1.0] {
        p.fill_round_rect(
            ((cx + sign * 11.0 - 1.0) * s, (cy - 2.5) * s, (cx + sign * 11.0 + 1.0) * s, (cy + 2.5) * s),
            0.8 * s,
            c(metal, 1.0),
        )?;
    }
    let beam = match pose.reaction {
        Reaction::Attention => (1.0, 0.35, 0.3),
        Reaction::Done => (0.5, 0.95, 0.6),
        Reaction::Sleep => (0.25, 0.4, 0.38),
        _ => (0.35, 0.92, 0.7),
    };
    for sign in [-1.0f32, 1.0] {
        let x = cx + sign * 4.4 + pose.look.0 * 1.2;
        let y = cy - 0.6 + pose.look.1 * 0.9;
        if pose.blink || pose.reaction == Reaction::Sleep {
            p.line(((x - 1.9) * s, y * s), ((x + 1.9) * s, y * s), 1.5 * s, c(beam, 0.9))?;
        } else {
            p.fill_round_rect(
                (
                    (x - 2.0) * s,
                    (y - 2.4 * (1.0 - pose.motion.lid * 0.94)) * s,
                    (x + 2.0) * s,
                    (y + 2.4 * (1.0 - pose.motion.lid * 0.94)) * s,
                ),
                1.2 * s,
                c(beam, 1.0),
            )?;
        }
    }
    for x in [-1.6, 0.0, 1.6] {
        p.fill_ellipse(
            (cx + x) * s,
            (cy + 4.0) * s,
            0.32 * s,
            (0.32 + pose.motion.mouth * (1.1 - x.abs() * 0.25)) * s,
            c(beam, 0.55),
        )?;
    }
    p.line((cx * s, (cy - 8.0) * s), (cx * s, (cy - 12.5) * s), 1.3 * s, c(metal, 1.0))?;
    let tip = if pose.reaction == Reaction::Music { pose.mood(WHITE) } else { (1.0, 0.72, 0.28) };
    let glow = if pose.reaction == Reaction::Music { 1.8 + pose.beat * 1.4 } else { 1.8 };
    lit(p, s, (cx, cy - 13.2, glow, glow), tip)
}

fn draw_blob(p: &Painter, s: f32, pose: &Pose) -> Result<()> {
    let beat = if pose.reaction == Reaction::Music { pose.beat } else { 0.0 };
    let (cx, cy) = (20.0 + pose.shake(), 17.0 + pose.bob());
    let (rx, ry) = (11.0 * (1.0 + 0.09 * beat), 9.2 * (1.0 - 0.13 * beat));
    let skin = match pose.reaction {
        Reaction::Attention => (1.0, 0.55, 0.5),
        Reaction::Done => (0.7, 0.92, 0.6),
        Reaction::Sleep => (0.76, 0.62, 0.7),
        _ => (0.97, 0.63, 0.68),
    };
    lit(p, s, (cx, cy, rx, ry), skin)?;
    p.fill_ellipse((cx - 4.5) * s, (cy - 4.6) * s, 3.0 * s, 1.1 * s, c(WHITE, 0.34))?;
    for sign in [-1.0f32, 1.0] {
        p.fill_ellipse((cx + sign * 7.0) * s, (cy + 3.2) * s, 2.3 * s, 1.5 * s, c((0.93, 0.4, 0.5), 0.55))?;
    }
    eye(p, s, pose, cx - 4.2, cy - 0.6, 1.5, 2.4, DARK)?;
    eye(p, s, pose, cx + 4.2, cy - 0.6, 1.5, 2.4, DARK)?;
    smile(p, s, pose, cx, cy + 3.3, 1.8, (0.48, 0.19, 0.29))?;
    zzz(p, s, pose)
}

fn draw_designed(p: &Painter, s: f32, pose: &Pose, design: &super::pet_design::PetDesign) -> Result<()> {
    use super::pet_design::{PetAccessory, PetBody, PetPattern};
    let rgb = |value: [u8; 3]| (value[0] as f32 / 255.0, value[1] as f32 / 255.0, value[2] as f32 / 255.0);
    let (fur, belly, eyes, accent) = (rgb(design.fur), rgb(design.belly), rgb(design.eyes), rgb(design.accent));
    let lining = mix(belly, fur, 0.35);
    let blush = mix(fur, (0.97, 0.56, 0.61), 0.5);
    let luminance = |color: Rgb| color.0 * 0.2126 + color.1 * 0.7152 + color.2 * 0.0722;
    let eyes = if (luminance(fur) - luminance(eyes)).abs() < 0.25 {
        if luminance(fur) > 0.45 { DARK } else { WHITE }
    } else {
        eyes
    };
    let (cx, cy) = (20.0 + pose.look.0 * 0.4, 13.6 + pose.bob() * 0.5);
    let width = 9.4 * design.width;
    let breathe = (pose.t * 1.65).sin();
    if design.tail {
        let wag = (pose.t * (3.5 + pose.motion.warmth * 3.0)).sin() * 1.2;
        if design.body == PetBody::Cat {
            p.bezier(
                [
                    ((cx + width * 0.45) * s, (cy + 10.0) * s),
                    ((cx + width + 5.0) * s, (cy + 13.0) * s),
                    ((cx + width + 5.0) * s, (cy + 3.0 + wag) * s),
                    ((cx + width + 1.5) * s, (cy + 5.0 + wag) * s),
                ],
                2.1 * s,
                c(fur, 1.0),
            )?;
        } else if design.body == PetBody::Rabbit {
            lit(p, s, (cx + width * 0.95, cy + 9.2, 2.5, 2.5), belly)?;
        } else {
            lit(p, s, (cx + width + 2.5, cy + 8.0 + wag, 3.5, 2.3), fur)?;
            p.fill_ellipse((cx + width + 4.4) * s, (cy + 7.4 + wag) * s, 1.5 * s, 1.5 * s, c(belly, 1.0))?;
        }
    }
    for sign in [-1.0, 1.0] {
        let ex = cx + sign * width * 0.76;
        let ey = cy - 6.8 + sign * pose.look.0 * 0.35;
        match design.body {
            PetBody::Bear => {
                lit(p, s, (ex, ey, 4.1 * design.ear_size, 4.0 * design.ear_size), fur)?;
                p.fill_ellipse(ex * s, ey * s, 2.3 * design.ear_size * s, 2.2 * design.ear_size * s, c(lining, 0.9))?;
            }
            PetBody::Cat | PetBody::Fox => {
                let tip = (ey - 4.5 * design.ear_size).max(0.8);
                shape(
                    p,
                    s,
                    (ex - 3.6, ey + 4.0),
                    &[
                        [(ex - 3.3, ey + 0.7), (ex + sign * 1.1 - 0.8, tip), (ex + sign * 1.1, tip)],
                        [(ex + sign * 1.1 + 0.8, tip), (ex + 3.3, ey + 0.7), (ex + 3.6, ey + 4.0)],
                        [(ex + 2.0, ey + 4.4), (ex - 2.0, ey + 4.4), (ex - 3.6, ey + 4.0)],
                    ],
                    fur,
                    1.0,
                )?;
                p.fill_triangle(
                    ((ex - 2.0) * s, (ey + 2.3) * s),
                    ((ex + sign * 0.7) * s, (tip + 2.0) * s),
                    ((ex + 2.0) * s, (ey + 2.3) * s),
                    c(lining, 0.9),
                )?;
            }
            PetBody::Rabbit => {
                lit(p, s, (ex, ey - 0.8, 2.3, 5.0 * design.ear_size), fur)?;
                p.fill_ellipse(ex * s, (ey - 1.2) * s, 1.0 * s, 3.3 * design.ear_size * s, c(lining, 0.9))?;
            }
            PetBody::Robot => {
                p.line((ex * s, ey * s), (ex * s, (ey - 2.0) * s), s, c(belly, 1.0))?;
                p.fill_ellipse(ex * s, (ey - 2.5) * s, 1.3 * s, 1.3 * s, c(accent, 1.0))?;
            }
            PetBody::Ghost => {}
        }
    }
    if design.body == PetBody::Ghost {
        p.fill_round_rect(((cx - width) * s, cy * s, (cx + width) * s, (cy + 9.0) * s), 3.5 * s, c(fur, 1.0))?;
        for offset in [-0.65, 0.0, 0.65] {
            p.fill_ellipse(
                (cx + width * offset) * s,
                (cy + 9.0 + breathe * offset * 0.45) * s,
                width * 0.35 * s,
                3.2 * s,
                c(fur, 1.0),
            )?;
        }
    } else {
        let (torso, chest, foot, arm) = match design.body {
            PetBody::Cat => (0.56, 0.29, 2.3, 1.5),
            PetBody::Fox => (0.62, 0.34, 2.4, 1.6),
            PetBody::Rabbit => (0.82, 0.56, 3.7, 1.7),
            PetBody::Robot => (0.62, 0.42, 2.6, 1.7),
            _ => (0.78, 0.48, 3.0, 2.1),
        };
        if design.body == PetBody::Robot {
            p.fill_round_gradient(
                ((cx - width * torso) * s, (cy + 3.0) * s, (cx + width * torso) * s, (cy + 11.5) * s),
                1.5 * s,
                c(fur, 1.0),
                c(mix(fur, DARK, 0.25), 1.0),
            )?;
            p.fill_round_rect(
                ((cx - width * chest) * s, (cy + 6.3) * s, (cx + width * chest) * s, (cy + 9.6) * s),
                0.8 * s,
                c(belly, 1.0),
            )?;
            p.fill_ellipse(cx * s, (cy + 7.9) * s, 0.85 * s, 0.85 * s, c(accent, 1.0))?;
        } else {
            lit(p, s, (cx, cy + 7.3, width * torso, 6.8 + breathe * 0.12), fur)?;
            lit(p, s, (cx, cy + 7.5, width * chest, if design.body == PetBody::Fox { 5.4 } else { 4.5 }), belly)?;
        }
        for sign in [-1.0, 1.0] {
            let step = pose.motion.behavior.stride.sin() * pose.motion.behavior.walk * sign;
            let foot_x = cx + sign * width * (torso * 0.74 + pose.motion.behavior.sit * 0.06) + step;
            let foot_y = cy + 12.0 + step * 1.5;
            lit(p, s, (foot_x, foot_y, foot, if design.body == PetBody::Rabbit { 2.2 } else { 1.9 }), fur)?;
            p.fill_ellipse(foot_x * s, (foot_y + 0.3) * s, 1.45 * s, 0.7 * s, c(belly, 0.30))?;
            let paw = if pose.motion.behavior.walk > 0.0 {
                -step * 1.1
            } else if pose.motion.warmth > 0.2 {
                (pose.t * 5.0).sin() * pose.motion.warmth
            } else {
                breathe * 0.15
            };
            p.fill_ellipse(
                (cx + sign * width * (torso + 0.06)) * s,
                (cy + 6.0 + paw) * s,
                arm * s,
                3.1 * s,
                c(fur, 1.0),
            )?;
        }
    }
    if design.body == PetBody::Robot {
        p.fill_round_gradient(
            ((cx - width) * s, (cy - 6.6) * s, (cx + width) * s, (cy + 5.6) * s),
            3.6 * s,
            c(mix(fur, WHITE, 0.25), 1.0),
            c(mix(fur, DARK, 0.3), 1.0),
        )?;
    } else if design.body == PetBody::Fox {
        shape(
            p,
            s,
            (cx, cy - 7.5),
            &[
                [(cx + width * 0.7, cy - 8.0), (cx + width * 1.05, cy - 2.0), (cx + width, cy)],
                [(cx + width * 0.8, cy + 3.5), (cx + 3.0, cy + 7.2), (cx, cy + 7.2)],
                [(cx - 3.0, cy + 7.2), (cx - width * 0.8, cy + 3.5), (cx - width, cy)],
                [(cx - width * 1.05, cy - 2.0), (cx - width * 0.7, cy - 8.0), (cx, cy - 7.5)],
            ],
            fur,
            1.0,
        )?;
    } else {
        let (rx, ry) = match design.body {
            PetBody::Cat => (width * 1.04, 7.2),
            PetBody::Rabbit => (width * 0.88, 8.6),
            _ => (width, 7.9),
        };
        lit(p, s, (cx, cy, rx, ry), fur)?;
    }
    match design.pattern {
        PetPattern::Patches => {
            p.fill_ellipse((cx - width * 0.45) * s, (cy - 2.1) * s, 3.2 * s, 4.2 * s, c(belly, 0.75))?
        }
        PetPattern::Spots => {
            for (x, y) in [(-6.5, -3.5), (5.9, -4.0), (7.0, 1.4)] {
                p.fill_ellipse((cx + x) * s, (cy + y) * s, 1.5 * s, 1.2 * s, c(belly, 0.8))?;
            }
        }
        PetPattern::Stripes => {
            for x in [-3.2, 0.0, 3.2] {
                p.line(((cx + x) * s, (cy - 6.4) * s), ((cx + x * 0.65) * s, (cy - 3.5) * s), 1.4 * s, c(belly, 0.85))?;
            }
        }
        PetPattern::Solid => {}
    }
    if design.body == PetBody::Fox {
        for sign in [-1.0, 1.0] {
            shape(
                p,
                s,
                (cx, cy + 4.8),
                &[
                    [
                        (cx + sign * width * 0.4, cy + 0.1),
                        (cx + sign * width * 0.82, cy - 0.9),
                        (cx + sign * width * 0.92, cy - 2.7),
                    ],
                    [(cx + sign * width, cy + 1.6), (cx + sign * width * 0.6, cy + 6.8), (cx, cy + 4.8)],
                ],
                belly,
                0.95,
            )?;
        }
    } else if !matches!(design.body, PetBody::Ghost | PetBody::Robot) {
        let muzzle = if design.body == PetBody::Bear { (4.4, 2.85) } else { (3.1, 1.8) };
        lit(p, s, (cx, cy + 3.5, muzzle.0, muzzle.1), belly)?;
        if design.body == PetBody::Cat {
            for sign in [-1.0, 1.0] {
                for y in [1.5, 3.1] {
                    p.line(
                        ((cx + sign * 5.0) * s, (cy + y) * s),
                        ((cx + sign * 9.0) * s, (cy + y - 0.6) * s),
                        0.35 * s,
                        c(eyes, 0.45),
                    )?;
                }
            }
        }
    }
    eye(p, s, pose, cx - 3.9, cy - 0.65, 1.35, 1.9, eyes)?;
    eye(p, s, pose, cx + 3.9, cy - 0.65, 1.35, 1.9, eyes)?;
    if design.body == PetBody::Ghost {
        p.fill_ellipse(
            cx * s,
            (cy + 4.0) * s,
            1.3 * s,
            (1.2 + pose.motion.warmth * 0.4 + pose.motion.mouth) * s,
            c(eyes, 0.9),
        )?;
    } else if design.body == PetBody::Robot {
        p.fill_round_rect(
            ((cx - 2.0) * s, (cy + 3.0 - pose.motion.mouth) * s, (cx + 2.0) * s, (cy + 3.8 + pose.motion.mouth) * s),
            0.4 * s,
            c(accent, 1.0),
        )?;
    } else {
        lit(p, s, (cx, cy + 2.65, 1.45, 0.9), eyes)?;
        p.fill_ellipse((cx - 0.35) * s, (cy + 2.3) * s, 0.45 * s, 0.18 * s, c(WHITE, 0.24))?;
        smile(p, s, pose, cx, cy + 4.05, 1.8, eyes)?;
    }
    for sign in [-1.0, 1.0] {
        p.fill_ellipse(
            (cx + sign * 6.1) * s,
            (cy + 2.3) * s,
            1.8 * s,
            0.9 * s,
            c(blush, if design.body == PetBody::Robot { 0.0 } else { 0.18 + pose.motion.warmth * 0.22 }),
        )?;
    }
    match design.accessory {
        PetAccessory::Scarf => {
            p.fill_round_gradient(
                ((cx - 6.5) * s, (cy + 5.8) * s, (cx + 6.5) * s, (cy + 8.0) * s),
                s,
                c(mix(accent, WHITE, 0.18), 1.0),
                c(mix(accent, DARK, 0.20), 1.0),
            )?;
            p.fill_round_gradient(
                ((cx + 2.5) * s, (cy + 7.0) * s, (cx + 5.0) * s, (cy + 11.7) * s),
                s,
                c(accent, 1.0),
                c(mix(accent, DARK, 0.2), 1.0),
            )?;
            p.line(((cx + 3.2) * s, (cy + 8.5) * s), ((cx + 3.2) * s, (cy + 10.4) * s), 0.35 * s, c(WHITE, 0.22))?;
        }
        PetAccessory::Bow => {
            for sign in [-1.0, 1.0] {
                lit(p, s, (cx + sign * 2.1, cy + 7.0, 2.1, 1.65), accent)?;
            }
            lit(p, s, (cx, cy + 7.0, 0.9, 1.1), mix(accent, DARK, 0.1))?;
        }
        PetAccessory::Star => {
            let points: [(f32, f32); 10] = std::array::from_fn(|i| {
                let angle = i as f32 * std::f32::consts::PI / 5.0 - std::f32::consts::FRAC_PI_2;
                let radius = if i % 2 == 0 { 2.8 } else { 1.3 };
                (cx + angle.cos() * radius, cy + 9.0 + angle.sin() * radius)
            });
            let edges: [[(f32, f32); 3]; 10] =
                std::array::from_fn(|i| [points[i], points[(i + 1) % 10], points[(i + 1) % 10]]);
            shape(p, s, points[0], &edges, accent, 1.0)?;
            p.fill_ellipse((cx - 0.5) * s, (cy + 8.3) * s, 0.45 * s, 0.35 * s, c(WHITE, 0.38))?;
        }
        PetAccessory::None => {}
    }
    zzz(p, s, pose)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepping_wraps_in_both_directions() {
        assert_eq!(Character::None.step(false), Character::Custom);
        assert_eq!(Character::Custom.step(true), Character::None);
        assert_eq!(Character::Pixel.step(true), Character::Spark);
        assert_eq!(Character::Pixel.step(false), Character::None);
    }

    #[test]
    fn every_character_has_a_name_and_round_trips_through_json() {
        for ch in Character::ALL {
            assert!(!ch.name().is_empty());
            let json = serde_json::to_string(&ch).unwrap();
            assert_eq!(serde_json::from_str::<Character>(&json).unwrap(), ch);
        }
        assert_eq!(serde_json::to_string(&Character::Spark).unwrap(), "\"spark\"");
    }
}

#[cfg(test)]
#[path = "character_render_tests.rs"]
mod render_tests;
