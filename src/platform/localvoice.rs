use std::ffi::{CStr, CString, c_char, c_void};
use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::{LOAD_WITH_ALTERED_SEARCH_PATH, LoadLibraryExW};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
use windows::core::HSTRING;

use crate::platform::{speech, wake, wake_install};
use crate::services::http;

pub const WM_LOCAL_VOICE: u32 = WM_APP + 84;

pub const VOICE_ID: &str = "irina";
const VOICE_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-piper-ru_RU-irina-medium-int8.tar.bz2";
const VOICE_SHA256: &str = "b0000a509f7551a80742eed5c43b8eb03f469cb9f0a42f6feee96ce0da0ebab8";
const VOICE_MAX_BYTES: usize = 40 << 20;
const MODEL_FILE: &str = "ru_RU-irina-medium.onnx";
const MAX_UNPACKED_BYTES: u64 = 120 << 20;
const MAX_UNPACKED_FILE: u64 = 64 << 20;
const MAX_FILES: usize = 1500;
const IDLE_UNLOAD: Duration = Duration::from_secs(90);

pub fn thread_count() -> i32 {
    let cores = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
    (cores / 2).clamp(2, 4) as i32
}
const MAX_SPOKEN_CHARS: usize = 600;

#[repr(C)]
struct VitsConfig {
    model: *const c_char,
    lexicon: *const c_char,
    tokens: *const c_char,
    data_dir: *const c_char,
    noise_scale: f32,
    noise_scale_w: f32,
    length_scale: f32,
    dict_dir: *const c_char,
}

#[repr(C)]
struct MatchaConfig {
    acoustic_model: *const c_char,
    vocoder: *const c_char,
    lexicon: *const c_char,
    tokens: *const c_char,
    data_dir: *const c_char,
    noise_scale: f32,
    length_scale: f32,
    dict_dir: *const c_char,
}

#[repr(C)]
struct KokoroConfig {
    model: *const c_char,
    voices: *const c_char,
    tokens: *const c_char,
    data_dir: *const c_char,
    length_scale: f32,
    dict_dir: *const c_char,
    lexicon: *const c_char,
    lang: *const c_char,
}

#[repr(C)]
struct KittenConfig {
    model: *const c_char,
    voices: *const c_char,
    tokens: *const c_char,
    data_dir: *const c_char,
    length_scale: f32,
}

#[repr(C)]
struct ZipvoiceConfig {
    tokens: *const c_char,
    encoder: *const c_char,
    decoder: *const c_char,
    vocoder: *const c_char,
    data_dir: *const c_char,
    lexicon: *const c_char,
    feat_scale: f32,
    t_shift: f32,
    target_rms: f32,
    guidance_scale: f32,
}

#[repr(C)]
struct PocketConfig {
    lm_flow: *const c_char,
    lm_main: *const c_char,
    encoder: *const c_char,
    decoder: *const c_char,
    text_conditioner: *const c_char,
    vocab_json: *const c_char,
    token_scores_json: *const c_char,
    voice_embedding_cache_capacity: i32,
}

#[repr(C)]
struct SupertonicConfig {
    duration_predictor: *const c_char,
    text_encoder: *const c_char,
    vector_estimator: *const c_char,
    vocoder: *const c_char,
    tts_json: *const c_char,
    unicode_indexer: *const c_char,
    voice_style: *const c_char,
}

#[repr(C)]
struct ModelConfig {
    vits: VitsConfig,
    num_threads: i32,
    debug: i32,
    provider: *const c_char,
    matcha: MatchaConfig,
    kokoro: KokoroConfig,
    kitten: KittenConfig,
    zipvoice: ZipvoiceConfig,
    pocket: PocketConfig,
    supertonic: SupertonicConfig,
}

#[repr(C)]
struct TtsConfig {
    model: ModelConfig,
    rule_fsts: *const c_char,
    max_num_sentences: i32,
    rule_fars: *const c_char,
    silence_scale: f32,
}

