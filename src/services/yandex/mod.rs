mod api;
pub mod palette;
mod player;

use crate::platform::media::{self, Media};
use crate::services::integrations;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const LIBRARY_PAGE: usize = 4;

pub const SOURCE: &str = "Night Island · Yandex Music";
pub const WM_YANDEX: u32 = WM_APP + 58;
static SENDER: OnceLock<mpsc::SyncSender<Command>> = OnceLock::new();
static STATE: Mutex<Option<Snapshot>> = Mutex::new(None);
static AUTH_EPOCH: AtomicU64 = AtomicU64::new(0);
static SEARCH_REQUEST: AtomicU64 = AtomicU64::new(0);
static POSITION: Mutex<Option<(f32, f32, Instant, bool)>> = Mutex::new(None);

#[derive(Clone, Default, PartialEq, Eq)]
pub struct Group {
    pub key: String,
    pub name: String,
    pub items: Vec<(String, String)>,
}

#[derive(Clone, Default)]
pub struct Snapshot {
    pub account: String,
    pub has_token: bool,
    pub code: String,
    pub busy: bool,
    pub error: String,
    pub groups: Vec<Group>,
    pub config: Config,
    pub active: bool,
    pub media: Media,
    pub palette: palette::Palette,
    pub current_id: Option<String>,
    pub rating: Rating,
    pub library: Option<LibraryPage>,
    pub search: Option<SearchResult>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rating {
    #[default]
    None,
    Like,
    Dislike,
}

#[derive(Clone)]
pub struct LibraryPage {
    pub page: usize,
    pub total: usize,
    pub tracks: Vec<Track>,
}

#[derive(Clone)]
pub struct SearchResult {
    pub request: u64,
    pub query: String,
    pub tracks: Vec<Track>,
    pub error: Option<&'static str>,
}

fn search_key(text: &str) -> String {
    text.to_lowercase()
        .replace('ё', "е")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn match_track<'a>(tracks: &'a [Track], title: &str, artist: &str) -> Option<&'a Track> {
    let title = search_key(title);
    let artist = search_key(artist);
    let exact = tracks.iter().find(|track| {
        let name = search_key(&track.title);
        if name.is_empty() {
            return false;
        }
        if artist.is_empty() {
            let performer = search_key(&track.artist);
            title == name || title == format!("{name} {performer}") || title == format!("{performer} {name}")
        } else {
            name == title
                && (search_key(&track.artist) == artist
                    || track.artist.split(", ").any(|name| search_key(name) == artist))
        }
    });
    if exact.is_some() || artist.is_empty() || !title.chars().any(|ch| matches!(ch, 'а'..='я' | 'ё')) {
        return exact;
    }

    let spoken_key = |name: &str| crate::tools::texttools::translit(name).replace("dzh", "j").replace('z', "s");
    let spoken = spoken_key(&title);
    let mut matches = tracks.iter().filter(|track| {
        let name = search_key(&track.title);
        name.is_ascii()
            && spoken == spoken_key(&name)
            && track.artist.split(", ").any(|name| search_key(name) == artist)
    });
    let only = matches.next()?;
    matches.next().is_none().then_some(only)
}

struct LibraryPlayback {
    ids: Arc<Vec<String>>,
    next: usize,
}

fn track_key(id: &str) -> &str {
    id.split_once(':').map_or(id, |(track, _)| track)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub selection: BTreeMap<String, String>,
    pub autoplay: bool,
    pub volume: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self { selection: BTreeMap::new(), autoplay: false, volume: 100 }
    }
}

impl Config {
    fn seeds(&self) -> Vec<String> {
        let context = self.selection.get("context").cloned().unwrap_or_else(|| "user:onyourwave".into());
        std::iter::once(context)
            .chain(self.selection.iter().filter(|(group, _)| group.as_str() != "context").map(|(_, seed)| seed.clone()))
            .collect()
    }
}

#[derive(Clone)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub cover: String,
    pub batch: String,
    pub session: String,
    pub palette: palette::Palette,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoiceAction {
    Start,
    Resume,
    Pause,
    Next,
    Previous,
    Stop,
    Status,
    Filters,
    Set { group: String, seed: Option<String> },
    Reset,
    Station { query: String, artist: bool },
    Track { title: String, artist: String },
    Speaker,
    Rate(&'static str),
}

type VoiceReply = std::result::Result<String, &'static str>;

pub fn voice(action: VoiceAction) -> VoiceReply {
    let sender = SENDER.get().ok_or("Yandex Music is not ready")?;
    let (tx, rx) = mpsc::sync_channel(1);
    let deadline = Instant::now() + Duration::from_secs(25);
    sender.try_send(Command::Voice(action, deadline, tx)).map_err(|_| "Yandex Music is busy")?;
    rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| "Yandex Music did not answer in time")?
}

