use std::ffi::c_void;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{HOT_KEY_MODIFIERS, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SW_SHOWNORMAL, WM_APP};
use windows::core::{HSTRING, PCWSTR, w};

use crate::platform::gamepad::VirtualPad;
use crate::platform::input_inject::{Injector, find_window, foreground_title_contains};
use crate::tools::macro_script::{self, Button, Macro, Step};

pub const WM_MACRO: u32 = WM_APP + 132;
pub const HOTKEY_BASE: i32 = 0x1000;
pub const EXTENSION: &str = "nimacro";

const MAX_FILE_BYTES: u64 = 256 * 1024;
const MAX_MACROS: usize = 200;
const MAX_EXECUTED: u64 = 50_000_000;
const START_DELAY_MS: u64 = 300;

pub struct Entry {
    pub stem: String,
    pub text: String,
    pub parsed: Result<Macro, String>,
}

impl Entry {
    pub fn title(&self) -> String {
        match &self.parsed {
            Ok(m) if !m.name.is_empty() => m.name.clone(),
            _ => self.stem.clone(),
        }
    }
}

static RUN: Mutex<Option<(String, Arc<AtomicBool>)>> = Mutex::new(None);
static STATUS: Mutex<Option<(String, Result<(), String>)>> = Mutex::new(None);
static HOTKEYS: Mutex<Vec<(i32, String)>> = Mutex::new(Vec::new());

pub fn dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("NightIsland").join("macros"))
}

pub fn path(stem: &str) -> Option<PathBuf> {
    Some(dir()?.join(format!("{stem}.{EXTENSION}")))
}

pub fn sanitize_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '(' | ')') { c } else { ' ' })
        .collect();
    let joined = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut stem: String = joined.trim_matches(|c| c == '.' || c == ' ').chars().take(60).collect();
    let upper = stem.to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&upper.as_str())
        || (upper.len() == 4 && (upper.starts_with("COM") || upper.starts_with("LPT")));
    if stem.is_empty() {
        stem.push_str("macro");
    } else if reserved {
        stem.push('_');
    }
    stem
}

pub fn unique_stem(base: &str) -> String {
    let base = sanitize_stem(base);
    let mut stem = base.clone();
    let mut n = 2;
    while path(&stem).is_some_and(|p| p.exists()) {
        stem = format!("{base} {n}");
        n += 1;
    }
    stem
}

fn read_limited(path: &PathBuf) -> Option<String> {
    let mut bytes = Vec::new();
    fs::File::open(path).ok()?.take(MAX_FILE_BYTES).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn list() -> Vec<Entry> {
    let Some(dir) = dir() else { return Vec::new() };
    let _ = fs::create_dir_all(&dir);
    let mut stems: Vec<String> = fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| {
            let path = e.path();
            (path.extension().and_then(|x| x.to_str()) == Some(EXTENSION))
                .then(|| path.file_stem().and_then(|s| s.to_str()).map(str::to_string))
                .flatten()
        })
        .collect();
    stems.sort_by_key(|s| s.to_lowercase());
    stems.truncate(MAX_MACROS);
    stems.into_iter().filter_map(|stem| load(&stem)).collect()
}

pub fn load(stem: &str) -> Option<Entry> {
    let text = read_limited(&path(stem)?)?;
    let parsed = macro_script::parse(&text);
    Some(Entry { stem: stem.to_string(), text, parsed })
}

pub fn save_text(stem: &str, text: &str) -> bool {
    let Some(path) = path(stem) else { return false };
    if text.len() as u64 > MAX_FILE_BYTES {
        return false;
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text).is_ok() && fs::rename(&tmp, &path).is_ok()
}

pub fn create(name: &str, text: &str) -> Option<String> {
    let stem = unique_stem(name);
    save_text(&stem, text).then_some(stem)
}

pub fn delete(stem: &str) {
    if let Some(path) = path(stem) {
        let _ = fs::remove_file(path);
    }
}

const GUIDE: &str = include_str!("../../docs/macros.md");