#[repr(C)]
struct Generated {
    samples: *const f32,
    n: i32,
    sample_rate: i32,
}

enum Tts {}

type FnCreate = unsafe extern "C" fn(*const TtsConfig) -> *const Tts;
type FnDestroy = unsafe extern "C" fn(*const Tts);
type FnGenerate = unsafe extern "C" fn(*const Tts, *const c_char, i32, f32) -> *const Generated;
type FnDestroyAudio = unsafe extern "C" fn(*const Generated);
type FnVersion = unsafe extern "C" fn() -> *const c_char;

struct Api {
    create: FnCreate,
    destroy: FnDestroy,
    generate: FnGenerate,
    destroy_audio: FnDestroyAudio,
}

impl Api {
    fn load(root: &Path) -> Option<Api> {
        unsafe {
            let ort = root.join("bin").join("onnxruntime.dll");
            let sherpa = root.join("bin").join("sherpa-onnx-c-api.dll");
            LoadLibraryExW(&HSTRING::from(ort.as_os_str()), None, LOAD_WITH_ALTERED_SEARCH_PATH).ok()?;
            let module =
                LoadLibraryExW(&HSTRING::from(sherpa.as_os_str()), None, LOAD_WITH_ALTERED_SEARCH_PATH).ok()?;
            let version: FnVersion = wake::symbol(module, "SherpaOnnxGetVersionStr")?;
            if CStr::from_ptr(version()).to_string_lossy() != wake::RUNTIME_VERSION {
                return None;
            }
            Some(Api {
                create: wake::symbol(module, "SherpaOnnxCreateOfflineTts")?,
                destroy: wake::symbol(module, "SherpaOnnxDestroyOfflineTts")?,
                generate: wake::symbol(module, "SherpaOnnxOfflineTtsGenerate")?,
                destroy_audio: wake::symbol(module, "SherpaOnnxDestroyOfflineTtsGeneratedAudio")?,
            })
        }
    }
}

pub struct Engine {
    api: Api,
    tts: *const Tts,
}

unsafe impl Send for Engine {}

pub fn to_wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let pcm: Vec<u8> = samples.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
    speech::pcm_wav(&pcm, rate)
}

impl Engine {
    pub fn open(root: &Path) -> Option<Engine> {
        let api = Api::load(root)?;
        let voice = root.join("tts").join(VOICE_ID);
        let path = |p: PathBuf| CString::new(p.to_string_lossy().as_bytes()).ok();
        let model = path(voice.join(MODEL_FILE))?;
        let tokens = path(voice.join("tokens.txt"))?;
        let data = path(voice.join("espeak-ng-data"))?;
        let provider = CString::new("cpu").ok()?;
        let mut cfg: TtsConfig = unsafe { std::mem::zeroed() };
        cfg.model.vits.model = model.as_ptr();
        cfg.model.vits.tokens = tokens.as_ptr();
        cfg.model.vits.data_dir = data.as_ptr();
        cfg.model.vits.noise_scale = 0.667;
        cfg.model.vits.noise_scale_w = 0.8;
        cfg.model.vits.length_scale = 1.0;
        cfg.model.num_threads = thread_count();
        cfg.model.provider = provider.as_ptr();
        cfg.max_num_sentences = 1;
        cfg.silence_scale = 0.2;
        let tts = unsafe { (api.create)(&cfg) };
        (!tts.is_null()).then_some(Engine { api, tts })
    }

