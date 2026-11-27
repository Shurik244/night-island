use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub use crate::platform::hid_battery::Kind;

pub const WM_PERIPHERALS: u32 = WM_APP + 134;

pub const LOW_PERCENT: u8 = 15;

const EVERY: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    Bluetooth,
    Radio,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub key: String,
    pub name: String,
    pub kind: Kind,
    pub link: Link,
    pub percent: u8,
    pub charging: bool,
}

impl Device {
    pub fn glyph(&self) -> char {
        match (self.kind, self.link) {
            (Kind::Mouse, _) => '\u{E962}',
            (Kind::Keyboard, _) => '\u{E765}',
            (Kind::Headset, _) => '\u{E7F6}',
            (Kind::Other, Link::Bluetooth) => '\u{E702}',
            (Kind::Other, Link::Radio) => '\u{E772}',
        }
    }

    pub fn low(&self) -> bool {
        !self.charging && self.percent <= LOW_PERCENT
    }
}

static LATEST: Mutex<Vec<Device>> = Mutex::new(Vec::new());
static STARTED: AtomicBool = AtomicBool::new(false);
static NOW: AtomicBool = AtomicBool::new(false);

pub fn latest() -> Vec<Device> {
    LATEST.lock().map(|d| d.clone()).unwrap_or_default()
}

pub fn lowest(devices: &[Device]) -> Option<&Device> {
    devices.iter().min_by_key(|d| (d.charging, d.percent))
}

pub fn refresh_soon() {
    NOW.store(true, Ordering::Relaxed);
}

fn default_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Mouse => "Wireless mouse",
        Kind::Keyboard => "Wireless keyboard",
        Kind::Headset => "Wireless headset",
        Kind::Other => "Wireless device",
    }
}

pub fn collect(
    bluetooth: Vec<crate::platform::bt_battery::Device>,
    radio: Vec<crate::platform::hid_battery::Reading>,
) -> Vec<Device> {
    let mut out: Vec<Device> = bluetooth
        .into_iter()
        .map(|d| Device {
            key: format!("bt/{}", d.name),
            name: if d.name.trim().is_empty() { "Bluetooth device".into() } else { d.name },
            kind: Kind::Other,
            link: Link::Bluetooth,
            percent: d.percent,
            charging: false,
        })
        .collect();
    out.extend(radio.into_iter().map(|r| Device {
        key: r.key,
        name: r.name.unwrap_or_else(|| crate::ui::i18n::tr(default_name(r.kind)).into_owned()),
        kind: r.kind,
        link: Link::Radio,
        percent: r.percent,
        charging: r.charging,
    }));
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.key.cmp(&b.key)));
    out
}

fn read() -> Vec<Device> {
    collect(crate::platform::bt_battery::read(), crate::platform::hid_battery::read())
}

pub fn watch(hwnd: HWND) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let raw = hwnd.0 as isize;
    std::thread::spawn(move || {
        let mut next = Instant::now();
        loop {
            if Instant::now() >= next || NOW.swap(false, Ordering::Relaxed) {
                let found = read();
                let changed = LATEST.lock().map(|mut slot| {
                    let changed = *slot != found;
                    *slot = found;
                    changed
                });
                if changed.unwrap_or(false) {
                    unsafe {
                        let _ = PostMessageW(Some(HWND(raw as *mut c_void)), WM_PERIPHERALS, WPARAM(0), LPARAM(0));
                    }
                }
                next = Instant::now() + EVERY;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{bt_battery, hid_battery};

    fn radio(name: Option<&str>, percent: u8, charging: bool) -> hid_battery::Reading {
        hid_battery::Reading {
            key: format!("k{percent}"),
            name: name.map(Into::into),
            kind: Kind::Mouse,
            percent,
            charging,
        }
    }

    #[test]
    fn devices_from_both_links_are_listed_by_name() {
        let bt = vec![bt_battery::Device { name: "Buds".into(), percent: 40 }];
        let list = collect(bt, vec![radio(Some("Atlantis"), 80, false)]);
        assert_eq!(list.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(), ["Atlantis", "Buds"]);
        assert_eq!(list[1].link, Link::Bluetooth);
        assert_eq!(list[0].glyph(), '\u{E962}');
    }

    #[test]
    fn a_device_without_a_name_gets_one_by_its_kind() {
        let list = collect(Vec::new(), vec![radio(None, 50, false)]);
        assert!(!list[0].name.is_empty());
    }

    #[test]
    fn the_lowest_device_skips_charging_ones_first() {
        let list = collect(Vec::new(), vec![radio(Some("a"), 10, true), radio(Some("b"), 30, false)]);
        assert_eq!(lowest(&list).map(|d| d.name.as_str()), Some("b"));
        assert!(!list[0].low());
        assert!(!list[1].low());
        let low = collect(Vec::new(), vec![radio(Some("c"), 15, false)]);
        assert!(low[0].low());
    }
}