pub enum Command {
    Voice(VoiceAction, Instant, mpsc::SyncSender<VoiceReply>),
    Connect,
    Cancel,
    Disconnect,
    Retry,
    Toggle,
    Resume,
    Pause,
    Next,
    Previous,
    Stop,
    Seek(f32),
    Volume(u8),
    Ratings,
    Rate(String, Rating),
    Library(usize),
    PlayLiked(String),
    Search { request: u64, query: String },
    SearchDone { account: u64, result: SearchResult },
    PlaySearch { query: String, id: String },
    Select(String, Option<String>),
    Autoplay(bool),
    Ended(u64),
    Failed(u64),
    Cover(u64, std::sync::Arc<media::Cover>),
}

pub fn snapshot() -> Snapshot {
    STATE.lock().ok().and_then(|s| s.clone()).unwrap_or_default()
}
pub fn volume() -> Option<f32> {
    let state = STATE.lock().ok()?;
    Some(f32::from(state.as_ref()?.config.volume) / 100.0)
}

pub fn set_volume(level: f32) -> bool {
    level.is_finite()
        && SENDER
            .get()
            .is_some_and(|tx| tx.try_send(Command::Volume((level.clamp(0.0, 1.0) * 100.0).round() as u8)).is_ok())
}

pub fn media() -> Option<Media> {
    STATE.lock().ok().and_then(|s| s.as_ref().filter(|s| s.active).map(|s| s.media.clone()))
}
pub fn progress() -> Option<(f32, f32)> {
    let (p, l, at, playing) = (*POSITION.lock().ok()?)?;
    (l > 0.0).then(|| ((p + if playing { at.elapsed().as_secs_f32() } else { 0.0 }).clamp(0.0, l), l))
}

#[cfg(test)]
pub fn debug_timeline(pos: f32, len: f32) {
    if let Ok(mut position) = POSITION.lock() {
        *position = Some((pos, len, Instant::now(), false));
    }
}
pub fn search(query: String) -> Option<u64> {
    let tx = SENDER.get()?;
    let request = SEARCH_REQUEST.fetch_add(1, Ordering::AcqRel) + 1;
    tx.try_send(Command::Search { request, query }).ok()?;
    Some(request)
}

pub fn send(command: Command) {
    if matches!(command, Command::Cancel | Command::Disconnect) {
        AUTH_EPOCH.fetch_add(1, Ordering::AcqRel);
    }
    if let Some(tx) = SENDER.get() {
        let _ = tx.try_send(command);
    }
}

