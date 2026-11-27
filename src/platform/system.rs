use std::ffi::c_void;
use std::path::PathBuf;
use std::process::Command;

use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD, RegGetValueW, RegSetKeyValueW};
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, PostMessageW, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_APP, WM_SETTINGCHANGE,
};
use windows::core::{HSTRING, PCWSTR, w};

pub const WM_RADIOS: u32 = WM_APP + 3;

const PERSONALIZE: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");

fn read_dword(name: PCWSTR) -> Option<u32> {
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PERSONALIZE,
            name,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    status.is_ok().then_some(value)
}

pub fn dark_theme() -> bool {
    read_dword(w!("AppsUseLightTheme")) == Some(0)
}

pub fn set_dark_theme(dark: bool) {
    let value: u32 = if dark { 0 } else { 1 };
    unsafe {
        for name in [w!("AppsUseLightTheme"), w!("SystemUsesLightTheme")] {
            let _ = RegSetKeyValueW(
                HKEY_CURRENT_USER,
                PERSONALIZE,
                name,
                REG_DWORD.0,
                Some(&value as *const u32 as *const c_void),
                size_of::<u32>() as u32,
            );
        }
        let setting = HSTRING::from("ImmersiveColorSet");
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(setting.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            200,
            None,
        );
    }
}

pub fn lock() {
    unsafe {
        let _ = LockWorkStation();
    }
}

pub fn sleep() {
    unsafe {
        let _ = SetSuspendState(false, false, false);
    }
}

fn shutdown_exe() -> PathBuf {
    let root = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    root.join("System32").join("shutdown.exe")
}

pub fn restart() {
    let _ = Command::new(shutdown_exe()).args(["/r", "/t", "0"]).spawn();
}

pub fn power_off() {
    let _ = Command::new(shutdown_exe()).args(["/s", "/t", "0"]).spawn();
}

#[derive(Clone, Copy)]
pub enum RadioSwitch {
    WiFi,
    Bluetooth,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Radios {
    pub wifi: Option<bool>,
    pub bluetooth: Option<bool>,
}

fn pack(r: Radios) -> usize {
    let code = |s: Option<bool>| match s {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    };
    code(r.wifi) | (code(r.bluetooth) << 2)
}

pub fn unpack(bits: usize) -> Radios {
    let decode = |c: usize| match c & 3 {
        1 => Some(false),
        2 => Some(true),
        _ => None,
    };
    Radios { wifi: decode(bits), bluetooth: decode(bits >> 2) }
}

fn snapshot(change: Option<(RadioSwitch, bool)>) -> windows::core::Result<Radios> {
    if Radio::RequestAccessAsync()?.join()? != RadioAccessStatus::Allowed {
        return Ok(Radios::default());
    }
    let mut out = Radios::default();
    for radio in Radio::GetRadiosAsync()?.join()? {
        let slot = match radio.Kind()? {
            RadioKind::WiFi => (&mut out.wifi, RadioSwitch::WiFi),
            RadioKind::Bluetooth => (&mut out.bluetooth, RadioSwitch::Bluetooth),
            _ => continue,
        };
        if let Some((which, on)) = change
            && std::mem::discriminant(&which) == std::mem::discriminant(&slot.1)
        {
            let state = if on { RadioState::On } else { RadioState::Off };
            let _ = radio.SetStateAsync(state)?.join();
        }
        *slot.0 = Some(radio.State()? == RadioState::On);
    }
    Ok(out)
}

pub fn radios_async(hwnd: HWND, change: Option<(RadioSwitch, bool)>) {
    let raw = hwnd.0 as isize;
    std::thread::spawn(move || {
        let radios = unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
            snapshot(change).unwrap_or_default()
        };
        crate::platform::bt_battery::refresh();
        crate::platform::peripherals::refresh_soon();
        unsafe {
            let hwnd = HWND(raw as *mut c_void);
            let _ = PostMessageW(Some(hwnd), WM_RADIOS, WPARAM(pack(radios)), LPARAM(0));
        }
    });
}
