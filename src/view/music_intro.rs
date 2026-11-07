use crate::data::settings::Edge;
use crate::view::*;

pub struct MusicEntranceLayout {
    pub cover: Rect,
    pub title: Rect,
    pub final_cover: Option<Rect>,
    pub final_title: Option<Rect>,
    pub rotation: f32,
}

pub fn music_entrance_width(ui: &UiState) -> f32 {
    let reserve = if ui.cfg.pet_music_curiosity && ui.char_inset > 0.0 { ui.char_inset + 10.0 } else { 0.0 };
    (reserve + 28.0 + 10.0 + music_entrance_text_width(ui) + 28.0).max(176.0)
}

fn music_entrance_text_width(ui: &UiState) -> f32 {
    (crate::ui::fonts::width(ui.cfg.interface_font, &ui.media.title, 15.0, DWRITE_FONT_WEIGHT_SEMI_BOLD).ceil() + 2.0)
        .clamp(48.0, 220.0)
}

pub fn music_entrance_cover_at(pill: Rect, scale: f32, ui: &UiState, progress: f32) -> Rect {
    let target = music_entrance_layout(pill, scale, ui).cover;
    let side = 28.0 * scale;
    let start = (pill.0 + (pill.2 - side) / 2.0, pill.1 + (pill.3 - side) / 2.0);
    let blend =
        crate::ui::anim::ease(crate::data::settings::AnimCurve::Smooth, ((progress - 0.24) / 0.76).clamp(0.0, 1.0));
    let x = start.0 + (target.0 - start.0) * blend;
    let y = start.1 + (target.1 - start.1) * blend;
    (x, y, x + side, y + side)
}

pub fn music_entrance_layout(pill: Rect, scale: f32, ui: &UiState) -> MusicEntranceLayout {
    let (x, y, w, h) = pill;
    let text = music_entrance_text_width(ui) * scale;
    let reserve = if ui.cfg.pet_music_curiosity && ui.char_inset > 0.0 { ui.char_inset + 10.0 } else { 0.0 } * scale;
    let cover = 28.0 * scale;
    let gap = 10.0 * scale;
    if ui.cfg.vertical() {
        let top = y + reserve + (h - reserve - cover - gap - text) / 2.0;
        let cx = x + w / 2.0;
        let slots = vertical_slots(ui, scale, h);
        let side = (w - 14.0 * scale).min(30.0 * scale);
        return MusicEntranceLayout {
            cover: (cx - cover / 2.0, top, cx + cover / 2.0, top + cover),
            title: (cx - 17.0 * scale, top + cover + gap, cx + 17.0 * scale, top + cover + gap + text),
            final_cover: slots
                .cover
                .filter(|_| ui.media.cover.is_some())
                .map(|s| (cx - side / 2.0, y + s.0, cx + side / 2.0, y + s.0 + side)),
            final_title: slots.title.map(|s| (x, y + s.0, x + w, y + s.0 + s.1)),
            rotation: if ui.cfg.dock_edge == Edge::Left { -90.0 } else { 90.0 },
        };
    }
    let left = x + reserve + (w - reserve - cover - gap - text) / 2.0;
    let slots = music_slots(ui, scale, w);

    MusicEntranceLayout {
        cover: (left, y + (h - cover) / 2.0, left + cover, y + (h + cover) / 2.0),
        title: (left + cover + gap, y, left + cover + gap + text, y + h),
        final_cover: pill_cover_rect(pill, scale, ui),
        final_title: slots.title.map(|s| (x + s.0, y, x + s.0 + s.1, y + h)),
        rotation: 0.0,
    }
}

pub fn paint_music_entrance_cover(p: &Painter, scale: f32, ui: &UiState) -> Result<()> {
    let size = 28.0 * scale;
    let rect = (0.0, 0.0, size, size);
    if let Some(cover) = &ui.media.cover {
        p.cover(rect, 7.0 * scale, cover, crate::data::settings::CoverMode::Crop)
    } else {
        p.fill_round_rect(rect, 7.0 * scale, acc(0.16))?;
        p.glyph('\u{E8D6}', rect, 17.0 * scale, acc(1.0))
    }
}

pub fn paint_music_entrance_title(p: &Painter, scale: f32, ui: &UiState) -> Result<()> {
    p.text(
        &ui.media.title,
        (0.0, 0.0, music_entrance_text_width(ui) * scale, 34.0 * scale),
        15.0 * scale,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        WHITE,
    )
}

