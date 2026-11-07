use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eConsole, eRender};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::core::Result;

fn endpoint() -> Result<IAudioEndpointVolume> {
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
    }
}

pub fn muted() -> Option<bool> {
    unsafe { endpoint().ok()?.GetMute().ok().map(|v| v.as_bool()) }
}

pub fn set_muted(muted: bool) {
    unsafe {
        if let Ok(ep) = endpoint() {
            let _ = ep.SetMute(muted, std::ptr::null());
        }
    }
}

fn microphone() -> Result<IAudioEndpointVolume> {
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        enumerator.GetDefaultAudioEndpoint(eCapture, eConsole)?.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
    }
}

pub fn microphone_muted() -> Option<bool> {
    unsafe { microphone().ok()?.GetMute().ok().map(|v| v.as_bool()) }
}

pub fn toggle_microphone() -> bool {
    unsafe {
        let Ok(endpoint) = microphone() else {
            return false;
        };
        let Ok(current) = endpoint.GetMute() else {
            return false;
        };
        endpoint.SetMute(!current.as_bool(), std::ptr::null()).is_ok()
    }
}

pub fn get() -> Option<f32> {
    unsafe { endpoint().ok()?.GetMasterVolumeLevelScalar().ok() }
}

pub fn set(level: f32) {
    unsafe {
        if let Ok(ep) = endpoint() {
            let _ = ep.SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), std::ptr::null());
            let _ = ep.SetMute(level <= 0.0, std::ptr::null());
        }
    }
}

fn friendly_name(device: &windows::Win32::Media::Audio::IMMDevice) -> Option<String> {
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::System::Com::STGM_READ;
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ).ok()?;
        let value = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
        let text = windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc(&value).ok()?;
        let name = text.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(text.0 as *const _));
        name
    }
}

pub fn device_name() -> Option<String> {
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        friendly_name(&enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?)
    }
}

#[derive(Clone)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub capture: bool,
    pub current: bool,
}

pub fn devices() -> Vec<Device> {
    use windows::Win32::Media::Audio::{DEVICE_STATE_ACTIVE, EDataFlow};
    let mut out = Vec::new();
    unsafe {
        let Ok(enumerator) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else {
            return out;
        };
        for (flow, capture) in [(eRender, false), (eCapture, true)] {
            let flow: EDataFlow = flow;
            let current = enumerator
                .GetDefaultAudioEndpoint(flow, eConsole)
                .ok()
                .and_then(|d| d.GetId().ok())
                .and_then(|id| id.to_string().ok());
            let Ok(list) = enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) else { continue };
            for i in 0..list.GetCount().unwrap_or(0) {
                let Ok(device) = list.Item(i) else { continue };
                let (Ok(id), Some(name)) = (device.GetId(), friendly_name(&device)) else { continue };
                let Ok(id_text) = id.to_string() else { continue };
                windows::Win32::System::Com::CoTaskMemFree(Some(id.0 as *const _));
                out.push(Device { current: current.as_deref() == Some(id_text.as_str()), id: id_text, name, capture });
            }
        }
    }
    out
}

#[allow(non_snake_case, dead_code)]
mod policy {
    use std::ffi::c_void;

    #[windows_core::interface("f8679f50-850a-41cf-9c72-430f290290c8")]
    pub unsafe trait IPolicyConfig: windows_core::IUnknown {
        unsafe fn GetMixFormat(
            &self,
            device: windows::core::PCWSTR,
            format: *mut *mut c_void,
        ) -> windows::core::HRESULT;
        unsafe fn GetDeviceFormat(
            &self,
            device: windows::core::PCWSTR,
            default: i32,
            format: *mut *mut c_void,
        ) -> windows::core::HRESULT;
        unsafe fn ResetDeviceFormat(&self, device: windows::core::PCWSTR) -> windows::core::HRESULT;
        unsafe fn SetDeviceFormat(
            &self,
            device: windows::core::PCWSTR,
            a: *mut c_void,
            b: *mut c_void,
        ) -> windows::core::HRESULT;
        unsafe fn GetProcessingPeriod(
            &self,
            device: windows::core::PCWSTR,
            default: i32,
            a: *mut i64,
            b: *mut i64,
        ) -> windows::core::HRESULT;
        unsafe fn SetProcessingPeriod(&self, device: windows::core::PCWSTR, a: *mut i64) -> windows::core::HRESULT;
        unsafe fn GetShareMode(&self, device: windows::core::PCWSTR, mode: *mut c_void) -> windows::core::HRESULT;
        unsafe fn SetShareMode(&self, device: windows::core::PCWSTR, mode: *mut c_void) -> windows::core::HRESULT;
        unsafe fn GetPropertyValue(
            &self,
            device: windows::core::PCWSTR,
            fx: i32,
            key: *const c_void,
            value: *mut c_void,
        ) -> windows::core::HRESULT;
        unsafe fn SetPropertyValue(
            &self,
            device: windows::core::PCWSTR,
            fx: i32,
            key: *const c_void,
            value: *mut c_void,
        ) -> windows::core::HRESULT;
        unsafe fn SetDefaultEndpoint(&self, device: windows::core::PCWSTR, role: i32) -> windows::core::HRESULT;
        unsafe fn SetEndpointVisibility(&self, device: windows::core::PCWSTR, visible: i32) -> windows::core::HRESULT;
    }

