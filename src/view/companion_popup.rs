use crate::ui::characters::{self, Character, Pose, Reaction};
use crate::view::*;

pub const COMPANION_CHAT_SIZE: (f32, f32) = (416.0, 460.0);
pub const COMPANION_PLAYER_SIZE: (f32, f32) = (300.0, 140.0);
pub const POPUP_MARGIN: f32 = crate::ui::popup::POPUP_MARGIN;

pub fn paint_popup_shadow(p: &Painter, size: (f32, f32), scale: f32) -> Result<()> {
    let m = POPUP_MARGIN * scale;
    let rect = (m, m, size.0 - m, size.1 - m);
    for i in (1..=8).rev() {
        let grow = i as f32 * scale;
        p.fill_round_rect(
            (rect.0 - grow, rect.1 - grow + 2.0 * scale, rect.2 + grow, rect.3 + grow + 2.0 * scale),
            28.0 * scale + grow,
            rgba(0.0, 0.0, 0.0, 0.025),
        )?;
    }
    Ok(())
}

pub fn paint_popup_panel(p: &Painter, size: (f32, f32), scale: f32) -> Result<()> {
    paint_popup_shadow(p, size, scale)?;
    let m = POPUP_MARGIN * scale;
    let rect = (m, m, size.0 - m, size.1 - m);
    let (r, g, b) = crate::ui::theme::panel();
    p.fill_round_rect(rect, 28.0 * scale, rgba(r, g, b, 0.98))?;
    p.stroke_round_rect(rect, 28.0 * scale, scale, rgba(1.0, 1.0, 1.0, 0.07))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompanionChatHit {
    Pet,
    PetAction(PetStudioHit),
    Close,
    Setup,
    Send,
    Voice,
    Input,
    History,
    Header,
    Resize,
}

pub struct CompanionChatView {
    pub chat: ChatUi,
    pub character: Character,
    pub custom_pet: crate::ui::pet_design::PetDesign,
    pub provider: String,
    pub hover: Option<CompanionChatHit>,
    pub focus: CompanionChatHit,
    pub caret_on: bool,
    pub active: bool,
    pub voice_enabled: bool,
}

impl Default for CompanionChatView {
    fn default() -> Self {
        Self {
            chat: ChatUi::default(),
            character: Character::Spark,
            custom_pet: Default::default(),
            provider: String::new(),
            hover: None,
            focus: CompanionChatHit::Input,
            caret_on: true,
            active: true,
            voice_enabled: true,
        }
    }
}

pub struct CompanionChatLayout {
    pub pet: Rect,
    pub close: Rect,
    pub setup: Rect,
    pub header: Rect,
    pub history: Rect,
    pub composer: Rect,
    pub input: Rect,
    pub send: Rect,
    pub voice: Rect,
    pub resize: Rect,
}

pub fn companion_chat_layout(size: (f32, f32), scale: f32, input_h: f32) -> CompanionChatLayout {
    let (w, h) = size;
    let pad = 28.0 * scale;
    let bottom = h - 40.0 * scale;
    let field_h = input_h.clamp(64.0 * scale, 112.0 * scale);
    let composer = (pad, bottom - field_h, w - pad, bottom);
    let close = (w - pad - 30.0 * scale, 26.0 * scale, w - pad, 56.0 * scale);
    let setup = (close.0 - 36.0 * scale, close.1, close.0 - 6.0 * scale, close.3);
    let send =
        (composer.2 - 43.0 * scale, composer.3 - 43.0 * scale, composer.2 - 9.0 * scale, composer.3 - 9.0 * scale);
    let voice = (send.0 - 39.0 * scale, send.1, send.0 - 5.0 * scale, send.3);
    CompanionChatLayout {
        pet: (setup.0 - 82.0 * scale, setup.1, setup.0 - 6.0 * scale, setup.3),
        close,
        setup,
        header: (pad, 20.0 * scale, w - pad, 76.0 * scale),
        history: (pad, 88.0 * scale, w - pad, composer.1 - 16.0 * scale),
        composer,
        input: (composer.0 + 12.0 * scale, composer.1 + 12.0 * scale, voice.0 - 8.0 * scale, composer.3 - 12.0 * scale),
        send,
        voice,
        resize: (w - 27.0 * scale, h - 27.0 * scale, w - 11.0 * scale, h - 11.0 * scale),
    }
}

pub fn hit_companion_chat(layout: &CompanionChatLayout, x: f32, y: f32) -> Option<CompanionChatHit> {
    [
        (CompanionChatHit::Pet, layout.pet),
        (CompanionChatHit::Close, layout.close),
        (CompanionChatHit::Setup, layout.setup),
        (CompanionChatHit::Send, layout.send),
        (CompanionChatHit::Voice, layout.voice),
        (CompanionChatHit::Input, layout.composer),
        (CompanionChatHit::History, layout.history),
        (CompanionChatHit::Header, layout.header),
        (CompanionChatHit::Resize, layout.resize),
    ]
    .into_iter()
    .find_map(|(hit, rect)| inside(rect, x, y).then_some(hit))
}

pub fn paint_companion_chat(p: &Painter, size: (f32, f32), scale: f32, ui: &CompanionChatView) -> Result<()> {
    paint_popup_panel(p, size, scale)?;
    let mut layout = companion_chat_layout(size, scale, ui.chat.input_h.get());
    if let Ok(text) = crate::ui::gfx::make_layout(
        p.dwrite(),
        &ui.chat.input.text(),
        layout.input.2 - layout.input.0,
        AGY_FONT * scale,
    ) {
        ui.chat.input_h.set(p.layout_height(&text) + 24.0 * scale);
        layout = companion_chat_layout(size, scale, ui.chat.input_h.get());
    }
    let pose = Pose {
        motion: Default::default(),
        t: 0.0,
        reaction: Reaction::Idle,
        look: (0.0, 0.0),
        blink: false,
        accent: crate::ui::theme::accent(),
        beat: 0.0,
    };
    let transform = p.push_offset(26.0 * scale, 25.0 * scale);
    let drawn = characters::draw_with_design(p, ui.character, 40.0 * scale, &pose, Some(&ui.custom_pet));
    p.set_transform(&transform);
    drawn?;
    let title = if ui.chat.pet.enabled {
        crate::ui::i18n::tr("Pet studio").into_owned()
    } else {
        format!(
            "{} · {}",
            if ui.character == Character::Custom {
                ui.custom_pet.name.clone()
            } else {
                crate::ui::i18n::tr(ui.character.name()).into_owned()
            },
            crate::ui::i18n::tr("Chat")
        )
    };
    p.text(
        &title,
        (74.0 * scale, 26.0 * scale, layout.pet.0 - 8.0 * scale, 48.0 * scale),
        14.0 * scale,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        WHITE,
    )?;
    p.text(
        &ui.provider,
        (74.0 * scale, 47.0 * scale, layout.pet.0 - 8.0 * scale, 65.0 * scale),
        11.0 * scale,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        rgba(1.0, 1.0, 1.0, 0.45),
    )?;
    for (hit, rect, glyph) in
        [(CompanionChatHit::Setup, layout.setup, '\u{E713}'), (CompanionChatHit::Close, layout.close, '\u{E711}')]
    {
        if ui.hover == Some(hit) || ui.focus == hit {
            p.fill_round_rect(rect, 15.0 * scale, rgba(1.0, 1.0, 1.0, 0.1))?;
        }
        p.glyph(glyph, rect, 13.0 * scale, rgba(1.0, 1.0, 1.0, 0.65))?;
    }
    p.fill_round_rect(
        layout.pet,
        12.0 * scale,
        if ui.hover == Some(CompanionChatHit::Pet) {
            rgba(1.0, 1.0, 1.0, 0.15)
        } else if ui.chat.pet.enabled {
            acc(0.14)
        } else {
            rgba(1.0, 1.0, 1.0, 0.06)
        },
    )?;
    p.text(
        if ui.chat.pet.enabled { "Back to chat" } else { "Pet studio" },
        layout.pet,
        11.5 * scale,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_CENTER,
        WHITE,
    )?;
    if ui.active && ui.focus == CompanionChatHit::Pet {
        p.stroke_round_rect(layout.pet, 12.0 * scale, 1.3 * scale, rgba(0.86, 0.9, 1.0, 0.9))?;
    }
    if ui.chat.pet.enabled {
        paint_pet_studio(
            p,
            layout.history,
            scale,
            &ui.chat.pet,
            match ui.hover {
                Some(CompanionChatHit::PetAction(action)) => Some(action),
                _ => None,
            },
            match ui.focus {
                CompanionChatHit::PetAction(action) => Some(action),
                _ => None,
            },
        )?;
    } else if ui.chat.msgs.is_empty() && !ui.chat.pending {
        let (cx, cy) =
            ((layout.history.0 + layout.history.2) / 2.0, (layout.history.1 + layout.history.3) / 2.0 - 34.0 * scale);
        p.fill_round_rect(
            (cx - 28.0 * scale, cy - 28.0 * scale, cx + 28.0 * scale, cy + 28.0 * scale),
            22.0 * scale,
            acc(0.1),
        )?;
        p.glyph(
            '\u{E8F2}',
            (cx - 22.0 * scale, cy - 22.0 * scale, cx + 22.0 * scale, cy + 22.0 * scale),
            24.0 * scale,
            acc(0.85),
        )?;
        p.text(
            "Ask anything",
            (layout.history.0, cy + 40.0 * scale, layout.history.2, cy + 66.0 * scale),
            15.0 * scale,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            WHITE,
        )?;
        p.text(
            "Your companion is here",
            (layout.history.0, cy + 66.0 * scale, layout.history.2, cy + 88.0 * scale),
            12.0 * scale,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_CENTER,
            rgba(1.0, 1.0, 1.0, 0.42),
        )?;
    } else {
        let history = History {
            msgs: &ui.chat.msgs,
            cache: &ui.chat.cache,
            id: format!("companion:{}", ui.chat.generation),
            hist: ui.chat.hist,
            max: &ui.chat.max,
            input_h: 0.0,
            reserve: 0.0,
            geom: Some(&ui.chat.geom),
            sel: ui.chat.selection(),
        };
        paint_history_in(p, layout.history, scale, &history, ui.chat.pending)?;
    }
    paint_editor(
        p,
        layout.composer,
        layout.input,
        &ui.chat.input,
        ui.active && ui.focus == CompanionChatHit::Input,
        ui.caret_on,
        if ui.chat.pet.enabled { "Describe your pet…" } else { "Message…" },
        AGY_FONT,
        scale,
    )?;
    let enabled = !ui.chat.pending
        && !ui.chat.pet.pending
        && (!ui.chat.input.text().trim().is_empty()
            || (ui.chat.pet.enabled && !ui.chat.pet.text_only && !ui.chat.pet.files.is_empty()));
    if ui.voice_enabled {
        p.fill_round_rect(
            layout.voice,
            12.0 * scale,
            rgba(
                1.0,
                1.0,
                1.0,
                if ui.hover == Some(CompanionChatHit::Voice) || ui.focus == CompanionChatHit::Voice {
                    0.15
                } else {
                    0.07
                },
            ),
        )?;
        p.glyph('\u{E720}', layout.voice, 15.0 * scale, WHITE)?;
    }
    p.fill_round_rect(
        layout.send,
        12.0 * scale,
        acc(if !enabled {
            0.12
        } else if ui.hover == Some(CompanionChatHit::Send) {
            0.55
        } else {
            0.35
        }),
    )?;
    p.glyph(
        if ui.chat.pending || ui.chat.pet.pending { '\u{E895}' } else { '\u{E724}' },
        layout.send,
        15.0 * scale,
        rgba(1.0, 1.0, 1.0, if enabled { 1.0 } else { 0.35 }),
    )?;
    if ui.active && ui.focus == CompanionChatHit::Send {
        p.stroke_round_rect(layout.send, 12.0 * scale, 1.2 * scale, acc(0.8))?;
    }
    p.text(
        "Enter to send · Shift+Enter for a new line",
        (28.0 * scale, size.1 - 35.0 * scale, size.0 - 28.0 * scale, size.1 - 13.0 * scale),
        10.0 * scale,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        rgba(1.0, 1.0, 1.0, 0.35),
    )?;
    Ok(())
}

pub fn companion_player_button(i: usize, scale: f32) -> Rect {
    let left = (88.0 + i as f32 * 42.0) * scale;
    (left, 80.0 * scale, left + 36.0 * scale, 116.0 * scale)
}

pub fn paint_companion_player(
    p: &Painter,
    scale: f32,
    media: &Media,
    progress: Option<(f32, f32)>,
    hover: Option<usize>,
    pressed: Option<usize>,
) -> Result<()> {
    let size = (COMPANION_PLAYER_SIZE.0 * scale, COMPANION_PLAYER_SIZE.1 * scale);
    paint_popup_panel(p, size, scale)?;
    let cover = (26.0 * scale, 26.0 * scale, 70.0 * scale, 70.0 * scale);
    if let Some(image) = &media.cover {
        p.image(cover, 12.0 * scale, image.size, &image.bgra)?;
    } else {
        p.fill_round_rect(cover, 12.0 * scale, rgba(1.0, 1.0, 1.0, 0.06))?;
        p.glyph('\u{E8D6}', cover, 20.0 * scale, acc(0.7))?;
    }
    p.text(
        if media.title.is_empty() { "Nothing is playing" } else { &media.title },
        (82.0 * scale, 25.0 * scale, 274.0 * scale, 47.0 * scale),
        13.5 * scale,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        WHITE,
    )?;
    p.text(
        &media.artist,
        (82.0 * scale, 48.0 * scale, 274.0 * scale, 68.0 * scale),
        11.5 * scale,
        DWRITE_FONT_WEIGHT_NORMAL,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        rgba(1.0, 1.0, 1.0, 0.5),
    )?;
    for i in 0..3 {
        let r = companion_player_button(i, scale);
        if i == 1 || hover == Some(i) {
            let alpha = if pressed == Some(i) {
                0.5
            } else if hover == Some(i) {
                0.3
            } else {
                0.18
            };
            p.fill_round_rect(r, 18.0 * scale, if i == 1 { acc(alpha) } else { rgba(1.0, 1.0, 1.0, alpha * 0.5) })?;
        }
        p.glyph(
            ['\u{E892}', if media.playing { '\u{E769}' } else { '\u{E768}' }, '\u{E893}'][i],
            r,
            15.0 * scale,
            WHITE,
        )?;
    }
    if let Some((position, duration)) = progress.filter(|(_, duration)| *duration > 0.0) {
        let time = |seconds: f32| {
            let seconds = seconds.max(0.0) as u32;
            format!("{}:{:02}", seconds / 60, seconds % 60)
        };
        for (label, r) in [(time(position), (26.0, 87.0, 76.0, 106.0)), (time(duration), (228.0, 87.0, 274.0, 106.0))] {
            p.text(
                &label,
                (r.0 * scale, r.1 * scale, r.2 * scale, r.3 * scale),
                10.5 * scale,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_CENTER,
                rgba(1.0, 1.0, 1.0, 0.4),
            )?;
        }
        let track = (26.0 * scale, 121.0 * scale, 274.0 * scale, 123.0 * scale);
        p.fill_round_rect(track, scale, rgba(1.0, 1.0, 1.0, 0.08))?;
        let fraction = (position / duration).clamp(0.0, 1.0);
        if fraction > 0.0 {
            p.fill_round_rect((track.0, track.1, track.0 + (track.2 - track.0) * fraction, track.3), scale, acc(0.7))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_layouts_keep_text_and_actions_separate_at_every_scale() {
        assert!(!crate::ui::popup::hit_panel((0.0, 0.0), (1.0, 1.0), 1.0));
        assert!(!crate::ui::popup::hit_panel((10.0, 10.0), (416.0, 460.0), 1.0));
        assert!(crate::ui::popup::hit_panel((200.0, 200.0), (416.0, 460.0), 1.0));
        for scale in [0.75, 1.0, 1.25, 1.5, 2.0] {
            for (w, h) in [(360.0, 360.0), (416.0, 460.0), (620.0, 500.0)] {
                for measured in [0.0, 64.0, 112.0, 1000.0] {
                    let size = (w * scale, h * scale);
                    let l = companion_chat_layout(size, scale, measured * scale);
                    assert!(l.header.3 < l.history.1 && l.history.3 < l.composer.1);
                    assert!(l.input.2 < l.send.0 && l.send.2 < l.composer.2);
                    assert!(l.input.1 >= l.composer.1 && l.input.3 <= l.composer.3);
                    assert!(l.setup.2 < l.close.0);
                    for (expected, rect) in [
                        (CompanionChatHit::Close, l.close),
                        (CompanionChatHit::Setup, l.setup),
                        (CompanionChatHit::Send, l.send),
                        (CompanionChatHit::History, l.history),
                    ] {
                        assert_eq!(
                            hit_companion_chat(&l, (rect.0 + rect.2) / 2.0, (rect.1 + rect.3) / 2.0),
                            Some(expected)
                        );
                    }
                    assert_eq!(hit_companion_chat(&l, 0.0, 0.0), None);
                }
            }
        }
    }
}
