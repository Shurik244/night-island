use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_float, c_void};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LOAD_WITH_ALTERED_SEARCH_PATH, LoadLibraryExW};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
use windows::core::{HSTRING, PCSTR};

use crate::platform::dictation;

pub const WM_WAKE: u32 = WM_APP + 80;
pub const WM_WAKE_STATUS: u32 = WM_APP + 112;

pub const RUNTIME_VERSION: &str = "1.13.5";
const SCORE: f32 = 3.2;
const THRESHOLD: f32 = 0.09;
const GATE_RMS: f32 = 0.003;
const HANGOVER_CHUNKS: u32 = 36;
const SAMPLE_RATE: i32 = 16_000;

pub const FILES: [&str; 6] = [
    "bin/onnxruntime.dll",
    "bin/sherpa-onnx-c-api.dll",
    "model/encoder.int8.onnx",
    "model/decoder.onnx",
    "model/joiner.int8.onnx",
    "model/tokens.txt",
];

static LISTENING: AtomicBool = AtomicBool::new(false);
static REVISION: AtomicU64 = AtomicU64::new(0);
static CONTROL: Mutex<Control> = Mutex::new(Control { request: None, running: false });
static STATUS: Mutex<Option<(u64, Result<(), String>)>> = Mutex::new(None);

#[derive(Clone)]
struct Request {
    root: PathBuf,
    words: Vec<String>,
    hwnd: isize,
    revision: u64,
}
struct Control {
    request: Option<Request>,
    running: bool,
}
impl Control {
    fn replace(&mut self, request: Request) -> bool {
        self.request = Some(request);
        if self.running {
            false
        } else {
            self.running = true;
            true
        }
    }
    fn finish(&mut self, revision: u64) -> bool {
        if self.request.as_ref().is_some_and(|r| r.revision != revision) {
            return true;
        }
        self.running = false;
        self.request = None;
        false
    }
}
static PAUSED: AtomicBool = AtomicBool::new(false);

pub fn dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("NightIsland").join("wake"))
}

pub fn installed_in(root: &Path) -> bool {
    FILES.iter().all(|f| root.join(f).is_file())
}

pub fn installed() -> bool {
    dir().is_some_and(|d| installed_in(&d))
}

pub fn current_revision(revision: u64) -> bool {
    REVISION.load(Ordering::Acquire) == revision
}

pub fn listening() -> bool {
    LISTENING.load(Ordering::SeqCst)
}

pub fn load_vocab(tokens: &str) -> HashMap<String, ()> {
    tokens.lines().filter_map(|l| l.split_whitespace().next()).map(|t| (t.to_string(), ())).collect()
}

pub fn tokenize(word: &str, vocab: &HashMap<String, ()>) -> Option<Vec<String>> {
    let chars: Vec<char> = std::iter::once('\u{2581}').chain(word.trim().to_lowercase().chars()).collect();
    if chars.len() < 2 {
        return None;
    }
    let mut out = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let piece = (at + 1..=chars.len())
            .rev()
            .map(|end| (end, chars[at..end].iter().collect::<String>()))
            .find(|(_, p)| vocab.contains_key(p))?;
        out.push(piece.1);
        at = piece.0;
    }
    Some(out)
}

pub fn translit(word: &str) -> Vec<String> {
    let lower = word.trim().to_lowercase();
    if lower.is_empty() {
        return Vec::new();
    }
    if lower == "island" {
        return ["айленд", "исланд", "айлэнд", "ислэнд", "айланд"].map(String::from).to_vec();
    }
    if !lower.chars().any(|c| c.is_ascii_alphabetic()) {
        return vec![lower];
    }
    let map = |c: char| -> &'static str {
        match c {
            'a' => "а",
            'b' => "б",
            'c' => "к",
            'd' => "д",
            'e' => "е",
            'f' => "ф",
            'g' => "г",
            'h' => "х",
            'i' => "и",
            'j' => "дж",
            'k' => "к",
            'l' => "л",
            'm' => "м",
            'n' => "н",
            'o' => "о",
            'p' => "п",
            'q' => "к",
            'r' => "р",
            's' => "с",
            't' => "т",
            'u' => "у",
            'v' => "в",
            'w' => "в",
            'x' => "кс",
            'y' => "и",
            'z' => "з",
            _ => "",
        }
    };
    vec![lower.chars().map(|c| if c.is_ascii_alphabetic() { map(c) } else { "" }).collect()]
}

