use std::collections::HashMap;
use std::sync::Mutex;

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CM_Get_Device_Interface_List_SizeW, CM_Get_Device_Interface_ListW, CR_SUCCESS,
};
use windows::Win32::Devices::HumanInterfaceDevice::{
    HIDD_ATTRIBUTES, HIDP_CAPS, HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetHidGuid, HidD_GetPreparsedData,
    HidD_GetProductString, HidD_SetFeature, HidD_SetOutputReport, HidP_GetCaps, PHIDP_PREPARSED_DATA,
};
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_OVERLAPPED, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    ReadFile, WriteFile,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::core::PCWSTR;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Mouse,
    Keyboard,
    Headset,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reading {
    pub key: String,
    pub name: Option<String>,
    pub kind: Kind,
    pub percent: u8,
    pub charging: bool,
}

#[derive(Clone)]
struct Collection {
    path: String,
    vid: u16,
    pid: u16,
    page: u16,
    usage: u16,
    input_len: u16,
    output_len: u16,
    feature_len: u16,
    product: Option<String>,
}

const LOGITECH: u16 = 0x046D;
const COMPX_LEN: u16 = 17;
const COMPX_FEATURE_ID: u8 = 8;
const COMPX_BATTERY: u8 = 4;
const HIDPP_LONG: u8 = 0x11;
const HIDPP_LONG_LEN: u16 = 20;
const HIDPP_SOFTWARE_ID: u8 = 0x0B;
const HIDPP_RECEIVER_SLOTS: u8 = 6;
const HIDPP_DIRECT: u8 = 0xFF;
const UNIFIED_BATTERY: u16 = 0x1004;
const BATTERY_STATUS: u16 = 0x1000;
const DEVICE_NAME: u16 = 0x0005;
const REPLY_MS: u32 = 250;
const SLOT_PROBE_MS: u32 = 120;

static NAMES: Mutex<Option<HashMap<String, (String, Kind)>>> = Mutex::new(None);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn interface_paths() -> Vec<String> {
    let guid = unsafe { HidD_GetHidGuid() };
    let mut len = 0u32;
    unsafe {
        if CM_Get_Device_Interface_List_SizeW(&mut len, &guid, PCWSTR::null(), CM_GET_DEVICE_INTERFACE_LIST_PRESENT)
            != CR_SUCCESS
        {
            return Vec::new();
        }
    }
    let mut buf = vec![0u16; len as usize];
    unsafe {
        if CM_Get_Device_Interface_ListW(&guid, PCWSTR::null(), &mut buf, CM_GET_DEVICE_INTERFACE_LIST_PRESENT)
            != CR_SUCCESS
        {
            return Vec::new();
        }
    }
    buf.split(|c| *c == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy).collect()
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CancelIoEx(self.0, None);
            let _ = CloseHandle(self.0);
        }
    }
}

fn open(path: &str, access: u32, overlapped: bool) -> Option<Handle> {
    let name = wide(path);
    let flags = if overlapped { FILE_FLAG_OVERLAPPED } else { FILE_FLAGS_AND_ATTRIBUTES(0) };
    unsafe {
        CreateFileW(PCWSTR(name.as_ptr()), access, FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING, flags, None)
            .ok()
            .map(Handle)
    }
}

fn describe(path: &str) -> Option<Collection> {
    let h = open(path, 0, false)?;
    let mut attrs = HIDD_ATTRIBUTES { Size: size_of::<HIDD_ATTRIBUTES>() as u32, ..Default::default() };
    let mut caps = HIDP_CAPS::default();
    unsafe {
        if !HidD_GetAttributes(h.0, &mut attrs) {
            return None;
        }
        let mut data = PHIDP_PREPARSED_DATA::default();
        if !HidD_GetPreparsedData(h.0, &mut data) {
            return None;
        }
        let status = HidP_GetCaps(data, &mut caps);
        let _ = HidD_FreePreparsedData(data);
        status.ok().ok()?;
    }
    let mut name = [0u16; 128];
    let product = unsafe { HidD_GetProductString(h.0, name.as_mut_ptr().cast(), (name.len() * 2) as u32) }
        .then(|| String::from_utf16_lossy(&name[..name.iter().position(|c| *c == 0).unwrap_or(name.len())]))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Some(Collection {
        path: path.to_string(),
        vid: attrs.VendorID,
        pid: attrs.ProductID,
        page: caps.UsagePage,
        usage: caps.Usage,
        input_len: caps.InputReportByteLength,
        output_len: caps.OutputReportByteLength,
        feature_len: caps.FeatureReportByteLength,
        product,
    })
}

