use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows::core::HSTRING;

const MARKER: &str = r"Software\NightIsland\Startup";
const NAME: &str = "NightIsland";

const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

fn command() -> Option<String> {
    Some(format!("\"{}\" --startup", std::env::current_exe().ok()?.display()))
}

fn open(path: &str, access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<HKEY> {
    let mut key = HKEY::default();
    unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, &HSTRING::from(path), None, access, &mut key).ok().ok()? };
    Some(key)
}

fn state() -> &'static AtomicBool {
    static ENABLED: OnceLock<AtomicBool> = OnceLock::new();
    ENABLED.get_or_init(|| AtomicBool::new(registered()))
}

fn registered() -> bool {
    if read(MARKER, NAME).is_some() {
        return true;
    }
    if read(KEY, NAME).is_none() {
        return false;
    }
    let Some(key) = open(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run", KEY_QUERY_VALUE)
    else {
        return true;
    };
    let mut bytes = [0u8; 12];
    let mut size = bytes.len() as u32;
    let status =
        unsafe { RegQueryValueExW(key, &HSTRING::from(NAME), None, None, Some(bytes.as_mut_ptr()), Some(&mut size)) };
    unsafe {
        let _ = RegCloseKey(key);
    }
    status != ERROR_SUCCESS || !matches!(bytes[0], 3 | 7)
}

pub fn enabled() -> bool {
    state().load(Ordering::Relaxed)
}

fn read(path: &str, name: &str) -> Option<String> {
    let key = open(path, KEY_QUERY_VALUE)?;
    let mut buf = [0u16; 32768];
    let mut size = (buf.len() * 2) as u32;
    let mut kind = REG_SZ;
    let status = unsafe {
        RegQueryValueExW(
            key,
            &HSTRING::from(name),
            None,
            Some(&mut kind),
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    if status != ERROR_SUCCESS || kind != REG_SZ {
        return None;
    }
    let len = (size as usize / 2).min(buf.len());
    let value = String::from_utf16_lossy(&buf[..len]);
    let value = value.trim_end_matches('\0');
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
fn enabled_as(name: &str) -> bool {
    let Some(want) = command() else { return false };
    read(KEY, name).is_some_and(|have| have.eq_ignore_ascii_case(&want))
}

pub fn set(on: bool) {
    state().store(on, Ordering::Relaxed);
    let _ = worker().send(Request::Set(on));
}

pub fn refresh() {
    if enabled() {
        let _ = worker().send(Request::Refresh);
    }
}

enum Request {
    Set(bool),
    Refresh,
}

fn worker() -> &'static mpsc::Sender<Request> {
    static WORKER: OnceLock<mpsc::Sender<Request>> = OnceLock::new();
    WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
            let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
            for request in rx {
                apply(request);
            }
            if initialized {
                unsafe {
                    CoUninitialize();
                }
            }
        });
        tx
    })
}

fn apply(request: Request) {
    let session = super::startup_task::Session::connect();
    if matches!(request, Request::Refresh) {
        if read(MARKER, NAME).is_some()
            && let Ok(session) = &session
        {
            match session.enabled(&session.name()) {
                Ok(false) => {
                    write(MARKER, NAME, None);
                    state().store(false, Ordering::Relaxed);
                    return;
                }
                Err(error) if error.code().0 as u32 == 0x80070002 => {
                    write(MARKER, NAME, None);
                    state().store(false, Ordering::Relaxed);
                    return;
                }
                Err(error) => {
                    crate::log_warn!("Could not inspect logon task: {error}");
                    return;
                }
                Ok(true) => {}
            }
        }
        if !registered() {
            return;
        }
    }
    let on = !matches!(request, Request::Set(false));
    if on {
        let result = session.and_then(|session| {
            let executable = std::env::current_exe()
                .map_err(|error| windows::core::Error::new(windows::Win32::Foundation::E_FAIL, error.to_string()))?;
            session.install(&session.name(), &executable)
        });
        match result {
            Ok(()) => {
                if write(MARKER, NAME, command().as_deref()) {
                    write(KEY, NAME, None);
                }
                crate::log_info!("Autostart: immediate per-user logon task configured");
            }
            Err(error) => {
                if matches!(request, Request::Set(true)) {
                    write(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run", NAME, None);
                }
                write(KEY, NAME, command().as_deref());
                crate::log_warn!("Immediate autostart unavailable; using Windows startup queue: {error}");
            }
        }
    } else {
        write(KEY, NAME, None);
        match session.and_then(|session| session.remove(&session.name())) {
            Ok(()) => {
                write(MARKER, NAME, None);
            }
            Err(error) => crate::log_warn!("Could not remove logon task: {error}"),
        }
    }
    state().store(registered(), Ordering::Relaxed);
}

fn open_or_create(path: &str) -> Option<HKEY> {
    let mut key = HKEY::default();
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(path),
            None,
            windows::core::PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
        .ok()
        .ok()?
    };
    Some(key)
}

#[cfg(test)]
fn set_as(name: &str, on: bool) {
    write(KEY, name, if on { command() } else { None }.as_deref());
}

fn write(path: &str, name: &str, value: Option<&str>) -> bool {
    let Some(key) = open_or_create(path) else { return false };
    unsafe {
        let result = if let Some(value) = value {
            let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2);
            RegSetValueExW(key, &HSTRING::from(name), None, REG_SZ, Some(bytes))
        } else {
            RegDeleteValueW(key, &HSTRING::from(name))
        };
        let _ = RegCloseKey(key);
        result == ERROR_SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_run_entry_can_be_switched_on_and_off() {
        let name = "NightIslandTest";
        set_as(name, false);
        assert!(!enabled_as(name));
        set_as(name, true);
        assert!(enabled_as(name));
        set_as(name, false);
        assert!(!enabled_as(name));
    }
}