const GREETINGS: [&str; 7] = ["хей", "эй", "окей", "окэй", "ок", "хэй", "привет"];

pub fn keywords_text(wake_words: &[String], vocab: &HashMap<String, ()>) -> String {
    let mut lines: Vec<String> = Vec::new();
    for word in wake_words {
        for spoken in translit(word) {
            let Some(body) = tokenize(&spoken, vocab) else { continue };
            lines.push(format!("{} :{SCORE} #{THRESHOLD} @{spoken}", body.join(" ")));
            for greeting in GREETINGS {
                let Some(head) = tokenize(greeting, vocab) else { continue };
                let label: String = format!("{greeting}_{spoken}").replace(' ', "_");
                lines.push(format!("{} {} :{SCORE} #{THRESHOLD} @{label}", head.join(" "), body.join(" ")));
            }
        }
    }
    lines.dedup();
    lines.join("\n")
}

#[repr(C)]
struct TransducerConfig {
    encoder: *const c_char,
    decoder: *const c_char,
    joiner: *const c_char,
}

#[repr(C)]
struct ParaformerConfig {
    encoder: *const c_char,
    decoder: *const c_char,
}

#[repr(C)]
struct SingleModelConfig {
    model: *const c_char,
}

#[repr(C)]
struct OnlineModelConfig {
    transducer: TransducerConfig,
    paraformer: ParaformerConfig,
    zipformer2_ctc: SingleModelConfig,
    tokens: *const c_char,
    num_threads: i32,
    provider: *const c_char,
    debug: i32,
    model_type: *const c_char,
    modeling_unit: *const c_char,
    bpe_vocab: *const c_char,
    tokens_buf: *const c_char,
    tokens_buf_size: i32,
    nemo_ctc: SingleModelConfig,
    t_one_ctc: SingleModelConfig,
}

#[repr(C)]
struct FeatureConfig {
    sample_rate: i32,
    feature_dim: i32,
}

#[repr(C)]
struct KeywordResult {
    keyword: *const c_char,
    tokens: *const c_char,
    tokens_arr: *const *const c_char,
    count: i32,
    timestamps: *mut c_float,
    start_time: c_float,
    json: *const c_char,
}

#[repr(C)]
struct SpotterConfig {
    feat_config: FeatureConfig,
    model_config: OnlineModelConfig,
    max_active_paths: i32,
    num_trailing_blanks: i32,
    keywords_score: c_float,
    keywords_threshold: c_float,
    keywords_file: *const c_char,
    keywords_buf: *const c_char,
    keywords_buf_size: i32,
}

enum Spotter {}
enum Stream {}

type FnVersion = unsafe extern "C" fn() -> *const c_char;
type FnCreateSpotter = unsafe extern "C" fn(*const SpotterConfig) -> *const Spotter;
type FnDestroySpotter = unsafe extern "C" fn(*const Spotter);
type FnCreateStream = unsafe extern "C" fn(*const Spotter) -> *const Stream;
type FnDestroyStream = unsafe extern "C" fn(*const Stream);
type FnAccept = unsafe extern "C" fn(*const Stream, i32, *const c_float, i32);
type FnReady = unsafe extern "C" fn(*const Spotter, *const Stream) -> i32;
type FnDecode = unsafe extern "C" fn(*const Spotter, *const Stream);
type FnReset = unsafe extern "C" fn(*const Spotter, *const Stream);
type FnResult = unsafe extern "C" fn(*const Spotter, *const Stream) -> *const KeywordResult;
type FnDestroyResult = unsafe extern "C" fn(*const KeywordResult);

