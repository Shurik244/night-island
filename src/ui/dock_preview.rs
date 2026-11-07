use std::cell::RefCell;
use std::sync::{Arc, Once};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING,
};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetCapture, ReleaseCapture, SetCapture, VK_ESCAPE,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{Result, w};

use crate::platform::{
    desktop_dock as desktop,
    dock_actions::{Client, MediaAction, Snapshot},
    window_preview::Thumbnail,
};
use crate::ui::{
    desktop_dock::Entry,
    gfx::{Gpu, Painter, rgba},
    i18n::tr,
};
use crate::view::Rect;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Dismiss,
    Open,
    PreviousWindow,
    NextWindow,
    CloseWindow,
    PreviousPlayer,
    NextPlayer,
    PreviousTrack,
    PlayPause,
    NextTrack,
    Volume,
    Mute,
    Microphone,
    Deafen,
    SetupDiscord,
}

struct Button {
    rect: Rect,
    label: String,
    glyph: Option<char>,
    action: Action,
    enabled: bool,
    selected: bool,
}

#[derive(Clone, Copy)]
struct Layout {
    width: f32,
    height: f32,
    image: Rect,
    caption: Rect,
    media_y: f32,
    audio_y: f32,
    discord_y: f32,
}

impl Layout {
    fn new(work: RECT, scale: f32, media: bool, audio: bool, discord: bool, multiple: bool, players: bool) -> Self {
        let width = ((work.right - work.left) as f32 / scale - 16.0).clamp(220.0, 320.0);
        let height = (work.bottom - work.top) as f32 / scale;
        let discord_y = 52.0;
        let image_top = if discord { 140.0 } else { 52.0 };
        let caption_height = if media && !multiple { 0.0 } else { 28.0 };
        let media_height = if media { if players { 80.0 } else { 52.0 } } else { 0.0 };
        let fixed_height = image_top + caption_height + 4.0 + media_height + if audio { 40.0 } else { 0.0 } + 20.0;
        let image_height = (height - 16.0 - fixed_height).clamp(40.0, if discord { 92.0 } else { 124.0 });
        let image = (22.0, image_top, width - 22.0, image_top + image_height);
        let caption = (22.0, image.3, width - 22.0, image.3 + caption_height);
        let media_y = caption.3 + 4.0;
        let audio_y = media_y + media_height;
        Self {
            width,
            height: audio_y + if audio { 40.0 } else { 0.0 } + 20.0,
            image,
            caption,
            media_y,
            audio_y,
            discord_y,
        }
    }
}

struct State {
    hwnd: HWND,
    parent: HWND,
    gpu: Gpu,
    surface: RefCell<Option<((i32, i32), crate::ui::popup::PopupSurface)>>,
    entry: Entry,
    actions: Arc<Client>,
    settings: crate::data::desktop_dock::DockSettings,
    backdrop: Option<crate::ui::popup::PopupBackdrop>,
    snapshot: Snapshot,
    thumbnail: Option<Thumbnail>,
    thumbnail_visible: bool,
    window_index: usize,
    media_id: Option<u64>,
    scale: f32,
    work: RECT,
    anchor: Rect,
    origin: (i32, i32),
    layout: Layout,
    buttons: Vec<Button>,
    hover: Option<usize>,
    pressed: Option<usize>,
    visible: bool,
    closing: bool,
    reveal: crate::ui::reveal::Reveal,
    last_inside: Instant,
    dismissed: bool,
    thumbnail_attempt: Instant,
    error: Option<(&'static str, Instant)>,
}

pub struct Options {
    pub capture_hidden: bool,
    pub settings: crate::data::desktop_dock::DockSettings,
    pub actions: Arc<Client>,
}

pub struct Preview {
    state: Box<State>,
}

fn inside(r: Rect, p: (f32, f32)) -> bool {
    p.0 >= r.0 && p.0 < r.2 && p.1 >= r.1 && p.1 < r.3
}

fn scaled(r: Rect, s: f32) -> Rect {
    (r.0 * s, r.1 * s, r.2 * s, r.3 * s)
}

fn fit(source: (i32, i32), area: Rect) -> RECT {
    let scale = ((area.2 - area.0) / source.0.max(1) as f32).min((area.3 - area.1) / source.1.max(1) as f32);
    let w = (source.0 as f32 * scale).round().max(1.0);
    let h = (source.1 as f32 * scale).round().max(1.0);
    let x = (area.0 + (area.2 - area.0 - w) / 2.0).round();
    let y = (area.1 + (area.3 - area.1 - h) / 2.0).round();
    RECT { left: x as i32, top: y as i32, right: (x + w) as i32, bottom: (y + h) as i32 }
}

impl State {
    fn invalidate(&self) {
        let size = ((self.layout.width * self.scale).ceil() as i32, (self.layout.height * self.scale).ceil() as i32);
        let mut surface = self.surface.borrow_mut();
        if surface.as_ref().is_none_or(|(previous, _)| *previous != size) {
            *surface = crate::ui::popup::PopupSurface::new(size).ok().map(|surface| (size, surface));
        }
        if let Some((_, surface)) = &*surface
            && surface.paint(&self.gpu, |p| self.paint(p)).is_ok()
        {
            let _ = surface.present(self.hwnd, None, (self.reveal.value(Instant::now()) * 255.0).round() as u8);
        }
    }