pub fn group_key(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    let mut parts = lower.split('#');
    let _ = parts.next();
    let ids = parts.next().unwrap_or_default();
    let instance = parts.next().unwrap_or_default();
    let ids = ids.split("&col").next().unwrap_or(ids);
    let instance = instance.rsplit_once('&').map_or(instance, |(head, _)| head);
    format!("{ids}#{instance}")
}

struct Port {
    handle: Handle,
    event: HANDLE,
}

impl Port {
    fn open(path: &str) -> Option<Self> {
        let handle = open(path, GENERIC_READ.0 | GENERIC_WRITE.0, true)?;
        let event = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.ok()?;
        Some(Self { handle, event })
    }

    fn read(&self, buf: &mut [u8], ms: u32) -> Option<usize> {
        let mut ov = OVERLAPPED { hEvent: self.event, ..Default::default() };
        let mut got = 0u32;
        unsafe {
            if ReadFile(self.handle.0, Some(buf), None, Some(&mut ov)).is_ok() {
                GetOverlappedResult(self.handle.0, &ov, &mut got, false).ok()?;
                return Some(got as usize);
            }
            if WaitForSingleObject(self.event, ms) != WAIT_OBJECT_0 {
                let _ = CancelIoEx(self.handle.0, Some(&ov));
                let _ = GetOverlappedResult(self.handle.0, &ov, &mut got, true);
                return None;
            }
            GetOverlappedResult(self.handle.0, &ov, &mut got, false).ok()?;
        }
        Some(got as usize)
    }

    fn write(&self, buf: &[u8]) -> bool {
        let mut ov = OVERLAPPED { hEvent: self.event, ..Default::default() };
        let mut done = 0u32;
        unsafe {
            if WriteFile(self.handle.0, Some(buf), None, Some(&mut ov)).is_ok() {
                return true;
            }
            if WaitForSingleObject(self.event, REPLY_MS) != WAIT_OBJECT_0 {
                let _ = CancelIoEx(self.handle.0, Some(&ov));
                let _ = GetOverlappedResult(self.handle.0, &ov, &mut done, true);
                return false;
            }
            GetOverlappedResult(self.handle.0, &ov, &mut done, false).is_ok()
        }
    }
}

impl Drop for Port {
    fn drop(&mut self) {
        unsafe {
            let _ = CancelIoEx(self.handle.0, None);
            let _ = CloseHandle(self.event);
        }
    }
}

pub fn compx_packet(command: u8) -> [u8; COMPX_LEN as usize] {
    let mut packet = [0u8; COMPX_LEN as usize];
    packet[0] = COMPX_FEATURE_ID;
    packet[1] = command;
    let sum = packet[1..16].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    packet[16] = 0x55u8.wrapping_sub(COMPX_FEATURE_ID).wrapping_sub(sum);
    packet
}

pub fn compx_battery(reply: &[u8]) -> Option<(u8, bool)> {
    if reply.len() < COMPX_LEN as usize || reply[1] != COMPX_BATTERY {
        return None;
    }
    let percent = reply[6];
    (1..=100).contains(&percent).then_some((percent, reply[7] == 1))
}

fn vendor(c: &Collection) -> bool {
    c.page >= 0xFF00
}

fn read_compx(group: &[Collection]) -> Option<(u8, bool)> {
    let input = group.iter().find(|c| vendor(c) && c.input_len == COMPX_LEN)?;
    let command = group.iter().find(|c| vendor(c) && (c.feature_len == COMPX_LEN || c.output_len == COMPX_LEN))?;
    let listen = Port::open(&input.path)?;
    let send = open(&command.path, GENERIC_READ.0 | GENERIC_WRITE.0, false)?;
    let packet = compx_packet(COMPX_BATTERY);
    let sent = unsafe {
        (command.feature_len == COMPX_LEN && HidD_SetFeature(send.0, packet.as_ptr().cast(), packet.len() as u32))
            || (command.output_len == COMPX_LEN
                && HidD_SetOutputReport(send.0, packet.as_ptr().cast(), packet.len() as u32))
    };
    if !sent {
        return None;
    }
    let mut buf = [0u8; COMPX_LEN as usize];
    for _ in 0..8 {
        let n = listen.read(&mut buf, REPLY_MS)?;
        if let Some(found) = compx_battery(&buf[..n]) {
            return Some(found);
        }
    }
    None
}

struct Hidpp<'a> {
    port: &'a Port,
    slot: u8,
}