struct Api {
    version: FnVersion,
    create_spotter: FnCreateSpotter,
    destroy_spotter: FnDestroySpotter,
    create_stream: FnCreateStream,
    destroy_stream: FnDestroyStream,
    accept: FnAccept,
    ready: FnReady,
    decode: FnDecode,
    reset: FnReset,
    result: FnResult,
    destroy_result: FnDestroyResult,
}

pub(crate) unsafe fn symbol<T: Copy>(module: windows::Win32::Foundation::HMODULE, name: &str) -> Option<T> {
    let c = CString::new(name).ok()?;
    let addr = unsafe { GetProcAddress(module, PCSTR(c.as_ptr().cast())) }?;
    Some(unsafe { std::mem::transmute_copy::<_, T>(&addr) })
}

impl Api {
    fn load(root: &Path) -> Option<Api> {
        unsafe {
            let ort = root.join("bin").join("onnxruntime.dll");
            let sherpa = root.join("bin").join("sherpa-onnx-c-api.dll");
            LoadLibraryExW(&HSTRING::from(ort.as_os_str()), None, LOAD_WITH_ALTERED_SEARCH_PATH).ok()?;
            let module =
                LoadLibraryExW(&HSTRING::from(sherpa.as_os_str()), None, LOAD_WITH_ALTERED_SEARCH_PATH).ok()?;
            let api = Api {
                version: symbol(module, "SherpaOnnxGetVersionStr")?,
                create_spotter: symbol(module, "SherpaOnnxCreateKeywordSpotter")?,
                destroy_spotter: symbol(module, "SherpaOnnxDestroyKeywordSpotter")?,
                create_stream: symbol(module, "SherpaOnnxCreateKeywordStream")?,
                destroy_stream: symbol(module, "SherpaOnnxDestroyOnlineStream")?,
                accept: symbol(module, "SherpaOnnxOnlineStreamAcceptWaveform")?,
                ready: symbol(module, "SherpaOnnxIsKeywordStreamReady")?,
                decode: symbol(module, "SherpaOnnxDecodeKeywordStream")?,
                reset: symbol(module, "SherpaOnnxResetKeywordStream")?,
                result: symbol(module, "SherpaOnnxGetKeywordResult")?,
                destroy_result: symbol(module, "SherpaOnnxDestroyKeywordResult")?,
            };
            let text = CStr::from_ptr((api.version)()).to_string_lossy().into_owned();
            (text == RUNTIME_VERSION).then_some(api)
        }
    }
}

struct Detector {
    api: Api,
    spotter: *const Spotter,
    stream: *const Stream,
    hangover: u32,
}

impl Detector {
    fn new(root: &Path, keywords: &str) -> Option<Detector> {
        let api = Api::load(root)?;
        let path = |p: &str| CString::new(root.join(p).to_string_lossy().as_bytes()).ok();
        let (encoder, decoder, joiner, tokens) = (
            path("model/encoder.int8.onnx")?,
            path("model/decoder.onnx")?,
            path("model/joiner.int8.onnx")?,
            path("model/tokens.txt")?,
        );
        let provider = CString::new("cpu").ok()?;
        let buf = CString::new(keywords).ok()?;
        let mut cfg: SpotterConfig = unsafe { std::mem::zeroed() };
        cfg.feat_config = FeatureConfig { sample_rate: SAMPLE_RATE, feature_dim: 80 };
        cfg.model_config.transducer =
            TransducerConfig { encoder: encoder.as_ptr(), decoder: decoder.as_ptr(), joiner: joiner.as_ptr() };
        cfg.model_config.tokens = tokens.as_ptr();
        cfg.model_config.num_threads = 1;
        cfg.model_config.provider = provider.as_ptr();
        cfg.max_active_paths = 4;
        cfg.num_trailing_blanks = 1;
        cfg.keywords_score = SCORE;
        cfg.keywords_threshold = THRESHOLD;
        cfg.keywords_buf = buf.as_ptr();
        cfg.keywords_buf_size = keywords.len() as i32;
        let spotter = unsafe { (api.create_spotter)(&cfg) };
        if spotter.is_null() {
            return None;
        }
        let stream = unsafe { (api.create_stream)(spotter) };
        if stream.is_null() {
            unsafe { (api.destroy_spotter)(spotter) };
            return None;
        }
        Some(Detector { api, spotter, stream, hangover: 0 })
    }

