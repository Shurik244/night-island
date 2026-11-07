use std::collections::VecDeque;
use std::time::{Duration, Instant};

use windows::Win32::Media::Audio::*;
use windows::core::PSTR;

use super::{NATURAL_RATE, PLAYBACK, Playback, current, pcm_levels};

const BLOCK: usize = NATURAL_RATE as usize * 2 / 25;
const BUFFER_COUNT: usize = 5;

struct Packet {
    header: WAVEHDR,
    bytes: Vec<u8>,
}

pub(super) struct Output {
    device: HWAVEOUT,
    packets: VecDeque<Box<Packet>>,
    tail: Vec<u8>,
    generation: u32,
    factor: f32,
    pub started: bool,
}

impl Output {
    pub fn new(generation: u32, volume: u8) -> Option<Self> {
        let mut device = HWAVEOUT::default();
        let format = WAVEFORMATEX {
            wFormatTag: 1,
            nChannels: 1,
            nSamplesPerSec: NATURAL_RATE,
            nAvgBytesPerSec: NATURAL_RATE * 2,
            nBlockAlign: 2,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        let status =
            unsafe { waveOutOpen(Some(&mut device), WAVE_MAPPER, &format, None, None, MIDI_WAVE_OPEN_TYPE(0)) };
        (status == 0).then(|| Self {
            device,
            packets: VecDeque::new(),
            tail: Vec::new(),
            generation,
            factor: f32::from(volume.min(100)) / 100.0,
            started: false,
        })
    }

    fn reap(&mut self) {
        while self
            .packets
            .front()
            .is_some_and(|packet| unsafe { std::ptr::read_volatile(&raw const packet.header.dwFlags) & WHDR_DONE != 0 })
        {
            let mut packet = self.packets.pop_front().unwrap();
            unsafe {
                let _ = waveOutUnprepareHeader(self.device, &mut packet.header, size_of::<WAVEHDR>() as u32);
            }
        }
    }

    fn write(&mut self, mut bytes: Vec<u8>) -> Option<()> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if !current(self.generation) || Instant::now() >= deadline {
                return None;
            }
            self.reap();
            if self.packets.len() < BUFFER_COUNT {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let fresh = self.packets.is_empty();
        for pair in bytes.chunks_exact_mut(2) {
            let sample = f32::from(i16::from_le_bytes([pair[0], pair[1]]));
            pair.copy_from_slice(&((sample * self.factor) as i16).to_le_bytes());
        }
        let levels = pcm_levels(&bytes, BLOCK / 2);
        let mut packet = Box::new(Packet {
            header: WAVEHDR {
                lpData: PSTR(bytes.as_mut_ptr()),
                dwBufferLength: bytes.len() as u32,
                ..Default::default()
            },
            bytes,
        });
        unsafe {
            let size = size_of::<WAVEHDR>() as u32;
            if waveOutPrepareHeader(self.device, &mut packet.header, size) != 0 {
                return None;
            }
            if waveOutWrite(self.device, &mut packet.header, size) != 0 {
                let _ = waveOutUnprepareHeader(self.device, &mut packet.header, size);
                return None;
            }
        }
        self.packets.push_back(packet);
        self.started = true;
        if let Ok(mut playback) = PLAYBACK.lock()
            && current(self.generation)
        {
            if fresh || playback.as_ref().is_none_or(|p| p.generation != self.generation) {
                *playback = Some(Playback { generation: self.generation, started: Instant::now(), levels });
            } else if let Some(playback) = playback.as_mut() {
                playback.levels.extend(levels);
            }
        }
        Some(())
    }

    pub fn push(&mut self, mut data: &[u8]) -> Option<()> {
        while !data.is_empty() {
            let count = (BLOCK - self.tail.len()).min(data.len());
            self.tail.extend_from_slice(&data[..count]);
            data = &data[count..];
            if self.tail.len() == BLOCK {
                let packet = std::mem::take(&mut self.tail);
                self.write(packet)?;
            }
        }
        Some(())
    }

    pub fn drain(&mut self) {
        self.tail.truncate(self.tail.len() & !1);
        if !self.tail.is_empty() {
            let tail = std::mem::take(&mut self.tail);
            let _ = self.write(tail);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while current(self.generation) && Instant::now() < deadline {
            self.reap();
            if self.packets.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        unsafe {
            let _ = waveOutReset(self.device);
            for mut packet in self.packets.drain(..) {
                let _ = waveOutUnprepareHeader(self.device, &mut packet.header, size_of::<WAVEHDR>() as u32);
                drop(packet.bytes);
            }
            let _ = waveOutClose(self.device);
        }
    }
}
