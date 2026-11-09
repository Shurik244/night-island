use crate::platform::media::{self, Cover};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

const MAX_BYTES: u64 = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 256;
const MAX_AGE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

fn directory() -> Option<PathBuf> {
    Some(
        PathBuf::from(std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("APPDATA"))?)
            .join("NightIsland")
            .join("cache")
            .join("yandex-covers"),
    )
}

fn cache_path(dir: &Path, url: &str) -> PathBuf {
    dir.join(format!("{:x}.img", Sha256::digest(url.as_bytes())))
}

fn read(path: &Path) -> Option<Arc<Cover>> {
    let meta = fs::metadata(path).ok()?;
    if meta.len() > 3_000_000 || SystemTime::now().duration_since(meta.modified().ok()?).unwrap_or_default() > MAX_AGE {
        return None;
    }
    media::cover_bytes_sized(&fs::read(path).ok()?, 128)
}

fn prune(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_stem()?.to_str()?;
            if path.extension()?.to_str()? != "img" || name.len() != 64 || !name.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return None;
            }
            let meta = entry.metadata().ok()?;
            meta.is_file().then(|| (path, meta.len(), meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)))
        })
        .collect();
    entries.sort_unstable_by_key(|v| std::cmp::Reverse(v.2));
    let mut bytes = 0u64;
    for (index, (path, len, modified)) in entries.into_iter().enumerate() {
        bytes = bytes.saturating_add(len);
        if index >= MAX_ENTRIES
            || bytes > MAX_BYTES
            || SystemTime::now().duration_since(modified).unwrap_or_default() > MAX_AGE
        {
            let _ = fs::remove_file(path);
        }
    }
}

fn load_from(url: &str, dir: Option<&Path>) -> Option<Arc<Cover>> {
    load_with(url, dir, |url| crate::services::http::get_bytes(url, 3_000_000, None))
}

fn load_with(
    url: &str,
    dir: Option<&Path>,
    fetch: impl FnOnce(&str) -> Option<crate::services::http::BytesReply>,
) -> Option<Arc<Cover>> {
    if !super::api::trusted_url(url) {
        return None;
    }
    let url = url.replace("300x300", "200x200");
    let path = dir.map(|dir| cache_path(dir, &url));
    if let Some(image) = path.as_deref().and_then(read) {
        return Some(image);
    }
    let reply = fetch(&url)?;
    if reply.status != 200 {
        return None;
    }
    let image = media::cover_bytes_sized(&reply.body, 128)?;
    if let (Some(dir), Some(path)) = (dir, path)
        && fs::create_dir_all(dir).is_ok()
    {
        let tmp = path.with_extension("tmp");
        if fs::write(&tmp, &reply.body).is_ok() && fs::rename(&tmp, path).is_err() {
            let _ = fs::remove_file(tmp);
        }
    }
    Some(image)
}

pub fn trim_cache() {
    if let Some(dir) = directory() {
        prune(&dir);
    }
}

pub fn load(url: &str, persist: bool) -> Option<Arc<Cover>> {
    let dir = persist.then(directory).flatten();
    load_from(url, dir.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Needs WinRT imaging, which wine does not provide"]
    fn cache_serves_covers_offline_and_replaces_corrupt_entries() {
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
        }
        let dir = std::env::temp_dir().join(format!(
            "island-cover-offline-{}",
            SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let url = "https://avatars.yandex.net/get-music-content/fixture/300x300";
        let expected = url.replace("300x300", "200x200");
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().unwrap().write_image_data(&[255, 0, 0, 255].repeat(4)).unwrap();
        }
        let fetch = |url: &str| {
            assert!(url.ends_with("200x200"));
            Some(crate::services::http::BytesReply { status: 200, body: bytes.clone() })
        };
        assert_eq!(load_with(url, Some(&dir), fetch).unwrap().size, 128);
        assert!(load_with(url, Some(&dir), |_| panic!("A cached cover must not need the network")).is_some());
        fs::write(cache_path(&dir, &expected), b"corrupt").unwrap();
        assert!(load_with(url, Some(&dir), fetch).is_some());
        assert!(load_with(url, None, fetch).is_some());
        fs::remove_file(cache_path(&dir, &expected)).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn cache_keys_ignore_filesystem_segments_and_pruning_is_bounded() {
        let dir = std::env::temp_dir().join(format!(
            "island-cover-cache-{}",
            SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = cache_path(&dir, "https://avatars.yandex.net/a/../../cover?size=200x200");
        assert_eq!(path.parent(), Some(dir.as_path()));
        for i in 0..MAX_ENTRIES + 4 {
            fs::write(cache_path(&dir, &i.to_string()), [1, 2, 3]).unwrap();
        }
        fs::write(dir.join("keep.txt"), b"keep").unwrap();
        prune(&dir);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), MAX_ENTRIES + 1);
        assert!(dir.join("keep.txt").exists());
        for entry in fs::read_dir(&dir).unwrap().flatten() {
            fs::remove_file(entry.path()).unwrap();
        }
        fs::remove_dir(dir).unwrap();
    }
}
