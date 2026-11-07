use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::data::desktop_dock::{MAX_PINS, Pin};

pub const REQUEST_IMPORT: usize = 4;
pub const IMPORT_READY: usize = 5;

pub struct Import {
    pub manual: bool,
    pub pins: Vec<(Pin, String)>,
}

static BUSY: AtomicBool = AtomicBool::new(false);
static RESULT: Mutex<Option<Import>> = Mutex::new(None);

pub fn start(hwnd: HWND, manual: bool) {
    if BUSY.swap(true, Ordering::AcqRel) {
        return;
    }
    let hwnd = hwnd.0 as isize;
    let worker = std::thread::Builder::new().name("taskbar-pins".into()).spawn(move || {
        let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
        let pins = std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .map(|root| {
                read(
                    &root.join("Microsoft/Internet Explorer/Quick Launch/User Pinned/TaskBar"),
                    &root.join("NightIsland/dock-shortcuts"),
                )
            })
            .unwrap_or_default();
        if initialized {
            unsafe { CoUninitialize() };
        }
        if let Ok(mut result) = RESULT.lock() {
            *result = Some(Import { manual, pins });
        }
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd as _)),
                crate::ui::desktop_dock::WM_DESKTOP_DOCK,
                WPARAM(IMPORT_READY),
                LPARAM(0),
            );
        }
        BUSY.store(false, Ordering::Release);
    });
    if worker.is_err() {
        BUSY.store(false, Ordering::Release);
    }
}

pub fn take() -> Option<Import> {
    RESULT.lock().ok()?.take()
}

fn read(root: &std::path::Path, destination: &std::path::Path) -> Vec<(Pin, String)> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("lnk")))
        .take(MAX_PINS * 2)
        .collect();
    paths.sort_by_cached_key(|path| path.file_name().unwrap_or_default().to_string_lossy().to_lowercase());
    if std::fs::create_dir_all(destination).is_err() {
        return Vec::new();
    }
    paths
        .into_iter()
        .take(MAX_PINS)
        .filter_map(|path| {
            if path.metadata().ok()?.len() > 1024 * 1024 {
                return None;
            }
            let stored = destination.join(path.file_name()?);
            std::fs::copy(&path, &stored).ok()?;
            let pin = Pin::from_path(&stored.to_string_lossy());
            let target = super::desktop_dock::resolved_target(&pin.target);
            Some((pin, target))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn importing_taskbar_links_preserves_the_shortcut_and_ignores_other_files() {
        let root = std::env::temp_dir().join(format!("ni-taskbar-fixture-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for name in ["Browser.lnk", "Chat.LNK", "unrelated.txt"] {
            std::fs::write(root.join(name), []).unwrap();
        }
        let destination = root.join("imported");
        let pins = read(&root, &destination);
        assert_eq!(pins.len(), 2);
        assert_eq!(pins[0].0.name, "Browser");
        assert_eq!(pins[1].0.name, "Chat");
        assert!(pins.iter().all(|(pin, _)| std::path::Path::new(&pin.target).starts_with(&destination)));
        for name in ["Browser.lnk", "Chat.LNK", "unrelated.txt"] {
            std::fs::remove_file(root.join(name)).unwrap();
        }
        assert!(pins.iter().all(|(pin, _)| std::path::Path::new(&pin.target).exists()));
        for (pin, _) in pins {
            std::fs::remove_file(pin.target).unwrap();
        }
        std::fs::remove_dir(destination).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
