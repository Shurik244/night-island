use crate::view::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveKind {
    Reminder(i64),
    Recording,
    Download,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveActivity {
    pub kind: LiveKind,
    pub title: String,
    pub seconds: Option<u64>,
}

impl LiveActivity {
    pub fn glyph(&self) -> char {
        match self.kind {
            LiveKind::Reminder(_) => '\u{E916}',
            LiveKind::Recording => '\u{E714}',
            LiveKind::Download => '\u{E896}',
        }
    }

    pub fn value(&self) -> String {
        self.seconds.map_or_else(
            || crate::ui::i18n::tr("Downloading").into_owned(),
            |seconds| {
                if seconds >= 3600 {
                    format!("{}:{:02}:{:02}", seconds / 3600, seconds / 60 % 60, seconds % 60)
                } else {
                    format!("{:02}:{:02}", seconds / 60, seconds % 60)
                }
            },
        )
    }

    pub fn width(&self) -> f32 {
        if self.seconds.is_none() {
            112.0
        } else if self.seconds.unwrap_or(0) >= 3600 {
            106.0
        } else {
            88.0
        }
    }
}

pub fn live_tile_count(ui: &UiState) -> usize {
    if ui.home.edit || ui.home.widgets.editing {
        0
    } else {
        ui.live.len().min(4).min(MAX_HOME_TILES.saturating_sub(notification_tile_count(ui)))
    }
}

pub fn legacy_download(ui: &UiState) -> Option<&str> {
    ui.downloading.as_deref().filter(|_| {
        !ui.cfg.live_activities || !ui.cfg.live_downloads || !ui.live.iter().any(|v| v.kind == LiveKind::Download)
    })
}

pub fn paint_live_chip(p: &Painter, rect: Rect, scale: f32, ui: &UiState) -> Result<()> {
    let Some(activity) = ui.live.first() else { return Ok(()) };
    let cy = (rect.1 + rect.3) / 2.0;
    let r = (rect.0, cy - 12.0 * scale, rect.2, cy + 12.0 * scale);
    let colour = if activity.kind == LiveKind::Recording { rgba(1.0, 0.35, 0.35, 1.0) } else { acc(1.0) };
    p.fill_round_rect(r, 9.0 * scale, rgba(colour.r, colour.g, colour.b, 0.14))?;
    p.glyph(activity.glyph(), (r.0 + 3.0 * scale, r.1, r.0 + 24.0 * scale, r.3), 13.0 * scale, colour)?;
    let extra = if ui.live.len() > 1 { 22.0 * scale } else { 0.0 };
    if extra > 0.0 {
        p.text(
            &format!("+{}", ui.live.len() - 1),
            (r.2 - extra, r.1, r.2 - 3.0 * scale, r.3),
            10.0 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            colour,
        )?;
    }
    p.text(
        &activity.value(),
        (r.0 + 25.0 * scale, r.1, r.2 - 5.0 * scale - extra, r.3),
        12.0 * scale,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_CENTER,
        WHITE,
    )
}

pub fn paint_live_tiles(p: &Painter, pill: Rect, scale: f32, ui: &UiState) -> Result<()> {
    let offset = notification_tile_count(ui);
    let alpha = ui.live_changed.map_or(1.0, |at| (at.elapsed().as_secs_f32() / 0.28).clamp(0.0, 1.0));
    for (index, activity) in ui.live.iter().take(live_tile_count(ui)).enumerate() {
        let r = action_tile(pill, scale, offset + index);
        p.fill_round_rect(r, 16.0 * scale, acc(0.14 * alpha))?;
        p.glyph(activity.glyph(), (r.0 + 5.0 * scale, r.1, r.0 + 35.0 * scale, r.3), 16.0 * scale, acc(alpha))?;
        p.text(
            &activity.title,
            (r.0 + 37.0 * scale, r.1 + 2.0 * scale, r.2 - 5.0 * scale, r.1 + 21.0 * scale),
            10.5 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, 0.65 * alpha),
        )?;
        p.text(
            &activity.value(),
            (r.0 + 37.0 * scale, r.1 + 20.0 * scale, r.2 - 5.0 * scale, r.3 - 2.0 * scale),
            13.0 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            rgba(1.0, 1.0, 1.0, alpha),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countdown_width_is_stable_and_activity_does_not_overlap_other_content() {
        let mut ui = UiState::new();
        ui.media.playing = true;
        ui.media.title = "A new track".into();
        ui.live.push(LiveActivity { kind: LiveKind::Reminder(1), title: "Tea".into(), seconds: Some(599) });
        for scale in [0.75, 1.0, 1.25, 1.5, 2.0] {
            let width = collapsed_width(&ui, 200.0);
            let slots = music_slots(&ui, scale, width * scale);
            let activity = slots.live.unwrap();
            assert!(activity.0 + activity.1 <= slots.clock.0 + 0.01);
            if let Some(title) = slots.title {
                assert!(title.0 + title.1 <= activity.0);
            }
            let vertical = vertical_slots(&ui, scale, 0.0);
            let activity = vertical.live.unwrap();
            assert!(activity.0 + activity.1 <= vertical.clock.0 + 0.01);
            ui.live[0].seconds = Some(1);
            assert_eq!(collapsed_width(&ui, 200.0), width);
        }
    }

    #[test]
    fn activity_tiles_keep_notifications_and_editing_available() {
        let mut ui = UiState::new();
        ui.live = (0..4)
            .map(|id| LiveActivity { kind: LiveKind::Reminder(id), title: "Timer".into(), seconds: Some(300) })
            .collect();
        let pill = (0.0, 0.0, 640.0, 380.0);
        for index in 0..live_tile_count(&ui) {
            let r = action_tile(pill, 1.0, index);
            assert!(
                matches!(hit_home(pill, 1.0, (r.0+r.2)/2.0, (r.1+r.3)/2.0, &ui), Some(HomeHit::Live(i)) if i == index)
            );
        }
        ui.home.edit = true;
        assert_eq!(live_tile_count(&ui), 0);
        ui.downloading = Some("archive.zip".into());
        ui.live.push(LiveActivity { kind: LiveKind::Download, title: "archive.zip".into(), seconds: None });
        assert!(legacy_download(&ui).is_none());
        ui.cfg.live_activities = false;
        assert_eq!(legacy_download(&ui), Some("archive.zip"));
    }
}
