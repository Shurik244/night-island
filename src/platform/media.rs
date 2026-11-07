use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat, BitmapTransform, ColorManagementMode, ExifOrientationMode,
};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::{Buffer, DataReader};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_MEDIA: u32 = WM_APP + 7;

const POLL: Duration = Duration::from_millis(1000);
const MAX_TEXT_CHARS: usize = 200;

pub struct Cover {
    id: u64,
    pub size: u32,
    pub bgra: Vec<u8>,
    pub full_w: u32,
    pub full_h: u32,
    pub full: Vec<u8>,
}

impl PartialEq for Cover {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Cover {}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Media {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub accent: Option<(u8, u8, u8)>,
    pub cover: Option<Arc<Cover>>,
    pub source: String,
    pub players: u8,
}

static TIMELINE: Mutex<Option<(f32, f32, Instant, bool)>> = Mutex::new(None);
static TIMELINE_START: Mutex<f32> = Mutex::new(0.0);

#[cfg(test)]
pub fn debug_progress(pos: f32, length: f32) {
    *TIMELINE.lock().unwrap() = Some((pos, length, Instant::now(), false));
}

pub fn progress() -> Option<(f32, f32)> {
    if selected() == Source::Wave {
        return crate::services::yandex::progress();
    }
    let (pos, len, at, playing) = (*TIMELINE.lock().ok()?)?;
    if len <= 0.0 {
        return None;
    }
    let now = if playing { pos + at.elapsed().as_secs_f32() } else { pos };
    Some((now.clamp(0.0, len), len))
}

#[derive(Clone, Copy)]
pub enum Control {
    Previous,
    PlayPause,
    Next,
}

#[derive(Clone, Default, PartialEq, Eq)]
struct External {
    id: String,
    media: Media,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
enum Source {
    #[default]
    None,
    Wave,
    External(String),
}

static SNAPSHOT: Mutex<Option<External>> = Mutex::new(None);
static SELECTED: Mutex<Source> = Mutex::new(Source::None);
static PREFER_PLAYING: AtomicBool = AtomicBool::new(false);

pub fn configure_priority(enabled: bool) {
    PREFER_PLAYING.store(enabled, Ordering::Relaxed);
}

fn selected() -> Source {
    SELECTED.lock().map(|s| s.clone()).unwrap_or_default()
}

fn choose_source(prefer: bool, wave: Option<&Media>, external: &External, previous: &Source) -> Source {
    let keep_external = matches!(previous, Source::External(id) if *id == external.id);
    if let Some(wave) = wave
        && (!prefer || wave.playing || external.id.is_empty() || (!external.media.playing && !keep_external))
    {
        return Source::Wave;
    }
    if external.id.is_empty() { Source::None } else { Source::External(external.id.clone()) }
}
type CoverInfo = (Option<(u8, u8, u8)>, Option<Arc<Cover>>);
type CoverCache = ((String, String), Option<(u8, u8, u8)>, Option<Arc<Cover>>);
static COVER: Mutex<Option<CoverCache>> = Mutex::new(None);
static COVER_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn snapshot() -> Media {
    let wave = crate::services::yandex::media();
    let external = SNAPSHOT.lock().ok().and_then(|g| g.clone()).unwrap_or_default();
    let source = choose_source(PREFER_PLAYING.load(Ordering::Relaxed), wave.as_ref(), &external, &selected());
    if let Ok(mut chosen) = SELECTED.lock() {
        *chosen = source.clone();
    }
    match source {
        Source::Wave => wave.unwrap_or_default(),
        Source::External(_) => external.media,
        Source::None => Media::default(),
    }
}

fn clip(s: windows::core::HSTRING) -> String {
    s.to_string().chars().take(MAX_TEXT_CHARS).collect()
}

type Session = windows::Media::Control::GlobalSystemMediaTransportControlsSession;

static CHOSEN: Mutex<Option<String>> = Mutex::new(None);
static WINDOW: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

fn app_id(session: &Session) -> String {
    session.SourceAppUserModelId().map(|s| s.to_string()).unwrap_or_default()
}

fn pick(manager: &Manager) -> Option<Session> {
    let chosen = CHOSEN.lock().ok().and_then(|c| c.clone());
    let current = manager.GetCurrentSession().ok().map(|s| app_id(&s));
    let sessions: Vec<_> = manager.GetSessions().ok()?.into_iter().filter(|s| !own_session(&app_id(s))).collect();
    let candidates: Vec<_> = sessions.iter().map(|s| (app_id(s), is_playing(s))).collect();
    let previous = match selected() {
        Source::External(id) => Some(id),
        _ => None,
    };
    let index = choose_external(
        &candidates,
        chosen.as_deref(),
        current.as_deref(),
        previous.as_deref(),
        PREFER_PLAYING.load(Ordering::Relaxed),
    )?;
    sessions.into_iter().nth(index)
}

fn is_playing(session: &Session) -> bool {
    session.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).is_ok_and(|s| s == Status::Playing)
}

pub(crate) fn own_session(id: &str) -> bool {
    std::env::current_exe().ok().is_some_and(|path| {
        id.eq_ignore_ascii_case(&path.to_string_lossy())
            || path.file_name().is_some_and(|name| id.eq_ignore_ascii_case(&name.to_string_lossy()))
            || path.file_stem().is_some_and(|stem| id.eq_ignore_ascii_case(&stem.to_string_lossy()))
    })
}

fn choose_external(
    sessions: &[(String, bool)],
    chosen: Option<&str>,
    current: Option<&str>,
    previous: Option<&str>,
    prefer: bool,
) -> Option<usize> {
    let find = |id: Option<&str>| id.and_then(|id| sessions.iter().position(|s| s.0 == id));
    let manual = find(chosen);
    let current = find(current);
    let previous = find(previous);
    if prefer {
        for i in [manual, current, previous].into_iter().flatten() {
            if sessions[i].1 {
                return Some(i);
            }
        }
        if let Some(i) = sessions.iter().position(|s| s.1) {
            return Some(i);
        }
        if previous.is_some() {
            return previous;
        }
    }
    manual.or(current).or(previous).or_else(|| (!sessions.is_empty()).then_some(0))
}

fn target_session(manager: &Manager, source: &Source) -> Option<Session> {
    let Source::External(id) = source else { return None };
    manager.GetSessions().ok()?.into_iter().find(|s| app_id(s) == *id)
}

struct Report {
    pos: f32,
    raw: (f32, i64),
    len: f32,
    playing: bool,
}

#[derive(Default)]
struct Tracker {
    track: (String, String),
    raw: Option<(f32, i64)>,
    pending: Option<((f32, i64), f32, Instant)>,
}

static TRACKER: Mutex<Option<Tracker>> = Mutex::new(None);

fn update_timeline(track: &(String, String), r: Report) {
    let (Ok(mut tracker), Ok(mut line)) = (TRACKER.lock(), TIMELINE.lock()) else { return };
    let t = tracker.get_or_insert_with(Tracker::default);
    let now = Instant::now();
    let current = line.as_ref().map(|&(pos, len, at, playing)| {
        let moved = if playing { pos + at.elapsed().as_secs_f32() } else { pos };
        (moved.clamp(0.0, len.max(0.0)), len)
    });
    let new_track = t.track != *track || current.is_none_or(|(_, len)| (len - r.len).abs() > 1.5);
    let accept = |line: &mut Option<(f32, f32, Instant, bool)>, pos: f32| *line = Some((pos, r.len, now, r.playing));
    if new_track {
        t.track = track.clone();
        t.pending = None;
        t.raw = Some(r.raw);
        return accept(&mut line, r.pos);
    }
    let Some((estimate, _)) = current else { return };
    let news = t.raw != Some(r.raw);
    t.raw = Some(r.raw);
    let was_playing = line.as_ref().is_some_and(|l| l.3);
    if !news {
        if let Some((raw, pos, at)) = t.pending.take()
            && raw == r.raw
            && r.playing
        {
            return accept(&mut line, pos + at.elapsed().as_secs_f32());
        }
        if was_playing != r.playing {
            accept(&mut line, estimate);
        }
        return;
    }
    let reset = r.playing && r.pos < 1.5 && estimate > 15.0;
    if !reset {
        t.pending = None;
        return accept(&mut line, r.pos);
    }
    t.pending = Some((r.raw, r.pos, now));
}

fn fetch(manager: &Manager) -> External {
    let Some(session) = pick(manager) else {
        clear_timeline();
        return External::default();
    };
    let players =
        manager.GetSessions().map_or(0, |s| s.into_iter().filter(|s| !own_session(&app_id(s))).count()).min(255) as u8;
    let (mut title, mut artist) = (String::new(), String::new());
    if let Ok(props) = session.TryGetMediaPropertiesAsync().and_then(|op| op.join()) {
        title = props.Title().map(clip).unwrap_or_default();
        artist = props.Artist().map(clip).unwrap_or_default();
    }
    let playing = is_playing(&session);

    let key = (title.clone(), artist.clone());
    let cached =
        COVER.lock().ok().and_then(|c| c.as_ref().filter(|(k, _, _)| *k == key).map(|(_, a, i)| (*a, i.clone())));
    let (accent, cover) = match cached {
        Some(v) => v,
        None => {
            let (a, img) = cover_info(&session);
            if let Ok(mut c) = COVER.lock() {
                *c = Some((key, a, img.clone()));
            }
            (a, img)
        }
    };
    let source = session
        .SourceAppUserModelId()
        .map(|s| {
            let s = s.to_string();
            let stem = s.rsplit(['\\', '!', '.']).find(|p| !p.is_empty() && *p != "exe").unwrap_or(&s).to_string();
            stem.to_uppercase()
        })
        .unwrap_or_default();
    if let Ok(t) = session.GetTimelineProperties() {
        let secs = |d: Option<windows::Foundation::TimeSpan>| d.map_or(0.0, |t| t.Duration as f32 / 1e7);
        let (start, end, pos) = (secs(t.StartTime().ok()), secs(t.EndTime().ok()), secs(t.Position().ok()));
        let stamp = t.LastUpdatedTime().ok();
        let age = if playing { stamp.map_or(0.0, report_age) } else { 0.0 };
        let report = Report {
            pos: pos - start + age,
            raw: (pos, stamp.map_or(0, |s| s.UniversalTime)),
            len: end - start,
            playing,
        };
        update_timeline(&(format!("{}\n{title}", app_id(&session)), artist.clone()), report);
        if let Ok(mut s) = TIMELINE_START.lock() {
            *s = start;
        }
    } else {
        clear_timeline();
    }
    External { id: app_id(&session), media: Media { title, artist, playing, accent, cover, source, players } }
}

fn clear_timeline() {
    if let Ok(mut line) = TIMELINE.lock() {
        *line = None;
    }
    if let Ok(mut start) = TIMELINE_START.lock() {
        *start = 0.0;
    }
}

fn refresh(manager: &Manager) -> bool {
    let now = fetch(manager);
    let Ok(mut guard) = SNAPSHOT.lock() else { return false };
    let changed = guard.as_ref() != Some(&now);
    *guard = Some(now);
    if changed {
        let raw = WINDOW.load(std::sync::atomic::Ordering::Relaxed);
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut c_void)), WM_MEDIA, WPARAM(0), LPARAM(0));
        }
    }
    true
}