pub fn start(hwnd: HWND) {
    let (tx, rx) = mpsc::sync_channel(32);
    if SENDER.set(tx.clone()).is_err() {
        return;
    }
    let hwnd = hwnd.0 as isize;
    std::thread::spawn(move || {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let mut w = Worker::new(hwnd, tx);
        w.restore();
        loop {
            let next = if w.login.is_some() || w.player.is_some() {
                rx.recv_timeout(Duration::from_millis(500))
            } else {
                rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
            };
            match next {
                Ok(command) => {
                    if let Err(error) = w.command(command) {
                        w.state.error = error.into();
                        w.state.busy = false;
                    }
                    w.publish();
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            w.tick();
        }
    });
}

struct Login {
    epoch: u64,
    code: String,
    expires: Instant,
    next: Instant,
    interval: Duration,
}
struct Worker {
    hwnd: isize,
    tx: mpsc::SyncSender<Command>,
    api: api::Api,
    state: Snapshot,
    account_id: Option<u64>,
    likes: Option<Arc<Vec<String>>>,
    dislikes: HashSet<String>,
    dislikes_loaded: bool,
    library_playback: Option<LibraryPlayback>,
    login: Option<Login>,
    player: Option<player::Player>,
    current: Option<Track>,
    queue: VecDeque<Track>,
    history: Vec<Track>,
    recent: VecDeque<String>,
    session: String,
    station: Option<String>,
    generation: u64,
    played: f64,
    sampled: Instant,
    was_playing: bool,
    replace_pending: bool,
    started: bool,
}

impl Worker {
    fn new(hwnd: isize, tx: mpsc::SyncSender<Command>) -> Self {
        Self {
            hwnd,
            tx,
            api: api::Api::default(),
            state: Snapshot::default(),
            account_id: None,
            likes: None,
            dislikes: HashSet::new(),
            dislikes_loaded: false,
            library_playback: None,
            login: None,
            player: None,
            current: None,
            queue: VecDeque::new(),
            history: Vec::new(),
            recent: VecDeque::new(),
            session: String::new(),
            station: None,
            generation: 0,
            played: 0.0,
            sampled: Instant::now(),
            was_playing: false,
            replace_pending: false,
            started: false,
        }
    }

    fn restore(&mut self) {
        self.state.config = integrations::dir()
            .and_then(|d| std::fs::read(d.join("yandex-wave.json")).ok())
            .filter(|b| b.len() < 16_384)
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        self.state.config.volume = self.state.config.volume.min(100);
        self.api.token = integrations::load_secret("yandex-music").unwrap_or_default();
        self.state.has_token = !self.api.token.is_empty();
        if !self.api.token.is_empty() {
            self.state.busy = true;
            self.publish();
            match self.load_account() {
                Ok(()) => {
                    if self.state.config.autoplay
                        && let Err(e) = self.new_wave(false)
                    {
                        self.state.error = e.into();
                    }
                }
                Err(e) => self.state.error = e.into(),
            }
        }
        self.state.busy = false;
        self.publish();
    }

    fn load_account(&mut self) -> api::Result<()> {
        let (account, uid) = match self.api.account() {
            Ok(account) => account,
            Err(error) => {
                if error == api::SIGN_IN {
                    self.api.token.clear();
                    self.state.account.clear();
                    self.state.has_token = false;
                    self.account_id = None;
                    self.likes = None;
                    self.dislikes.clear();
                    self.dislikes_loaded = false;
                    self.state.library = None;
                    self.state.search = None;
                    integrations::delete_secret("yandex-music");
                }
                return Err(error);
            }
        };
        if self.account_id != Some(uid) {
            self.likes = None;
            self.dislikes.clear();
            self.dislikes_loaded = false;
            self.state.library = None;
            self.state.search = None;
        }
        self.state.account = account;
        self.account_id = Some(uid);
        self.state.has_token = true;
        match self.api.groups() {
            Ok(groups) if !groups.is_empty() => {
                self.state.groups = groups;
                self.state.config.selection.retain(|key, seed| {
                    self.state.groups.iter().any(|g| &g.key == key && g.items.iter().any(|i| &i.1 == seed))
                });
                self.state.error.clear();
            }
            Ok(_) => self.state.error = "Wave settings are unavailable. Press Try again to reload.".into(),
            Err(error) => self.state.error = error.into(),
        }
        Ok(())
    }

    fn publish(&self) {
        if let Ok(mut s) = STATE.lock() {
            let mut state = self.state.clone();
            if let Some(track) = &self.current {
                state.current_id = Some(track.id.clone());
                state.rating = self.rating(&track.id);
            }
            *s = Some(state);
        }
        unsafe {
            let _ = PostMessageW(Some(HWND(self.hwnd as *mut _)), WM_YANDEX, WPARAM(0), LPARAM(0));
        }
    }

    fn save_config(&mut self) {
        let result = (|| {
            let dir = integrations::dir()?;
            std::fs::create_dir_all(&dir).ok()?;
            let data = serde_json::to_vec(&self.state.config).ok()?;
            let path = dir.join("yandex-wave.json");
            let temp = dir.join("yandex-wave.json.tmp");
            std::fs::write(&temp, data).ok()?;
            std::fs::rename(temp, path).ok()
        })();
        if result.is_none() {
            self.state.error = "Could not save My Wave settings".into();
        }
    }

    fn command(&mut self, command: Command) -> api::Result<()> {
        match command {
            Command::Voice(action, deadline, reply) => {
                let result = if Instant::now() < deadline {
                    self.voice(action)
                } else {
                    Err("Yandex Music did not answer in time")
                };
                if let Err(error) = result {
                    self.state.error = error.into();
                    self.state.busy = false;
                }
                self.publish();
                let _ = reply.try_send(result);
            }
            Command::Connect => {
                if self.login.is_some() {
                    return Ok(());
                }
                self.state.error.clear();
                let epoch = AUTH_EPOCH.load(Ordering::Acquire);
                self.state.busy = true;
                self.publish();
                let device = integrations::random_bytes(12).iter().map(|b| format!("{b:02x}")).collect::<String>();
                let v = api::oauth(
                    "device/code",
                    &[
                        ("client_id", api::CLIENT_ID),
                        ("device_id", &device),
                        ("device_name", "Night Island for Windows"),
                    ],
                )?;
                if epoch != AUTH_EPOCH.load(Ordering::Acquire) {
                    self.state.busy = false;
                    return Ok(());
                }
                let code = v["device_code"].as_str().ok_or("Could not start Yandex sign-in. Try again.")?.to_owned();
                self.state.code = v["user_code"].as_str().ok_or(api::NETWORK)?.chars().take(24).collect();
                let interval = Duration::from_secs(v["interval"].as_u64().unwrap_or(5).clamp(5, 60));
                let expires =
                    Instant::now() + Duration::from_secs(v["expires_in"].as_u64().unwrap_or(300).clamp(30, 900));
                self.login = Some(Login { epoch, code, expires, next: Instant::now() + interval, interval });
                self.state.busy = false;
            }
            Command::Cancel => {
                self.login = None;
                self.state.code.clear();
                self.state.busy = false;
            }
            Command::Disconnect => {
                self.stop();
                self.login = None;
                integrations::delete_secret("yandex-music");
                self.api.token.clear();
                self.account_id = None;
                self.likes = None;
                self.dislikes.clear();
                self.dislikes_loaded = false;
                self.state = Snapshot { config: self.state.config.clone(), ..Default::default() };
                self.state.config.autoplay = false;
                self.save_config();
                if integrations::dir().is_some_and(|dir| dir.join("yandex-music.bin").exists()) {
                    self.state.error = "Could not remove the saved sign-in. Try signing out again.".into();
                }
            }
            Command::Retry => {
                self.state.error.clear();
                self.state.busy = true;
                self.publish();
                self.load_account()?;
                self.remember_sign_in();
                self.state.busy = false;
            }
            Command::Toggle => {
                self.state.error.clear();
                if let Some(p) = &self.player {
                    if p.state().2 {
                        p.pause();
                    } else {
                        p.play();
                    }
                } else {
                    self.new_wave(false)?;
                }
            }
            Command::Resume => {
                if let Some(p) = &self.player {
                    p.play();
                }
            }
            Command::Pause => {
                if let Some(p) = &self.player {
                    p.pause();
                }
            }
            Command::Stop => self.stop(),
            Command::Ratings => {
                if let Some(uid) = self.account_id {
                    if self.likes.is_none() {
                        self.likes = self.api.library_ids(uid, false).ok().map(Arc::new);
                    }
                    if !self.dislikes_loaded
                        && let Ok(ids) = self.api.library_ids(uid, true)
                    {
                        self.dislikes = ids.into_iter().map(|id| track_key(&id).to_owned()).collect();
                        self.dislikes_loaded = true;
                    }
                }
            }
            Command::Library(page) => {
                let uid = self.account_id.ok_or(api::SIGN_IN)?;
                self.state.error.clear();
                self.state.busy = true;
                self.publish();
                if page == 0 || self.likes.is_none() {
                    self.likes = Some(Arc::new(self.api.library_ids(uid, false)?));
                }
                self.load_library(page)?;
                self.state.busy = false;
            }
            Command::PlayLiked(id) => self.play_liked(&id)?,
            Command::Search { request, query } => {
                if let Some(account) = self.account_id.filter(|_| request == SEARCH_REQUEST.load(Ordering::Acquire)) {
                    let api = self.api.clone();
                    let tx = self.tx.clone();
                    std::thread::spawn(move || {
                        unsafe {
                            let _ = RoInitialize(RO_INIT_MULTITHREADED);
                        }
                        let tracks = api.search_tracks(&query);
                        if request == SEARCH_REQUEST.load(Ordering::Acquire) {
                            let error = tracks.as_ref().err().copied();
                            let result = SearchResult { request, query, tracks: tracks.unwrap_or_default(), error };
                            let _ = tx.send(Command::SearchDone { account, result });
                        }
                    });
                }
            }
            Command::SearchDone { account, result } => {
                if self.account_id == Some(account) && result.request == SEARCH_REQUEST.load(Ordering::Acquire) {
                    self.state.search = Some(result);
                }
            }
            Command::PlaySearch { query, id } => {
                if let Some(track) = self
                    .state
                    .search
                    .as_ref()
                    .filter(|s| s.query == query)
                    .and_then(|s| s.tracks.iter().find(|track| track.id == id))
                    .cloned()
                {
                    self.play_catalog_track(track)?;
                }
            }
            Command::Rate(id, rating) => {
                if let Some(track) = self.current.as_ref().filter(|track| track.id == id).cloned() {
                    let kind = match (rating, self.rating(&id)) {
                        (Rating::Like, Rating::Like) => "unlike",
                        (Rating::Dislike, Rating::Dislike) => "undislike",
                        (Rating::Like, _) => "like",
                        (Rating::Dislike, _) => "dislike",
                        _ => return Ok(()),
                    };
                    self.rate(track, kind)?;
                }
            }
            Command::Volume(volume) => {
                let volume = volume.min(100);
                if self.state.config.volume != volume {
                    if let Some(player) = &self.player {
                        player.set_volume(volume).map_err(|_| "Could not change My Wave volume")?;
                    }
                    self.state.config.volume = volume;
                    self.save_config();
                }
            }
            Command::Seek(seconds) => {
                self.sample();
                if let Some(p) = &self.player {
                    p.seek(seconds);
                }
            }
            Command::Next => {
                if self.state.active {
                    self.advance(false)?;
                }
            }
            Command::Previous => {
                if self.player.as_ref().is_some_and(|p| p.state().0 > 3.0) {
                    if let Some(p) = &self.player {
                        p.seek(0.0);
                    }
                } else if let Some(previous) = self.history.pop() {
                    self.finish(false);
                    self.player = None;
                    self.generation += 1;
                    if let Some(current) = self.current.take() {
                        self.queue.push_front(current);
                    }
                    self.play_track(previous)?;
                }
            }
            Command::Select(key, seed) => {
                if let Some(seed) = seed {
                    if !self.state.groups.iter().any(|g| g.key == key && g.items.iter().any(|i| i.1 == seed)) {
                        return Ok(());
                    }
                    self.state.config.selection.insert(key, seed);
                } else {
                    self.state.config.selection.remove(&key);
                }
                self.save_config();
                if self.state.active && self.library_playback.is_none() {
                    self.replace_pending = true;
                    self.queue.clear();
                }
            }
            Command::Autoplay(on) => {
                self.state.config.autoplay = on;
                self.save_config();
            }
            Command::Ended(generation) if generation == self.generation => self.advance(true)?,
            Command::Failed(generation) if generation == self.generation => {
                self.stop();
                return Err("Could not play this track. Check your connection and start the wave again.");
            }
            Command::Cover(generation, cover) if generation == self.generation && self.state.active => {
                self.state.palette = self.state.palette.with_cover(media::dominant(&cover.bgra));
                self.state.media.accent = self.state.palette.accent;
                self.state.media.cover = Some(cover);
            }
            Command::Ended(_) | Command::Failed(_) | Command::Cover(_, _) => {}
        }
        Ok(())
    }

    fn voice(&mut self, action: VoiceAction) -> VoiceReply {
        if action == VoiceAction::Speaker {
            return Err("Island has no direct control of Yandex Station. Use Alice on the speaker.");
        }
        if self.api.token.is_empty() {
            return Err(api::SIGN_IN);
        }
        match action {
            VoiceAction::Start => {
                let playing_personal = self.station.is_none()
                    && self.library_playback.is_none()
                    && !self.replace_pending
                    && self.player.is_some()
                    && self
                        .current
                        .as_ref()
                        .is_some_and(|track| !track.session.is_empty() && track.session == self.session);
                if !playing_personal {
                    let old = self.station.take();
                    if let Err(error) = self.new_wave(false) {
                        self.station = old;
                        return Err(error);
                    }
                } else {
                    self.command(Command::Resume)?;
                }
                return Ok(crate::ui::i18n::tr("Starting your My Wave").into_owned());
            }
            VoiceAction::Station { query, artist } => {
                let (seed, name) = self.api.station(&query, artist)?;
                let old = self.station.replace(seed);
                if let Err(error) = self.new_wave(false) {
                    self.station = old;
                    return Err(error);
                }
                return Ok(format!("Started a Yandex Music station based on {name}"));
            }
            VoiceAction::Track { title, artist } => {
                let query = if artist.is_empty() { title.clone() } else { format!("{title} {artist}") };
                let tracks = self.api.search_tracks(&query)?;
                let track = match_track(&tracks, &title, &artist)
                    .ok_or("No matching track in Yandex Music. Check the title or specify the artist.")?;
                self.play_catalog_track(track.clone())?;
                return Ok(format!("{} {} — {}", crate::ui::i18n::tr("Starting in Island"), track.artist, track.title));
            }
            VoiceAction::Speaker => unreachable!(),
            VoiceAction::Resume | VoiceAction::Pause | VoiceAction::Next | VoiceAction::Previous => {
                if self.player.is_none() {
                    return Err("My Wave is not playing");
                }
                self.command(match action {
                    VoiceAction::Resume => Command::Resume,
                    VoiceAction::Pause => Command::Pause,
                    VoiceAction::Next => Command::Next,
                    _ => Command::Previous,
                })?;
            }
            VoiceAction::Stop => self.stop(),
            VoiceAction::Status => {}
            VoiceAction::Filters => {
                let filters = self
                    .state
                    .groups
                    .iter()
                    .map(|group| {
                        let active = self.state.config.selection.get(&group.key).map_or("any", String::as_str);
                        let items = group
                            .items
                            .iter()
                            .map(|(name, seed)| format!("{name} = {seed}"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("{} ({}), selected {active}: {items}", group.name, group.key)
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                return Ok(filters.chars().take(3500).collect());
            }
            VoiceAction::Set { group, seed } => {
                if !self
                    .state
                    .groups
                    .iter()
                    .any(|g| g.key == group && seed.as_ref().is_none_or(|s| g.items.iter().any(|item| item.1 == *s)))
                {
                    return Err("Unknown My Wave filter. Read the available filters first.");
                }
                self.command(Command::Select(group, seed))?;
                return Ok("My Wave filter saved".into());
            }
            VoiceAction::Reset => {
                for key in self.state.config.selection.keys().cloned().collect::<Vec<_>>() {
                    self.command(Command::Select(key, None))?;
                }
                return Ok("My Wave filters cleared".into());
            }
            VoiceAction::Rate(kind) => {
                let track = self.current.clone().ok_or("No My Wave track is playing")?;
                self.rate(track, kind)?;
                return Ok(format!("Saved track feedback: {kind}"));
            }
        }
        let playing = self.player.as_ref().is_some_and(|p| p.state().2);
        Ok(self.current.as_ref().map_or_else(
            || "My Wave is stopped".into(),
            |t| format!("My Wave {}: {} — {}", if playing { "playing" } else { "paused" }, t.artist, t.title),
        ))
    }

    fn rating(&self, id: &str) -> Rating {
        let key = track_key(id);
        if self.dislikes.contains(key) {
            Rating::Dislike
        } else if self.likes.as_ref().is_some_and(|ids| ids.iter().any(|id| track_key(id) == key)) {
            Rating::Like
        } else {
            Rating::None
        }
    }

    fn rate(&mut self, track: Track, kind: &str) -> api::Result<()> {
        let uid = self.account_id.ok_or(api::SIGN_IN)?;
        let key = track_key(&track.id);
        self.state.error.clear();
        self.state.busy = true;
        self.publish();
        self.api.rate(uid, &track, kind)?;
        if matches!(kind, "like" | "unlike" | "dislike") {
            if let Some(ids) = &mut self.likes {
                let ids = Arc::make_mut(ids);
                ids.retain(|id| track_key(id) != key);
                if kind == "like" {
                    ids.insert(0, track.id.clone());
                }
            } else {
                self.likes = self.api.library_ids(uid, false).ok().map(Arc::new);
            }
        }
        match kind {
            "dislike" => {
                self.dislikes.insert(key.to_owned());
            }
            "like" | "undislike" => {
                self.dislikes.remove(key);
            }
            _ => {}
        }
        let page = self.state.library.as_ref().map(|library| library.page);
        self.state.busy = false;
        if kind == "dislike" {
            self.advance(false)?;
        }
        if let Some(page) = page {
            self.load_library(page)?;
        }
        Ok(())
    }

    fn load_library(&mut self, page: usize) -> api::Result<()> {
        let ids = self.likes.as_ref().ok_or("Could not load liked tracks")?;
        let total = ids.len();
        let page = page.min(total.saturating_sub(1) / LIBRARY_PAGE);
        let start = page * LIBRARY_PAGE;
        let end = (start + LIBRARY_PAGE).min(total);
        let tracks = self.api.library_tracks(&ids[start..end])?;
        self.state.library = Some(LibraryPage { page, total, tracks });
        Ok(())
    }

    fn play_catalog_track(&mut self, track: Track) -> api::Result<()> {
        let previous = self.current.clone();
        self.play_track(track)?;
        if self.library_playback.take().is_some() {
            self.queue.clear();
        }
        if let Some(previous) = previous {
            self.history.push(previous);
            if self.history.len() > 20 {
                self.history.remove(0);
            }
        }
        Ok(())
    }

    fn play_liked(&mut self, id: &str) -> api::Result<()> {
        let Some(page) = &self.state.library else { return Ok(()) };
        let Some(index) = page.tracks.iter().position(|t| t.id == id) else { return Ok(()) };
        let ids = self.likes.clone().ok_or("Could not load liked tracks")?;
        let track = page.tracks[index].clone();
        let queue = page.tracks[index + 1..].iter().cloned().collect();
        let next = ((page.page + 1) * LIBRARY_PAGE).min(ids.len());
        self.finish(false);
        self.play_track(track)?;
        self.history.clear();
        self.queue = queue;
        self.station = None;
        self.session.clear();
        self.library_playback = Some(LibraryPlayback { ids, next });
        Ok(())
    }

    fn stop(&mut self) {
        self.finish(false);
        self.player = None;
        self.current = None;
        self.queue.clear();
        self.history.clear();
        self.recent.clear();
        self.session.clear();
        self.station = None;
        self.library_playback = None;
        self.state.active = false;
        self.state.media = Media::default();
        self.state.palette = Default::default();
        self.generation += 1;
        self.replace_pending = false;
        self.was_playing = false;
        self.started = false;
        if let Ok(mut p) = POSITION.lock() {
            *p = None;
        }
    }

    fn new_wave(&mut self, keep: bool) -> api::Result<()> {
        if self.api.token.is_empty() {
            return Err(api::SIGN_IN);
        }
        self.state.busy = true;
        self.publish();
        let result = self.api.call(
            "/rotor/session/new",
            Some(json!({"seeds":self.station.as_ref().map_or_else(|| self.state.config.seeds(), |seed| vec![seed.clone()]),"queue":self.recent,"includeTracksInResponse":true})),
        )?;
        let session = result["radioSessionId"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("My Wave returned no tracks. Try different settings.")?
            .to_string();
        let mut tracks = api::parse_tracks(&result);
        if tracks.is_empty() {
            return Err("My Wave returned no tracks. Try different settings.");
        }
        for track in &mut tracks {
            track.session = session.clone();
        }
        self.session = session;
        self.library_playback = None;
        self.queue = tracks.into();
        self.replace_pending = false;
        self.api.feedback(
            &self.session,
            self.queue.front().map_or("", |t| t.batch.as_str()),
            "radioStarted",
            None,
            0.0,
        );
        self.state.busy = false;
        if !keep {
            self.advance(false)?;
        }
        Ok(())
    }

    fn refill(&mut self) -> api::Result<()> {
        if self.replace_pending || self.session.is_empty() {
            return self.new_wave(true);
        }
        let result = self.api.call(
            &format!("/rotor/session/{}/tracks", crate::services::http::pct(&self.session)),
            Some(json!({"queue":self.recent})),
        )?;
        let mut tracks = api::parse_tracks(&result);
        for track in &mut tracks {
            track.session = self.session.clone();
        }
        if result["unknownSession"].as_bool() == Some(true) || tracks.is_empty() {
            self.new_wave(true)
        } else {
            self.queue.extend(tracks);
            Ok(())
        }
    }

    fn sample(&mut self) {
        let now = Instant::now();
        if self.was_playing {
            self.played += now.duration_since(self.sampled).as_secs_f64();
        }
        self.sampled = now;
        if let Some(player) = &self.player {
            let (pos, len, playing) = player.state();
            self.was_playing = playing;
            if playing && !self.started {
                self.started = true;
                if let Some(track) = &self.current {
                    self.api.feedback(&track.session, &track.batch, "trackStarted", Some(&track.id), 0.0);
                }
            }
            if let Ok(mut p) = POSITION.lock() {
                *p = Some((pos, len, now, playing));
            }
            if self.state.media.playing != playing {
                self.state.media.playing = playing;
                self.publish();
            }
        } else {
            self.was_playing = false;
        }
    }

    fn finish(&mut self, natural: bool) {
        self.sample();
        if let Some(p) = &self.player {
            p.pause();
        }
        if self.started
            && let Some(t) = &self.current
        {
            self.api.feedback(
                &t.session,
                &t.batch,
                if natural { "trackFinished" } else { "skip" },
                Some(&t.id),
                self.played,
            );
        }
        self.was_playing = false;
        self.started = false;
    }

    fn advance(&mut self, natural: bool) -> api::Result<()> {
        self.finish(natural);
        self.player = None;
        self.generation += 1;
        if let Ok(mut position) = POSITION.lock() {
            *position = None;
        }
        if let Some(t) = self.current.take() {
            self.history.push(t);
            if self.history.len() > 20 {
                self.history.remove(0);
            }
        }
        self.state.busy = true;
        self.state.media.playing = false;
        self.publish();
        if let Some(library) = &mut self.library_playback {
            while self.queue.is_empty() && library.next < library.ids.len() {
                let end = (library.next + LIBRARY_PAGE).min(library.ids.len());
                self.queue = self.api.library_tracks(&library.ids[library.next..end])?.into();
                library.next = end;
            }
            if self.queue.is_empty() {
                self.stop();
                self.state.busy = false;
                return Ok(());
            }
        } else if self.queue.is_empty() {
            self.refill()?;
        }
        let track = self.queue.pop_front().ok_or("My Wave returned no tracks. Try different settings.")?;
        self.play_track(track)
    }

    fn play_track(&mut self, track: Track) -> api::Result<()> {
        self.state.busy = true;
        self.publish();
        let url = self.api.stream(&track.id)?;
        let generation = self.generation + 1;
        let player = player::Player::open(&url, &track, generation, self.tx.clone(), self.state.config.volume)
            .map_err(|_| "Could not start the Windows audio player")?;
        if self.player.is_some() {
            self.finish(false);
        }
        if let Ok(mut position) = POSITION.lock() {
            *position = None;
        }
        self.generation = generation;
        self.player = Some(player);
        self.started = false;
        self.played = 0.0;
        self.sampled = Instant::now();
        self.was_playing = false;
        self.state.active = true;
        self.state.palette = track.palette;
        self.state.media = Media {
            accent: track.palette.accent.or(track.palette.average),
            title: track.title.clone(),
            artist: track.artist.clone(),
            source: SOURCE.into(),
            players: 1,
            ..Default::default()
        };
        self.current = Some(track.clone());
        if self.likes.is_none() || !self.dislikes_loaded {
            let _ = self.tx.try_send(Command::Ratings);
        }
        self.state.busy = false;
        self.state.error.clear();
        self.publish();
        self.recent.push_back(track.id.clone());
        while self.recent.len() > 40 {
            self.recent.pop_front();
        }
        if !track.cover.is_empty() {
            player::load_cover(track.cover, self.generation, self.tx.clone());
        }
        Ok(())
    }

    fn tick(&mut self) {
        self.sample();
        let outcome = self.player.as_ref().map_or(0, |p| p.outcome());
        if outcome != 0 {
            let command = if outcome == 1 { Command::Ended(self.generation) } else { Command::Failed(self.generation) };
            if let Err(e) = self.command(command) {
                self.state.error = e.into();
                self.state.busy = false;
            }
            self.publish();
        }
        let Some(login) = &mut self.login else { return };
        if login.epoch != AUTH_EPOCH.load(Ordering::Acquire) {
            return;
        }
        if Instant::now() >= login.expires {
            self.login = None;
            self.state.code.clear();
            self.state.error = "The sign-in code expired. Connect again.".into();
            self.publish();
            return;
        }
        if Instant::now() < login.next {
            return;
        }
        login.next = Instant::now() + login.interval;
        let epoch = login.epoch;
        let result = api::oauth(
            "token",
            &[
                ("grant_type", "device_code"),
                ("code", &login.code),
                ("client_id", api::CLIENT_ID),
                ("client_secret", api::CLIENT_SECRET),
            ],
        );
        if epoch != AUTH_EPOCH.load(Ordering::Acquire) {
            return;
        }
        match result {
            Ok(v) if v["access_token"].as_str().is_some() => {
                self.api.token = v["access_token"].as_str().unwrap_or_default().to_string();
                self.state.has_token = true;
                self.login = None;
                self.state.code.clear();
                self.state.busy = true;
                self.publish();
                let account = self.load_account();
                if epoch != AUTH_EPOCH.load(Ordering::Acquire) {
                    self.api.token.clear();
                    self.state.account.clear();
                    self.state.has_token = false;
                    self.state.busy = false;
                    self.publish();
                    return;
                }
                match account {
                    Ok(()) => self.remember_sign_in(),
                    Err(e) => self.state.error = e.into(),
                }
                self.state.busy = false;
                self.publish();
            }
            Ok(v) if v["error"] == "authorization_pending" => {}
            Ok(v) if v["error"] == "slow_down" => {
                if let Some(l) = &mut self.login {
                    l.interval += Duration::from_secs(5);
                    l.next = Instant::now() + l.interval;
                }
            }
            Err(error) => {
                self.state.error = error.into();
                self.publish();
            }
            Ok(_) => {
                self.login = None;
                self.state.code.clear();
                self.state.error = "Yandex sign-in was cancelled or expired. Connect again.".into();
                self.publish();
            }
        }
    }

    fn remember_sign_in(&mut self) {
        integrations::save_secret("yandex-music", &self.api.token);
        if integrations::load_secret("yandex-music").as_deref() != Some(&self.api.token) {
            self.state.error = "Connected for this session; Windows could not save the sign-in".into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_wave_is_personal_and_autoplay_is_opt_in() {
        let mut c = Config::default();
        assert!(!c.autoplay);
        assert_eq!(c.seeds(), ["user:onyourwave"]);
        c.selection.insert("language".into(), "settingLanguage:russian".into());
        assert_eq!(c.seeds(), ["user:onyourwave", "settingLanguage:russian"]);
        c.selection.insert("context".into(), "activity:work-background".into());
        assert_eq!(c.seeds(), ["activity:work-background", "settingLanguage:russian"]);
        assert_eq!(serde_json::from_str::<Config>(r#"{"autoplay":true}"#).unwrap().volume, 100);
        for volume in [0, 37, 100] {
            c.volume = volume;
            let saved = serde_json::to_string(&c).unwrap();
            assert_eq!(serde_json::from_str::<Config>(&saved).unwrap().volume, volume);
        }
    }
    #[test]
    fn exact_track_request_matches_title_and_performer() {
        let tracks = [("1", "Небо", "Первый"), ("2", "Небо", "Второй, Третий"), ("3", "Огонёк!", "Группа")].map(
            |(id, title, artist)| Track {
                id: id.into(),
                title: title.into(),
                artist: artist.into(),
                cover: String::new(),
                batch: String::new(),
                session: String::new(),
                palette: Default::default(),
            },
        );
        assert_eq!(match_track(&tracks, "«Небо»", "ВТОРОЙ").unwrap().id, "2");
        assert_eq!(match_track(&tracks, "Группа Огонек", "").unwrap().id, "3");
        assert!(match_track(&tracks, "Небо", "Другой").is_none());
        assert!(match_track(&tracks, "Небо в огне", "").is_none());
    }

    #[test]
    fn stale_player_events_do_not_advance_or_stop_new_track() {
        let (tx, _) = mpsc::sync_channel(8);
        let mut w = Worker::new(0, tx);
        w.generation = 2;
        w.state.active = true;
        w.command(Command::Ended(1)).unwrap();
        w.command(Command::Failed(1)).unwrap();
        assert!(w.state.active);
    }

    #[test]
    fn delayed_cover_keeps_server_colours_and_old_track_cannot_recolour_the_player() {
        let (tx, _) = mpsc::sync_channel(8);
        let mut w = Worker::new(0, tx);
        w.generation = 4;
        w.state.active = true;
        w.state.palette =
            palette::Palette { accent: Some((250, 120, 50)), average: Some((80, 30, 10)), ..Default::default() };
        let cover = media::debug_cover(1, &[10, 30, 240, 255]).unwrap();
        w.command(Command::Cover(3, cover.clone())).unwrap();
        assert!(w.state.media.cover.is_none());
        w.command(Command::Cover(4, cover)).unwrap();
        assert_eq!(w.state.media.accent, Some((250, 120, 50)));
        assert_eq!(w.state.palette.average, Some((80, 30, 10)));
        assert!(w.state.palette.mini_player.is_some());
    }
}
