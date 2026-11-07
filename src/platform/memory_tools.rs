use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{EmptyWorkingSet, GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_INFORMATION, PROCESS_SET_QUOTA, PROCESS_VM_READ,
    QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
use windows::core::PWSTR;

pub const WM_MEMORY_TOOLS: u32 = WM_APP + 101;
const MB: u64 = 1 << 20;

#[derive(Clone, Default)]
pub struct Application {
    pub path: String,
    pub name: String,
    pub working: u64,
    pub private: u64,
    pub minimized: bool,
    pub before: Option<u64>,
    pub after: Option<u64>,
    pub growth: u64,
}

#[derive(Clone, Default)]
pub struct Snapshot {
    pub apps: Vec<Application>,
    pub total: u64,
    pub available: u64,
    pub available_before: Option<u64>,
    pub available_after: Option<u64>,
    pub trimmed: usize,
    pub skipped: usize,
    pub watching: bool,
    pub elapsed: u64,
    pub failed: bool,
}

pub enum Command {
    Open,
    Close,
    Refresh,
    Watch(bool),
    Clean(Vec<String>, bool),
}

static CLIENT: OnceLock<Mutex<Option<mpsc::Sender<Command>>>> = OnceLock::new();
static SNAPSHOT: Mutex<Option<Snapshot>> = Mutex::new(None);
static CLEANING: AtomicBool = AtomicBool::new(false);

pub fn busy() -> bool {
    CLEANING.load(Ordering::Acquire)
}

pub fn snapshot() -> Option<Snapshot> {
    SNAPSHOT.lock().ok()?.clone()
}

pub fn send(hwnd: HWND, command: Command) -> bool {
    let Ok(mut slot) = CLIENT.get_or_init(|| Mutex::new(None)).lock() else { return false };
    if slot.is_none() {
        let (tx, rx) = mpsc::channel();
        let target = hwnd.0 as isize;
        if std::thread::Builder::new().name("memory-tools".into()).spawn(move || worker(target, rx)).is_err() {
            return false;
        }
        *slot = Some(tx);
    }
    let cleaning = matches!(command, Command::Clean(..));
    if cleaning && CLEANING.swap(true, Ordering::AcqRel) {
        return false;
    }
    let sent = slot.as_ref().is_some_and(|tx| tx.send(command).is_ok());
    if cleaning && !sent {
        CLEANING.store(false, Ordering::Release);
    }
    sent
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct Process {
    pid: u32,
    created: u64,
    path: String,
    working: u64,
    private: u64,
}

fn identity(handle: HANDLE) -> Option<u64> {
    let (mut created, mut exit, mut kernel, mut user) =
        (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe {
        GetProcessTimes(handle, &mut created, &mut exit, &mut kernel, &mut user).ok()?;
    }
    Some((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

fn usage(handle: HANDLE) -> Option<(u64, u64)> {
    let mut counters =
        PROCESS_MEMORY_COUNTERS_EX { cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32, ..Default::default() };
    unsafe {
        GetProcessMemoryInfo(handle, (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX).cast(), counters.cb).ok()?;
    }
    Some((counters.WorkingSetSize as u64, counters.PrivateUsage as u64))
}

fn process(pid: u32) -> Option<Process> {
    let handle = Handle(unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) }.ok()?);
    let mut buffer = [0u16; 2048];
    let mut length = buffer.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(handle.0, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut length).ok()?;
    }
    let path = crate::data::desktop_dock::key(&String::from_utf16_lossy(&buffer[..length as usize]));
    let created = identity(handle.0)?;
    let (working, private) = usage(handle.0)?;
    Some(Process { pid, created, path, working, private })
}

fn system_memory() -> (u64, u64) {
    let mut info = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    if unsafe { GlobalMemoryStatusEx(&mut info) }.is_ok() { (info.ullTotalPhys, info.ullAvailPhys) } else { (0, 0) }
}

fn scan() -> (Snapshot, Vec<Process>) {
    let (total, available) = system_memory();
    let mut result = Snapshot { total, available, ..Default::default() };
    let mut roots = HashMap::new();
    for window in super::desktop_dock::windows() {
        if window.island_settings {
            continue;
        }
        let path = crate::data::desktop_dock::key(&window.path);
        let minimized = window.minimized && !window.active;
        roots.entry(path).and_modify(|value| *value &= minimized).or_insert(minimized);
    }
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    let Ok(snapshot) = snapshot else {
        result.failed = true;
        return (result, Vec::new());
    };
    let snapshot = Handle(snapshot);
    let own = std::process::id();
    let mut session = 0;
    if unsafe { ProcessIdToSessionId(own, &mut session) }.is_err() {
        result.failed = true;
        return (result, Vec::new());
    }
    let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut valid = unsafe { Process32FirstW(snapshot.0, &mut entry) }.is_ok();
    let mut processes = Vec::new();
    while valid {
        let pid = entry.th32ProcessID;
        let mut candidate_session = u32::MAX;
        if pid != own
            && unsafe { ProcessIdToSessionId(pid, &mut candidate_session) }.is_ok()
            && candidate_session == session
            && let Some(process) = process(pid)
            && roots.contains_key(&process.path)
        {
            processes.push(process);
        }
        valid = unsafe { Process32NextW(snapshot.0, &mut entry) }.is_ok();
    }
    let windows =
        std::env::var("WINDIR").map(|p| crate::data::desktop_dock::key(&p)).unwrap_or_else(|_| "c:\\windows".into());
    let mut grouped: HashMap<String, Application> = HashMap::new();
    for process in &processes {
        let app = grouped.entry(process.path.clone()).or_insert_with(|| Application {
            path: process.path.clone(),
            name: std::path::Path::new(&process.path).file_stem().unwrap_or_default().to_string_lossy().into_owned(),
            minimized: roots[&process.path] && !process.path.starts_with(&(windows.clone() + "\\")),
            ..Default::default()
        });
        app.working += process.working;
        app.private += process.private;
    }
    result.apps = grouped.into_values().collect();
    result.apps.sort_by(|a, b| b.working.cmp(&a.working).then_with(|| a.path.cmp(&b.path)));
    result.apps.truncate(128);
    (result, processes)
}

fn trim(process: &Process) -> bool {
    let Ok(handle) =
        (unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_SET_QUOTA, false, process.pid) })
    else {
        return false;
    };
    let handle = Handle(handle);
    if identity(handle.0) != Some(process.created) {
        return false;
    }
    unsafe { EmptyWorkingSet(handle.0).is_ok() }
}

#[derive(Default)]
struct Report {
    baseline: HashMap<String, (u64, u64)>,
    available: (u64, u64),
    trimmed: usize,
    skipped: usize,
    at: Option<Instant>,
}

impl Report {
    fn apply(&self, snapshot: &mut Snapshot, watching: bool) {
        if let Some(at) = self.at {
            snapshot.available_before = Some(self.available.0);
            snapshot.available_after = Some(self.available.1);
            snapshot.trimmed = self.trimmed;
            snapshot.skipped = self.skipped;
            snapshot.elapsed = at.elapsed().as_secs();
            snapshot.watching = watching;
            for app in &mut snapshot.apps {
                if let Some(&(before, after)) = self.baseline.get(&app.path) {
                    app.before = Some(before);
                    app.after = Some(after);
                    app.growth = app.working.saturating_sub(after);
                }
            }
            snapshot.apps.sort_by(|a, b| {
                b.growth.cmp(&a.growth).then_with(|| b.working.cmp(&a.working)).then_with(|| a.path.cmp(&b.path))
            });
        }
    }
}

fn clean(paths: Vec<String>) -> (Snapshot, Report) {
    let selected: HashSet<_> = paths.into_iter().take(128).collect();
    let (before, processes) = scan();
    let eligible: HashSet<_> = before
        .apps
        .iter()
        .filter(|app| app.minimized && selected.contains(&app.path))
        .map(|app| app.path.clone())
        .collect();
    let mut report = Report { skipped: selected.len().saturating_sub(eligible.len()), ..Default::default() };
    for process in processes.iter().filter(|p| eligible.contains(&p.path)) {
        if trim(process) {
            report.trimmed += 1;
        } else {
            report.skipped += 1;
        }
    }
    let (after, _) = scan();
    report.available = (before.available, after.available);
    report.at = Some(Instant::now());
    for app in before.apps.iter().filter(|app| eligible.contains(&app.path)) {
        if let Some(after) = after.apps.iter().find(|a| a.path == app.path) {
            report.baseline.insert(app.path.clone(), (app.working, after.working));
        }
    }
    (after, report)
}

fn worker(hwnd: isize, rx: mpsc::Receiver<Command>) {
    let mut visible = false;
    let mut until = None;
    let mut report = Report::default();
    loop {
        let watching = until.is_some();
        let received = if visible || watching {
            rx.recv_timeout(Duration::from_secs(3))
        } else {
            rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        };
        let command = match received {
            Ok(command) => Some(command),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let mut next = match command {
            Some(Command::Clean(paths, watch)) => {
                let (snapshot, latest) = clean(paths);
                report = latest;
                CLEANING.store(false, Ordering::Release);
                until = watch.then(|| Instant::now() + Duration::from_secs(120));
                snapshot
            }
            Some(Command::Close) => {
                visible = false;
                continue;
            }
            Some(Command::Watch(on)) => {
                until = on.then(|| report.at.map(|at| at + Duration::from_secs(120))).flatten();
                scan().0
            }
            Some(Command::Open) => {
                visible = true;
                scan().0
            }
            Some(Command::Refresh) | None => scan().0,
        };
        if until.is_some_and(|until| Instant::now() >= until) {
            until = None;
        }
        report.apply(&mut next, until.is_some());
        if let Ok(mut snapshot) = SNAPSHOT.lock() {
            *snapshot = Some(next);
        }
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as _)), WM_MEMORY_TOOLS, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn megabytes(bytes: u64) -> u64 {
    bytes / MB
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_only_growth_since_cleaning_and_preserves_private_memory() {
        let report = Report {
            baseline: HashMap::from([("a".into(), (200 * MB, 50 * MB))]),
            at: Some(Instant::now()),
            ..Default::default()
        };
        let mut snapshot = Snapshot {
            apps: vec![
                Application { path: "a".into(), working: 90 * MB, private: 220 * MB, ..Default::default() },
                Application { path: "b".into(), working: 500 * MB, ..Default::default() },
            ],
            ..Default::default()
        };
        report.apply(&mut snapshot, true);
        assert_eq!(snapshot.apps[0].path, "a");
        assert_eq!(snapshot.apps[0].growth, 40 * MB);
        assert_eq!(snapshot.apps[0].private, 220 * MB);
        assert_eq!(snapshot.apps[1].growth, 0);
        snapshot.apps[0].working = 30 * MB;
        report.apply(&mut snapshot, false);
        assert!(snapshot.apps.iter().all(|app| app.growth == 0));
        assert!(!snapshot.watching);
    }

    #[test]
    #[ignore = "Reads native Windows process and system memory counters"]
    fn native_memory_scan_is_read_only_and_reports_physical_memory() {
        let (snapshot, _) = scan();
        assert!(!snapshot.failed);
        assert!(snapshot.total > 256 * MB);
        assert!(snapshot.available <= snapshot.total);
        assert!(snapshot.apps.iter().all(|app| app.before.is_none() && app.after.is_none()));
    }

    #[test]
    #[ignore = "Helper for the isolated memory trimming test"]
    fn memory_fixture_child() {
        let Some(path) = std::env::var_os("NI_MEMORY_FIXTURE") else { return };
        let mut pages = vec![0u8; 64 * MB as usize];
        for byte in pages.iter_mut().step_by(4096) {
            *byte = 123;
        }
        std::fs::write(path, "ready").unwrap();
        std::thread::sleep(Duration::from_secs(15));
        std::hint::black_box(pages);
    }

    #[test]
    #[ignore = "Trims only an isolated child test process on Windows"]
    fn native_trim_checks_process_identity_and_keeps_allocations() {
        use std::os::windows::process::CommandExt;
        let marker = std::env::temp_dir().join(format!("ni-memory-fixture-{}.ready", std::process::id()));
        if marker.exists() {
            std::fs::remove_file(&marker).unwrap();
        }
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "platform::memory_tools::tests::memory_fixture_child", "--ignored"])
            .env("NI_MEMORY_FIXTURE", &marker)
            .creation_flags(0x08000000)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let checked = std::panic::catch_unwind(|| {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !marker.exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(30));
            }
            assert!(marker.exists(), "Memory fixture did not become ready");
            let before = process(child.id()).unwrap();
            let invalid = Process {
                pid: before.pid,
                created: before.created ^ 1,
                path: before.path.clone(),
                working: before.working,
                private: before.private,
            };
            assert!(!trim(&invalid));
            assert!(trim(&before));
            let after = process(child.id()).unwrap();
            assert!(before.working.saturating_sub(after.working) > 16 * MB);
            assert!(before.private.abs_diff(after.private) < 4 * MB);
        });
        let _ = child.kill();
        let _ = child.wait();
        if marker.exists() {
            std::fs::remove_file(marker).unwrap();
        }
        if let Err(error) = checked {
            std::panic::resume_unwind(error);
        }
    }
}
