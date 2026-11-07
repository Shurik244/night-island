use std::f32::consts::TAU;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
    MMDeviceEnumerator, WAVEFORMATEX, eConsole, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree};
use windows::core::Result;

pub const BANDS: usize = 20;
const WINDOW: usize = 1024;
const AUDCLNT_BUFFERFLAGS_SILENT: u32 = 0x2;

static RUNNING: AtomicBool = AtomicBool::new(false);
static LEVELS: [AtomicU32; BANDS] = [const { AtomicU32::new(0) }; BANDS];

pub fn level(band: usize) -> f32 {
    LEVELS.get(band).map_or(0.0, |l| f32::from_bits(l.load(Ordering::Relaxed)))
}

#[cfg(test)]
pub fn debug_levels(values: [f32; BANDS]) {
    for (level, value) in LEVELS.iter().zip(values) {
        level.store(value.to_bits(), Ordering::Relaxed);
    }
}

pub fn start() {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        let _ = capture();
        RUNNING.store(false, Ordering::SeqCst);
        for l in &LEVELS {
            l.store(0, Ordering::Relaxed);
        }
    });
}

pub fn stop() {
    RUNNING.store(false, Ordering::SeqCst);
}

fn centres() -> [f32; BANDS] {
    std::array::from_fn(|i| 55.0 * (12_000.0f32 / 55.0).powf(i as f32 / (BANDS - 1) as f32))
}

fn goertzel(block: &[f32], window: &[f32], freq: f32, rate: f32) -> f32 {
    let coeff = 2.0 * (TAU * freq / rate).cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for (x, w) in block.iter().zip(window) {
        let s = x * w + coeff * s1 - s2;
        s2 = s1;
        s1 = s;
    }
    let power = s1 * s1 + s2 * s2 - coeff * s1 * s2;
    power.max(0.0).sqrt() / (block.len() as f32 / 4.0)
}

fn capture() -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        let format: *mut WAVEFORMATEX = client.GetMixFormat()?;
        let (rate, channels, bits, tag) = (
            (*format).nSamplesPerSec as f32,
            ((*format).nChannels as usize).max(1),
            (*format).wBitsPerSample,
            (*format).wFormatTag,
        );
        let is_float = bits == 32 && (tag == 3 || tag == 0xFFFE);
        let init =
            client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, 1_000_000, 0, format, None);
        CoTaskMemFree(Some(format as *const _));
        init?;
        let capture: IAudioCaptureClient = client.GetService()?;
        client.Start()?;

        let freqs = centres();
        let window: Vec<f32> = (0..WINDOW).map(|n| 0.5 - 0.5 * (TAU * n as f32 / (WINDOW - 1) as f32).cos()).collect();
        let mut ring: Vec<f32> = Vec::with_capacity(WINDOW * 3);
        let mut fresh = 0usize;
        let mut smoothed = [0.0f32; BANDS];
        let mut peak = 0.3f32;

        while RUNNING.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(14));
            while capture.GetNextPacketSize().unwrap_or(0) > 0 {
                let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0u32, 0u32);
                if capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None).is_err() {
                    break;
                }
                let frames_n = frames as usize;
                for f in 0..frames_n {
                    let mut sum = 0.0f32;
                    if flags & AUDCLNT_BUFFERFLAGS_SILENT == 0 && !data.is_null() {
                        for c in 0..channels {
                            let idx = f * channels + c;
                            sum += if is_float {
                                (data as *const f32).add(idx).read_unaligned()
                            } else {
                                f32::from((data as *const i16).add(idx).read_unaligned()) / 32768.0
                            };
                        }
                    }
                    ring.push(sum / channels as f32);
                }
                fresh += frames_n;
                let _ = capture.ReleaseBuffer(frames);
            }
            if ring.len() > WINDOW * 2 {
                ring.drain(..ring.len() - WINDOW * 2);
            }
            if fresh >= 600 && ring.len() >= WINDOW {
                fresh = 0;
                let block = &ring[ring.len() - WINDOW..];
                let mut targets = [0.0f32; BANDS];
                for (i, freq) in freqs.iter().enumerate() {
                    let amplitude = goertzel(block, &window, *freq, rate);
                    let db = 20.0 * (amplitude + 1e-7).log10() + 3.5 * i as f32 / BANDS as f32 * 6.0;
                    targets[i] = ((db + 66.0) / 62.0).clamp(0.0, 1.0);
                }
                let loudest = targets.iter().copied().fold(0.0f32, f32::max);
                peak = (peak * 0.992).max(loudest).max(0.3);
                let gain = (0.9 / peak).clamp(1.0, 3.0);
                for i in 0..BANDS {
                    let target = (targets[i] * gain).clamp(0.0, 1.0).powf(1.15);
                    let old = smoothed[i];
                    smoothed[i] = if target > old { old + (target - old) * 0.65 } else { old * 0.86 + target * 0.14 };
                    LEVELS[i].store(smoothed[i].to_bits(), Ordering::Relaxed);
                }
            } else if capture.GetNextPacketSize().unwrap_or(0) == 0 {
                for i in 0..BANDS {
                    smoothed[i] *= 0.97;
                    LEVELS[i].store(smoothed[i].to_bits(), Ordering::Relaxed);
                }
            }
        }
        let _ = client.Stop();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goertzel_finds_the_band_of_a_sine() {
        let rate = 48_000.0;
        let window: Vec<f32> = (0..WINDOW).map(|n| 0.5 - 0.5 * (TAU * n as f32 / (WINDOW - 1) as f32).cos()).collect();
        let tone: Vec<f32> = (0..WINDOW).map(|n| 0.5 * (TAU * 1000.0 * n as f32 / rate).sin()).collect();
        let at_tone = goertzel(&tone, &window, 1000.0, rate);
        let far = goertzel(&tone, &window, 6000.0, rate);
        assert!(at_tone > 0.15 && at_tone < 0.6, "amplitude near the tone was {at_tone}");
        assert!(far < at_tone / 20.0, "leakage {far} vs {at_tone}");
    }

    #[test]
    fn bands_are_ordered_and_in_range() {
        let c = centres();
        assert!((c[0] - 55.0).abs() < 0.01 && (c[BANDS - 1] - 12_000.0).abs() < 1.0);
        assert!(c.windows(2).all(|w| w[1] > w[0]));
    }
}
