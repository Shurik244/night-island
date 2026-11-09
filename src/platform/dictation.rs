use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
    IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX, eCapture, eConsole,
};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
use windows::core::Result;

pub const WM_VOICE: u32 = WM_APP + 25;

const RATE: u32 = 16_000;
pub const MAX_SECS: usize = 120;
const MIN_SAMPLES: usize = RATE as usize * 6 / 10;
const SILENCE_PEAK: i16 = 120;
const AUDCLNT_BUFFERFLAGS_SILENT: u32 = 0x2;

static RUNNING: AtomicBool = AtomicBool::new(false);
static WANTED: AtomicBool = AtomicBool::new(false);
static AUTO_STOP: AtomicBool = AtomicBool::new(false);
static LEVEL: AtomicU32 = AtomicU32::new(0);
static RESULT: Mutex<Option<std::result::Result<Vec<u8>, &'static str>>> = Mutex::new(None);

const CALIBRATE_MS: u32 = 300;
const NO_SPEECH_MS: u32 = 7_000;
const END_QUIET_MS: u32 = 1_300;
const MIN_THRESHOLD: i16 = 900;
const MAX_THRESHOLD: i16 = 6_000;

pub struct Endpoint {
    seen_ms: u32,
    floor: i16,
    threshold: i16,
    spoke: bool,
    quiet_ms: u32,
}

impl Endpoint {
    pub fn new() -> Self {
        Self { seen_ms: 0, floor: 0, threshold: MIN_THRESHOLD, spoke: false, quiet_ms: 0 }
    }

    pub fn feed(&mut self, chunk: &[i16]) -> bool {
        let ms = (chunk.len() as u64 * 1000 / u64::from(RATE)) as u32;
        let peak = chunk.iter().map(|s| s.saturating_abs()).max().unwrap_or(0);
        if self.seen_ms < CALIBRATE_MS {
            self.floor = self.floor.max(peak);
            self.threshold = self.floor.saturating_mul(2).clamp(MIN_THRESHOLD, MAX_THRESHOLD);
            self.seen_ms += ms;
            return false;
        }
        self.seen_ms = self.seen_ms.saturating_add(ms);
        if peak >= self.threshold {
            self.spoke = true;
            self.quiet_ms = 0;
        } else {
            self.quiet_ms += ms;
        }
        if self.spoke { self.quiet_ms >= END_QUIET_MS } else { self.seen_ms >= NO_SPEECH_MS }
    }
}

impl Default for Endpoint {
    fn default() -> Self {
        Self::new()
    }
}

pub fn active() -> bool {
    RUNNING.load(Ordering::SeqCst)
}

pub fn level() -> f32 {
    LEVEL.load(Ordering::Relaxed) as f32 / 1000.0
}

fn packet_level(samples: &[i16]) -> u32 {
    if samples.is_empty() {
        return 0;
    }
    let energy = samples.iter().map(|sample| (f64::from(*sample) / 32768.0).powi(2)).sum::<f64>();
    ((energy / samples.len() as f64).sqrt() * 4000.0).min(1000.0) as u32
}

pub fn start_auto(hwnd: HWND) -> bool {
    begin(hwnd, true)
}

pub fn start(hwnd: HWND) -> bool {
    begin(hwnd, false)
}

fn begin(hwnd: HWND, auto: bool) -> bool {
    let Ok(result) = RESULT.lock() else { return false };
    if result.is_some() {
        return false;
    }
    if RUNNING.swap(true, Ordering::SeqCst) {
        return false;
    }
    drop(result);
    LEVEL.store(0, Ordering::Relaxed);
    AUTO_STOP.store(auto, Ordering::SeqCst);
    WANTED.store(true, Ordering::SeqCst);
    let raw = hwnd.0 as isize;
    std::thread::spawn(move || {
        let outcome = match capture() {
            Ok(samples) => finish(&samples),
            Err(_) => Err("The microphone could not be opened"),
        };
        LEVEL.store(0, Ordering::Relaxed);
        if let Ok(mut r) = RESULT.lock() {
            *r = Some(outcome);
        }
        RUNNING.store(false, Ordering::SeqCst);
        unsafe {
            let _ = PostMessageW(Some(HWND(raw as *mut c_void)), WM_VOICE, WPARAM(0), LPARAM(0));
        }
    });
    true
}

pub fn stop() {
    WANTED.store(false, Ordering::SeqCst);
}

pub fn take() -> Option<std::result::Result<Vec<u8>, &'static str>> {
    RESULT.lock().ok()?.take()
}

fn finish(samples: &[i16]) -> std::result::Result<Vec<u8>, &'static str> {
    if samples.len() < MIN_SAMPLES {
        return Err("Too short, nothing was recorded");
    }
    if samples.iter().map(|s| s.saturating_abs()).max().unwrap_or(0) < SILENCE_PEAK {
        return Err("Nothing was heard (is the microphone muted?)");
    }
    Ok(wav(samples))
}