    fn clear(&mut self) {
        self.hangover = 0;
        unsafe { (self.api.reset)(self.spotter, self.stream) };
    }

    fn feed(&mut self, chunk: &[i16]) -> bool {
        let floats: Vec<f32> = chunk.iter().map(|&s| f32::from(s) / 32768.0).collect();
        if rms(&floats) >= GATE_RMS {
            self.hangover = HANGOVER_CHUNKS;
        } else if self.hangover > 0 {
            self.hangover -= 1;
        } else {
            return false;
        }
        unsafe {
            (self.api.accept)(self.stream, SAMPLE_RATE, floats.as_ptr(), floats.len() as i32);
            while (self.api.ready)(self.spotter, self.stream) != 0 {
                (self.api.decode)(self.spotter, self.stream);
            }
            let found = (self.api.result)(self.spotter, self.stream);
            if found.is_null() {
                return false;
            }
            let hit = !(*found).keyword.is_null() && !CStr::from_ptr((*found).keyword).to_bytes().is_empty();
            if hit {
                crate::log_info!("Wake word heard: {}", CStr::from_ptr((*found).keyword).to_string_lossy());
            }
            (self.api.destroy_result)(found);
            if hit {
                (self.api.reset)(self.spotter, self.stream);
                self.hangover = 0;
            }
            hit
        }
    }
}

impl Drop for Detector {
    fn drop(&mut self) {
        unsafe {
            (self.api.destroy_stream)(self.stream);
            (self.api.destroy_spotter)(self.spotter);
        }
    }
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

fn report(request: &Request, result: Result<(), String>) {
    if REVISION.load(Ordering::Acquire) != request.revision {
        return;
    }
    *STATUS.lock().unwrap_or_else(|e| e.into_inner()) = Some((request.revision, result));
    unsafe {
        let _ = PostMessageW(Some(HWND(request.hwnd as *mut c_void)), WM_WAKE_STATUS, WPARAM(0), LPARAM(0));
    }
}

pub fn take_status() -> Option<Result<(), String>> {
    STATUS
        .lock()
        .ok()?
        .take()
        .filter(|(revision, _)| *revision == REVISION.load(Ordering::Acquire))
        .map(|(_, result)| result)
}

fn listen(request: &Request) -> Result<(), String> {
    let vocab = std::fs::read_to_string(request.root.join("model/tokens.txt"))
        .map(|t| load_vocab(&t))
        .map_err(|_| "Could not read the wake-word vocabulary")?;
    let keywords = keywords_text(&request.words, &vocab);
    if keywords.is_empty() {
        return Err("This name cannot be recognized. Try another name.".into());
    }
    let mut detector = Detector::new(&request.root, &keywords).ok_or("Could not load the wake-word model")?;
    let mut was_paused = false;
    report(request, Ok(()));
    dictation::stream(|chunk| {
        if REVISION.load(Ordering::Acquire) != request.revision {
            return false;
        }
        if PAUSED.load(Ordering::SeqCst) {
            was_paused = true;
            return true;
        }
        if was_paused {
            was_paused = false;
            detector.clear();
        }
        if detector.feed(chunk) && REVISION.load(Ordering::Acquire) == request.revision {
            PAUSED.store(true, Ordering::SeqCst);
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(request.hwnd as *mut c_void)),
                    WM_WAKE,
                    WPARAM(request.revision as usize),
                    LPARAM(0),
                );
            }
        }
        true
    })
    .map_err(|_| "The microphone is unavailable. Check the recording device.".into())
}