    pub fn say(&self, text: &str) -> Option<Vec<u8>> {
        let text: String = text.chars().filter(|c| !c.is_control()).take(MAX_SPOKEN_CHARS).collect();
        let text = CString::new(text.trim()).ok().filter(|t| !t.as_bytes().is_empty())?;
        unsafe {
            let audio = (self.api.generate)(self.tts, text.as_ptr(), 0, 1.0);
            if audio.is_null() {
                return None;
            }
            let (n, rate) = ((*audio).n, (*audio).sample_rate);
            let wav = (n > 0 && rate > 0 && !(*audio).samples.is_null())
                .then(|| to_wav(std::slice::from_raw_parts((*audio).samples, n as usize), rate as u32));
            (self.api.destroy_audio)(audio);
            wav
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe { (self.api.destroy)(self.tts) };
    }
}

pub fn voice_dir_in(root: &Path) -> PathBuf {
    root.join("tts").join(VOICE_ID)
}

pub fn runtime_in(root: &Path) -> bool {
    ["bin/onnxruntime.dll", "bin/sherpa-onnx-c-api.dll"].iter().all(|f| root.join(f).is_file())
}

pub fn voice_ready_in(dir: &Path) -> bool {
    dir.join(MODEL_FILE).is_file() && dir.join("tokens.txt").is_file() && dir.join("espeak-ng-data").is_dir()
}

pub fn ready() -> bool {
    wake::dir().is_some_and(|root| runtime_in(&root) && voice_ready_in(&voice_dir_in(&root)))
}

pub const HELPER_FLAG: &str = "--tts-helper";
const MAX_REQUEST_BYTES: usize = 8 * 1024;
const MAX_REPLY_BYTES: usize = 64 << 20;
const REPLY_TIMEOUT: Duration = Duration::from_secs(90);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn write_frame(out: &mut impl Write, payload: &[u8]) -> std::io::Result<()> {
    out.write_all(&(payload.len() as u32).to_le_bytes())?;
    out.write_all(payload)?;
    out.flush()
}

pub fn read_frame(input: &mut impl Read, limit: usize) -> Option<Vec<u8>> {
    let mut len = [0u8; 4];
    input.read_exact(&mut len).ok()?;
    let len = u32::from_le_bytes(len) as usize;
    if len > limit {
        return None;
    }
    let mut payload = vec![0u8; len];
    input.read_exact(&mut payload).ok()?;
    Some(payload)
}

pub fn serve(root: &Path, mut input: impl Read, mut output: impl Write) {
    let mut engine: Option<Engine> = None;
    while let Some(request) = read_frame(&mut input, MAX_REQUEST_BYTES) {
        if engine.is_none() {
            engine = Engine::open(root);
        }
        let text = String::from_utf8_lossy(&request).into_owned();
        let wav = engine.as_ref().and_then(|e| e.say(&text)).unwrap_or_default();
        if write_frame(&mut output, &wav).is_err() {
            break;
        }
    }
}

pub fn run_helper() -> bool {
    let args: Vec<String> = std::env::args().collect();
    let Some(at) = args.iter().position(|a| a == HELPER_FLAG) else { return false };
    if let Some(root) = args.get(at + 1) {
        serve(Path::new(root), std::io::stdin().lock(), std::io::stdout().lock());
    }
    true
}

struct Helper {
    child: Child,
    stdin: ChildStdin,
    replies: Receiver<Vec<u8>>,
    used: Instant,
}

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

static HELPER: Mutex<Option<Helper>> = Mutex::new(None);

fn start_helper(root: &Path) -> Option<Helper> {
    let mut child = Command::new(std::env::current_exe().ok()?)
        .arg(HELPER_FLAG)
        .arg(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .ok()?;
    let stdin = child.stdin.take()?;
    let mut stdout = child.stdout.take()?;
    let (sender, replies) = mpsc::channel();
    std::thread::spawn(move || {
        while let Some(wav) = read_frame(&mut stdout, MAX_REPLY_BYTES) {
            if sender.send(wav).is_err() {
                break;
            }
        }
    });
    Some(Helper { child, stdin, replies, used: Instant::now() })
}

fn unload_later() {
    std::thread::spawn(|| {
        std::thread::sleep(IDLE_UNLOAD + Duration::from_secs(1));
        if let Ok(mut guard) = HELPER.lock()
            && guard.as_ref().is_some_and(|h| h.used.elapsed() >= IDLE_UNLOAD)
        {
            *guard = None;
        }
    });
}

pub fn warm_up() {
    let running = HELPER.lock().map_or(true, |guard| guard.is_some());
    if running || !ready() {
        return;
    }
    std::thread::spawn(|| {
        let _ = speak_wav("Да.");
    });
}

pub fn speak_wav(text: &str) -> Option<Vec<u8>> {
    let root = wake::dir().filter(|r| runtime_in(r) && voice_ready_in(&voice_dir_in(r)))?;
    let started = Instant::now();
    let mut guard = HELPER.lock().ok()?;
    if guard.is_none() {
        *guard = Some(start_helper(&root)?);
    }
    let helper = guard.as_mut()?;
    let request = text.as_bytes();
    let request = &request[..request.len().min(MAX_REQUEST_BYTES)];
    let wav = write_frame(&mut helper.stdin, request)
        .ok()
        .and_then(|()| helper.replies.recv_timeout(REPLY_TIMEOUT).ok())
        .filter(|wav| !wav.is_empty());
    match &wav {
        Some(wav) => {
            helper.used = Instant::now();
            let secs = wav.len().saturating_sub(44) as f32 / 2.0 / 22_050.0;
            crate::log_info!(
                "Local voice: {} characters, {secs:.1} s of sound in {} ms",
                text.chars().count(),
                started.elapsed().as_millis()
            );
        }
        None => {
            crate::log_warn!("Local voice: the helper gave no sound, restarting it next time");
            *guard = None;
        }
    }
    drop(guard);
    unload_later();
    wav
}

pub fn extract_tree(archive: impl Read, into: &Path) -> Result<usize, &'static str> {
    let mut tar = tar::Archive::new(archive);
    let (mut files, mut total) = (0usize, 0u64);
    for entry in tar.entries().map_err(|_| "The archive is damaged")? {
        let mut entry = entry.map_err(|_| "The archive is damaged")?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().map_err(|_| "The archive is damaged")?.into_owned();
        let mut parts: Vec<std::ffi::OsString> = Vec::new();
        for part in path.components() {
            match part {
                Component::Normal(name) if !name.to_string_lossy().contains(':') => parts.push(name.to_os_string()),
                Component::CurDir => {}
                _ => return Err("The archive is not safe"),
            }
        }
        if parts.len() < 2 {
            continue;
        }
        files += 1;
        total += entry.size();
        if files > MAX_FILES {
            return Err("The archive has too many files");
        }
        if total > MAX_UNPACKED_BYTES || entry.size() > MAX_UNPACKED_FILE {
            return Err("The archive is too large");
        }
        let target = parts[1..].iter().fold(into.to_path_buf(), |p, part| p.join(part));
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir).map_err(|_| "Could not write to the profile folder")?;
        }
        let mut out = std::fs::File::create(&target).map_err(|_| "Could not write to the profile folder")?;
        let copied = std::io::copy(&mut (&mut entry).take(MAX_UNPACKED_FILE + 1), &mut out)
            .map_err(|_| "Could not write to the profile folder")?;
        if copied > MAX_UNPACKED_FILE {
            return Err("The archive is too large");
        }
    }
    Ok(files)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Idle,
    Working(u8),
    Ready,
    Failed(&'static str),
}