pub fn next_player() {
    if selected() == Source::Wave {
        crate::services::yandex::send(crate::services::yandex::Command::Stop);
        return;
    }
    std::thread::spawn(|| {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let Ok(manager) = Manager::RequestAsync().and_then(|op| op.join()) else { return };
        let Ok(sessions) = manager.GetSessions() else { return };
        let ids: Vec<String> = sessions.into_iter().map(|s| app_id(&s)).filter(|id| !own_session(id)).collect();
        if ids.len() < 2 {
            return;
        }
        let current = pick(&manager).map(|s| app_id(&s));
        let at = current.and_then(|c| ids.iter().position(|id| *id == c)).unwrap_or(0);
        if let Ok(mut chosen) = CHOSEN.lock() {
            *chosen = Some(ids[(at + 1) % ids.len()].clone());
        }
        refresh(&manager);
    });
}

fn report_age(stamp: windows::Foundation::DateTime) -> f32 {
    const EPOCH_GAP: f64 = 11_644_473_600.0;
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else { return 0.0 };
    let age = now.as_secs_f64() + EPOCH_GAP - stamp.UniversalTime as f64 / 1e7;
    if (0.0..=21_600.0).contains(&age) { age as f32 } else { 0.0 }
}

fn cover_info(session: &windows::Media::Control::GlobalSystemMediaTransportControlsSession) -> CoverInfo {
    const SIZE: u32 = 96;
    const LONG_MAX: u32 = 384;
    let pixels = (|| {
        let props = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
        let stream = props.Thumbnail().ok()?.OpenReadAsync().ok()?.join().ok()?;
        let decoder = BitmapDecoder::CreateAsync(&stream).ok()?.join().ok()?;
        let transform = BitmapTransform::new().ok()?;
        let (sw, sh) = (decoder.PixelWidth().ok()?.max(1), decoder.PixelHeight().ok()?.max(1));
        let k = (SIZE as f32 / sw.min(sh) as f32).min(LONG_MAX as f32 / sw.max(sh) as f32).clamp(0.0, 1.0);
        let (tw, th) = if sw.min(sh) <= SIZE {
            (sw, sh)
        } else {
            (((sw as f32 * k).round() as u32).max(1), ((sh as f32 * k).round() as u32).max(1))
        };
        transform.SetScaledWidth(tw).ok()?;
        transform.SetScaledHeight(th).ok()?;
        let bitmap = decoder
            .GetSoftwareBitmapTransformedAsync(
                BitmapPixelFormat::Bgra8,
                BitmapAlphaMode::Premultiplied,
                &transform,
                ExifOrientationMode::IgnoreExifOrientation,
                ColorManagementMode::DoNotColorManage,
            )
            .ok()?
            .join()
            .ok()?;
        let (w, h) = (bitmap.PixelWidth().ok()? as usize, bitmap.PixelHeight().ok()? as usize);
        let buffer = Buffer::Create((w * h * 4) as u32).ok()?;
        bitmap.CopyToBuffer(&buffer).ok()?;
        let mut pixels = vec![0u8; w * h * 4];
        DataReader::FromBuffer(&buffer).ok()?.ReadBytes(&mut pixels).ok()?;
        Some((pixels, w as u32, h as u32))
    })();
    let Some((pixels, w, h)) = pixels else { return (None, None) };
    let accent = Some(dominant(&pixels));
    let side = w.min(h);
    let mut square = Vec::with_capacity((side * side * 4) as usize);
    let (ox, oy) = ((w - side) / 2, (h - side) / 2);
    for y in 0..side {
        let start = (((oy + y) * w + ox) * 4) as usize;
        square.extend_from_slice(&pixels[start..start + (side * 4) as usize]);
    }
    let id = COVER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    (accent, Some(Arc::new(Cover { id, size: side, bgra: square, full_w: w, full_h: h, full: pixels })))
}

fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    ((h + 360.0) % 360.0, if max == 0.0 { 0.0 } else { d / max }, max)
}

pub fn cover_url(url: &str) -> Option<Arc<Cover>> {
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};
    let reply = crate::services::http::get_bytes(url, 3_000_000, None)?;
    if reply.status != 200 {
        return None;
    }
    let stream = InMemoryRandomAccessStream::new().ok()?;
    let writer = DataWriter::CreateDataWriter(&stream).ok()?;
    writer.WriteBytes(&reply.body).ok()?;
    writer.StoreAsync().ok()?.join().ok()?;
    stream.Seek(0).ok()?;
    let decoder = BitmapDecoder::CreateAsync(&stream).ok()?.join().ok()?;
    if decoder.PixelWidth().ok()? > 2048 || decoder.PixelHeight().ok()? > 2048 {
        return None;
    }
    let transform = BitmapTransform::new().ok()?;
    transform.SetScaledWidth(300).ok()?;
    transform.SetScaledHeight(300).ok()?;
    let bitmap = decoder
        .GetSoftwareBitmapTransformedAsync(
            BitmapPixelFormat::Bgra8,
            BitmapAlphaMode::Premultiplied,
            &transform,
            ExifOrientationMode::IgnoreExifOrientation,
            ColorManagementMode::DoNotColorManage,
        )
        .ok()?
        .join()
        .ok()?;
    let buffer = Buffer::Create(300 * 300 * 4).ok()?;
    bitmap.CopyToBuffer(&buffer).ok()?;
    let mut bgra = vec![0; 300 * 300 * 4];
    DataReader::FromBuffer(&buffer).ok()?.ReadBytes(&mut bgra).ok()?;
    Some(Arc::new(Cover {
        id: COVER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        size: 300,
        full_w: 300,
        full_h: 300,
        full: bgra.clone(),
        bgra,
    }))
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h.rem_euclid(360.0) / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to8 = |f: f32| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (to8(r), to8(g), to8(b))
}