impl Hidpp<'_> {
    fn call(&self, feature: u8, function: u8, params: &[u8], ms: u32) -> Option<[u8; 16]> {
        let mut out = [0u8; HIDPP_LONG_LEN as usize];
        out[0] = HIDPP_LONG;
        out[1] = self.slot;
        out[2] = feature;
        out[3] = (function << 4) | HIDPP_SOFTWARE_ID;
        out[4..4 + params.len()].copy_from_slice(params);
        if !self.port.write(&out) {
            return None;
        }
        let mut buf = [0u8; HIDPP_LONG_LEN as usize];
        for _ in 0..8 {
            let n = self.port.read(&mut buf, ms)?;
            if n < HIDPP_LONG_LEN as usize || buf[0] != HIDPP_LONG || buf[1] != self.slot {
                continue;
            }
            if buf[2] == 0xFF && buf[3] == feature {
                return None;
            }
            if buf[2] == feature && buf[3] == out[3] {
                let mut reply = [0u8; 16];
                reply.copy_from_slice(&buf[4..20]);
                return Some(reply);
            }
        }
        None
    }

    fn feature(&self, id: u16, ms: u32) -> Option<u8> {
        let reply = self.call(0, 0, &id.to_be_bytes(), ms)?;
        (reply[0] != 0).then_some(reply[0])
    }

    fn battery(&self, ms: u32) -> Option<(u8, bool)> {
        if let Some(index) = self.feature(UNIFIED_BATTERY, ms) {
            let r = self.call(index, 1, &[], REPLY_MS)?;
            return unified_battery(&r);
        }
        let index = self.feature(BATTERY_STATUS, REPLY_MS)?;
        battery_status(&self.call(index, 0, &[], REPLY_MS)?)
    }

    fn identity(&self) -> Option<(String, Kind)> {
        let index = self.feature(DEVICE_NAME, REPLY_MS)?;
        let len = usize::from(self.call(index, 0, &[], REPLY_MS)?[0]).min(64);
        let mut bytes = Vec::with_capacity(len);
        while bytes.len() < len {
            let part = self.call(index, 1, &[bytes.len() as u8], REPLY_MS)?;
            let take = (len - bytes.len()).min(16);
            bytes.extend_from_slice(&part[..take]);
        }
        let name = String::from_utf8_lossy(&bytes).trim_matches(char::from(0)).trim().to_string();
        let kind = self.call(index, 2, &[], REPLY_MS).map_or(Kind::Other, |r| hidpp_kind(r[0]));
        (!name.is_empty()).then_some((name, kind))
    }
}

pub fn hidpp_kind(code: u8) -> Kind {
    match code {
        0 | 2 => Kind::Keyboard,
        3..=5 => Kind::Mouse,
        8 => Kind::Headset,
        _ => Kind::Other,
    }
}

pub fn unified_battery(r: &[u8]) -> Option<(u8, bool)> {
    let percent = r[0];
    let level = r[1];
    let percent = if percent == 0 {
        match level {
            l if l & 8 != 0 => 100,
            l if l & 4 != 0 => 60,
            l if l & 2 != 0 => 20,
            l if l & 1 != 0 => 5,
            _ => return None,
        }
    } else {
        percent
    };
    (percent <= 100).then_some((percent, matches!(r[2], 1..=3)))
}

pub fn battery_status(r: &[u8]) -> Option<(u8, bool)> {
    (1..=100).contains(&r[0]).then_some((r[0], matches!(r[2], 1..=4)))
}

fn cached_identity(key: &str, read: impl FnOnce() -> Option<(String, Kind)>) -> Option<(String, Kind)> {
    if let Some(found) = NAMES.lock().ok().and_then(|n| n.as_ref().and_then(|m| m.get(key).cloned())) {
        return Some(found);
    }
    let found = read()?;
    if let Ok(mut names) = NAMES.lock() {
        names.get_or_insert_with(HashMap::new).insert(key.to_string(), found.clone());
    }
    Some(found)
}