pub fn start(hwnd: HWND, wake_words: Vec<String>) -> bool {
    let Some(root) = dir().filter(|d| installed_in(d)) else { return false };
    let mut control = CONTROL.lock().unwrap_or_else(|e| e.into_inner());
    if control.request.as_ref().is_some_and(|r| r.words == wake_words && r.hwnd == hwnd.0 as isize) {
        return true;
    }
    let revision = REVISION.fetch_add(1, Ordering::AcqRel) + 1;
    let spawn = control.replace(Request { root, words: wake_words, hwnd: hwnd.0 as isize, revision });
    PAUSED.store(false, Ordering::SeqCst);
    if !spawn {
        return true;
    }
    LISTENING.store(true, Ordering::SeqCst);
    drop(control);
    std::thread::spawn(|| {
        loop {
            let request = {
                let mut control = CONTROL.lock().unwrap_or_else(|e| e.into_inner());
                match control.request.clone() {
                    Some(request) => request,
                    None => {
                        control.running = false;
                        LISTENING.store(false, Ordering::SeqCst);
                        return;
                    }
                }
            };
            if let Err(why) = listen(&request) {
                report(&request, Err(why));
            }
            let mut control = CONTROL.lock().unwrap_or_else(|e| e.into_inner());
            if !control.finish(request.revision) {
                LISTENING.store(false, Ordering::SeqCst);
                return;
            }
        }
    });
    true
}

pub fn stop() {
    let mut control = CONTROL.lock().unwrap_or_else(|e| e.into_inner());
    control.request = None;
    REVISION.fetch_add(1, Ordering::AcqRel);
}

pub fn pause() {
    PAUSED.store(true, Ordering::SeqCst);
}