pub fn gradient(rgb: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let (h, s, v) = rgb_to_hsv(f32::from(rgb.0) / 255.0, f32::from(rgb.1) / 255.0, f32::from(rgb.2) / 255.0);
    hsv_to_rgb(h + 45.0 * t, s, v)
}

pub fn dominant(bgra: &[u8]) -> (u8, u8, u8) {
    const BUCKETS: usize = 12;
    let mut weight = [0.0f32; BUCKETS];
    let mut sum = [(0.0f32, 0.0f32, 0.0f32); BUCKETS];
    let mut avg_sat = 0.0f32;
    let mut n = 0.0f32;
    for px in bgra.chunks_exact(4) {
        let (r, g, b) = (f32::from(px[2]) / 255.0, f32::from(px[1]) / 255.0, f32::from(px[0]) / 255.0);
        let (h, s, v) = rgb_to_hsv(r, g, b);
        avg_sat += s;
        n += 1.0;
        if v < 0.15 {
            continue;
        }
        let i = ((h / 360.0 * BUCKETS as f32) as usize).min(BUCKETS - 1);
        let w = s * s * v;
        weight[i] += w;
        sum[i].0 += h * w;
        sum[i].1 += s * w;
        sum[i].2 += v * w;
    }
    let best = (0..BUCKETS).max_by(|&a, &b| weight[a].total_cmp(&weight[b])).unwrap_or(0);
    if weight[best] < 0.5 || avg_sat / n.max(1.0) < 0.12 {
        return (200, 205, 235);
    }
    let w = weight[best];
    let (h, s, v) = (sum[best].0 / w, sum[best].1 / w, sum[best].2 / w);
    hsv_to_rgb(h, s.clamp(0.45, 0.95), v.max(0.85))
}