fn read_hidpp(group: &[Collection], key: &str, out: &mut Vec<Reading>) {
    let Some(long) = group.iter().find(|c| c.page == 0xFF00 && c.usage == 2 && c.input_len == HIDPP_LONG_LEN) else {
        return;
    };
    let Some(port) = Port::open(&long.path) else { return };
    let receiver = group.iter().any(|c| c.page == 0xFF00 && c.usage == 1)
        && long.product.as_deref().is_some_and(|p| p.to_ascii_lowercase().contains("receiver"));
    let slots: Vec<u8> = if receiver { (1..=HIDPP_RECEIVER_SLOTS).collect() } else { vec![HIDPP_DIRECT] };
    for slot in slots {
        let device = Hidpp { port: &port, slot };
        let wait = if receiver { SLOT_PROBE_MS } else { REPLY_MS };
        let Some((percent, charging)) = device.battery(wait) else { continue };
        let slot_key = format!("{key}/{slot}");
        let (name, kind) = match cached_identity(&slot_key, || device.identity()) {
            Some((name, kind)) => (Some(name), kind),
            None => (long.product.clone(), Kind::Other),
        };
        out.push(Reading { key: slot_key, name, kind, percent, charging });
    }
}

pub fn generic_name(product: &str) -> bool {
    let lower = product.to_ascii_lowercase();
    ["receiver", "dongle", "2.4g", "wireless", "usb device"].iter().any(|w| lower.contains(w))
}

pub fn read() -> Vec<Reading> {
    let mut groups: Vec<(String, Vec<Collection>)> = Vec::new();
    for path in interface_paths() {
        let Some(c) = describe(&path) else { continue };
        let key = group_key(&path);
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) => list.push(c),
            None => groups.push((key, vec![c])),
        }
    }
    let mut out = Vec::new();
    for (key, group) in &groups {
        if !group.iter().any(vendor) {
            continue;
        }
        if group[0].vid == LOGITECH {
            read_hidpp(group, key, &mut out);
            continue;
        }
        if let Some((percent, charging)) = read_compx(group) {
            let product = group.iter().find_map(|c| c.product.clone());
            let name = product.filter(|p| !generic_name(p));
            let first = &group[0];
            out.push(Reading {
                key: format!("{:04x}:{:04x}/{key}", first.vid, first.pid),
                name,
                kind: Kind::Mouse,
                percent,
                charging,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_compx_battery_request_carries_its_checksum() {
        let p = compx_packet(COMPX_BATTERY);
        assert_eq!(p[0], 8);
        assert_eq!(p[1], 4);
        assert_eq!(p[16], 0x49);
        assert_eq!(compx_packet(0x12)[16], 0x3B);
    }

    #[test]
    fn a_compx_reply_gives_percent_and_charging() {
        let mut r = [0u8; 17];
        r[0] = 9;
        r[1] = 4;
        r[5] = 2;
        r[6] = 0x50;
        assert_eq!(compx_battery(&r), Some((80, false)));
        r[7] = 1;
        assert_eq!(compx_battery(&r), Some((80, true)));
        r[6] = 0;
        assert_eq!(compx_battery(&r), None);
        r[6] = 50;
        r[1] = 0x12;
        assert_eq!(compx_battery(&r), None);
    }

    #[test]
    fn hidpp_battery_features_are_decoded() {
        assert_eq!(unified_battery(&[73, 4, 0, 0]), Some((73, false)));
        assert_eq!(unified_battery(&[0, 8, 1, 1]), Some((100, true)));
        assert_eq!(unified_battery(&[0, 0, 0, 0]), None);
        assert_eq!(battery_status(&[40, 20, 0]), Some((40, false)));
        assert_eq!(battery_status(&[90, 80, 1]), Some((90, true)));
        assert_eq!(battery_status(&[0, 0, 0]), None);
    }

    #[test]
    fn collections_of_one_device_share_a_key() {
        let a = r"\\?\hid#vid_25a7&pid_fa7c&mi_01&col05#7&b7c7a56&0&0004#{4d1e55b2-f16f-11cf-88cb-001111000030}";
        let b = r"\\?\hid#vid_25a7&pid_fa7c&mi_01&col07#7&b7c7a56&0&0006#{4d1e55b2-f16f-11cf-88cb-001111000030}";
        let other = r"\\?\hid#vid_25a7&pid_fa7c&mi_00#7&33e7281f&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}";
        assert_eq!(group_key(a), group_key(b));
        assert_ne!(group_key(a), group_key(other));
    }

    #[test]
    fn receiver_names_are_not_device_names() {
        assert!(generic_name("2.4G Wireless Receiver"));
        assert!(generic_name("USB Receiver"));
        assert!(!generic_name("G502 LIGHTSPEED"));
    }

    #[test]
    #[ignore = "talks to the wireless receivers of this computer"]
    fn reads_the_receivers_of_this_computer() {
        for r in read() {
            eprintln!("HID|{:?}|{:?}|{}%|charging={}|{}", r.name, r.kind, r.percent, r.charging, r.key);
        }
    }
}