#[cfg(test)]
pub fn render_music_entrance_preview(gpu: &crate::ui::gfx::Gpu, ui: &UiState, path: &std::path::Path) -> Result<()> {
    let surface = crate::ui::popup::PopupSurface::new((560, 320))?;
    surface.paint(gpu, |p| {
        p.fill_rect((0.0, 0.0, 560.0, 320.0), rgba(0.025, 0.035, 0.06, 1.0))?;
        for (row, progress) in [0.24, 0.65, 1.0, 2.0].into_iter().enumerate() {
            let y = 30.0 + row as f32 * 78.0;
            let width = if progress > 1.0 { collapsed_width(ui, 200.0) } else { music_entrance_width(ui) };
            let pill = ((560.0 - width) / 2.0, y, width, 34.0);
            p.fill_round_rect((pill.0, y, pill.0 + width, y + 34.0), 17.0, rgba(0.0, 0.0, 0.0, 1.0))?;
            p.text(
                ["Обложка в центре", "Название раскрывается", "Трек представлен", "Обычный остров"][row],
                (20.0, y - 23.0, 540.0, y - 4.0),
                10.0,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_CENTER,
                rgba(1.0, 1.0, 1.0, 0.6),
            )?;
            if progress > 1.0 {
                paint_collapsed(p, pill, 1.0, ui)?;
                if ui.track_in_flight {
                    let layout = music_entrance_layout(pill, 1.0, ui);
                    if let (Some(rect), Some(cover)) = (layout.final_cover, &ui.media.cover) {
                        p.cover(rect, 6.0, cover, ui.cfg.cover_mode)?;
                    }
                    if let Some(rect) = layout.final_title {
                        p.text(
                            &ui.media.title,
                            rect,
                            if ui.cfg.pill_unified_layout { 13.0 } else { 12.5 },
                            DWRITE_FONT_WEIGHT_SEMI_BOLD,
                            DWRITE_TEXT_ALIGNMENT_LEADING,
                            WHITE,
                        )?;
                    }
                }
            } else {
                let layout = music_entrance_layout(pill, 1.0, ui);
                let cover = music_entrance_cover_at(pill, 1.0, ui, progress);
                let old = p.push_offset(cover.0, cover.1);
                paint_music_entrance_cover(p, 1.0, ui)?;
                p.set_transform(&old);
                let reveal = ((progress - 0.24) / 0.76).clamp(0.0, 1.0);
                let x = layout.title.0 + cover.0 - layout.cover.0;
                let length = (layout.title.2 - layout.title.0) * reveal;
                if length > 0.0 {
                    p.push_clip((x, y, x + length, y + 34.0));
                    p.text(
                        &ui.media.title,
                        (x, y, x + music_entrance_text_width(ui), y + 34.0),
                        15.0,
                        DWRITE_FONT_WEIGHT_SEMI_BOLD,
                        DWRITE_TEXT_ALIGNMENT_LEADING,
                        rgba(1.0, 1.0, 1.0, reveal),
                    )?;
                    p.pop_clip();
                }
            }
            if ui.char_inset > 0.0 {
                let old = p.push_offset(pill.0 + 6.0, y + 1.0);
                crate::ui::characters::draw_attached(
                    p,
                    crate::ui::characters::Character::Spark,
                    40.0,
                    &crate::ui::characters::Pose {
                        t: 0.0,
                        reaction: crate::ui::characters::Reaction::Idle,
                        look: if progress > 1.0 { (0.0, 0.0) } else { (0.7, 0.0) },
                        blink: false,
                        accent: (0.6, 0.7, 1.0),
                        beat: 0.0,
                        motion: Default::default(),
                    },
                    None,
                )?;
                p.set_transform(&old);
            }
        }
        Ok(())
    })?;
    let rgba: Vec<u8> = surface.pixels().chunks_exact(4).flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
    let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), 560, 320);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(&rgba).unwrap();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_entrance_begins_at_the_center_then_makes_room_for_the_title() {
        let mut ui = UiState::new();
        ui.char_inset = 30.0;
        ui.media.title = "sexyswag2010".into();
        assert!(music_entrance_width(&ui) < 260.0);
        for scale in [0.75, 1.0, 1.5, 2.0] {
            let pill = (20.0, 16.0, music_entrance_width(&ui) * scale, 34.0 * scale);
            let start = music_entrance_cover_at(pill, scale, &ui, 0.0);
            assert!(((start.0 + start.2) / 2.0 - pill.0 - pill.2 / 2.0).abs() < 0.01);
            assert_eq!(start, music_entrance_cover_at(pill, scale, &ui, 0.2));
            let end = music_entrance_cover_at(pill, scale, &ui, 1.0);
            assert!(end.0 < start.0);
            let title = music_entrance_layout(pill, scale, &ui).title;
            assert!(title.0 >= end.2 && title.2 < pill.0 + pill.2);
        }
    }

    #[test]
    fn music_presentations_fit_horizontal_and_vertical_islands_at_every_scale() {
        for scale in [0.75, 1.0, 1.25, 2.0] {
            for edge in Edge::ALL {
                let mut ui = UiState::new();
                ui.cfg.placement = crate::data::settings::Placement::Docked;
                ui.cfg.dock_edge = edge;
                ui.media.playing = true;
                ui.media.title = "A very long first song title which must fit the capsule".into();
                let pill = if ui.cfg.vertical() {
                    (12.0, 12.0, VERT_W * scale, 600.0 * scale)
                } else {
                    (12.0, 12.0, 600.0 * scale, 34.0 * scale)
                };
                let layout = music_entrance_layout(pill, scale, &ui);
                for rect in [Some(layout.cover), Some(layout.title), layout.final_cover, layout.final_title]
                    .into_iter()
                    .flatten()
                {
                    assert!(rect.0 >= pill.0 - 0.01 && rect.1 >= pill.1 - 0.01);
                    assert!(rect.2 <= pill.0 + pill.2 + 0.01 && rect.3 <= pill.1 + pill.3 + 0.01);
                }
            }
        }
    }
}
