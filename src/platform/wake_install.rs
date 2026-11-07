use std::ffi::c_void;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::platform::wake;
use crate::services::http;

pub const WM_WAKE_SETUP: u32 = WM_APP + 81;

pub struct Package {
    pub url: &'static str,
    pub sha256: &'static str,
    pub max_bytes: usize,
    pub files: &'static [(&'static str, &'static str)],
}

pub const PACKAGES: [Package; 2] = [
    Package {
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.13.5/sherpa-onnx-v1.13.5-win-x64-shared-MT-Release.tar.bz2",
        sha256: "70b7193e40ce5c393a218c6383069a64cf6479962ae87cdc19373b8b57e3f285",
        max_bytes: 40 << 20,
        files: &[("onnxruntime.dll", "bin/onnxruntime.dll"), ("sherpa-onnx-c-api.dll", "bin/sherpa-onnx-c-api.dll")],
    },
    Package {
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-streaming-zipformer-small-ru-vosk-int8-2025-08-16.tar.bz2",
        sha256: "6ba68a01ff3c5445aaf2d61e9b97b026f1149dcc9049d11af3f44f55176341d8",
        max_bytes: 40 << 20,
        files: &[
            ("encoder.int8.onnx", "model/encoder.int8.onnx"),
            ("decoder.onnx", "model/decoder.onnx"),
            ("joiner.int8.onnx", "model/joiner.int8.onnx"),
            ("tokens.txt", "model/tokens.txt"),
        ],
    },
];

const MAX_FILE_BYTES: u64 = 48 << 20;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Idle,
    Working(u8),
    Ready,
    Failed(&'static str),
}

static STATE: Mutex<State> = Mutex::new(State::Idle);
static BUSY: AtomicBool = AtomicBool::new(false);

pub(super) fn begin_install() -> bool {
    !BUSY.swap(true, Ordering::SeqCst)
}

pub(super) fn end_install() {
    BUSY.store(false, Ordering::SeqCst);
}

pub fn state() -> State {
    STATE.lock().map_or(State::Idle, |s| *s)
}

fn set(state: State) {
    if let Ok(mut s) = STATE.lock() {
        *s = state;
    }
}

pub fn digest_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

pub fn extract(archive: impl Read, files: &[(&str, &str)], into: &Path) -> Result<usize, &'static str> {
    let mut tar = tar::Archive::new(archive);
    let mut done = 0;
    let mut seen: Vec<&str> = Vec::new();
    for entry in tar.entries().map_err(|_| "The archive is damaged")? {
        let mut entry = entry.map_err(|_| "The archive is damaged")?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let name =
            entry.path().ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).unwrap_or_default();
        let Some((_, rel)) = files.iter().find(|(want, _)| *want == name) else { continue };
        if seen.contains(rel) {
            continue;
        }
        seen.push(rel);
        if entry.size() > MAX_FILE_BYTES {
            return Err("A file in the archive is too large");
        }
        let target = into.join(rel);
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir).map_err(|_| "Could not write to the profile folder")?;
        }
        let tmp = target.with_extension("part");
        let mut out = std::fs::File::create(&tmp).map_err(|_| "Could not write to the profile folder")?;
        let copied = std::io::copy(&mut (&mut entry).take(MAX_FILE_BYTES + 1), &mut out)
            .map_err(|_| "Could not write to the profile folder")?;
        drop(out);
        if copied > MAX_FILE_BYTES {
            let _ = std::fs::remove_file(&tmp);
            return Err("A file in the archive is too large");
        }
        std::fs::rename(&tmp, &target).map_err(|_| "Could not write to the profile folder")?;
        done += 1;
    }
    Ok(done)
}

pub(crate) fn fetch(package: &Package) -> Result<Vec<u8>, &'static str> {
    let reply = http::get_bytes(package.url, package.max_bytes, None).ok_or("Could not download the voice runtime")?;
    if reply.status != 200 {
        return Err("The download server refused the request");
    }
    if digest_hex(&reply.body) != package.sha256 {
        return Err("The download does not match the expected checksum");
    }
    Ok(reply.body)
}

fn install_to(root: &Path) -> Result<(), &'static str> {
    for (n, package) in PACKAGES.iter().enumerate() {
        if n == 0 && super::localvoice::runtime_in(root) {
            continue;
        }
        set(State::Working(n as u8 * 50));
        let body = fetch(package)?;
        set(State::Working(n as u8 * 50 + 25));
        let reader = bzip2_rs::DecoderReader::new(body.as_slice());
        let count = extract(reader, package.files, root)?;
        if count != package.files.len() {
            return Err("The archive is missing files");
        }
    }
    if wake::installed_in(root) { Ok(()) } else { Err("The installation is incomplete") }
}

pub fn root() -> Option<PathBuf> {
    wake::dir()
}

pub fn start(hwnd: HWND) -> bool {
    if !begin_install() {
        return false;
    }
    let Some(root) = root() else {
        set(State::Failed("No profile folder"));
        end_install();
        return false;
    };
    let raw = hwnd.0 as isize;
    set(State::Working(0));
    std::thread::spawn(move || {
        match install_to(&root) {
            Ok(()) => set(State::Ready),
            Err(why) => set(State::Failed(why)),
        }
        end_install();
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut c_void)), WM_WAKE_SETUP, WPARAM(0), LPARAM(0));
        }
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tar_with(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, name, *data).unwrap();
        }
        builder.into_inner().unwrap()
    }

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ni-wake-install-{name}-{}", std::process::id()))
    }

    #[test]
    fn only_the_listed_files_are_taken_and_paths_inside_the_archive_do_not_matter() {
        let archive = tar_with(&[
            ("pkg/lib/onnxruntime.dll", b"ort"),
            ("pkg/lib/other.dll", b"nope"),
            ("pkg/sherpa-onnx-c-api.dll", b"api"),
        ]);
        let into = scratch("pick");
        let want = [("onnxruntime.dll", "bin/onnxruntime.dll"), ("sherpa-onnx-c-api.dll", "bin/sherpa-onnx-c-api.dll")];
        assert_eq!(extract(archive.as_slice(), &want, &into), Ok(2));
        assert_eq!(std::fs::read(into.join("bin/onnxruntime.dll")).unwrap(), b"ort");
        assert!(!into.join("bin/other.dll").exists());
        assert!(!into.join("bin/onnxruntime.part").exists());
        let _ = std::fs::remove_dir_all(&into);
    }

    #[test]
    fn a_broken_archive_is_reported() {
        let into = scratch("broken");
        let junk = vec![7u8; 2048];
        assert!(extract(junk.as_slice(), &[("a", "b")], &into).is_err());
        let _ = std::fs::remove_dir_all(&into);
    }

    #[test]
    fn checksums_are_lowercase_hex() {
        assert_eq!(digest_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert!(PACKAGES.iter().all(|p| p.sha256.len() == 64 && p.url.starts_with("https://github.com/")));
    }

    #[test]
    fn every_file_the_detector_needs_is_delivered_by_some_package() {
        let delivered: Vec<&str> = PACKAGES.iter().flat_map(|p| p.files.iter().map(|f| f.1)).collect();
        assert!(wake::FILES.iter().all(|f| delivered.contains(f)));
    }

    #[test]
    #[ignore = "downloads about 46 MB from GitHub into a scratch folder"]
    fn the_real_packages_download_verify_and_extract() {
        let into = scratch("real");
        install_to(&into).expect("installed");
        assert!(wake::installed_in(&into));
        let _ = std::fs::remove_dir_all(&into);
    }
}
