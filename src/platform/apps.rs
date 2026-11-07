use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::c_void;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, mpsc};

use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW, HGDIOBJ,
    ReleaseDC,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize};
use windows::Win32::UI::Shell::{
    BHID_EnumItems, IEnumShellItems, IShellItem, IShellItemImageFactory, SHCreateItemFromParsingName,
    SIGDN_DESKTOPABSOLUTEPARSING, SIGDN_NORMALDISPLAY, SIIGBF, SIIGBF_BIGGERSIZEOK, SIIGBF_CROPTOSQUARE,
    SIIGBF_ICONONLY, SIIGBF_THUMBNAILONLY,
};
use windows::core::{HSTRING, Interface, PWSTR, w};

const MAX_ENTRIES: usize = 4000;
const MAX_DEPTH: usize = 5;

pub struct AppEntry {
    pub name: String,
    lower: String,
    pub path: PathBuf,
    pub launch: String,
}

pub struct Icon {
    pub side: u32,
    pub bgra: Vec<u8>,
}

static ICONS: Mutex<Option<HashMap<String, Option<Arc<Icon>>>>> = Mutex::new(None);

pub fn icon(entry: &AppEntry) -> Option<Arc<Icon>> {
    let mut guard = ICONS.lock().ok()?;
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some(hit) = cache.get(&entry.launch) {
        return hit.clone();
    }
    let loaded = load_icon(&entry.launch).map(Arc::new);
    cache.insert(entry.launch.clone(), loaded.clone());
    loaded
}

pub fn icon_for_path(path: &str) -> Option<Arc<Icon>> {
    let mut guard = ICONS.lock().ok()?;
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some(hit) = cache.get(path) {
        return hit.clone();
    }
    if cache.len() > 500 {
        cache.clear();
    }
    let loaded = load_icon(path).map(Arc::new);
    cache.insert(path.to_string(), loaded.clone());
    loaded
}

type DockIconKey = (String, u32);
type DockIconResult = (DockIconKey, Option<Arc<Icon>>);

pub struct DockIconCache {
    requests: Option<mpsc::SyncSender<DockIconKey>>,
    results: mpsc::Receiver<DockIconResult>,
    ready: HashMap<DockIconKey, Option<Arc<Icon>>>,
    pending: HashSet<DockIconKey>,
    order: VecDeque<DockIconKey>,
}

impl DockIconCache {
    pub fn new() -> Self {
        let (requests, queue) = mpsc::sync_channel::<DockIconKey>(96);
        let (finished, results) = mpsc::channel();
        let worker = std::thread::Builder::new().name("dock-icons".into()).spawn(move || {
            if unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_err() {
                return;
            }
            while let Ok((path, side)) = queue.recv() {
                let picture = std::path::Path::new(&path).extension().is_some_and(|e| {
                    ["png", "jpg", "jpeg", "webp", "bmp"].iter().any(|ext| e.eq_ignore_ascii_case(ext))
                });
                let flags = if picture {
                    SIIGBF_THUMBNAILONLY | SIIGBF_CROPTOSQUARE
                } else {
                    SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK
                };
                let icon = shell_image(&path, side as i32, flags).map(normalize_dock_icon).map(Arc::new);
                if finished.send(((path, side), icon)).is_err() {
                    break;
                }
            }
            unsafe {
                CoUninitialize();
            }
        });
        Self {
            requests: worker.ok().map(|_| requests),
            results,
            ready: HashMap::new(),
            pending: HashSet::new(),
            order: VecDeque::new(),
        }
    }

    pub fn get(&mut self, path: &str, pixels: u32) -> Option<Arc<Icon>> {
        let key = (path.to_string(), icon_bucket(pixels));
        if let Some(icon) = self.ready.get(&key) {
            self.order.retain(|old| *old != key);
            self.order.push_back(key);
            return icon.clone();
        }
        if self.pending.len() < 96
            && !self.pending.contains(&key)
            && let Some(requests) = &self.requests
            && requests.try_send(key.clone()).is_ok()
        {
            self.pending.insert(key);
        }
        None
    }

    #[cfg(test)]
    pub fn pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        loop {
            let (key, icon) = match self.results.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    changed |= !self.pending.is_empty();
                    self.pending.clear();
                    self.requests = None;
                    break;
                }
            };
            if !self.pending.remove(&key) {
                continue;
            }
            self.order.retain(|old| *old != key);
            self.order.push_back(key.clone());
            self.ready.insert(key, icon);
            while self.ready.len() > 96 {
                if let Some(old) = self.order.pop_front() {
                    self.ready.remove(&old);
                }
            }
            changed = true;
        }
        changed
    }

    pub fn retain(&mut self, targets: &[String]) {
        let keep = |key: &DockIconKey| targets.contains(&key.0);
        self.ready.retain(|key, _| keep(key));
        self.pending.retain(keep);
        self.order.retain(keep);
    }
}