pub fn wav(samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

fn capture() -> Result<Vec<i16>> {
    let mut samples: Vec<i16> = Vec::with_capacity(RATE as usize * 10);
    let mut endpoint = Endpoint::new();
    let auto = AUTO_STOP.load(Ordering::SeqCst);
    let started = Instant::now();
    stream(|chunk| {
        let count = chunk.len().min(RATE as usize * MAX_SECS - samples.len());
        samples.extend_from_slice(&chunk[..count]);
        LEVEL.store(packet_level(chunk), Ordering::Relaxed);
        WANTED.load(Ordering::SeqCst)
            && samples.len() < RATE as usize * MAX_SECS
            && started.elapsed() < Duration::from_secs(MAX_SECS as u64)
            && !(auto && endpoint.feed(chunk))
    })?;
    Ok(samples)
}

pub fn stream(mut on_chunk: impl FnMut(&[i16]) -> bool) -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        struct Runtime;
        impl Drop for Runtime {
            fn drop(&mut self) {
                unsafe { CoUninitialize() };
            }
        }
        let _runtime = Runtime;
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eCapture, eConsole)?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        let format = WAVEFORMATEX {
            wFormatTag: 1,
            nChannels: 1,
            nSamplesPerSec: RATE,
            nAvgBytesPerSec: RATE * 2,
            nBlockAlign: 2,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
            2_000_000,
            0,
            &format,
            None,
        )?;
        let capture: IAudioCaptureClient = client.GetService()?;
        client.Start()?;
        struct Recording(IAudioClient);
        impl Drop for Recording {
            fn drop(&mut self) {
                unsafe {
                    let _ = self.0.Stop();
                }
            }
        }
        let _recording = Recording(client);
        let mut chunk: Vec<i16> = Vec::with_capacity(RATE as usize / 4);
        'run: loop {
            std::thread::sleep(Duration::from_millis(30));
            chunk.clear();
            while chunk.len() < RATE as usize && capture.GetNextPacketSize()? > 0 {
                let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0u32, 0u32);
                capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                let n = frames as usize;
                if flags & AUDCLNT_BUFFERFLAGS_SILENT != 0 || data.is_null() {
                    chunk.resize(chunk.len() + n, 0);
                } else {
                    chunk.extend((0..n).map(|i| (data as *const i16).add(i).read_unaligned()));
                }
                capture.ReleaseBuffer(frames)?;
            }
            if !on_chunk(&chunk) {
                break 'run;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wav_file_has_a_correct_header() {
        let file = wav(&[0, 1, -1, 300]);
        assert_eq!(&file[..4], b"RIFF");
        assert_eq!(&file[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(file[24..28].try_into().unwrap()), RATE);
        assert_eq!(u32::from_le_bytes(file[40..44].try_into().unwrap()), 8);
        assert_eq!(file.len(), 44 + 8);
        assert_eq!(u32::from_le_bytes(file[4..8].try_into().unwrap()) as usize, file.len() - 8);
    }

    #[test]
    #[ignore = "needs a microphone"]
    fn records_the_default_microphone() {
        WANTED.store(true, Ordering::SeqCst);
        let stopper = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(1500));
            WANTED.store(false, Ordering::SeqCst);
        });
        let samples = capture().expect("the microphone opened");
        stopper.join().unwrap();
        let peak = samples.iter().map(|s| s.saturating_abs()).max().unwrap_or(0);
        println!("{} samples ({:.2} s), peak {peak}", samples.len(), samples.len() as f32 / RATE as f32);
        assert!(samples.len() > RATE as usize);
    }

    fn run(endpoint: &mut Endpoint, level: i16, ms: u32) -> Option<u32> {
        let chunk = vec![level; (RATE * 30 / 1000) as usize];
        (0..ms / 30).find(|_| endpoint.feed(&chunk)).map(|i| (i + 1) * 30)
    }

    #[test]
    fn the_recording_ends_after_a_pause_that_follows_speech() {
        let mut e = Endpoint::new();
        assert_eq!(run(&mut e, 200, 400), None);
        assert_eq!(run(&mut e, 9000, 1500), None);
        let ended = run(&mut e, 200, 3000).expect("the pause ends it");
        assert!((END_QUIET_MS..END_QUIET_MS + 60).contains(&ended), "ended after {ended} ms");
    }

    #[test]
    fn a_noisy_room_raises_the_speech_threshold() {
        let mut e = Endpoint::new();
        assert_eq!(run(&mut e, 2500, 400), None);
        assert_eq!(run(&mut e, 3000, 5000), None);
        assert!(!e.spoke, "steady noise is not speech");
        assert_eq!(run(&mut e, 12000, 300), None);
        assert!(e.spoke);
    }

    #[test]
    fn nobody_speaking_gives_up_after_a_while() {
        let mut e = Endpoint::new();
        let ended = run(&mut e, 100, 20_000).expect("gives up");
        assert!((NO_SPEECH_MS..NO_SPEECH_MS + 400).contains(&ended), "gave up after {ended} ms");
    }

    #[test]
    fn short_and_silent_recordings_are_refused() {
        assert!(finish(&[5000; 100]).is_err());
        assert!(finish(&vec![3; MIN_SAMPLES * 2]).is_err());
        assert!(finish(&vec![4000; MIN_SAMPLES * 2]).is_ok());
    }
}
