use std::mem::size_of;

use windows::Win32::Foundation::{HWND, LPARAM, POINT};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PCWSTR;

use crate::ui::menu::Entry;

pub const WM_TRAY: u32 = WM_APP + 1;
pub const ID_TOGGLE: usize = 1;
pub const ID_SETTINGS: usize = 2;
pub const ID_RESET_POS: usize = 3;
pub const ID_EXIT: usize = 4;
pub const ID_REBUILD: usize = 5;
pub const ID_GUIDE: usize = 6;

const ICON_ID: u32 = 1;
const APP_ICON: usize = 1;
const TIP: &str = "Night Island";

fn data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid =
        NOTIFYICONDATAW { cbSize: size_of::<NOTIFYICONDATAW>() as u32, hWnd: hwnd, uID: ICON_ID, ..Default::default() };
    for (dst, src) in nid.szTip.iter_mut().zip(TIP.encode_utf16()) {
        *dst = src;
    }
    nid
}

pub fn app_icon(cx: i32, cy: i32) -> HICON {
    unsafe {
        let module = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap_or_default();
        LoadImageW(Some(module.into()), PCWSTR(APP_ICON as *const u16), IMAGE_ICON, cx, cy, LR_DEFAULTCOLOR)
            .map(|h| HICON(h.0))
            .or_else(|_| LoadIconW(None, IDI_APPLICATION))
            .unwrap_or_default()
    }
}

pub fn add(hwnd: HWND) {
    unsafe {
        let mut nid = data(hwnd);
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = app_icon(GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON));
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);
    }
}

pub fn remove(hwnd: HWND) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &data(hwnd));
    }
}

pub enum Click {
    Primary,
    Secondary,
    None,
}

pub fn click(lparam: LPARAM) -> Click {
    match (lparam.0 & 0xFFFF) as u32 {
        WM_LBUTTONUP => Click::Primary,
        WM_RBUTTONUP => Click::Secondary,
        _ => Click::None,
    }
}

fn dev_repo() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let target = exe.parent()?.parent()?;
    if target.file_name()? != "target" {
        return None;
    }
    let root = target.parent()?;
    root.join("Cargo.toml").is_file().then(|| root.to_path_buf())
}

pub fn rebuild_and_restart() {
    use std::os::windows::process::CommandExt;
    let Some(root) = dev_repo() else { return };
    let Some(name) = std::env::current_exe().ok().and_then(|e| e.file_name().map(|n| n.to_owned())) else { return };
    let quote = |p: &std::path::Path| p.to_string_lossy().replace('\'', "''");
    let new_exe = root.join("target").join("release").join(name);
    let script = format!(
        "Wait-Process -Id {pid} -ErrorAction SilentlyContinue; Set-Location '{root}'; cargo build --release; \
         if ($LASTEXITCODE -eq 0) {{ Start-Process '{exe}' }} else {{ Read-Host 'Build failed, Enter to close' }}",
        pid = std::process::id(),
        root = quote(&root),
        exe = quote(&new_exe),
    );
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn();
}

pub fn menu(scale: f32, with_gpu: crate::ui::menu::WithGpu) -> Option<usize> {
    let mut entries = vec![
        Entry::item(ID_TOGGLE, "Open / close island").glyph('\u{E8A7}'),
        Entry::item(ID_SETTINGS, "Settings").glyph('\u{E713}'),
        Entry::item(ID_GUIDE, "Quick tour").glyph('\u{E82D}'),
        Entry::item(ID_RESET_POS, "Reset position").glyph('\u{E72C}'),
        Entry::divider(),
    ];
    if dev_repo().is_some() {
        entries.push(Entry::item(ID_REBUILD, "Rebuild and restart").glyph('\u{E777}'));
    }
    entries.push(Entry::item(ID_EXIT, "Exit").glyph('\u{E7E8}').danger());
    let mut pt = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pt);
    }
    crate::ui::menu::show(entries, (pt.x, pt.y), true, scale, with_gpu)
}