fn has_association(extension: &str) -> bool {
    use windows::Win32::UI::Shell::{ASSOCF_NONE, ASSOCSTR_EXECUTABLE, AssocQueryStringW};
    use windows::core::PWSTR;
    let mut buffer = [0u16; 520];
    let mut length = buffer.len() as u32;
    unsafe {
        AssocQueryStringW(
            ASSOCF_NONE,
            ASSOCSTR_EXECUTABLE,
            &HSTRING::from(extension),
            PCWSTR::null(),
            Some(PWSTR(buffer.as_mut_ptr())),
            &mut length,
        )
    }
    .is_ok()
}

fn open_with_default(path: &std::path::Path) {
    let extension = path.extension().and_then(|e| e.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
    let file = HSTRING::from(path.to_string_lossy().as_ref());
    unsafe {
        if has_association(&extension) {
            ShellExecuteW(None, w!("open"), &file, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
        } else {
            let quoted = HSTRING::from(format!("\"{}\"", path.display()));
            ShellExecuteW(None, w!("open"), w!("notepad.exe"), &quoted, PCWSTR::null(), SW_SHOWNORMAL);
        }
    }
}

pub fn open_file(stem: &str) -> bool {
    match path(stem).filter(|p| p.is_file()) {
        Some(path) => {
            open_with_default(&path);
            true
        }
        None => false,
    }
}

pub fn open_guide() -> bool {
    let Some(dir) = dir() else { return false };
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("Guide.md");
    if fs::read_to_string(&path).ok().as_deref() != Some(GUIDE) && fs::write(&path, GUIDE).is_err() {
        return false;
    }
    open_with_default(&path);
    true
}

pub fn open_folder() {
    if let Some(dir) = dir() {
        let _ = fs::create_dir_all(&dir);
        unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                &HSTRING::from(dir.to_string_lossy().as_ref()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            );
        }
    }
}

pub fn running() -> Option<String> {
    RUN.lock().ok()?.as_ref().map(|(stem, _)| stem.clone())
}

pub fn stop() {
    if let Ok(run) = RUN.lock()
        && let Some((_, cancel)) = run.as_ref()
    {
        cancel.store(true, Ordering::SeqCst);
    }
}

pub fn take_status() -> Option<(String, Result<(), String>)> {
    STATUS.lock().ok()?.take()
}

fn wait_for_release() {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    let held = || {
        [0x10, 0x11, 0x12, 0x5B, 0x5C].into_iter().any(|vk| {
            let state = unsafe { GetAsyncKeyState(vk) };
            state < 0
        })
    };
    let end = Instant::now() + Duration::from_secs(2);
    while held() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn start(hwnd: HWND, stem: &str, program: Macro, delayed: bool) -> bool {
    let Ok(mut run) = RUN.lock() else { return false };
    if run.is_some() {
        return false;
    }
    let cancel = Arc::new(AtomicBool::new(false));
    *run = Some((stem.to_string(), cancel.clone()));
    let (raw, stem) = (hwnd.0 as isize, stem.to_string());
    std::thread::spawn(move || {
        if delayed {
            std::thread::sleep(Duration::from_millis(START_DELAY_MS));
        }
        wait_for_release();
        let mut runner = Runner::new(&program, &cancel);
        let result = runner.run(&program);
        runner.release_all();
        if let Ok(mut status) = STATUS.lock() {
            *status = Some((stem, result));
        }
        if let Ok(mut run) = RUN.lock() {
            *run = None;
        }
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut c_void)), WM_MACRO, WPARAM(0), LPARAM(0));
        }
    });
    true
}

pub fn register_hotkeys(hwnd: HWND, entries: &[Entry]) -> Vec<String> {
    unregister_hotkeys(hwnd);
    let mut failed = Vec::new();
    let mut registered = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let Ok(program) = &entry.parsed else { continue };
        let Some(combo) = program.hotkey else { continue };
        let id = HOTKEY_BASE + index as i32;
        let ok =
            unsafe { RegisterHotKey(Some(hwnd), id, HOT_KEY_MODIFIERS(combo.mods) | MOD_NOREPEAT, combo.vk).is_ok() };
        if ok {
            registered.push((id, entry.stem.clone()));
        } else {
            failed.push(entry.title());
        }
    }
    if let Ok(mut slot) = HOTKEYS.lock() {
        *slot = registered;
    }
    failed
}

