use crate::services::yandex::{LIBRARY_PAGE, Rating, Snapshot};
use crate::view::*;

#[derive(Default)]
pub struct YandexUi {
    pub state: Snapshot,
    pub hover: Option<YandexHit>,
    pub copied: bool,
    pub show_library: bool,
    pub search_open: bool,
    pub search_hidden: bool,
    pub search_input: edit::TextEdit,
    pub search_sent: Option<(u64, String)>,
    pub search_page: usize,
    pub search_selected: Option<usize>,
    pub ratings_hidden: bool,
    pub volume_preview: Option<u8>,
    pub colour_transition: Option<(WaveColours, std::time::Instant)>,
}

impl YandexUi {
    pub fn search_query(&self) -> String {
        self.search_input.text().trim().to_owned()
    }

    pub fn search_result(&self) -> Option<&crate::services::yandex::SearchResult> {
        let query = self.search_query();
        self.state.search.as_ref().filter(|result| {
            result.query == query
                && self.search_sent.as_ref().is_none_or(|(request, sent)| sent != &query || *request == result.request)
        })
    }

    pub fn search_rows(&self) -> &[crate::services::yandex::Track] {
        let Some(result) = self.search_result() else { return &[] };
        let start = (self.search_page * SEARCH_ROWS).min(result.tracks.len());
        let end = (start + SEARCH_ROWS).min(result.tracks.len());
        &result.tracks[start..end]
    }

    pub fn colours(&self) -> WaveColours {
        let target =
            WaveColours::new(self.state.palette, self.state.media.accent.unwrap_or_else(crate::ui::theme::accent_u8));
        self.colour_transition.map_or(target, |(from, at)| {
            let t = (at.elapsed().as_secs_f32() / 0.42).clamp(0.0, 1.0);
            from.blend(target, t * t * (3.0 - 2.0 * t))
        })
    }

    pub fn accept(&mut self, next: Snapshot, animate: bool) {
        let from = self.colours();
        if next.code != self.state.code {
            self.copied = false;
        }
        if next.account.is_empty() {
            self.show_library = false;
            self.search_open = false;
            self.search_sent = None;
        }
        if self.volume_preview == Some(next.config.volume) || (!next.error.is_empty() && next.error != self.state.error)
        {
            self.volume_preview = None;
        }
        self.state = next;
        self.colour_transition = None;
        if animate && from != self.colours() {
            self.colour_transition = Some((from, std::time::Instant::now()));
        }
    }