    use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
    pub fn set_default(id: &str) -> bool {
        const CLSID_POLICY_CONFIG: windows::core::GUID =
            windows::core::GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);
        let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let Ok(config) = CoCreateInstance::<_, IPolicyConfig>(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL) else {
                return false;
            };
            (0..3).all(|role| config.SetDefaultEndpoint(windows::core::PCWSTR(wide.as_ptr()), role).is_ok())
        }
    }
}
pub use policy::set_default;

fn each_session(source: &str, mut f: impl FnMut(&windows::Win32::Media::Audio::ISimpleAudioVolume)) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Media::Audio::{IAudioSessionControl2, IAudioSessionManager2, ISimpleAudioVolume};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows::core::{Interface, PWSTR};
    let mut found = false;
    unsafe {
        let Ok(enumerator) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else {
            return false;
        };
        let Ok(device) = enumerator.GetDefaultAudioEndpoint(eRender, eConsole) else { return false };
        let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { return false };
        let Ok(list) = manager.GetSessionEnumerator() else { return false };
        for i in 0..list.GetCount().unwrap_or(0) {
            let Ok(control) = list.GetSession(i) else { continue };
            let Ok(control2) = control.cast::<IAudioSessionControl2>() else { continue };
            let pid = control2.GetProcessId().unwrap_or(0);
            if pid == 0 {
                continue;
            }
            let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { continue };
            let mut buf = [0u16; 520];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
            let _ = CloseHandle(process);
            if !ok {
                continue;
            }
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            let stem =
                std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
            if session_matches(source, &stem)
                && let Ok(volume) = control.cast::<ISimpleAudioVolume>()
            {
                f(&volume);
                found = true;
            }
        }
    }
    found
}

fn session_matches(source: &str, stem: &str) -> bool {
    !source.is_empty() && stem.eq_ignore_ascii_case(source)
}

pub fn app_volume(source: &str) -> Option<f32> {
    if source == crate::services::yandex::SOURCE {
        return crate::services::yandex::volume();
    }
    let mut level = None;
    each_session(source, |v| {
        if level.is_none() {
            level = unsafe { v.GetMasterVolume().ok() };
        }
    });
    level
}

pub fn set_app_volume(source: &str, level: f32) -> bool {
    if source == crate::services::yandex::SOURCE {
        return crate::services::yandex::set_volume(level);
    }
    each_session(source, |v| unsafe {
        let _ = v.SetMasterVolume(level.clamp(0.0, 1.0), std::ptr::null());
        let _ = v.SetMute(false, std::ptr::null());
    })
}

pub fn step(delta: f32) -> Option<f32> {
    unsafe {
        let ep = endpoint().ok()?;
        let level = (ep.GetMasterVolumeLevelScalar().ok()? + delta).clamp(0.0, 1.0);
        ep.SetMasterVolumeLevelScalar(level, std::ptr::null()).ok()?;
        if delta > 0.0 {
            let _ = ep.SetMute(false, std::ptr::null());
        }
        Some(level)
    }
}

fn mic_endpoint() -> Result<IAudioEndpointVolume> {
    unsafe {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eCapture, eConsole)?;
        device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
    }
}

pub fn mic_muted() -> Option<bool> {
    unsafe { Some(mic_endpoint().ok()?.GetMute().ok()?.as_bool()) }
}

pub fn mic_toggle() -> Option<bool> {
    unsafe {
        let ep = mic_endpoint().ok()?;
        let muted = !ep.GetMute().ok()?.as_bool();
        ep.SetMute(muted, std::ptr::null()).ok()?;
        Some(muted)
    }
}

#[cfg(test)]
mod source_tests {
    use super::*;

    #[test]
    fn wave_volume_targets_only_this_island_process() {
        assert!(!session_matches(crate::services::yandex::SOURCE, "night-island"));
        assert!(session_matches("CHROME", "chrome"));
        assert!(!session_matches("CHROME", "night-island"));
        assert!(!session_matches("", ""));
    }
}