pub fn resume() {
    PAUSED.store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Loads the installed keyword model with different names; does not open a microphone"]
    fn installed_keyword_model_can_reload_with_a_new_name() {
        let root = dir().expect("Profile directory");
        let tokens = std::fs::read_to_string(root.join("model/tokens.txt")).unwrap();
        let vocab = load_vocab(&tokens);
        for name in ["Island", "Нова", "Айленд"] {
            let keywords = keywords_text(&[name.into()], &vocab);
            assert!(!keywords.is_empty());
            let mut detector = Detector::new(&root, &keywords).expect("Keyword model should reload for every name");
            assert!(!detector.feed(&[0; 480]));
        }
    }
    #[test]
    fn a_new_name_is_not_lost_while_the_previous_listener_is_stopping() {
        let request =
            |revision: u64| Request { root: PathBuf::new(), words: vec![revision.to_string()], hwnd: 0, revision };
        let mut state = Control { request: None, running: false };
        assert!(state.replace(request(1)));
        state.request = None;
        assert!(!state.replace(request(2)));
        assert!(state.finish(1));
        assert_eq!(state.request.as_ref().unwrap().words, ["2"]);
        assert!(state.running);
        assert!(!state.finish(2));
        assert!(state.replace(request(3)));
    }
    fn vocab() -> HashMap<String, ()> {
        load_vocab("<blk> 0\n▁х 1\nей 2\n▁а 3\nй 4\nле 5\nн 6\nд 7\n▁и 8\n▁э 9\nл 10\nа 11\nс 12\n▁ис 13\nла 14\n")
    }

    #[test]
    fn words_are_cut_into_the_longest_known_pieces() {
        let v = vocab();
        assert_eq!(tokenize("хей", &v).unwrap(), ["▁х", "ей"]);
        assert_eq!(tokenize("Айленд", &v).unwrap(), ["▁а", "й", "ле", "н", "д"]);
        assert_eq!(tokenize("исланд", &v).unwrap(), ["▁ис", "ла", "н", "д"]);
        assert_eq!(tokenize("ж", &v), None);
        assert_eq!(tokenize("  ", &v), None);
    }

    #[test]
    fn latin_call_words_are_spelled_the_way_a_russian_model_hears_them() {
        assert_eq!(translit("Island"), ["айленд", "исланд", "айлэнд", "ислэнд", "айланд"]);
        assert_eq!(translit("Айленд"), ["айленд"]);
        assert_eq!(translit("Nova"), ["нова"]);
        assert!(translit("  ").is_empty());
    }

    #[test]
    fn keywords_cover_every_greeting_and_spelling_and_skip_unknown_words() {
        let v = vocab();
        let text = keywords_text(&["Island".to_string()], &v);
        assert!(text.lines().count() >= 2);
        assert!(text.lines().all(|l| l.contains(&format!(" :{SCORE} #{THRESHOLD} @"))));
        assert!(text.lines().any(|l| l.starts_with("▁а й ле н д ")), "the bare word is a call too");
        assert!(text.contains("▁х ей ▁а й ле н д"));
        assert!(text.contains("▁ис ла н д"));
        assert!(keywords_text(&["Жжж".to_string()], &v).is_empty());
        assert!(keywords_text(&[], &v).is_empty());
    }

    #[test]
    fn the_energy_gate_tells_silence_from_speech() {
        assert!(rms(&[0.0; 480]) < GATE_RMS);
        assert!(rms(&[0.05; 480]) > GATE_RMS);
        assert_eq!(rms(&[]), 0.0);
    }

    #[test]
    fn the_runtime_counts_as_installed_only_when_every_file_is_there() {
        let root = std::env::temp_dir().join(format!("ni-wake-test-{}", std::process::id()));
        assert!(!installed_in(&root));
        for f in FILES {
            let p = root.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"x").unwrap();
        }
        assert!(installed_in(&root));
        std::fs::remove_file(root.join(FILES[3])).unwrap();
        assert!(!installed_in(&root));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[ignore = "needs the downloaded runtime and NI_WAKE_WAV pointing at a 16 kHz mono recording"]
    fn a_recording_of_the_call_phrase_is_detected() {
        let root = dir().expect("a profile folder");
        let vocab = load_vocab(&std::fs::read_to_string(root.join("model/tokens.txt")).unwrap());
        let keywords = match std::env::var("NI_WAKE_KEYWORDS") {
            Ok(path) => std::fs::read_to_string(path).unwrap(),
            Err(_) => keywords_text(&["Island".to_string()], &vocab),
        };
        println!("{keywords}");
        let mut d = Detector::new(&root, &keywords).expect("the runtime loaded");
        let wav = match std::env::var("NI_WAKE_WAV") {
            Ok(path) => std::fs::read(path).unwrap(),
            Err(_) => {
                unsafe {
                    let _ = windows::Win32::System::Com::CoInitializeEx(
                        None,
                        windows::Win32::System::Com::COINIT_MULTITHREADED,
                    );
                }
                crate::platform::speech::synthesize(&std::env::var("NI_WAKE_SAY").unwrap_or("Хей, Айленд".into()))
                    .expect("a voice")
            }
        };
        let data_at = wav.windows(4).position(|w| w == b"data").expect("a data chunk") + 8;
        let rate = u32::from_le_bytes(wav[24..28].try_into().unwrap()) as f64;
        let source: Vec<i16> = wav[data_at..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        let n = (source.len() as f64 * 16_000.0 / rate) as usize;
        let samples: Vec<i16> = (0..n)
            .map(|i| {
                let at = i as f64 * rate / 16_000.0;
                let (a, b) = (source[at as usize], source[(at as usize + 1).min(source.len() - 1)]);
                (f64::from(a) + (f64::from(b) - f64::from(a)) * at.fract()) as i16
            })
            .collect();
        let mut padded = samples.clone();
        padded.extend(std::iter::repeat_n(0, 16_000));
        assert!(padded.chunks(480).any(|c| d.feed(c)), "the phrase was not heard");
    }

    #[test]
    #[ignore = "needs the downloaded runtime in %APPDATA%\\NightIsland\\wake"]
    fn the_installed_runtime_loads_and_accepts_silence() {
        let root = dir().expect("a profile folder");
        let vocab = load_vocab(&std::fs::read_to_string(root.join("model/tokens.txt")).unwrap());
        let keywords = keywords_text(&["Island".to_string()], &vocab);
        let mut d = Detector::new(&root, &keywords).expect("the runtime loaded");
        for _ in 0..50 {
            assert!(!d.feed(&[0; 480]));
        }
    }
}