    pub fn animate_colours(&mut self) -> bool {
        if self.colour_transition.is_some_and(|(_, at)| at.elapsed().as_millis() < 420) {
            true
        } else {
            self.colour_transition = None;
            false
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum YandexHit {
    Connect,
    Browser,
    Copy,
    Cancel,
    Toggle,
    Previous,
    Next,
    Stop,
    Disconnect,
    Retry,
    Autoplay,
    Filter(usize, i32),
    Home,
    Seek,
    Volume,
    Library,
    LibraryPage(i32),
    PlayLiked(usize),
    Search,
    SearchInput,
    SearchPage(i32),
    PlaySearch(usize),
    Rate(Rating),
}

pub fn wave_seek_rect(pill: Rect, s: f32) -> Rect {
    (pill.0 + 24.0 * s, pill.1 + 284.0 * s, pill.0 + 220.0 * s, pill.1 + 307.0 * s)
}

pub fn wave_fraction(pill: Rect, s: f32, x: f32) -> f32 {
    let r = wave_seek_rect(pill, s);
    ((x - r.0) / (r.2 - r.0).max(1.0)).clamp(0.0, 1.0)
}

pub fn wave_volume_level(pill: Rect, s: f32, x: f32) -> u8 {
    let left = pill.0 + 30.0 * s;
    (((x - left) / (150.0 * s)).clamp(0.0, 1.0) * 100.0).round() as u8
}

pub const SEARCH_ROWS: usize = 3;
pub const SEARCH_MAX_CHARS: usize = 180;

pub fn wave_search_field((x, y, w, _): Rect, s: f32) -> (Rect, Rect) {
    let frame = (x + 244.0 * s, y + 105.0 * s, x + w - 24.0 * s, y + 141.0 * s);
    (frame, (frame.0 + 12.0 * s, frame.1, frame.2 - 12.0 * s, frame.3))
}

pub fn yandex_controls(pill: Rect, s: f32, ui: &YandexUi) -> Vec<(Rect, YandexHit)> {
    let (x, y, w, _) = pill;
    let r = |a: f32, b: f32, c: f32, d: f32| (x + a * s, y + b * s, x + c * s, y + d * s);
    let width = w / s;
    if ui.state.account.is_empty() {
        if ui.state.code.is_empty() {
            vec![(
                r(32.0, 240.0, width - 32.0, 282.0),
                if ui.state.has_token { YandexHit::Retry } else { YandexHit::Connect },
            )]
        } else {
            vec![
                (r(32.0, 238.0, width * 0.5 - 5.0, 280.0), YandexHit::Browser),
                (r(width * 0.5 + 5.0, 238.0, width - 32.0, 280.0), YandexHit::Copy),
                (r(width - 106.0, 62.0, width - 24.0, 90.0), YandexHit::Cancel),
            ]
        }
    } else {
        let mut controls = vec![
            (r(24.0, 244.0, 65.0, 280.0), YandexHit::Previous),
            (r(73.0, 244.0, 171.0, 280.0), YandexHit::Toggle),
            (r(179.0, 244.0, 220.0, 280.0), YandexHit::Next),
            (r(24.0, 322.0, 220.0, 346.0), YandexHit::Volume),
            (r(width - 106.0, 62.0, width - 24.0, 90.0), YandexHit::Disconnect),
        ];
        if !ui.search_hidden {
            controls.push((r(316.0, 62.0, 394.0, 90.0), YandexHit::Search));
        }
        if !ui.ratings_hidden {
            controls.push((r(182.0, 62.0, 310.0, 90.0), YandexHit::Library));
            if ui.state.current_id.is_some() {
                controls.push((r(100.0, 144.0, 152.0, 176.0), YandexHit::Rate(Rating::Like)));
                controls.push((r(156.0, 144.0, 208.0, 176.0), YandexHit::Rate(Rating::Dislike)));
            }
        }
        if ui.state.active {
            controls.push((r(width - 132.0, 310.0, width - 24.0, 344.0), YandexHit::Stop));
            controls.push((r(width - 238.0, 62.0, width - 118.0, 90.0), YandexHit::Home));
            controls.push((wave_seek_rect(pill, s), YandexHit::Seek));
        }
        if !ui.state.error.is_empty() {
            controls.push((r(width - 116.0, 350.0, width - 24.0, 376.0), YandexHit::Retry));
        }
        if ui.search_open {
            controls.push((wave_search_field(pill, s).0, YandexHit::SearchInput));
            for (i, _) in ui.search_rows().iter().enumerate() {
                let top = 149.0 + i as f32 * 50.0;
                controls.push((r(244.0, top, width - 24.0, top + 42.0), YandexHit::PlaySearch(i)));
            }
            if let Some(result) = ui.search_result() {
                if ui.search_page > 0 {
                    controls.push((r(244.0, 310.0, 278.0, 344.0), YandexHit::SearchPage(-1)));
                }
                if (ui.search_page + 1) * SEARCH_ROWS < result.tracks.len() {
                    controls.push((r(369.0, 310.0, 403.0, 344.0), YandexHit::SearchPage(1)));
                }
            }
        } else if ui.show_library && !ui.ratings_hidden {
            if let Some(library) = &ui.state.library {
                for (i, _) in library.tracks.iter().enumerate() {
                    let top = 105.0 + i as f32 * 47.0;
                    controls.push((r(244.0, top, width - 24.0, top + 42.0), YandexHit::PlayLiked(i)));
                }
                if library.page > 0 {
                    controls.push((r(244.0, 310.0, 278.0, 344.0), YandexHit::LibraryPage(-1)));
                }
                if (library.page + 1) * LIBRARY_PAGE < library.total {
                    controls.push((r(369.0, 310.0, 403.0, 344.0), YandexHit::LibraryPage(1)));
                }
            }
        } else {
            controls.push((r(244.0, 310.0, width - 144.0, 344.0), YandexHit::Autoplay));
            for (i, _) in ui.state.groups.iter().take(4).enumerate() {
                let top = 107.0 + i as f32 * 47.0;
                controls.push((r(244.0, top, width - 57.0, top + 40.0), YandexHit::Filter(i, -1)));
                controls.push((r(width - 55.0, top, width - 24.0, top + 40.0), YandexHit::Filter(i, 1)));
            }
        }
        controls
    }
}

pub fn hit_yandex(pill: Rect, scale: f32, ui: &YandexUi, x: f32, y: f32) -> Option<YandexHit> {
    if ui.state.busy {
        return None;
    }
    yandex_controls(pill, scale, ui).into_iter().find(|(r, _)| inside(*r, x, y)).map(|(_, h)| h)
}

pub fn yandex_tip(pill: Rect, scale: f32, ui: &YandexUi, x: f32, y: f32) -> Option<(String, Rect)> {
    let (rect, hit) = yandex_controls(pill, scale, ui).into_iter().find(|(rect, _)| inside(*rect, x, y))?;
    let label = match hit {
        YandexHit::Rate(Rating::Like) if ui.state.rating == Rating::Like => "Remove like",
        YandexHit::Rate(Rating::Like) => "Like",
        YandexHit::Rate(Rating::Dislike) if ui.state.rating == Rating::Dislike => "Remove dislike",
        YandexHit::Rate(Rating::Dislike) => "Dislike and skip",
        YandexHit::Volume => "Yandex Music volume",
        _ => return None,
    };
    Some((label.into(), rect))
}

pub fn paint_yandex(
    p: &Painter,
    pill: Rect,
    s: f32,
    ui: &YandexUi,
    (editing, caret): (bool, bool),
    seek: Option<f32>,
    progress: Option<(f32, f32)>,
) -> Result<()> {
    let (x, y, w, _) = pill;
    let width = w / s;
    let r = |a: f32, b: f32, c: f32, d: f32| (x + a * s, y + b * s, x + c * s, y + d * s);
    let colours = ui.colours();
    if !ui.state.account.is_empty() {
        p.fill_round_gradient(
            r(12.0, 54.0, width - 12.0, 344.0),
            22.0 * s,
            wave_colour(colours.background, 1.0),
            rgba(0.0, 0.0, 0.0, 0.0),
        )?;
    }
    let text = |value: &str, rect: Rect, size: f32, alpha: f32| {
        p.text(
            value,
            rect,
            size * s,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            wave_colour(colours.text, alpha),
        )
    };
    p.text(
        if ui.show_library && !ui.ratings_hidden { "Liked tracks" } else { "My Wave" },
        r(24.0, 62.0, 179.0, 90.0),
        20.0 * s,
        DWRITE_FONT_WEIGHT_SEMI_BOLD,
        DWRITE_TEXT_ALIGNMENT_LEADING,
        WHITE,
    )?;
    if ui.state.account.is_empty() {
        p.fill_round_rect(r(24.0, 104.0, width - 24.0, 224.0), 18.0 * s, acc(0.12))?;
        if ui.state.code.is_empty() {
            p.glyph('\u{E189}', r(40.0, 124.0, 98.0, 184.0), 34.0 * s, acc(1.0))?;
            text("Yandex Music inside your island", r(114.0, 121.0, width - 44.0, 151.0), 16.0, 1.0)?;
            p.text_wrapped_trimmed(
                &crate::ui::i18n::tr("Sign in in your browser. Your password stays with Yandex."),
                r(114.0, 157.0, width - 44.0, 207.0),
                13.0 * s,
                DWRITE_FONT_WEIGHT_NORMAL,
                rgba(1.0, 1.0, 1.0, 0.7),
            )?;
        } else {
            text("Enter this code at ya.ru/device", r(44.0, 117.0, width - 44.0, 143.0), 13.0, 0.75)?;
            p.text(
                &ui.state.code,
                r(44.0, 147.0, width - 44.0, 192.0),
                30.0 * s,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_TEXT_ALIGNMENT_CENTER,
                WHITE,
            )?;
            text("Waiting for confirmation in your browser…", r(44.0, 192.0, width - 44.0, 218.0), 12.0, 0.7)?;
        }
        p.text_wrapped_trimmed(
            &crate::ui::i18n::tr("Requires Yandex Music access. The connection uses an unofficial API."),
            r(32.0, 294.0, width - 32.0, 334.0),
            12.0 * s,
            DWRITE_FONT_WEIGHT_NORMAL,
            rgba(1.0, 1.0, 1.0, 0.6),
        )?;
    } else {
        p.fill_round_gradient(
            r(24.0, 104.0, 220.0, 236.0),
            18.0 * s,
            wave_colour(colours.card, 1.0),
            wave_colour(colours.background, 1.0),
        )?;
        let cover = r(36.0, 116.0, 92.0, 172.0);
        if let Some(image) = &ui.state.media.cover {
            p.image(cover, 12.0 * s, image.size, &image.bgra)?;
        } else {
            p.fill_round_rect(cover, 12.0 * s, acc(0.22))?;
            p.glyph('\u{E189}', cover, 27.0 * s, acc(1.0))?;
        }
        text("Yandex Music", r(103.0, 119.0, 208.0, 142.0), 11.5, 0.65)?;
        if ui.ratings_hidden || ui.state.current_id.is_none() {
            text(&ui.state.account, r(103.0, 145.0, 208.0, 169.0), 12.0, 0.9)?;
        }
        text(
            if ui.state.media.title.is_empty() { "Made for you" } else { &ui.state.media.title },
            r(36.0, 181.0, 208.0, 207.0),
            15.0,
            1.0,
        )?;
        text(
            if ui.state.media.artist.is_empty() { "Music for this moment" } else { &ui.state.media.artist },
            r(36.0, 208.0, 208.0, 230.0),
            11.5,
            0.65,
        )?;
        if ui.search_open {
            let (frame, inner) = wave_search_field(pill, s);
            paint_line_edit(p, frame, inner, &ui.search_input, false, "Track title or artist", editing, caret, s)?;
            if let Some(result) = ui.search_result() {
                if let Some(error) = result.error {
                    text(error, r(244.0, 152.0, width - 24.0, 185.0), 12.0, 0.9)?;
                    text("Press Enter to try again", r(244.0, 190.0, width - 24.0, 222.0), 12.0, 0.7)?;
                } else if result.tracks.is_empty() {
                    text(
                        "No tracks found. Try another title or artist.",
                        r(244.0, 152.0, width - 24.0, 190.0),
                        13.0,
                        0.8,
                    )?;
                } else {
                    p.text(
                        &format!("{} / {}", ui.search_page + 1, result.tracks.len().div_ceil(SEARCH_ROWS)),
                        r(278.0, 310.0, 369.0, 344.0),
                        12.0 * s,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_TEXT_ALIGNMENT_CENTER,
                        wave_colour(colours.text, 0.75),
                    )?;
                }
            } else if !ui.search_query().is_empty() {
                text("Searching Yandex Music…", r(244.0, 152.0, width - 24.0, 190.0), 13.0, 0.8)?;
            }
        } else if ui.show_library && !ui.ratings_hidden {
            if ui.state.busy && ui.state.library.is_none() {
                text("Loading liked tracks…", r(244.0, 112.0, width - 24.0, 146.0), 13.0, 0.8)?;
            } else if let Some(library) = &ui.state.library {
                if library.tracks.is_empty() {
                    let label =
                        if library.total == 0 { "No liked tracks yet" } else { "No available tracks on this page" };
                    text(label, r(244.0, 112.0, width - 24.0, 146.0), 13.0, 0.8)?;
                }
                if library.total > 0 {
                    p.text(
                        &format!("{} / {}", library.page + 1, library.total.div_ceil(LIBRARY_PAGE)),
                        r(278.0, 310.0, 369.0, 344.0),
                        12.0 * s,
                        DWRITE_FONT_WEIGHT_NORMAL,
                        DWRITE_TEXT_ALIGNMENT_CENTER,
                        wave_colour(colours.text, 0.75),
                    )?;
                }
            }
        } else if ui.state.groups.is_empty() {
            p.text_wrapped_trimmed(
                &crate::ui::i18n::tr("Wave settings are unavailable. Press Try again to reload."),
                r(244.0, 124.0, width - 24.0, 220.0),
                13.0 * s,
                DWRITE_FONT_WEIGHT_NORMAL,
                rgba(1.0, 1.0, 1.0, 0.7),
            )?;
        }
    }
    for (rect, hit) in yandex_controls(pill, s, ui) {
        if hit == YandexHit::SearchInput {
            continue;
        }
        if hit == YandexHit::Volume {
            let level = ui.volume_preview.unwrap_or(ui.state.config.volume);
            text("Music volume", r(30.0, 307.0, 218.0, 322.0), 11.0, 0.7)?;
            let (left, right, y) = (x + 30.0 * s, x + 180.0 * s, y + 333.0 * s);
            let thumb = left + (right - left) * f32::from(level) / 100.0;
            p.fill_round_rect((left, y - 2.0 * s, right, y + 2.0 * s), 2.0 * s, rgba(1.0, 1.0, 1.0, 0.2))?;
            if thumb > left {
                p.fill_round_rect((left, y - 2.0 * s, thumb, y + 2.0 * s), 2.0 * s, wave_colour(colours.accent, 1.0))?;
            }
            p.fill_ellipse(thumb, y, 4.5 * s, 4.5 * s, wave_colour(colours.text, 1.0))?;
            text(&format!("{level}%"), r(187.0, 322.0, 224.0, 345.0), 11.0, 0.9)?;
            continue;
        }
        if hit == YandexHit::Seek {
            let (pos, len) = progress.unwrap_or_default();
            let pos = seek.map_or(pos, |f| f * len);
            let fraction = if len > 0.0 { (pos / len).clamp(0.0, 1.0) } else { 0.0 };
            let y = rect.1 + 3.0 * s;
            p.fill_round_rect((rect.0, y, rect.2, y + 3.0 * s), 1.5 * s, rgba(1.0, 1.0, 1.0, 0.16))?;
            if fraction > 0.0 {
                p.fill_round_rect(
                    (rect.0, y, rect.0 + (rect.2 - rect.0) * fraction, y + 3.0 * s),
                    1.5 * s,
                    wave_colour(colours.accent, 1.0),
                )?;
            }
            if len > 0.0 && (seek.is_some() || ui.hover == Some(YandexHit::Seek)) {
                let x = rect.0 + (rect.2 - rect.0) * fraction;
                p.fill_ellipse(x, y + 1.5 * s, 4.0 * s, 4.0 * s, wave_colour(colours.text, 1.0))?;
            }
            text(&mmss(pos), (rect.0, y + 4.0 * s, rect.0 + 60.0 * s, rect.3), 10.0, 0.7)?;
            p.text(
                &format!("-{}", mmss(len - pos)),
                (rect.2 - 60.0 * s, y + 4.0 * s, rect.2, rect.3),
                10.0 * s,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_TEXT_ALIGNMENT_TRAILING,
                rgba(1.0, 1.0, 1.0, 0.7),
            )?;
            continue;
        }
        let hover = !ui.state.busy && ui.hover == Some(hit);
        let selected = match hit {
            YandexHit::Rate(rating) => ui.state.rating == rating,
            YandexHit::PlaySearch(i) => ui.state.current_id.as_ref() == Some(&ui.search_rows()[i].id),
            YandexHit::PlayLiked(i) => {
                ui.state.library.as_ref().is_some_and(|page| ui.state.current_id.as_ref() == Some(&page.tracks[i].id))
            }
            _ => false,
        };
        let primary = selected
            || matches!(hit, YandexHit::Search) && ui.search_open
            || matches!(hit, YandexHit::PlaySearch(i) if ui.search_selected == Some(i))
            || matches!(hit, YandexHit::Connect | YandexHit::Toggle | YandexHit::Browser);
        p.fill_round_rect(
            rect,
            11.0 * s,
            if primary {
                wave_colour(colours.accent, if hover { 0.4 } else { 0.28 })
            } else {
                rgba(1.0, 1.0, 1.0, if hover { 0.12 } else { 0.06 })
            },
        )?;
        let label = match hit {
            YandexHit::Home => "Open main player",
            YandexHit::Seek | YandexHit::Volume | YandexHit::SearchInput => unreachable!(),
            YandexHit::Search => "Search tracks",
            YandexHit::Library => {
                if ui.show_library {
                    "My Wave"
                } else {
                    "Liked tracks"
                }
            }
            YandexHit::LibraryPage(dir) | YandexHit::SearchPage(dir) => {
                p.glyph(if dir < 0 { '\u{E76B}' } else { '\u{E76C}' }, rect, 11.0 * s, WHITE)?;
                continue;
            }
            YandexHit::Rate(rating) => {
                p.glyph(if rating == Rating::Like { '\u{E8E1}' } else { '\u{E8E0}' }, rect, 15.0 * s, WHITE)?;
                continue;
            }
            YandexHit::PlayLiked(i) | YandexHit::PlaySearch(i) => {
                let track = if matches!(hit, YandexHit::PlaySearch(_)) {
                    &ui.search_rows()[i]
                } else {
                    &ui.state.library.as_ref().unwrap().tracks[i]
                };
                let selected = ui.state.current_id.as_ref() == Some(&track.id);
                p.glyph(
                    if selected && ui.state.media.playing { '\u{E769}' } else { '\u{E768}' },
                    (rect.0 + 4.0 * s, rect.1, rect.0 + 38.0 * s, rect.3),
                    13.0 * s,
                    WHITE,
                )?;
                text(
                    &track.title,
                    (rect.0 + 40.0 * s, rect.1 + 3.0 * s, rect.2 - 10.0 * s, rect.1 + 23.0 * s),
                    13.0,
                    1.0,
                )?;
                text(
                    &track.artist,
                    (rect.0 + 40.0 * s, rect.1 + 22.0 * s, rect.2 - 10.0 * s, rect.3 - 3.0 * s),
                    11.0,
                    0.65,
                )?;
                continue;
            }
            YandexHit::Connect => {
                if ui.state.busy {
                    "Connecting…"
                } else {
                    "Connect Yandex Music"
                }
            }
            YandexHit::Browser => "Open Yandex",
            YandexHit::Copy => {
                if ui.copied {
                    "Copied"
                } else {
                    "Copy code"
                }
            }
            YandexHit::Cancel => "Cancel",
            YandexHit::Toggle => {
                if ui.state.busy {
                    "Loading music…"
                } else if ui.state.media.playing {
                    "Pause"
                } else if ui.state.active {
                    "Resume"
                } else {
                    "Start wave"
                }
            }
            YandexHit::Disconnect => "Sign out",
            YandexHit::Stop => "Stop wave",
            YandexHit::Retry => {
                if ui.state.busy {
                    "Connecting…"
                } else {
                    "Try again"
                }
            }
            YandexHit::Autoplay => {
                p.glyph(
                    if ui.state.config.autoplay { '\u{E73E}' } else { '\u{E739}' },
                    (rect.0 + 8.0 * s, rect.1, rect.0 + 36.0 * s, rect.3),
                    16.0 * s,
                    if ui.state.config.autoplay { acc(1.0) } else { WHITE },
                )?;
                text("Start with Island", (rect.0 + 42.0 * s, rect.1, rect.2 - 8.0 * s, rect.3), 11.5, 0.8)?;
                continue;
            }
            YandexHit::Previous | YandexHit::Next => {
                p.glyph(if hit == YandexHit::Previous { '\u{E892}' } else { '\u{E893}' }, rect, 18.0 * s, WHITE)?;
                continue;
            }
            YandexHit::Filter(i, dir) => {
                if dir == 1 {
                    p.glyph('\u{E76C}', rect, 10.0 * s, WHITE)?;
                } else {
                    let group = &ui.state.groups[i];
                    let chosen = ui
                        .state
                        .config
                        .selection
                        .get(&group.key)
                        .and_then(|seed| group.items.iter().find(|i| &i.1 == seed))
                        .map_or("Any", |i| i.0.as_str());
                    p.glyph(
                        '\u{E76B}',
                        (rect.0, rect.1, rect.0 + 28.0 * s, rect.3),
                        10.0 * s,
                        rgba(1.0, 1.0, 1.0, 0.5),
                    )?;
                    text(
                        &group.name,
                        (rect.0 + 32.0 * s, rect.1 + 2.0 * s, rect.2 - 8.0 * s, rect.1 + 17.0 * s),
                        10.5,
                        0.6,
                    )?;
                    text(
                        chosen,
                        (rect.0 + 32.0 * s, rect.1 + 16.0 * s, rect.2 - 8.0 * s, rect.3 - 2.0 * s),
                        12.5,
                        1.0,
                    )?;
                }
                continue;
            }
        };
        p.text(label, rect, 12.0 * s, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER, WHITE)?;
    }
    if !ui.state.error.is_empty() {
        p.text_wrapped_trimmed(
            &crate::ui::i18n::tr(&ui.state.error),
            r(24.0, 348.0, width - 124.0, 380.0),
            11.0 * s,
            DWRITE_FONT_WEIGHT_NORMAL,
            rgba(1.0, 0.65, 0.55, 1.0),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn busy_wave_blocks_duplicate_actions_and_code_has_copy_action() {
        let mut ui = YandexUi::default();
        let pill = (0.0, 0.0, 640.0, 384.0);
        ui.state.busy = true;
        assert!(hit_yandex(pill, 1.0, &ui, 100.0, 260.0).is_none());
        ui.state.busy = false;
        ui.state.code = "123456".into();
        assert!(yandex_controls(pill, 1.0, &ui).iter().any(|(_, h)| *h == YandexHit::Copy));
    }

    #[test]
    fn wave_seek_and_navigation_do_not_overlap_other_controls_at_any_scale() {
        let mut ui = YandexUi::default();
        ui.state.account = "Fixture".into();
        ui.state.active = true;
        for s in [0.8, 1.0, 1.25, 1.5, 2.0] {
            let pill = (13.0, 17.0, 640.0 * s, 384.0 * s);
            for (rect, hit) in yandex_controls(pill, s, &ui) {
                assert_eq!(hit_yandex(pill, s, &ui, (rect.0 + rect.2) / 2.0, (rect.1 + rect.3) / 2.0), Some(hit));
            }
            let seek = wave_seek_rect(pill, s);
            assert_eq!(wave_fraction(pill, s, seek.0 - 10.0), 0.0);
            assert_eq!(wave_fraction(pill, s, seek.2 + 10.0), 1.0);
        }
    }

    #[test]
    fn search_never_exposes_results_for_a_different_query() {
        let mut ui = YandexUi::default();
        ui.state.account = "Fixture".into();
        ui.state.active = true;
        ui.search_open = true;
        ui.search_input.set_text("Небо");
        ui.state.search = Some(crate::services::yandex::SearchResult {
            request: 1,
            query: "Небо".into(),
            error: None,
            tracks: vec![crate::services::yandex::Track {
                id: "1".into(),
                title: "Небо".into(),
                artist: "Группа".into(),
                cover: String::new(),
                batch: String::new(),
                session: String::new(),
                palette: Default::default(),
            }],
        });
        for scale in [0.8, 1.0, 1.5, 2.0] {
            let pill = (10.0, 12.0, 640.0 * scale, 384.0 * scale);
            for (rect, hit) in yandex_controls(pill, scale, &ui) {
                assert_eq!(hit_yandex(pill, scale, &ui, (rect.0 + rect.2) / 2.0, (rect.1 + rect.3) / 2.0), Some(hit));
            }
        }
        assert_eq!(ui.search_rows().len(), 1);
        ui.search_input.set_text("Облака");
        assert!(ui.search_rows().is_empty());
    }

    #[test]
    fn palette_updates_can_disable_motion_and_interrupted_transitions_start_from_current_colour() {
        let mut ui = YandexUi::default();
        let state = Snapshot {
            palette: crate::services::yandex::palette::Palette { average: Some((250, 80, 30)), ..Default::default() },
            ..Default::default()
        };
        ui.accept(state.clone(), false);
        assert!(ui.colour_transition.is_none());
        let old = ui.colours();
        let next = Snapshot {
            palette: crate::services::yandex::palette::Palette { average: Some((20, 150, 250)), ..Default::default() },
            ..Default::default()
        };
        ui.accept(next, true);
        assert_eq!(ui.colour_transition.unwrap().0, old);
        ui.accept(state, false);
        assert_eq!(ui.colours(), old);
    }
}