    fn animate_visibility(&mut self, show: bool) {
        let ms = if self.settings.animations && crate::ui::feedback::motion_allowed() {
            if show { 170 } else { 120 }
        } else {
            1
        };
        self.closing = !show;
        self.reveal.retarget(show, ms, crate::data::settings::AnimCurve::Smooth, Instant::now());
        self.visibility_frame();
    }

    fn visibility_frame(&mut self) {
        let now = Instant::now();
        let alpha = self.reveal.value(now);
        let opacity = (alpha * 255.0).round() as u8;
        if let Some((_, surface)) = &*self.surface.borrow() {
            let _ = surface.present(self.hwnd, None, opacity);
        }
        if let Some(backdrop) = &self.backdrop {
            backdrop.set_opacity(alpha);
        }
        if let Some(thumbnail) = &self.thumbnail {
            thumbnail.set_opacity(opacity);
        }
        if self.reveal.active(now) {
            unsafe {
                SetTimer(Some(self.hwnd), 1, if self.settings.economy { 33 } else { 10 }, None);
            }
        } else {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), 1);
            }
            if self.closing {
                self.hide();
            }
        }
    }

    fn set_volume_from_point(&mut self, x: f32) {
        if let Some(audio) = &mut self.snapshot.audio {
            audio.volume = ((x - 68.0) / (self.layout.width - 136.0).max(1.0)).clamp(0.0, 1.0);
            self.actions.set_volume(audio.volume);
            self.invalidate();
        }
    }

    fn media_index(&self) -> Option<usize> {
        self.media_id
            .and_then(|id| self.snapshot.media.iter().position(|m| m.id == id))
            .or_else(|| (!self.snapshot.media.is_empty()).then_some(0))
    }

    fn button(&mut self, rect: Rect, label: &str, glyph: Option<char>, action: Action, enabled: bool, selected: bool) {
        self.buttons.push(Button { rect, label: tr(label).into_owned(), glyph, action, enabled, selected });
    }

    fn arrange(&mut self) {
        self.hover = None;
        let pressed = self.pressed.and_then(|i| self.buttons.get(i)).map(|b| b.action);
        if self.media_id.is_none_or(|id| !self.snapshot.media.iter().any(|m| m.id == id)) {
            self.media_id = self.snapshot.media.first().map(|m| m.id);
        }
        self.layout = Layout::new(
            self.work,
            self.scale,
            !self.snapshot.media.is_empty(),
            self.snapshot.audio.is_some(),
            self.snapshot.discord.is_some(),
            self.entry.windows.len() > 1,
            self.snapshot.media.len() > 1,
        );
        let l = self.layout;
        self.buttons.clear();
        self.button(
            (l.width - 48.0, 18.0, l.width - 22.0, 44.0),
            "Close preview",
            Some('\u{E711}'),
            Action::Dismiss,
            true,
            false,
        );
        if !self.entry.windows.is_empty() {
            self.button(l.image, "Open window", None, Action::Open, true, false);
            if self.entry.windows.len() > 1 {
                self.button(
                    (l.width - 118.0, l.caption.1, l.width - 90.0, l.caption.3),
                    "Previous window",
                    Some('\u{E76B}'),
                    Action::PreviousWindow,
                    true,
                    false,
                );
                self.button(
                    (l.width - 88.0, l.caption.1, l.width - 60.0, l.caption.3),
                    "Next window",
                    Some('\u{E76C}'),
                    Action::NextWindow,
                    true,
                    false,
                );
            }
            if l.caption.3 > l.caption.1 {
                self.button(
                    (l.width - 48.0, l.caption.1, l.width - 22.0, l.caption.3),
                    "Close window",
                    Some('\u{E8BB}'),
                    Action::CloseWindow,
                    true,
                    false,
                );
            }
        }
        if let Some(i) = self.media_index() {
            let media = &self.snapshot.media[i];
            let (prev, toggle, next, playing) = (media.can_previous, media.can_toggle, media.can_next, media.playing);
            let y = l.media_y + 10.0;
            self.button(
                (l.width - 120.0, y + 2.0, l.width - 94.0, y + 28.0),
                "Previous track",
                Some('\u{E892}'),
                Action::PreviousTrack,
                prev,
                false,
            );
            self.button(
                (l.width - 90.0, y, l.width - 58.0, y + 32.0),
                if playing { "Pause" } else { "Play" },
                Some(if playing { '\u{E769}' } else { '\u{E768}' }),
                Action::PlayPause,
                toggle,
                false,
            );
            self.button(
                (l.width - 54.0, y + 2.0, l.width - 28.0, y + 28.0),
                "Next track",
                Some('\u{E893}'),
                Action::NextTrack,
                next,
                false,
            );
            if self.snapshot.media.len() > 1 {
                self.button(
                    (l.width - 88.0, l.media_y + 48.0, l.width - 60.0, l.media_y + 76.0),
                    "Previous player",
                    Some('\u{E76B}'),
                    Action::PreviousPlayer,
                    true,
                    false,
                );
                self.button(
                    (l.width - 56.0, l.media_y + 48.0, l.width - 28.0, l.media_y + 76.0),
                    "Next player",
                    Some('\u{E76C}'),
                    Action::NextPlayer,
                    true,
                    false,
                );
            }
        }

        if let Some(audio) = &self.snapshot.audio {
            let muted = audio.muted;
            let y = l.audio_y + 4.0;
            self.button(
                (26.0, y, 58.0, y + 32.0),
                if muted { "Unmute application" } else { "Mute application" },
                Some(if muted { '\u{E74F}' } else { '\u{E767}' }),
                Action::Mute,
                true,
                muted,
            );
            self.button((64.0, y, l.width - 64.0, y + 32.0), "Application volume", None, Action::Volume, true, false);
        }
        if let Some(voice) = &self.snapshot.discord {
            let (connected, available, muted, deaf) = (voice.connected, voice.available, voice.mute, voice.deaf);
            let y = l.discord_y + 12.0;
            if connected {
                self.button(
                    (l.width - 100.0, y, l.width - 66.0, y + 34.0),
                    if muted { "Unmute microphone" } else { "Mute microphone" },
                    Some(if muted { '\u{F781}' } else { '\u{E720}' }),
                    Action::Microphone,
                    available,
                    muted,
                );
                self.button(
                    (l.width - 62.0, y, l.width - 28.0, y + 34.0),
                    if deaf { "Undeafen" } else { "Deafen" },
                    Some(if deaf { '\u{E74F}' } else { '\u{E7F5}' }),
                    Action::Deafen,
                    available,
                    deaf,
                );
            } else {
                self.button(
                    (30.0, l.discord_y + 48.0, l.width - 30.0, l.discord_y + 76.0),
                    "Connect Discord in settings",
                    Some('\u{E713}'),
                    Action::SetupDiscord,
                    true,
                    false,
                );
            }
        }
        self.pressed = pressed.and_then(|action| self.buttons.iter().position(|b| b.action == action && b.enabled));
        let size = ((l.width * self.scale).ceil() as i32, (l.height * self.scale).ceil() as i32);
        self.origin = position(self.work, self.anchor, size, self.scale);
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                self.origin.0,
                self.origin.1,
                size.0,
                size.1,
                SWP_NOACTIVATE,
            );
        }
        if let Some(backdrop) = &self.backdrop {
            backdrop.show(self.origin, size, self.scale, self.visible && self.settings.blur);
        }
        self.update_thumbnail();
        if let Some(backdrop) = &self.backdrop {
            backdrop.set_opacity(self.reveal.value(Instant::now()));
        }
        self.invalidate();
    }

    fn rebuild_thumbnail(&mut self) {
        self.thumbnail_attempt = Instant::now();
        self.thumbnail.take();
        self.thumbnail_visible = false;
        if let Some(source) = self.entry.windows.get(self.window_index) {
            self.thumbnail = Thumbnail::new(self.hwnd, source);
        }
        self.update_thumbnail();
    }

    fn update_thumbnail(&mut self) {
        self.thumbnail_visible = self.thumbnail.as_mut().is_some_and(|thumbnail| {
            let Some(size) = thumbnail.source_size() else { return false };
            thumbnail.update(fit((size.cx, size.cy), scaled(self.layout.image, self.scale)), self.visible)
        });
        if !self.thumbnail_visible {
            self.thumbnail.take();
        } else if let Some(thumbnail) = &self.thumbnail {
            thumbnail.set_opacity((self.reveal.value(Instant::now()) * 255.0).round() as u8);
        }
    }

    fn paint(&self, p: &Painter) -> Result<()> {
        let s = self.scale;
        let l = self.layout;
        let text = |value: &str, rect: Rect, size: f32, strong: bool, muted: bool| {
            p.text(
                value,
                scaled(rect, s),
                size * s,
                if strong { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL },
                DWRITE_TEXT_ALIGNMENT_LEADING,
                rgba(1.0, 1.0, 1.0, if muted { 0.5 } else { 0.96 }),
            )
        };
        crate::view::paint_popup_shadow(p, (l.width * s, l.height * s), s)?;
        let margin = crate::ui::popup::POPUP_MARGIN * s;
        crate::ui::desktop_dock::paint_material(
            p,
            (margin, margin, l.width * s - margin, l.height * s - margin),
            28.0 * s,
            s,
            &self.settings,
        )?;
        if let Some(icon) = &self.entry.icon {
            p.image(scaled((24.0, 21.0, 44.0, 41.0), s), 0.0, icon.side, &icon.bgra)?;
        } else {
            p.glyph(
                if self.snapshot.discord.is_some() { '\u{E8F2}' } else { '\u{E737}' },
                scaled((24.0, 21.0, 44.0, 41.0), s),
                17.0 * s,
                rgba(1.0, 1.0, 1.0, 0.65),
            )?;
        }
        text(&self.entry.pin.name, (52.0, 18.0, l.width - 58.0, 44.0), 13.0, true, false)?;
        if let Some(voice) = &self.snapshot.discord {
            let y = l.discord_y;
            p.fill_round_rect(scaled((22.0, y, l.width - 22.0, y + 80.0), s), 18.0 * s, rgba(1.0, 1.0, 1.0, 0.07))?;
            let title = if voice.channel.is_empty() {
                tr("Voice chat")
            } else {
                std::borrow::Cow::Borrowed(voice.channel.as_str())
            };
            let right = if voice.connected { l.width - 106.0 } else { l.width - 40.0 };
            text(&title, (34.0, y + 8.0, right, y + 30.0), 13.0, true, false)?;
            let status = if !voice.connected {
                tr("Connect to control your call")
            } else if voice.error.is_some() {
                tr("Discord voice controls unavailable")
            } else if !voice.available {
                tr("Waiting for Discord voice settings")
            } else if voice.channel.is_empty() {
                tr("Not in a voice channel")
            } else {
                tr("Voice channel")
            };
            text(&status, (34.0, y + 28.0, right, y + 47.0), 11.0, false, true)?;
            if voice.connected {
                let mut x = 34.0;
                let mut shown = 0;
                for member in &voice.members {
                    let width = (member.name.chars().count() as f32 * 6.0 + 34.0).clamp(56.0, 106.0);
                    if x + width > l.width - 42.0 {
                        break;
                    }
                    let chip = (x, y + 52.0, x + width, y + 74.0);
                    p.fill_round_rect(
                        scaled(chip, s),
                        12.0 * s,
                        rgba(1.0, 1.0, 1.0, if member.speaking { 0.16 } else { 0.06 }),
                    )?;
                    let color = if member.speaking {
                        rgba(0.35, 0.9, 0.5, 1.0)
                    } else if member.muted || member.deaf {
                        rgba(1.0, 0.45, 0.45, 0.9)
                    } else {
                        rgba(1.0, 1.0, 1.0, 0.3)
                    };
                    p.fill_ellipse((x + 10.0) * s, (y + 63.0) * s, 3.0 * s, 3.0 * s, color)?;
                    text(&member.name, (x + 20.0, y + 52.0, x + width - 8.0, y + 74.0), 11.0, false, false)?;
                    x += width + 6.0;
                    shown += 1;
                }
                if shown < voice.members.len() {
                    text(
                        &format!("+{}", voice.members.len() - shown),
                        (x, y + 52.0, l.width - 32.0, y + 74.0),
                        11.0,
                        false,
                        true,
                    )?;
                }
                if voice.members.is_empty() {
                    p.glyph(
                        '\u{E716}',
                        scaled((42.0, y + 52.0, 62.0, y + 74.0), s),
                        13.0 * s,
                        rgba(1.0, 1.0, 1.0, 0.35),
                    )?;
                    text(&tr("No voice participants"), (68.0, y + 52.0, l.width - 40.0, y + 74.0), 11.0, false, true)?;
                }
            }
        }
        p.fill_round_rect(scaled(l.image, s), 18.0 * s, rgba(1.0, 1.0, 1.0, 0.045))?;
        if !self.thumbnail_visible {
            let label = if self.thumbnail.as_ref().is_some_and(|t| t.is_minimized())
                || self.entry.windows.get(self.window_index).is_some_and(|w| w.minimized)
            {
                "Window minimized"
            } else {
                "Preview unavailable"
            };
            p.text(
                &tr(label),
                scaled(l.image, s),
                12.0 * s,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_CENTER,
                rgba(1.0, 1.0, 1.0, 0.4),
            )?;
        }
        let caption_right = l.width - if self.entry.windows.len() > 1 { 124.0 } else { 62.0 };
        if let Some(window) = self.entry.windows.get(self.window_index)
            && l.caption.3 > l.caption.1
        {
            let title = if self.entry.windows.len() > 1 {
                format!("{}/{} · {}", self.window_index + 1, self.entry.windows.len(), window.title)
            } else {
                window.title.clone()
            };
            text(&title, (30.0, l.caption.1, caption_right, l.caption.3), 11.0, false, true)?;
        }
        if let Some(i) = self.media_index() {
            let media = &self.snapshot.media[i];
            let multiple = self.snapshot.media.len() > 1;
            p.fill_round_rect(
                scaled((22.0, l.media_y, l.width - 22.0, l.media_y + if multiple { 76.0 } else { 48.0 }), s),
                16.0 * s,
                rgba(1.0, 1.0, 1.0, 0.07),
            )?;
            let right = l.width - 124.0;
            text(&media.title, (34.0, l.media_y + 5.0, right, l.media_y + 27.0), 12.0, true, false)?;
            let subtitle = if media.artist.is_empty() { &media.source } else { &media.artist };
            text(subtitle, (34.0, l.media_y + 26.0, right, l.media_y + 43.0), 10.5, false, true)?;
            if multiple {
                text(
                    &format!("{}/{} · {}", i + 1, self.snapshot.media.len(), media.source),
                    (34.0, l.media_y + 48.0, l.width - 94.0, l.media_y + 76.0),
                    11.0,
                    false,
                    true,
                )?;
            }
        }

        if let Some(audio) = &self.snapshot.audio {
            let y = l.audio_y;
            p.fill_round_rect(
                scaled((22.0, y + 2.0, l.width - 22.0, y + 38.0), s),
                16.0 * s,
                rgba(1.0, 1.0, 1.0, 0.07),
            )?;
            let track = (68.0, y + 20.0, l.width - 68.0);
            p.fill_round_rect(
                scaled((track.0, track.1 - 2.0, track.2, track.1 + 2.0), s),
                2.0 * s,
                rgba(1.0, 1.0, 1.0, 0.2),
            )?;
            let x = track.0 + (track.2 - track.0) * audio.volume.clamp(0.0, 1.0);
            p.fill_round_rect(
                scaled((track.0, track.1 - 2.0, x, track.1 + 2.0), s),
                2.0 * s,
                rgba(1.0, 1.0, 1.0, if audio.muted { 0.35 } else { 0.95 }),
            )?;
            p.fill_ellipse(x * s, track.1 * s, 5.0 * s, 5.0 * s, rgba(1.0, 1.0, 1.0, 0.96))?;
            text(
                &format!("{}", (audio.volume * 100.0).round() as u32),
                (l.width - 58.0, y + 2.0, l.width - 28.0, y + 38.0),
                12.0,
                false,
                true,
            )?;
        }
        for (i, button) in self.buttons.iter().enumerate() {
            if matches!(button.action, Action::Open | Action::Volume) {
                continue;
            }
            let hot = self.hover == Some(i) && button.enabled;
            let primary = button.action == Action::PlayPause && button.enabled;
            let voice = matches!(button.action, Action::Microphone | Action::Deafen);
            let background = if primary {
                rgba(1.0, 1.0, 1.0, if hot { 0.85 } else { 1.0 })
            } else if button.selected {
                rgba(1.0, 0.35, 0.35, 0.32)
            } else {
                rgba(
                    1.0,
                    1.0,
                    1.0,
                    if hot {
                        0.16
                    } else if voice || button.action == Action::SetupDiscord {
                        0.08
                    } else {
                        0.0
                    },
                )
            };
            let radius = (button.rect.3 - button.rect.1) / 2.0;
            p.fill_round_rect(scaled(button.rect, s), radius * s, background)?;
            let color = if primary {
                rgba(0.0, 0.0, 0.0, 1.0)
            } else {
                rgba(
                    1.0,
                    1.0,
                    1.0,
                    if !button.enabled {
                        0.25
                    } else if button.selected || hot || voice {
                        0.95
                    } else {
                        0.6
                    },
                )
            };
            let has_text = button.action == Action::SetupDiscord;
            if let Some(glyph) = button.glyph {
                let rect = if has_text {
                    (button.rect.0 + 12.0, button.rect.1, button.rect.0 + 40.0, button.rect.3)
                } else {
                    button.rect
                };
                p.glyph(
                    glyph,
                    scaled(rect, s),
                    if matches!(button.action, Action::Dismiss | Action::CloseWindow) { 11.0 } else { 15.0 } * s,
                    color,
                )?;
            }
            if has_text {
                p.text(
                    &button.label,
                    scaled((button.rect.0 + 46.0, button.rect.1, button.rect.2 - 12.0, button.rect.3), s),
                    12.0 * s,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                    color,
                )?;
            }
        }
        if let Some((message, _)) = self.error {
            text(&tr(message), (30.0, l.caption.1, caption_right, l.caption.3), 11.0, false, false)?;
        }
        Ok(())
    }

    fn hit(&self, point: (f32, f32)) -> Option<usize> {
        self.buttons.iter().position(|b| b.enabled && inside(b.rect, point))
    }

    fn hide(&mut self) {
        self.visible = false;
        self.closing = false;
        self.reveal = crate::ui::reveal::Reveal::new(0.0);
        if let Some(backdrop) = &self.backdrop {
            backdrop.hide();
        }
        self.pressed = None;
        unsafe {
            let _ = PostMessageW(Some(self.hwnd), WM_APP + 88, WPARAM(0), LPARAM(0));
        }
        self.thumbnail.take();
        self.thumbnail_visible = false;
        self.actions.clear();
        self.snapshot = Snapshot::default();
        unsafe {
            let _ = KillTimer(Some(self.hwnd), 1);
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    fn action(&mut self, action: Action) -> Option<desktop::Window> {
        match action {
            Action::Dismiss => {
                self.dismissed = true;
                self.animate_visibility(false);
            }
            Action::Open => {
                let window = self.entry.windows.get(self.window_index).cloned();
                self.dismissed = true;
                self.hide();
                return window;
            }
            Action::PreviousWindow | Action::NextWindow => {
                let count = self.entry.windows.len();
                if count > 0 {
                    self.window_index =
                        (self.window_index + if action == Action::NextWindow { 1 } else { count - 1 }) % count;
                    self.rebuild_thumbnail();
                }
            }
            Action::CloseWindow => {
                if let Some(window) = self.entry.windows.get(self.window_index)
                    && !desktop::change_window(window, desktop::WindowAction::Close)
                {
                    self.error = Some(("Window is no longer available", Instant::now()));
                }
            }
            Action::PreviousPlayer | Action::NextPlayer => {
                if let Some(i) = self.media_index() {
                    let count = self.snapshot.media.len();
                    self.media_id = Some(
                        self.snapshot.media[(i + if action == Action::NextPlayer { 1 } else { count - 1 }) % count].id,
                    );
                }
            }
            Action::PreviousTrack | Action::PlayPause | Action::NextTrack => {
                if let Some(i) = self.media_index() {
                    self.actions.control(
                        self.snapshot.media[i].id,
                        match action {
                            Action::PreviousTrack => MediaAction::Previous,
                            Action::NextTrack => MediaAction::Next,
                            _ => MediaAction::PlayPause,
                        },
                    );
                }
            }
            Action::Volume => {}
            Action::Mute => {
                if let Some(audio) = &mut self.snapshot.audio {
                    audio.muted = !audio.muted;
                    self.actions.set_muted(audio.muted);
                }
            }
            Action::Microphone => {
                if let Some(voice) = &self.snapshot.discord {
                    self.actions.set_discord_mute(!voice.mute);
                }
            }
            Action::Deafen => {
                if let Some(voice) = &self.snapshot.discord {
                    self.actions.set_discord_deaf(!voice.deaf);
                }
            }
            Action::SetupDiscord => {
                unsafe {
                    let _ =
                        PostMessageW(Some(self.parent), crate::ui::desktop_dock::WM_DESKTOP_DOCK, WPARAM(3), LPARAM(0));
                }
                self.dismissed = true;
                self.hide();
            }
        }
        if self.visible {
            self.arrange();
        }
        None
    }
}

fn position(work: RECT, anchor: Rect, size: (i32, i32), scale: f32) -> (i32, i32) {
    let x = (((anchor.0 + anchor.2) / 2.0) as i32 - size.0 / 2)
        .clamp(work.left + 4, (work.right - size.0 - 4).max(work.left + 4));
    let y = (anchor.1 as i32 - size.1 - (10.0 * scale) as i32)
        .clamp(work.top + 4, (work.bottom - size.1 - 4).max(work.top + 4));
    (x, y)
}

extern "system" fn proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, message, w, l);
        }
        match message {
            WM_TIMER if w.0 == 1 => {
                let _ = KillTimer(Some(hwnd), 1);
                (*ptr).visibility_frame();
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                BeginPaint(hwnd, &mut paint);
                let _ = EndPaint(hwnd, &paint);
                (*ptr).invalidate();
                LRESULT(0)
            }
            WM_NCHITTEST => {
                let state = &*ptr;
                let point =
                    (l.0 as i16 as f32 - state.origin.0 as f32, (l.0 >> 16) as i16 as f32 - state.origin.1 as f32);
                LRESULT(
                    if crate::ui::popup::hit_panel(
                        point,
                        (state.layout.width * state.scale, state.layout.height * state.scale),
                        state.scale,
                    ) {
                        HTCLIENT as isize
                    } else {
                        HTTRANSPARENT as isize
                    },
                )
            }
            message if message == WM_APP + 88 => {
                if GetCapture() == hwnd {
                    let _ = ReleaseCapture();
                }
                LRESULT(0)
            }
            WM_CAPTURECHANGED => {
                (*ptr).pressed = None;
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let state = &mut *ptr;
                let point = (l.0 as i16 as f32 / state.scale, (l.0 >> 16) as i16 as f32 / state.scale);
                if state.pressed.and_then(|i| state.buttons.get(i)).is_some_and(|b| b.action == Action::Volume) {
                    state.set_volume_from_point(point.0);
                }
                let hover = state.hit(point);
                if hover != state.hover {
                    state.hover = hover;
                    state.invalidate();
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let state = &mut *ptr;
                state.pressed = state.hit((l.0 as i16 as f32 / state.scale, (l.0 >> 16) as i16 as f32 / state.scale));
                if state.pressed.and_then(|i| state.buttons.get(i)).is_some_and(|b| b.action == Action::Volume) {
                    state.set_volume_from_point(l.0 as i16 as f32 / state.scale);
                    SetCapture(hwnd);
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let activate = {
                    let state = &mut *ptr;
                    let hit = state.hit((l.0 as i16 as f32 / state.scale, (l.0 >> 16) as i16 as f32 / state.scale));
                    let pressed = state.pressed.take();
                    if let Some(i) = hit.filter(|_| hit == pressed) {
                        state.action(state.buttons[i].action)
                    } else {
                        None
                    }
                };
                if GetCapture() == hwnd {
                    let _ = ReleaseCapture();
                }
                if let Some(window) = activate {
                    desktop::activate(&window, false);
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                (*ptr).dismissed = true;
                (*ptr).hide();
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, w, l),
        }
    }
}

impl Preview {
    pub fn new(
        parent: HWND,
        gpu: &Gpu,
        entry: &Entry,
        anchor: Rect,
        work: RECT,
        scale: f32,
        options: Options,
    ) -> Result<Self> {
        static CLASS: Once = Once::new();
        let instance = unsafe { GetModuleHandleW(None)? };
        CLASS.call_once(|| unsafe {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: w!("NightIsland.DockPreview"),
                ..Default::default()
            });
        });
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("NightIsland.DockPreview"),
                w!("Night Island Preview"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(instance.into()),
                None,
            )?
        };
        let backdrop =
            gpu.compositor().ok().and_then(|compositor| crate::ui::popup::PopupBackdrop::new(hwnd, &compositor).ok());
        let state = Box::new(State {
            hwnd,
            parent,
            gpu: gpu.popup(),
            surface: RefCell::new(None),
            entry: entry.clone(),
            actions: options.actions,
            settings: options.settings,
            backdrop,
            snapshot: Snapshot::default(),
            thumbnail: None,
            thumbnail_visible: false,
            window_index: entry.windows.iter().position(|w| w.active).unwrap_or(0),
            media_id: None,
            scale,
            work,
            anchor,
            origin: (0, 0),
            layout: Layout::new(work, scale, false, false, false, false, false),
            buttons: Vec::new(),
            hover: None,
            pressed: None,
            visible: false,
            closing: false,
            reveal: crate::ui::reveal::Reveal::new(0.0),
            last_inside: Instant::now(),
            dismissed: false,
            thumbnail_attempt: Instant::now(),
            error: None,
        });
        let mut preview = Self { state };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&raw mut *preview.state) as isize);
        }
        preview.set_capture_hidden(options.capture_hidden);
        preview.show(entry, anchor, work, scale);
        Ok(preview)
    }

    pub fn show(&mut self, entry: &Entry, anchor: Rect, work: RECT, scale: f32) {
        self.state.thumbnail.take();
        self.state.thumbnail_visible = false;
        self.state.entry = entry.clone();
        self.state.window_index = entry.windows.iter().position(|w| w.active).unwrap_or(0);
        self.state.media_id = None;
        self.state.anchor = anchor;
        self.state.work = work;
        self.state.scale = scale;
        self.state.visible = true;
        self.state.dismissed = false;
        self.state.last_inside = Instant::now();
        self.state.pressed = None;
        self.state.hover = None;
        self.state.error = None;
        self.state.animate_visibility(true);
        let target = entry
            .windows
            .first()
            .map(|w| w.path.clone())
            .unwrap_or_else(|| desktop::resolved_target(&entry.pin.target));
        self.state.actions.request(&target, &entry.windows.iter().map(|w| w.handle).collect::<Vec<_>>());
        self.state.snapshot = self.state.actions.snapshot();
        self.state.arrange();
        self.state.rebuild_thumbnail();
        unsafe {
            let _ = ShowWindow(self.state.hwnd, SW_SHOWNOACTIVATE);
        }
        self.state.arrange();
        self.refresh_backdrop();
    }

    pub fn set_material(&mut self, settings: &crate::data::desktop_dock::DockSettings) {
        self.state.settings = settings.clone();
    }

    pub fn refresh_backdrop(&self) {
        if self.state.visible
            && self.state.settings.blur
            && let Some(backdrop) = &self.state.backdrop
        {
            backdrop.refresh();
        }
    }

    pub fn repaint_icons(&self) {
        if self.state.visible {
            self.state.invalidate();
        }
    }

    pub fn refresh_actions(&mut self) {
        if !self.state.visible {
            return;
        }
        let mut snapshot = self.state.actions.snapshot();
        if snapshot.target != self.state.snapshot.target {
            return;
        }
        if self.state.pressed.and_then(|i| self.state.buttons.get(i)).is_some_and(|b| b.action == Action::Volume) {
            snapshot.audio = self.state.snapshot.audio.clone();
        }
        if snapshot != self.state.snapshot {
            self.state.snapshot = snapshot;
            self.state.arrange();
        }
    }

    pub fn sync_entry(&mut self, entry: &Entry) {
        if !self.state.visible || self.state.entry.pin.target != entry.pin.target {
            return;
        }
        if self.state.entry.windows == entry.windows {
            return;
        }
        let selected = self.state.entry.windows.get(self.state.window_index).map(|w| w.handle);
        self.state.entry = entry.clone();
        self.state.window_index =
            selected.and_then(|handle| entry.windows.iter().position(|w| w.handle == handle)).unwrap_or(0);
        if entry.windows.is_empty() {
            self.state.hide();
            return;
        }
        let path = entry.windows[0].path.clone();
        self.state.actions.request(&path, &entry.windows.iter().map(|w| w.handle).collect::<Vec<_>>());
        self.state.snapshot = self.state.actions.snapshot();
        self.state.arrange();
        self.state.rebuild_thumbnail();
    }

    pub fn allow_reopen(&mut self) {
        self.state.dismissed = false;
    }

    pub fn is_open(&self) -> bool {
        self.state.visible
    }
    pub fn dismissed(&self) -> bool {
        self.state.dismissed
    }
    pub fn target(&self) -> &str {
        &self.state.entry.pin.target
    }
    pub fn hide(&mut self) {
        if self.state.visible {
            self.state.hide();
        }
    }

    pub fn contains(&self, point: POINT) -> bool {
        self.state.visible
            && crate::ui::popup::hit_panel(
                ((point.x - self.state.origin.0) as f32, (point.y - self.state.origin.1) as f32),
                (self.state.layout.width * self.state.scale, self.state.layout.height * self.state.scale),
                self.state.scale,
            )
    }

    pub fn tick(&mut self, point: POINT, over_source: bool) {
        if !self.state.visible {
            return;
        }
        if unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) } < 0 {
            self.state.dismissed = true;
            self.state.hide();
            return;
        }
        if over_source || self.contains(point) || self.state.pressed.is_some() {
            self.state.last_inside = Instant::now();
            if self.state.closing && !self.state.dismissed {
                self.state.animate_visibility(true);
            }
        } else if self.state.last_inside.elapsed() > Duration::from_millis(250) {
            if !self.state.closing {
                self.state.animate_visibility(false);
            }
            return;
        }
        self.refresh_actions();
        if self.state.thumbnail.is_none() && self.state.thumbnail_attempt.elapsed() >= Duration::from_secs(1) {
            self.state.rebuild_thumbnail();
        }
        let previous = self.state.thumbnail_visible;
        self.state.update_thumbnail();
        if previous != self.state.thumbnail_visible {
            self.state.invalidate();
        }
        if self.state.error.is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(3)) {
            self.state.error = None;
            self.state.invalidate();
        }
    }

    pub fn set_capture_hidden(&self, hidden: bool) {
        if let Some(backdrop) = &self.state.backdrop {
            backdrop.set_capture_hidden(hidden);
        }
        unsafe {
            let _ = SetWindowDisplayAffinity(self.state.hwnd, if hidden { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE });
        }
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.state.thumbnail.take();
        self.state.backdrop.take();
        unsafe {
            SetWindowLongPtrW(self.state.hwnd, GWLP_USERDATA, 0);
            let _ = DestroyWindow(self.state.hwnd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_fits_windows_without_stretching_and_stays_on_negative_monitor() {
        let work = RECT { left: -1920, top: 0, right: 0, bottom: 1040 };
        for scale in [1.0, 1.5, 2.0] {
            let layout = Layout::new(work, scale, true, true, true, true, true);
            let size = ((layout.width * scale).ceil() as i32, (layout.height * scale).ceil() as i32);
            let at = position(work, (-100.0, 950.0, -40.0, 1010.0), size, scale);
            assert!(at.0 >= work.left && at.0 + size.0 <= work.right);
            assert!(at.1 >= work.top && at.1 + size.1 <= work.bottom);
            let area = scaled(layout.image, scale);
            for source in [(1920, 1080), (800, 1200), (3440, 1440)] {
                let rect = fit(source, area);
                assert!(rect.left as f32 >= area.0 - 1.0 && rect.right as f32 <= area.2 + 1.0);
                assert!(rect.top as f32 >= area.1 - 1.0 && rect.bottom as f32 <= area.3 + 1.0);
                let ratio = (rect.right - rect.left) as f32 / (rect.bottom - rect.top) as f32;
                assert!((ratio - source.0 as f32 / source.1 as f32).abs() < 0.08);
            }
        }
    }
}

#[cfg(test)]
#[path = "dock_preview_tests.rs"]
mod native_tests;