fn icon_bucket(pixels: u32) -> u32 {
    pixels.clamp(64, 256).next_power_of_two().min(256)
}

pub fn normalize_dock_icon(icon: Icon) -> Icon {
    let side = icon.side as usize;
    if side == 0 || icon.bgra.len() != side * side * 4 {
        return icon;
    }
    let mut bounds = (side, side, 0, 0);
    for y in 0..side {
        for x in 0..side {
            if icon.bgra[(y * side + x) * 4 + 3] > 8 {
                bounds.0 = bounds.0.min(x);
                bounds.1 = bounds.1.min(y);
                bounds.2 = bounds.2.max(x + 1);
                bounds.3 = bounds.3.max(y + 1);
            }
        }
    }
    if bounds.2 == 0 {
        return icon;
    }
    let width = bounds.2 - bounds.0;
    let height = bounds.3 - bounds.1;
    let extent = width.max(height);
    let pad = (extent as f32 * 0.06).ceil() as usize;
    let output_side = extent + pad * 2;
    let ox = (output_side - width) / 2;
    let oy = (output_side - height) / 2;
    let straight = icon.bgra.chunks_exact(4).any(|p| p[3] != 0 && p[..3].iter().any(|c| *c > p[3]));
    let mut pixels = vec![0; output_side * output_side * 4];
    for y in 0..height {
        for x in 0..width {
            let from = ((bounds.1 + y) * side + bounds.0 + x) * 4;
            let to = ((oy + y) * output_side + ox + x) * 4;
            let mut pixel = [0u8; 4];
            pixel.copy_from_slice(&icon.bgra[from..from + 4]);
            if straight {
                for c in &mut pixel[..3] {
                    *c = ((*c as u16 * icon.bgra[from + 3] as u16 + 127) / 255) as u8;
                }
            } else if pixel[3] == 0 {
                pixel[..3].fill(0);
            }
            pixels[to..to + 4].copy_from_slice(&pixel);
        }
    }
    Icon { side: output_side as u32, bgra: pixels }
}

pub fn thumbnail(path: &str) -> Option<Arc<Icon>> {
    shell_image(path, 96, SIIGBF_THUMBNAILONLY | SIIGBF_CROPTOSQUARE).map(Arc::new)
}

fn load_icon(target: &str) -> Option<Icon> {
    shell_image(target, 48, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)
}

fn shell_image(target: &str, size: i32, flags: SIIGBF) -> Option<Icon> {
    unsafe {
        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(target), None).ok()?;
        let factory: IShellItemImageFactory = item.cast().ok()?;
        let bitmap = factory.GetImage(SIZE { cx: size, cy: size }, flags).ok()?;
        let mut info = BITMAP::default();
        GetObjectW(HGDIOBJ(bitmap.0), size_of::<BITMAP>() as i32, Some((&raw mut info).cast::<c_void>()));
        let (w, h) = (info.bmWidth, info.bmHeight);
        if w <= 0 || h <= 0 || w > 512 || h > 512 {
            let _ = DeleteObject(bitmap.into());
            return None;
        }
        let mut header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let dc = GetDC(None);
        let rows = GetDIBits(dc, bitmap, 0, h as u32, Some(buf.as_mut_ptr().cast()), &mut header, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        let _ = DeleteObject(bitmap.into());
        if rows == 0 {
            return None;
        }
        if buf.chunks_exact(4).all(|p| p[3] == 0) {
            for p in buf.chunks_exact_mut(4) {
                p[3] = 255;
            }
        }
        let side = w.max(h) as usize;
        let (ox, oy) = ((side - w as usize) / 2, (side - h as usize) / 2);
        let mut square = vec![0; side * side * 4];
        for y in 0..h as usize {
            let from = y * w as usize * 4;
            let to = ((oy + y) * side + ox) * 4;
            square[to..to + w as usize * 4].copy_from_slice(&buf[from..from + w as usize * 4]);
        }
        Some(Icon { side: side as u32, bgra: square })
    }
}