pub fn start(hwnd: HWND) {
    WINDOW.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
    std::thread::spawn(move || {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let Ok(manager) = Manager::RequestAsync().and_then(|op| op.join()) else {
            crate::log_error!("media session manager is unavailable");
            return;
        };
        loop {
            if !refresh(&manager) {
                return;
            }
            std::thread::sleep(POLL);
        }
    });
}

#[cfg(any(feature = "demo", test))]
pub fn debug_timeline(pos: f32, len: f32) {
    if let Ok(mut g) = TIMELINE.lock() {
        *g = Some((pos, len, Instant::now(), false));
    }
}

#[cfg(any(feature = "demo", test))]
pub fn debug_cover(side: u32, rgba: &[u8]) -> Option<Arc<Cover>> {
    if rgba.len() < (side * side * 4) as usize {
        return None;
    }
    let mut bgra = rgba[..(side * side * 4) as usize].to_vec();
    for px in bgra.chunks_exact_mut(4) {
        let a = u32::from(px[3]);
        let (r, g, b) = (u32::from(px[0]), u32::from(px[1]), u32::from(px[2]));
        px[0] = (b * a / 255) as u8;
        px[1] = (g * a / 255) as u8;
        px[2] = (r * a / 255) as u8;
    }
    let id = COVER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Some(Arc::new(Cover { id, size: side, full_w: side, full_h: side, full: bgra.clone(), bgra }))
}

pub fn seek(secs: f32) {
    let source = selected();
    if source == Source::Wave {
        crate::services::yandex::send(crate::services::yandex::Command::Seek(secs));
        return;
    }
    let start = TIMELINE_START.lock().map_or(0.0, |s| *s);
    if let Ok(mut g) = TIMELINE.lock()
        && let Some((_, len, _, playing)) = *g
    {
        *g = Some((secs, len, Instant::now(), playing));
    }
    std::thread::spawn(move || {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let Ok(manager) = Manager::RequestAsync().and_then(|op| op.join()) else { return };
        let Some(session) = target_session(&manager, &source) else { return };
        let _ = session.TryChangePlaybackPositionAsync(((secs + start) as f64 * 1e7) as i64).and_then(|op| op.join());
    });
}

