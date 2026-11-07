use crate::services::attach::Attachment;
use crate::ui::pet_design::PetDesign;
use crate::view::*;

#[derive(Clone, Default)]
pub struct PetStudio {
    pub enabled: bool,
    pub reference: Option<std::sync::Arc<crate::platform::apps::Icon>>,
    pub performance: crate::ui::pet_motion::PetPerformance,
    pub preview_started: Option<std::time::Instant>,
    pub text_only: bool,
    pub pending: bool,
    pub draft: Option<PetDesign>,
    pub source: PetDesign,
    pub files: Vec<Attachment>,
    pub error: Option<String>,
    pub applied: bool,
    pub last_prompt: String,
    pub from_companion: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PetStudioHit {
    Body,
    Apply,
    Photo,
    TextOnly,
}

impl PetStudio {
    pub fn preview_interval(&self) -> Option<u32> {
        (self.enabled
            && self.performance != crate::ui::pet_motion::PetPerformance::Still
            && crate::ui::feedback::motion_allowed())
        .then(|| self.performance.interval(false, false, false))
    }
}

pub fn pet_studio_buttons(area: Rect, scale: f32) -> [Rect; 3] {
    let gap = 8.0 * scale;
    let total = area.2 - area.0 - 2.0 * gap;
    let widths = [total * 0.40, total * 0.30, total * 0.30];
    let mut x = area.0;
    widths.map(|width| {
        let rect = (x, area.3 - 36.0 * scale, x + width, area.3);
        x += width + gap;
        rect
    })
}

pub fn hit_pet_studio(area: Rect, scale: f32, point: (f32, f32), studio: &PetStudio) -> Option<PetStudioHit> {
    if studio.pending {
        return None;
    }
    if inside(pet_body_button(area, scale), point.0, point.1) {
        return Some(PetStudioHit::Body);
    }
    pet_studio_buttons(area, scale)
        .into_iter()
        .zip([PetStudioHit::Apply, PetStudioHit::Photo, PetStudioHit::TextOnly])
        .find_map(|(rect, action)| {
            (inside(rect, point.0, point.1)
                && (action != PetStudioHit::Apply || (studio.draft.is_some() && !studio.applied)))
                .then_some(action)
        })
}

fn pet_body_button(area: Rect, scale: f32) -> Rect {
    (area.0 + 8.0 * scale, area.3 - 72.0 * scale, area.0 + 126.0 * scale, area.3 - 46.0 * scale)
}

pub fn paint_pet_studio(
    p: &Painter,
    area: Rect,
    scale: f32,
    studio: &PetStudio,
    hover: Option<PetStudioHit>,
    focus: Option<PetStudioHit>,
) -> Result<()> {
    let (l, t, r, b) = area;
    let card = (l, t, r, b - 44.0 * scale);
    let height = (card.3 - t - 28.0 * scale).max(1.0);
    let size = (height - 8.0 * scale).min(162.0 * scale).min((r - l) * 0.43).max(36.0 * scale);
    let top = t + (height - size) / 2.0;
    let design = studio.draft.as_ref().unwrap_or(&studio.source);
    let tint = design.fur.map(|v| f32::from(v) / 255.0);
    p.fill_round_gradient(card, 18.0 * scale, rgba(tint[0], tint[1], tint[2], 0.16), rgba(1.0, 1.0, 1.0, 0.035))?;
    p.stroke_round_rect(card, 18.0 * scale, scale, rgba(1.0, 1.0, 1.0, 0.065))?;
    if let Some(error) = &studio.error {
        p.text(
            "Needs attention",
            (l + 16.0 * scale, t + 12.0 * scale, r - 16.0 * scale, t + 32.0 * scale),
            13.0 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 0.69, 0.61, 1.0),
        )?;
        p.text_wrapped_trimmed(
            &crate::ui::i18n::tr(error),
            (l + 16.0 * scale, t + 38.0 * scale, r - 16.0 * scale, card.3 - 36.0 * scale),
            11.5 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            rgba(1.0, 0.85, 0.79, 0.93),
        )?;
    } else {
        let portrait_x = l + 6.0 * scale;
        let center = portrait_x + size / 2.0;
        p.fill_ellipse_gradient(
            (center, top + size * 0.48, size * 0.47, size * 0.42),
            rgba(tint[0], tint[1], tint[2], 0.16),
            rgba(tint[0], tint[1], tint[2], 0.0),
        )?;
        p.fill_ellipse_gradient(
            (center, top + size * 0.86, size * 0.27, size * 0.04),
            rgba(0.0, 0.0, 0.0, 0.33),
            rgba(0.0, 0.0, 0.0, 0.0),
        )?;
        let time = if studio.preview_interval().is_some() {
            studio.preview_started.map_or(0.8, |start| start.elapsed().as_secs_f32())
        } else {
            0.8
        };
        let blinking = time % 5.4;
        let lid = if blinking < 0.18 && studio.preview_interval().is_some() {
            crate::ui::pet_motion::eyelid(0.18 - blinking)
        } else {
            0.0
        };
        let pose = crate::ui::characters::Pose {
            t: time,
            reaction: crate::ui::characters::Reaction::Idle,
            look: ((time * 0.48).sin() * 0.26, (time * 0.3).cos() * 0.06),
            blink: lid > 0.94,
            accent: crate::ui::theme::accent(),
            beat: 0.0,
            motion: crate::ui::pet_motion::Motion {
                lid,
                still: studio.performance == crate::ui::pet_motion::PetPerformance::Still,
                ..Default::default()
            },
        };
        let old = p.push_offset(portrait_x, top + size * 0.12);
        let drawn = crate::ui::characters::draw_with_design(
            p,
            crate::ui::characters::Character::Custom,
            size,
            &pose,
            Some(design),
        );
        p.set_transform(&old);
        drawn?;
        if let Some(reference) = &studio.reference {
            let photo = (l + 134.0 * scale, card.3 - 29.0 * scale, l + 160.0 * scale, card.3 - 3.0 * scale);
            p.image(photo, 8.0 * scale, reference.side, &reference.bgra)?;
            p.stroke_round_rect(photo, 8.0 * scale, scale, rgba(1.0, 1.0, 1.0, 0.3))?;
            if studio.text_only {
                p.fill_round_rect(photo, 8.0 * scale, rgba(0.0, 0.0, 0.0, 0.58))?;
            }
        }
        let x = portrait_x + size + 8.0 * scale;
        let right = r - 14.0 * scale;
        let spacious = height >= 150.0 * scale;
        let y = t + if spacious { 16.0 * scale } else { 10.0 * scale };
        let status = if studio.pending {
            "Creating…"
        } else if studio.applied {
            "With you"
        } else if studio.draft.is_some() {
            "Preview"
        } else {
            "Pet studio"
        };
        let color = if studio.applied { rgba(0.65, 0.87, 0.76, 1.0) } else { rgba(1.0, 1.0, 1.0, 0.59) };
        let status_rect = (x, y, right, y + 15.0 * scale);
        p.fill_ellipse(x + 2.5 * scale, y + 7.5 * scale, 2.5 * scale, 2.5 * scale, color)?;
        p.text(
            status,
            (x + 10.0 * scale, status_rect.1, right, status_rect.3),
            10.5 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            color,
        )?;
        let title = studio.draft.as_ref().map_or("Create your pet", |pet| pet.name.as_str());
        p.text(
            title,
            (x, y + 20.0 * scale, right, y + 44.0 * scale),
            if spacious { 19.0 } else { 16.0 } * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            WHITE,
        )?;
        let hint = if studio.pending {
            "Your pet is taking shape. Your current companion stays with you."
        } else if studio.applied {
            "Your pet is ready. Describe another change below."
        } else if studio.draft.is_some() {
            "Preview ready. Apply your pet or describe another change below."
        } else if studio.text_only {
            "Describe the shape, colors and details. Photos will not be sent."
        } else if studio.files.is_empty() {
            "Describe a look below or add a photo for a vision model."
        } else {
            "Photo attached. Tell us what to keep or change."
        };
        let hint = if spacious {
            hint
        } else if studio.pending {
            "Shaping your pet…"
        } else if studio.applied {
            "Describe another change below."
        } else if studio.draft.is_some() {
            "Apply your pet or describe a change."
        } else if studio.text_only {
            "Describe your pet below."
        } else {
            "Describe your pet or add a photo."
        };
        let text_bottom = if spacious { card.3 - 40.0 * scale } else { card.3 - 8.0 * scale };
        p.text_wrapped_trimmed(
            &crate::ui::i18n::tr(hint),
            (x, y + 49.0 * scale, right, text_bottom),
            11.5 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            rgba(1.0, 1.0, 1.0, 0.66),
        )?;
        if spacious {
            let palette_y = card.3 - 21.0 * scale;
            for (i, color) in [design.fur, design.belly, design.accent].into_iter().enumerate() {
                let sx = x + (6.0 + i as f32 * 17.0) * scale;
                p.fill_ellipse(sx, palette_y, 6.5 * scale, 6.5 * scale, rgba(1.0, 1.0, 1.0, 0.15))?;
                p.fill_ellipse(
                    sx,
                    palette_y,
                    5.3 * scale,
                    5.3 * scale,
                    rgba(color[0] as f32 / 255.0, color[1] as f32 / 255.0, color[2] as f32 / 255.0, 1.0),
                )?;
            }
            if right - x > 140.0 * scale {
                p.text(
                    "Palette",
                    (x + 58.0 * scale, palette_y - 9.0 * scale, right, palette_y + 9.0 * scale),
                    10.5 * scale,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                    rgba(1.0, 1.0, 1.0, 0.43),
                )?;
            }
        }
    }
    let body_button = pet_body_button(area, scale);
    let hot = hover == Some(PetStudioHit::Body) || focus == Some(PetStudioHit::Body);
    p.fill_round_rect(body_button, 9.0 * scale, rgba(1.0, 1.0, 1.0, if hot { 0.14 } else { 0.07 }))?;
    p.text(
        &format!("{}: {} ›", crate::ui::i18n::tr("Body"), crate::ui::i18n::tr(design.body.name())),
        (body_button.0 + 8.0 * scale, body_button.1, body_button.2 - 4.0 * scale, body_button.3),
        10.5 * scale,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        rgba(1.0, 1.0, 1.0, if studio.pending { 0.35 } else { 0.85 }),
    )?;
    for (i, (rect, action)) in pet_studio_buttons(area, scale)
        .into_iter()
        .zip([PetStudioHit::Apply, PetStudioHit::Photo, PetStudioHit::TextOnly])
        .enumerate()
    {
        let enabled = !studio.pending && (i != 0 || (studio.draft.is_some() && !studio.applied));
        let selected = (i == 1 && !studio.files.is_empty() && !studio.text_only) || (i == 2 && studio.text_only);
        let hot = hover == Some(action) && enabled;
        let primary = i == 0 && enabled;
        let fill = if primary {
            rgba(0.96, 0.97, 1.0, if hot { 1.0 } else { 0.94 })
        } else {
            rgba(
                1.0,
                1.0,
                1.0,
                if hot {
                    0.14
                } else if selected {
                    0.105
                } else {
                    0.055
                },
            )
        };
        p.fill_round_rect(rect, 11.0 * scale, fill)?;
        p.stroke_round_rect(rect, 11.0 * scale, scale, rgba(1.0, 1.0, 1.0, if selected { 0.25 } else { 0.065 }))?;
        let ink =
            if primary { rgba(0.08, 0.09, 0.12, 1.0) } else { rgba(1.0, 1.0, 1.0, if enabled { 0.9 } else { 0.35 }) };
        let label = match i {
            0 if studio.applied => "Applied",
            0 => "Apply pet",
            1 => "Pet photo",
            _ => "Text only",
        };
        let glyph = if i == 0 || selected {
            '\u{E73E}'
        } else if i == 1 {
            '\u{E91B}'
        } else {
            '\u{E8D2}'
        };
        let icon_width = if selected || i == 0 || rect.2 - rect.0 > 96.0 * scale { 20.0 * scale } else { 0.0 };
        if icon_width > 0.0 {
            let icon_rect = (rect.0 + 8.0 * scale, rect.1, rect.0 + 28.0 * scale, rect.3);
            if i == 2 && !selected {
                p.text("Aa", icon_rect, 11.0 * scale, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER, ink)?;
            } else {
                p.glyph(glyph, icon_rect, 12.0 * scale, ink)?;
            }
        }
        p.text(
            label,
            (rect.0 + icon_width, rect.1, rect.2 - 4.0 * scale, rect.3),
            11.5 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            ink,
        )?;
        if focus == Some(action) {
            p.stroke_round_rect(
                (rect.0 + 2.0 * scale, rect.1 + 2.0 * scale, rect.2 - 2.0 * scale, rect.3 - 2.0 * scale),
                9.0 * scale,
                1.5 * scale,
                if primary { rgba(0.12, 0.14, 0.2, 0.75) } else { rgba(0.85, 0.9, 1.0, 0.9) },
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_character_and_manual_body_choice_do_not_require_an_ai_request() {
        use crate::ui::{characters::Character, pet_design::PetBody};
        for (character, body) in
            [(Character::Cat, PetBody::Cat), (Character::Robot, PetBody::Robot), (Character::Spark, PetBody::Ghost)]
        {
            let studio = PetStudio {
                enabled: true,
                source: PetDesign::for_character(character, &PetDesign::default()),
                ..Default::default()
            };
            assert_eq!(studio.source.body, body);
            let area = (0.0, 0.0, 400.0, 240.0);
            let rect = pet_body_button(area, 1.0);
            assert_eq!(hit_pet_studio(area, 1.0, (rect.0 + 10.0, rect.1 + 10.0), &studio), Some(PetStudioHit::Body));
            assert!(rect.3 < pet_studio_buttons(area, 1.0)[0].1);
        }
        let mut body = PetBody::Bear;
        let mut seen = Vec::new();
        for _ in 0..6 {
            seen.push(body);
            body = body.next();
        }
        assert_eq!(seen, PetBody::ALL);
        assert_eq!(body, PetBody::Bear);
    }

    #[test]
    fn preview_animation_stops_when_closed_or_reduced_and_respects_the_pet_budget() {
        use crate::ui::pet_motion::PetPerformance;
        let mut studio = PetStudio::default();
        assert_eq!(studio.preview_interval(), None);
        studio.enabled = true;
        for quality in PetPerformance::ALL {
            studio.performance = quality;
            if quality == PetPerformance::Still {
                assert_eq!(studio.preview_interval(), None);
            } else {
                assert!(studio.preview_interval().unwrap() >= quality.interval(true, false, true));
            }
        }
    }

    #[test]
    fn generation_never_applies_without_an_explicit_preview_action() {
        let mut studio = PetStudio::default();
        for scale in [0.75, 1.0, 1.5, 2.0] {
            let area = (20.0 * scale, 50.0 * scale, 380.0 * scale, 260.0 * scale);
            let [apply, photo, text] = pet_studio_buttons(area, scale);
            assert!(apply.2 < photo.0 && photo.2 < text.0);
            let point = ((apply.0 + apply.2) / 2.0, (apply.1 + apply.3) / 2.0);
            studio.draft = None;
            assert_eq!(hit_pet_studio(area, scale, point, &studio), None);
            studio.draft = Some(Default::default());
            assert_eq!(hit_pet_studio(area, scale, point, &studio), Some(PetStudioHit::Apply));
            studio.pending = true;
            assert_eq!(hit_pet_studio(area, scale, point, &studio), None);
            studio.pending = false;
        }
    }
}