fn scan_apps_folder() -> Vec<(String, String)> {
    let mut out = Vec::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let Ok(folder) =
            SHCreateItemFromParsingName::<_, _, IShellItem>(w!("shell:::{4234D49B-0245-4DF3-B780-3893943456E1}"), None)
        else {
            return out;
        };
        let Ok(items) = folder.BindToHandler::<_, IEnumShellItems>(None, &BHID_EnumItems) else { return out };
        let text = |item: &IShellItem, kind| -> Option<String> {
            let raw: PWSTR = item.GetDisplayName(kind).ok()?;
            let s = raw.to_string().ok();
            CoTaskMemFree(Some(raw.0 as *const c_void));
            s
        };
        loop {
            let mut slot = [None];
            let mut fetched = 0u32;
            if items.Next(&mut slot, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(item) = slot[0].take() else { break };
            let (Some(name), Some(parse)) =
                (text(&item, SIGDN_NORMALDISPLAY), text(&item, SIGDN_DESKTOPABSOLUTEPARSING))
            else {
                continue;
            };
            let launch = if parse.starts_with("shell:") { parse } else { format!("shell:AppsFolder\\{parse}") };
            out.push((name, launch));
            if out.len() >= MAX_ENTRIES {
                break;
            }
        }
    }
    out
}

static INDEX: OnceLock<Vec<AppEntry>> = OnceLock::new();

pub fn start_indexing() {
    std::thread::spawn(|| {
        let _ = INDEX.set(scan());
    });
}

fn roots() -> Vec<PathBuf> {
    let programs = |base: Option<std::ffi::OsString>| {
        base.map(|b| PathBuf::from(b).join(r"Microsoft\Windows\Start Menu\Programs"))
    };
    [programs(std::env::var_os("ProgramData")), programs(std::env::var_os("APPDATA"))].into_iter().flatten().collect()
}

fn scan() -> Vec<AppEntry> {
    let mut out: Vec<AppEntry> = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = roots().into_iter().map(|p| (p, 0)).collect();
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if out.len() >= MAX_ENTRIES {
                break;
            }
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
                if depth < MAX_DEPTH {
                    stack.push((path, depth + 1));
                }
            } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk")) {
                let Some(name) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                    continue;
                };
                let lower = name.to_lowercase();
                if ["uninstall", "удалить", "деинсталл"].iter().any(|w| lower.contains(w))
                    || out.iter().any(|a| a.lower == lower)
                {
                    continue;
                }
                let launch = path.to_string_lossy().into_owned();
                out.push(AppEntry { name, lower, path, launch });
            }
        }
    }
    for (name, launch) in scan_apps_folder() {
        let lower = name.to_lowercase();
        let junk = ["uninstall", "удалить", "деинсталл", "удаление"].iter().any(|w| lower.contains(w));
        if junk || out.iter().any(|a| a.lower == lower) || out.len() >= MAX_ENTRIES {
            continue;
        }
        let tail = launch.strip_prefix("shell:AppsFolder\\").unwrap_or("");
        if tail.as_bytes().get(1) == Some(&b':') {
            out.push(AppEntry { name, lower, path: PathBuf::from(tail), launch: tail.to_string() });
        } else {
            out.push(AppEntry { name, lower, path: PathBuf::new(), launch });
        }
    }
    out
}

pub(super) fn notification_shortcuts() -> Vec<String> {
    let mut out = Vec::new();
    let mut stack: Vec<_> = roots().into_iter().map(|p| (p, 0)).collect();
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() && depth < MAX_DEPTH {
                stack.push((path, depth + 1));
            } else if kind.is_file() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk")) {
                out.push(path.to_string_lossy().into_owned());
                if out.len() >= MAX_ENTRIES {
                    return out;
                }
            }
        }
    }
    out
}

pub fn names() -> Vec<&'static str> {
    const NOISE: [&str; 11] = [
        "readme",
        "help",
        "manual",
        "documentation",
        "license",
        "release notes",
        "website",
        "справка",
        "руководство",
        "документация",
        "лицензи",
    ];
    let Some(index) = INDEX.get() else { return Vec::new() };
    let mut names: Vec<&str> =
        index.iter().filter(|a| !NOISE.iter().any(|n| a.lower.contains(n))).map(|a| a.name.as_str()).collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

pub fn find(query: &str, limit: usize) -> Vec<&'static AppEntry> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let Some(index) = INDEX.get() else { return Vec::new() };
    let mut scored: Vec<(u8, usize, &AppEntry)> = index
        .iter()
        .filter_map(|a| {
            let score = if a.lower == q {
                0
            } else if a.lower.starts_with(&q) {
                1
            } else if a.lower.split([' ', '-', '_', '.']).any(|w| w.starts_with(&q)) {
                2
            } else if a.lower.contains(&q) {
                3
            } else {
                return None;
            };
            Some((score, a.name.chars().count(), a))
        })
        .collect();
    scored.sort_by_key(|&(score, len, _)| (score, len));
    scored.into_iter().take(limit).map(|(_, _, a)| a).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn apps_folder_lists_apps() {
        let apps = scan_apps_folder();
        for (n, l) in apps.iter().take(15) {
            println!("{n} -> {l}");
        }
        assert!(!apps.is_empty());
    }
}