pub fn unregister_hotkeys(hwnd: HWND) {
    if let Ok(mut slot) = HOTKEYS.lock() {
        for (id, _) in slot.drain(..) {
            unsafe {
                let _ = UnregisterHotKey(Some(hwnd), id);
            }
        }
    }
}

pub fn stem_for_hotkey(id: i32) -> Option<String> {
    HOTKEYS.lock().ok()?.iter().find(|(i, _)| *i == id).map(|(_, stem)| stem.clone())
}

struct Runner<'a> {
    injector: Injector,
    pad: Option<VirtualPad>,
    cancel: &'a AtomicBool,
    keys: Vec<u16>,
    buttons: Vec<Button>,
    seed: u64,
    executed: u64,
}

impl<'a> Runner<'a> {
    fn new(program: &Macro, cancel: &'a AtomicBool) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0x9E37_79B9, |d| d.as_nanos() as u64)
            | 1;
        Self {
            injector: Injector::new(program.method, &program.target),
            pad: None,
            cancel,
            keys: Vec::new(),
            buttons: Vec::new(),
            seed,
            executed: 0,
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    fn random(&mut self, low: u32, high: u32) -> u32 {
        if low >= high {
            return low;
        }
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        low + (self.seed % (high - low + 1) as u64) as u32
    }

    fn sleep(&self, ms: u32) -> bool {
        let end = Instant::now() + Duration::from_millis(ms as u64);
        loop {
            if self.cancelled() {
                return false;
            }
            let now = Instant::now();
            if now >= end {
                return true;
            }
            std::thread::sleep((end - now).min(Duration::from_millis(15)));
        }
    }

    fn run(&mut self, program: &Macro) -> Result<(), String> {
        let mut round = 0u32;
        loop {
            if self.cancelled() {
                return Ok(());
            }
            if !self.steps(&program.steps, 0)? {
                return Ok(());
            }
            round += 1;
            if program.repeat != 0 && round >= program.repeat {
                return Ok(());
            }
            if program.steps.is_empty() || !self.sleep(1) {
                return Ok(());
            }
        }
    }

    fn steps(&mut self, steps: &[Step], depth: usize) -> Result<bool, String> {
        for step in steps {
            if self.cancelled() {
                return Ok(false);
            }
            self.executed += 1;
            if self.executed > MAX_EXECUTED {
                return Err("The macro ran too many steps".into());
            }
            if !self.step(step, depth)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn step(&mut self, step: &Step, depth: usize) -> Result<bool, String> {
        match step {
            Step::Tap(keys) => {
                for vk in keys {
                    self.press(*vk, true);
                    if !self.sleep(6) {
                        return Ok(false);
                    }
                }
                if !self.sleep(14) {
                    return Ok(false);
                }
                for vk in keys.iter().rev() {
                    self.press(*vk, false);
                    if !self.sleep(6) {
                        return Ok(false);
                    }
                }
            }
            Step::Down(vk) => self.press(*vk, true),
            Step::Up(vk) => self.press(*vk, false),
            Step::Type(text) => self.injector.text(text),
            Step::Delay(low, high) => {
                let ms = self.random(*low, *high);
                return Ok(self.sleep(ms));
            }
            Step::Move { x, y, relative } => self.injector.move_mouse(*x, *y, *relative),
            Step::Click { button, at } => {
                if let Some((x, y)) = at {
                    self.injector.move_mouse(*x, *y, false);
                    if !self.sleep(15) {
                        return Ok(false);
                    }
                }
                self.button(*button, true);
                if !self.sleep(25) {
                    return Ok(false);
                }
                self.button(*button, false);
            }
            Step::MouseDown(button) => self.button(*button, true),
            Step::MouseUp(button) => self.button(*button, false),
            Step::Wheel(delta) => self.injector.wheel(*delta),
            Step::Run { target, args } => {
                let result = unsafe {
                    ShellExecuteW(
                        None,
                        w!("open"),
                        &HSTRING::from(target.as_str()),
                        if args.is_empty() { PCWSTR::null() } else { PCWSTR(HSTRING::from(args.as_str()).as_ptr()) },
                        PCWSTR::null(),
                        SW_SHOWNORMAL,
                    )
                };
                if result.0 as isize <= 32 {
                    return Err(format!("Could not start {target}"));
                }
            }
            Step::WaitWindow { title, ms } => {
                let end = Instant::now() + Duration::from_millis(*ms as u64);
                while find_window(title).is_none() {
                    if Instant::now() >= end {
                        return Err(format!("The window \"{title}\" did not appear"));
                    }
                    if !self.sleep(100) {
                        return Ok(false);
                    }
                }
            }
            Step::PadTap(mask) => {
                self.pad()?.button(*mask, true);
                if !self.sleep(50) {
                    return Ok(false);
                }
                self.pad()?.button(*mask, false);
            }
            Step::PadDown(mask) => self.pad()?.button(*mask, true),
            Step::PadUp(mask) => self.pad()?.button(*mask, false),
            Step::PadStick { right, x, y } => self.pad()?.stick(*right, *x, *y),
            Step::PadTrigger { right, value } => self.pad()?.trigger(*right, *value),
            Step::Loop { count, body } => {
                if depth >= macro_script::MAX_DEPTH {
                    return Err("Blocks are nested too deeply".into());
                }
                let mut done = 0;
                while *count == 0 || done < *count {
                    if !self.steps(body, depth + 1)? {
                        return Ok(false);
                    }
                    done += 1;
                    if body.is_empty() && !self.sleep(1) {
                        return Ok(false);
                    }
                }
            }
            Step::IfWindow { title, body } => {
                if foreground_title_contains(title) && !self.steps(body, depth + 1)? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn pad(&mut self) -> Result<&mut VirtualPad, String> {
        if self.pad.is_none() {
            self.pad = Some(VirtualPad::connect()?);
        }
        self.pad.as_mut().ok_or_else(|| "The virtual gamepad is missing".to_string())
    }

    fn press(&mut self, vk: u16, down: bool) {
        self.injector.key(vk, down);
        if down {
            if !self.keys.contains(&vk) {
                self.keys.push(vk);
            }
        } else {
            self.keys.retain(|k| *k != vk);
        }
    }

    fn button(&mut self, button: Button, down: bool) {
        self.injector.button(button, down);
        if down {
            if !self.buttons.contains(&button) {
                self.buttons.push(button);
            }
        } else {
            self.buttons.retain(|b| *b != button);
        }
    }

    fn release_all(&mut self) {
        for vk in std::mem::take(&mut self.keys).into_iter().rev() {
            self.injector.key(vk, false);
        }
        for button in std::mem::take(&mut self.buttons) {
            self.injector.button(button, false);
        }
        if let Some(pad) = self.pad.as_mut() {
            pad.release_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_example_in_the_guide_parses() {
        let examples = GUIDE.split("## Examples").nth(1).and_then(|s| s.split("## Troubleshooting").next()).unwrap();
        let blocks: Vec<&str> = examples.split("```").skip(1).step_by(2).collect();
        assert!(blocks.len() >= 8);
        for block in blocks {
            let program = macro_script::parse(block).unwrap_or_else(|why| panic!("{why}\n{block}"));
            assert!(!program.steps.is_empty());
        }
    }

    #[test]
    fn file_names_are_made_safe() {
        assert_eq!(sanitize_stem("..\\evil/name"), "evil name");
        assert_eq!(sanitize_stem("CON"), "CON_");
        assert_eq!(sanitize_stem("   "), "macro");
        assert_eq!(sanitize_stem("Привет мир"), "Привет мир");
        assert!(sanitize_stem(&"a".repeat(200)).chars().count() <= 60);
    }
}
