use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const MAX_PINS: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pin {
    pub name: String,
    pub target: String,
    #[serde(default)]
    pub icon: String,
}

impl Pin {
    pub fn from_path(path: &str) -> Self {
        let name = Path::new(path).file_stem().unwrap_or_default().to_string_lossy().into_owned();
        Self { name, target: path.to_string(), icon: String::new() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DockSettings {
    pub enabled: bool,
    pub replace_taskbar: bool,
    pub experimental_capture: bool,
    pub auto_hide: bool,
    pub show_running: bool,
    pub show_add: bool,
    pub taskbar_imported: bool,
    pub hover_preview: bool,
    pub hide_fullscreen: bool,
    pub economy: bool,
    pub animations: bool,
    pub separator: bool,
    pub lock_pins: bool,
    pub icon_size: u32,
    pub zoom: f32,
    pub opacity: f32,
    pub blur: bool,
    pub blur_opacity: f32,
    pub tint_hue: f32,
    pub tint_saturation: f32,
    pub tint_brightness: f32,
    pub margin: u32,
    pub monitor: String,
    pub pins: Vec<Pin>,
    pub hidden_apps: Vec<String>,
}

impl Default for DockSettings {
    fn default() -> Self {
        let windows = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
        Self {
            enabled: false,
            replace_taskbar: true,
            experimental_capture: false,
            auto_hide: false,
            show_running: true,
            show_add: false,
            taskbar_imported: false,
            hover_preview: true,
            hide_fullscreen: true,
            economy: false,
            animations: true,
            separator: true,
            lock_pins: false,
            icon_size: 48,
            zoom: 1.3,
            opacity: 0.94,
            blur: true,
            blur_opacity: 0.58,
            tint_hue: 225.0,
            tint_saturation: 0.25,
            tint_brightness: 0.09,
            margin: 8,
            monitor: String::new(),
            hidden_apps: Vec::new(),
            pins: vec![Pin {
                name: "Explorer".into(),
                target: format!(r"{windows}\explorer.exe"),
                icon: String::new(),
            }],
        }
    }
}

pub fn key(target: &str) -> String {
    target.trim().replace('/', "\\").to_lowercase()
}

impl DockSettings {
    pub fn sanitize(&mut self) {
        self.icon_size = self.icon_size.clamp(32, 72);
        self.zoom = if self.zoom.is_finite() { self.zoom.clamp(1.0, 1.7) } else { 1.3 };
        self.opacity = if self.opacity.is_finite() { self.opacity.clamp(0.2, 1.0) } else { 0.94 };
        self.blur_opacity = if self.blur_opacity.is_finite() { self.blur_opacity.clamp(0.2, 1.0) } else { 0.58 };
        self.tint_hue = if self.tint_hue.is_finite() { self.tint_hue.clamp(0.0, 360.0) } else { 225.0 };
        self.tint_saturation =
            if self.tint_saturation.is_finite() { self.tint_saturation.clamp(0.0, 1.0) } else { 0.25 };
        self.tint_brightness =
            if self.tint_brightness.is_finite() { self.tint_brightness.clamp(0.02, 0.35) } else { 0.09 };
        self.margin = self.margin.min(40);
        self.monitor = self.monitor.chars().take(128).collect();
        self.hidden_apps = self
            .hidden_apps
            .iter()
            .filter(|path| path.len() <= 2048 && !path.contains(['\0', '\r', '\n']) && !path.trim().is_empty())
            .map(|path| key(path))
            .collect();
        self.hidden_apps.sort();
        self.hidden_apps.dedup();
        self.hidden_apps.truncate(32);
        let mut hidden_bytes = 0;
        self.hidden_apps.retain(|path| {
            hidden_bytes += path.len();
            hidden_bytes <= 12 * 1024
        });
        let mut seen = HashSet::new();
        self.pins.retain(|p| {
            !p.target.trim().is_empty()
                && p.target.len() <= 2048
                && !p.target.contains(['\0', '\r', '\n'])
                && seen.insert(key(&p.target))
        });
        self.pins.truncate(MAX_PINS);
        let mut bytes = 0;
        self.pins.retain(|p| {
            bytes += p.target.len() + p.name.len().min(320) + p.icon.len().min(2048);
            bytes <= 20 * 1024
        });
        for pin in &mut self.pins {
            if pin.icon.len() > 2048 || pin.icon.contains(['\0', '\r', '\n']) {
                pin.icon.clear();
            }
            pin.name = pin.name.chars().filter(|c| !c.is_control()).take(80).collect();
            if pin.name.is_empty() {
                pin.name = Pin::from_path(&pin.target).name;
            }
        }
    }

    pub fn add(&mut self, pin: Pin) -> bool {
        if self.pins.len() >= MAX_PINS || self.pins.iter().any(|p| key(&p.target) == key(&pin.target)) {
            return false;
        }
        let before = self.pins.len();
        self.pins.push(pin);
        self.sanitize();
        self.pins.len() > before
    }
}

pub fn import_mydock(bytes: &[u8]) -> Result<Vec<Pin>, &'static str> {
    if bytes.len() > 256 * 1024 {
        return Err("Dock configuration is too large");
    }
    let text = if bytes.starts_with(&[0xff, 0xfe]) {
        let wide: Vec<_> = bytes[2..].chunks_exact(2).map(|p| u16::from_le_bytes([p[0], p[1]])).collect();
        String::from_utf16(&wide).map_err(|_| "Unsupported dock configuration encoding")?
    } else {
        std::str::from_utf8(bytes).map_err(|_| "Unsupported dock configuration encoding")?.to_string()
    };
    let mut sections: Vec<(u32, String, String, bool)> = Vec::new();
    let mut current = None;
    for line in text.trim_start_matches('\u{feff}').lines().map(str::trim) {
        if line.starts_with('[') {
            current = line.strip_prefix("[ico").and_then(|s| s.strip_suffix(']')).and_then(|s| s.parse().ok());
            if let Some(id) = current {
                sections.push((id, String::new(), String::new(), false));
            }
        } else if current.is_some()
            && let Some((name, value)) = line.split_once('=')
            && let Some(row) = sections.last_mut()
        {
            match name.trim() {
                "tag" => row.1 = value.trim().to_string(),
                "filepath" => row.2 = value.trim().to_string(),
                "delimiter" => row.3 = value.trim() == "1",
                _ => {}
            }
        }
    }
    sections.sort_by_key(|s| s.0);
    let mut settings = DockSettings { pins: Vec::new(), ..Default::default() };
    for (_, name, target, separator) in sections {
        if !separator && !target.is_empty() {
            settings.add(Pin { name, target, icon: String::new() });
        }
    }
    if settings.pins.is_empty() { Err("No applications found in ico.ini") } else { Ok(settings.pins) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_shortcut_icons_migrate_and_roundtrip() {
        let pin: Pin = serde_json::from_str(r#"{"name":"Browser","target":"C:/Browser.exe"}"#).unwrap();
        assert!(pin.icon.is_empty());
        let mut config = DockSettings { pins: vec![Pin { icon: "C:/icon.png".into(), ..pin }], ..Default::default() };
        config.sanitize();
        let restored: DockSettings = serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(restored.pins[0].icon, "C:/icon.png");
        config.pins[0].icon = "bad\0icon".into();
        config.sanitize();
        assert!(config.pins[0].icon.is_empty());
    }

    #[test]
    fn dock_material_migration_keeps_density_and_color_controls_bounded() {
        let mut cfg: DockSettings = serde_json::from_str(r#"{"opacity":0.94}"#).unwrap();
        assert!(cfg.blur);
        assert_eq!(cfg.blur_opacity, 0.58);
        assert_eq!(cfg.opacity, 0.94);
        cfg.tint_hue = f32::INFINITY;
        cfg.tint_saturation = -3.0;
        cfg.tint_brightness = f32::NAN;
        cfg.blur_opacity = 2.0;
        cfg.hidden_apps = (0..32).map(|i| format!("C:/{}.exe", "x".repeat(1800) + &i.to_string())).collect();
        cfg.sanitize();
        assert_eq!(cfg.tint_hue, 225.0);
        assert_eq!(cfg.tint_saturation, 0.0);
        assert_eq!(cfg.tint_brightness, 0.09);
        assert_eq!(cfg.blur_opacity, 1.0);
        assert!(cfg.hidden_apps.iter().map(String::len).sum::<usize>() <= 12 * 1024);
        let restored: DockSettings = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(restored, cfg);
    }

    #[test]
    fn imports_unicode_and_numeric_order_without_separators_or_duplicates() {
        let ini = "[ico10]\ntag=Чат\nfilepath=C:\\chat.exe\n[ico2]\ntag=Browser\nfilepath=C:\\web.exe\n[ico3]\ndelimiter=1\n[ico11]\ntag=Duplicate\nfilepath=c:/chat.exe\n";
        let bytes: Vec<u8> = [0xff, 0xfe].into_iter().chain(ini.encode_utf16().flat_map(u16::to_le_bytes)).collect();
        let pins = import_mydock(&bytes).unwrap();
        assert_eq!(pins.len(), 2);
        assert_eq!(pins[0].name, "Browser");
        assert_eq!(pins[1].name, "Чат");
    }

    #[test]
    fn migration_is_opt_in_and_limits_persisted_input() {
        let mut cfg: DockSettings = serde_json::from_str("{}").unwrap();
        assert!(!cfg.enabled);
        cfg.zoom = f32::NAN;
        cfg.icon_size = u32::MAX;
        cfg.pins = (0..100)
            .map(|i| Pin { name: "x".repeat(100), target: format!("C:\\{i}.exe"), icon: String::new() })
            .collect();
        cfg.sanitize();
        assert_eq!(cfg.zoom, 1.3);
        assert_eq!(cfg.icon_size, 72);
        assert_eq!(cfg.pins.len(), MAX_PINS);
        assert_eq!(cfg.pins[0].name.len(), 80);
        assert!(import_mydock(b"[normal]\nzoom=1").is_err());
    }
}