#[cfg(test)]
mod dock_icon_tests {
    use super::*;

    #[test]
    #[ignore = "Loads a custom PNG through the actual Windows thumbnail provider"]
    fn native_dock_custom_png_icon() {
        let path = std::env::temp_dir().join(format!("island-icon-{}.png", std::process::id()));
        struct File(PathBuf);
        impl Drop for File {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let _file = File(path.clone());
        let mut pixels = vec![0u8; 64 * 64 * 4];
        for y in 12..52 {
            for x in 12..52 {
                pixels[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4].copy_from_slice(&[230, 25, 40, 255]);
            }
        }
        let mut encoder = png::Encoder::new(std::fs::File::create(&path).unwrap(), 64, 64);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&pixels).unwrap();
        let mut cache = DockIconCache::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let icon = loop {
            cache.poll();
            if let Some(icon) = cache.get(path.to_str().unwrap(), 64) {
                break icon;
            }
            assert!(std::time::Instant::now() < deadline, "Custom PNG did not load");
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert!(icon.bgra.chunks_exact(4).any(|p| p[2] > 180 && p[0] < 80 && p[3] > 200));
        assert!(icon.bgra.chunks_exact(4).any(|p| p[3] == 0));
    }

    #[test]
    fn icon_buckets_cover_physical_hover_size_without_unbounded_allocation() {
        assert_eq!(icon_bucket(48), 64);
        assert_eq!(icon_bucket(97), 128);
        assert_eq!(icon_bucket(129), 256);
        assert_eq!(icon_bucket(u32::MAX), 256);
    }

    #[test]
    fn optical_padding_preserves_non_square_artwork_and_premultiplies_edges() {
        let mut icon = Icon { side: 12, bgra: vec![0; 12 * 12 * 4] };
        for y in 4..8 {
            for x in 2..10 {
                icon.bgra[(y * 12 + x) * 4..(y * 12 + x) * 4 + 4].copy_from_slice(&[220, 100, 40, 128]);
            }
        }
        let normalized = normalize_dock_icon(icon);
        assert_eq!(normalized.side, 10);
        let opaque: Vec<_> = normalized.bgra.chunks_exact(4).filter(|p| p[3] != 0).collect();
        assert_eq!(opaque.len(), 32);
        assert!(opaque.iter().all(|p| p[0] == 110 && p[1] == 50 && p[2] == 20 && p[3] == 128));
        assert!(normalized.bgra[..10 * 4].iter().all(|v| *v == 0));
    }

    #[test]
    fn already_premultiplied_icons_are_not_darkened_twice() {
        let mut icon = Icon { side: 2, bgra: [40, 20, 10, 100].repeat(4) };
        icon.bgra[..4].copy_from_slice(&[255, 255, 255, 0]);
        let normalized = normalize_dock_icon(icon);
        assert!(normalized.bgra.chunks_exact(4).filter(|p| p[3] != 0).all(|p| p == [40, 20, 10, 100]));
    }

    #[test]
    fn failed_icon_worker_does_not_leave_pending_requests_forever() {
        let (requests, _queue) = mpsc::sync_channel(96);
        let (finished, results) = mpsc::channel();
        let mut cache = DockIconCache {
            requests: Some(requests),
            results,
            ready: HashMap::new(),
            pending: HashSet::new(),
            order: VecDeque::new(),
        };
        cache.get("test.exe", 128);
        assert!(cache.pending());
        drop(finished);
        assert!(cache.poll());
        assert!(!cache.pending());
        assert!(cache.requests.is_none());
    }

    #[test]
    fn async_icon_results_ignore_removed_targets_and_keep_cache_bounded() {
        let (requests, _queue) = mpsc::sync_channel(96);
        let (finished, results) = mpsc::channel();
        let mut cache = DockIconCache {
            requests: Some(requests),
            results,
            ready: HashMap::new(),
            pending: HashSet::new(),
            order: VecDeque::new(),
        };
        cache.pending.insert(("removed.exe".into(), 128));
        cache.retain(&[]);
        finished.send((("removed.exe".into(), 128), None)).unwrap();
        assert!(!cache.poll());
        for i in 0..120 {
            let key = (format!("{i}.exe"), 128);
            cache.pending.insert(key.clone());
            finished.send((key, None)).unwrap();
        }
        assert!(cache.poll());
        assert_eq!(cache.ready.len(), 96);
        assert!(!cache.pending());
        assert!(!cache.ready.contains_key(&("0.exe".into(), 128)));
    }
}