pub fn control(action: Control) {
    let source = selected();
    if source == Source::Wave {
        use crate::services::yandex::{self, Command};
        yandex::send(match action {
            Control::Previous => Command::Previous,
            Control::PlayPause => Command::Toggle,
            Control::Next => Command::Next,
        });
        return;
    }
    std::thread::spawn(move || {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let Ok(manager) = Manager::RequestAsync().and_then(|op| op.join()) else { return };
        let Some(session) = target_session(&manager, &source) else { return };
        let _ = match action {
            Control::Previous => session.TrySkipPreviousAsync().and_then(|op| op.join()),
            Control::PlayPause => session.TryTogglePlayPauseAsync().and_then(|op| op.join()),
            Control::Next => session.TrySkipNextAsync().and_then(|op| op.join()),
        };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_follows_playing_browser_and_keeps_its_pause_controls() {
        let mut wave = Media { playing: true, ..Default::default() };
        let mut browser = External { id: "browser".into(), media: Media { playing: true, ..Default::default() } };
        let mut selected = choose_source(true, Some(&wave), &browser, &Source::None);
        assert_eq!(selected, Source::Wave);
        wave.playing = false;
        selected = choose_source(true, Some(&wave), &browser, &selected);
        assert_eq!(selected, Source::External("browser".into()));
        browser.media.playing = false;
        assert_eq!(choose_source(true, Some(&wave), &browser, &selected), selected);
        wave.playing = true;
        assert_eq!(choose_source(true, Some(&wave), &browser, &selected), Source::Wave);
        wave.playing = false;
        assert_eq!(choose_source(true, Some(&wave), &External::default(), &selected), Source::Wave);
    }

    #[test]
    fn disabled_priority_keeps_wave_until_stopped() {
        let wave = Media::default();
        let browser = External { id: "browser".into(), media: Media { playing: true, ..Default::default() } };
        assert_eq!(choose_source(false, Some(&wave), &browser, &Source::None), Source::Wave);
        assert_eq!(choose_source(false, None, &browser, &Source::Wave), Source::External("browser".into()));
        assert_eq!(choose_source(true, Some(&wave), &External::default(), &Source::None), Source::Wave);
        assert_eq!(choose_source(true, None, &External::default(), &Source::Wave), Source::None);
    }

    #[test]
    fn playing_session_beats_stale_current_and_manual_sessions() {
        let mut sessions = vec![("spotify".into(), false), ("browser".into(), true)];
        assert_eq!(choose_external(&sessions, Some("spotify"), Some("spotify"), None, true), Some(1));
        assert_eq!(choose_external(&sessions, Some("spotify"), Some("browser"), None, false), Some(0));
        sessions[1].1 = false;
        assert_eq!(choose_external(&sessions, None, Some("spotify"), Some("browser"), true), Some(1));
        sessions[0].1 = true;
        assert_eq!(choose_external(&sessions, None, Some("browser"), Some("browser"), true), Some(0));
        sessions[1].1 = true;
        assert_eq!(choose_external(&sessions, None, Some("browser"), Some("spotify"), true), Some(1));
        assert_eq!(choose_external(&[], Some("closed"), Some("closed"), None, true), None);
    }

    #[test]
    fn own_media_session_is_not_an_external_player() {
        let path = std::env::current_exe().unwrap();
        assert!(own_session(&path.to_string_lossy()));
        assert!(own_session(&path.to_string_lossy().to_uppercase()));
        assert!(own_session(&path.file_name().unwrap().to_string_lossy()));
        assert!(!own_session("chrome.exe"));
        assert!(!own_session(""));
    }

    fn solid(r: u8, g: u8, b: u8, count: usize) -> Vec<u8> {
        std::iter::repeat_n([b, g, r, 255], count).flatten().collect()
    }

    #[test]
    fn a_bold_colour_beats_grey_filler() {
        let mut px = solid(128, 128, 128, 400);
        px.extend(solid(220, 30, 40, 120));
        let (r, g, b) = dominant(&px);
        assert!(r > 200 && g < 120 && b < 120, "got {r},{g},{b}");
    }

    #[test]
    fn dark_colours_are_lifted_to_be_readable() {
        let (r, g, b) = dominant(&solid(10, 20, 90, 300));
        assert!(b >= 200 && b > r && b > g, "got {r},{g},{b}");
    }

    #[test]
    fn grey_or_black_covers_get_a_neutral() {
        assert_eq!(dominant(&solid(120, 120, 120, 300)), (200, 205, 235));
        assert_eq!(dominant(&solid(0, 0, 0, 300)), (200, 205, 235));
    }

    #[test]
    fn hsv_round_trip() {
        for rgb in [(255u8, 0u8, 0u8), (0, 255, 0), (0, 0, 255), (200, 100, 50)] {
            let (h, s, v) = rgb_to_hsv(f32::from(rgb.0) / 255.0, f32::from(rgb.1) / 255.0, f32::from(rgb.2) / 255.0);
            let back = hsv_to_rgb(h, s, v);
            let close = |a: u8, b: u8| (i32::from(a) - i32::from(b)).abs() <= 1;
            assert!(close(back.0, rgb.0) && close(back.1, rgb.1) && close(back.2, rgb.2), "{rgb:?} -> {back:?}");
        }
    }
}