static STATE: Mutex<State> = Mutex::new(State::Idle);

pub fn state() -> State {
    STATE.lock().map_or(State::Idle, |s| *s)
}

fn set(state: State) {
    if let Ok(mut s) = STATE.lock() {
        *s = state;
    }
}

fn fetch_voice() -> Result<Vec<u8>, &'static str> {
    let reply = http::get_bytes(VOICE_URL, VOICE_MAX_BYTES, None).ok_or("Could not download the voice")?;
    if reply.status != 200 {
        return Err("The download server refused the request");
    }
    if wake_install::digest_hex(&reply.body) != VOICE_SHA256 {
        return Err("The download does not match the expected checksum");
    }
    Ok(reply.body)
}

fn install_to(root: &Path) -> Result<(), &'static str> {
    if !runtime_in(root) {
        set(State::Working(5));
        let package = &wake_install::PACKAGES[0];
        let body = wake_install::fetch(package)?;
        set(State::Working(35));
        let count = wake_install::extract(bzip2_rs::DecoderReader::new(body.as_slice()), package.files, root)?;
        if count != package.files.len() {
            return Err("The archive is missing files");
        }
    }
    set(State::Working(50));
    let body = fetch_voice()?;
    set(State::Working(80));
    let target = voice_dir_in(root);
    let staging = root.join("tts").join(format!("{VOICE_ID}.part"));
    let _ = std::fs::remove_dir_all(&staging);
    let unpacked = extract_tree(bzip2_rs::DecoderReader::new(body.as_slice()), &staging);
    if unpacked.is_err() || !voice_ready_in(&staging) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(unpacked.err().unwrap_or("The installation is incomplete"));
    }
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(&staging, &target).map_err(|_| "Could not write to the profile folder")
}

pub fn install(hwnd: HWND) -> bool {
    if !wake_install::begin_install() {
        return false;
    }
    let Some(root) = wake::dir() else {
        set(State::Failed("No profile folder"));
        wake_install::end_install();
        return false;
    };
    let raw = hwnd.0 as isize;
    set(State::Working(0));
    std::thread::spawn(move || {
        match install_to(&root) {
            Ok(()) => set(State::Ready),
            Err(why) => set(State::Failed(why)),
        }
        wake_install::end_install();
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut c_void)), WM_LOCAL_VOICE, WPARAM(0), LPARAM(0));
        }
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};

    fn tar_with(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, name, *data).unwrap();
        }
        builder.into_inner().unwrap()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ni-localvoice-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_structs_have_the_sizes_the_c_header_gives_on_64_bit_windows() {
        assert_eq!(size_of::<VitsConfig>(), 56);
        assert_eq!(size_of::<MatchaConfig>(), 56);
        assert_eq!(size_of::<KokoroConfig>(), 64);
        assert_eq!(size_of::<KittenConfig>(), 40);
        assert_eq!(size_of::<ZipvoiceConfig>(), 64);
        assert_eq!(size_of::<PocketConfig>(), 64);
        assert_eq!(size_of::<SupertonicConfig>(), 56);
        assert_eq!(size_of::<ModelConfig>(), 416);
        assert_eq!(size_of::<TtsConfig>(), 448);
        assert_eq!(size_of::<Generated>(), 16);
        assert_eq!(offset_of!(ModelConfig, provider), 64);
        assert_eq!(offset_of!(ModelConfig, matcha), 72);
        assert_eq!(offset_of!(ModelConfig, supertonic), 360);
        assert_eq!(offset_of!(TtsConfig, rule_fsts), 416);
        assert_eq!(offset_of!(TtsConfig, silence_scale), 440);
    }

    #[test]
    fn samples_become_a_clipped_16_bit_wav() {
        let wav = to_wav(&[0.0, 0.5, -0.5, 2.0, -3.0], 22_050);
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 22_050);
        let pcm: Vec<i16> = wav[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        assert_eq!(pcm, [0, 16383, -16383, 32767, -32767]);
        assert_eq!(to_wav(&[], 16_000).len(), 44);
    }

    #[test]
    fn the_whole_voice_folder_is_unpacked_without_its_top_folder() {
        let archive = tar_with(&[
            ("vits-x/ru.onnx", b"model"),
            ("vits-x/tokens.txt", b"t"),
            ("vits-x/espeak-ng-data/ru_dict", b"d"),
            ("vits-x/espeak-ng-data/voices/!v/m1", b"v"),
        ]);
        let into = scratch("tree");
        assert_eq!(extract_tree(archive.as_slice(), &into), Ok(4));
        assert_eq!(std::fs::read(into.join("ru.onnx")).unwrap(), b"model");
        assert_eq!(std::fs::read(into.join("espeak-ng-data/voices/!v/m1")).unwrap(), b"v");
        let _ = std::fs::remove_dir_all(&into);
    }

    #[test]
    fn a_path_that_climbs_out_or_names_a_stream_is_refused() {
        for name in ["vits/../../evil.dll", "/abs/file", "vits/a:stream", "C:/x/y"] {
            let mut builder = tar::Builder::new(Vec::new());
            let mut header = tar::Header::new_gnu();
            header.set_size(1);
            header.set_mode(0o644);
            let raw = name.as_bytes();
            header.as_old_mut().name[..raw.len()].copy_from_slice(raw);
            header.set_cksum();
            builder.append(&header, &b"x"[..]).unwrap();
            let archive = builder.into_inner().unwrap();
            let into = scratch("evil");
            assert_eq!(extract_tree(archive.as_slice(), &into), Err("The archive is not safe"), "{name}");
            assert!(!into.parent().unwrap().join("evil.dll").exists());
            let _ = std::fs::remove_dir_all(&into);
        }
    }

    #[test]
    fn a_broken_archive_and_an_oversized_one_are_reported() {
        let into = scratch("broken");
        assert!(extract_tree(vec![7u8; 2048].as_slice(), &into).is_err());
        let _ = std::fs::remove_dir_all(&into);
    }

    #[test]
    fn the_engine_uses_a_few_threads_and_the_helper_stays_for_a_while() {
        assert!((2..=4).contains(&thread_count()));
        assert!(IDLE_UNLOAD >= Duration::from_secs(60), "a cold start takes seconds, so do not unload too early");
    }

    #[test]
    fn the_voice_counts_as_installed_only_with_all_its_parts() {
        let dir = scratch("ready");
        assert!(!voice_ready_in(&dir));
        std::fs::create_dir_all(dir.join("espeak-ng-data")).unwrap();
        std::fs::write(dir.join(MODEL_FILE), b"m").unwrap();
        assert!(!voice_ready_in(&dir));
        std::fs::write(dir.join("tokens.txt"), b"t").unwrap();
        assert!(voice_ready_in(&dir));
        assert!(!runtime_in(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn frames_round_trip_and_oversized_or_cut_ones_are_refused() {
        let mut wire = Vec::new();
        write_frame(&mut wire, "привет".as_bytes()).unwrap();
        write_frame(&mut wire, b"").unwrap();
        let mut input = wire.as_slice();
        assert_eq!(read_frame(&mut input, 100).as_deref(), Some("привет".as_bytes()));
        assert_eq!(read_frame(&mut input, 100).as_deref(), Some(&b""[..]));
        assert_eq!(read_frame(&mut input, 100), None, "nothing is left");
        let mut big = Vec::new();
        write_frame(&mut big, &[1u8; 50]).unwrap();
        assert_eq!(read_frame(&mut big.as_slice(), 10), None, "over the limit");
        assert_eq!(read_frame(&mut &big[..20], 100), None, "cut in the middle");
    }

    #[test]
    fn the_helper_answers_with_silence_when_the_voice_cannot_load() {
        let mut input = Vec::new();
        write_frame(&mut input, b"one").unwrap();
        write_frame(&mut input, b"two").unwrap();
        let mut output = Vec::new();
        serve(&scratch("nothing-here"), input.as_slice(), &mut output);
        let mut replies = output.as_slice();
        assert_eq!(read_frame(&mut replies, 10), Some(Vec::new()));
        assert_eq!(read_frame(&mut replies, 10), Some(Vec::new()));
        assert_eq!(read_frame(&mut replies, 10), None);
    }

    #[test]
    fn the_pinned_download_is_a_github_release_with_a_full_checksum() {
        assert_eq!(VOICE_SHA256.len(), 64);
        assert!(VOICE_URL.starts_with("https://github.com/k2-fsa/sherpa-onnx/releases/download/"));
    }

    #[test]
    #[ignore = "downloads about 44 MB from GitHub into a scratch folder"]
    fn the_real_download_verifies_unpacks_and_then_speaks() {
        let root = scratch("install");
        install_to(&root).expect("installed");
        assert!(runtime_in(&root) && voice_ready_in(&voice_dir_in(&root)));
        assert!(!root.join("tts").join(format!("{VOICE_ID}.part")).exists());
        let engine = Engine::open(&root).expect("the engine loaded");
        let wav = engine.say("Проверка установки.").expect("speech");
        assert!(wav.len() > 20_000);
        drop(engine);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[ignore = "needs NI_PIPER_ROOT and a built target/debug/night-island.exe"]
    fn the_real_helper_process_speaks_and_ends_when_its_pipe_closes() {
        let root = PathBuf::from(std::env::var("NI_PIPER_ROOT").expect("NI_PIPER_ROOT"));
        let exe = std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().join("night-island.exe");
        assert!(exe.is_file(), "build the app first: {}", exe.display());
        let started = Instant::now();
        let mut child = Command::new(&exe)
            .arg(HELPER_FLAG)
            .arg(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let (mut stdin, mut stdout) = (child.stdin.take().unwrap(), child.stdout.take().unwrap());
        write_frame(&mut stdin, "Первое предложение. Второе предложение для проверки.".as_bytes()).unwrap();
        let wav = read_frame(&mut stdout, MAX_REPLY_BYTES).expect("a reply");
        println!("first reply {} bytes after {:?}", wav.len(), started.elapsed());
        assert!(wav.len() > 20_000 && &wav[..4] == b"RIFF");
        write_frame(&mut stdin, "Ещё одна фраза.".as_bytes()).unwrap();
        let second = Instant::now();
        let again = read_frame(&mut stdout, MAX_REPLY_BYTES).expect("a second reply");
        println!("second reply {} bytes after {:?}", again.len(), second.elapsed());
        drop(stdin);
        let status = child.wait().unwrap();
        assert!(status.success(), "{status:?}");
    }

    #[test]
    #[ignore = "needs NI_PIPER_ROOT: a folder with bin/ (the two DLLs) and tts/irina/ (the unpacked voice)"]
    fn the_real_voice_speaks_russian() {
        let root = PathBuf::from(std::env::var("NI_PIPER_ROOT").expect("NI_PIPER_ROOT"));
        let memory_now = || crate::platform::memory::usage().map_or((0, 0), |u| (u.working_mb(), u.private_mb()));
        println!("before: working/private MB {:?}", memory_now());
        let started = Instant::now();
        let engine = Engine::open(&root).expect("the engine loaded");
        println!("loaded in {:?}, now {:?}", started.elapsed(), memory_now());
        let started = Instant::now();
        let wav =
            engine.say("Привет! Это проверка голоса. Сегодня в Иваново облачно, около трёх градусов.").expect("speech");
        let took = started.elapsed();
        println!("after one phrase: {:?}", memory_now());
        let long = "Сегодня в Иваново облачно, температура около трёх градусов, ветер слабый. Завтра ожидается снег, \
                    днём около нуля, ночью до минус двух. К выходным потеплеет, но осадки сохранятся. ";
        let started = Instant::now();
        let long_wav = engine.say(&long.repeat(3)).expect("long speech");
        let long_secs = (long_wav.len() - 44) as f32 / 2.0 / 22_050.0;
        println!("a long text: {long_secs:.1} s of sound made in {:?}, now {:?}", started.elapsed(), memory_now());
        let rate = u32::from_le_bytes(wav[24..28].try_into().unwrap());
        let pcm: Vec<i16> = wav[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        let secs = pcm.len() as f32 / rate as f32;
        let peak = pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
        println!("{secs:.1} s of sound at {rate} Hz, peak {peak}, made in {took:?}");
        if let Ok(out) = std::env::var("NI_PIPER_OUT") {
            std::fs::write(out, &wav).unwrap();
        }
        assert!(secs > 3.0 && secs < 15.0, "{secs}");
        assert!(peak > 2000, "{peak}");
        assert!(engine.say("   ").is_none());
        drop(engine);
        println!("after drop: {:?}", memory_now());
        crate::platform::memory::trim();
        println!("after trim: {:?}", memory_now());
    }
}
