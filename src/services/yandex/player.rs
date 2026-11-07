use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU8, Ordering},
    mpsc::{self, SyncSender},
};
use windows::Foundation::{TimeSpan, TypedEventHandler, Uri};
use windows::Media::Core::MediaSource;
use windows::Media::Playback::{MediaPlaybackState, MediaPlayer};
use windows::Media::{MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControlsButton};
use windows::Storage::Streams::RandomAccessStreamReference;
use windows::core::HSTRING;

use super::{Command, Track};

pub struct Player {
    inner: MediaPlayer,
    outcome: Arc<AtomicU8>,
}

impl Player {
    pub fn open(
        url: &str,
        track: &Track,
        generation: u64,
        sender: SyncSender<Command>,
        volume: u8,
    ) -> windows::core::Result<Self> {
        let inner = MediaPlayer::new()?;
        inner.CommandManager()?.SetIsEnabled(false)?;
        let controls = inner.SystemMediaTransportControls()?;
        controls.SetIsEnabled(true)?;
        controls.SetIsPlayEnabled(true)?;
        controls.SetIsPauseEnabled(true)?;
        controls.SetIsNextEnabled(true)?;
        controls.SetIsPreviousEnabled(true)?;
        let outcome = Arc::new(AtomicU8::new(0));
        let tx = sender.clone();
        controls.ButtonPressed(&TypedEventHandler::new(
            move |_, args: windows::core::Ref<windows::Media::SystemMediaTransportControlsButtonPressedEventArgs>| {
                if let Some(args) = args.as_ref() {
                    let cmd = match args.Button()? {
                        SystemMediaTransportControlsButton::Play => Some(Command::Resume),
                        SystemMediaTransportControlsButton::Pause => Some(Command::Pause),
                        SystemMediaTransportControlsButton::Next => Some(Command::Next),
                        SystemMediaTransportControlsButton::Previous => Some(Command::Previous),
                        _ => None,
                    };
                    if let Some(cmd) = cmd {
                        let _ = tx.try_send(cmd);
                    }
                }
                Ok(())
            },
        ))?;
        let tx = sender.clone();
        let ended = outcome.clone();
        inner.MediaEnded(&TypedEventHandler::new(move |_, _| {
            ended.store(1, Ordering::Release);
            let _ = tx.try_send(Command::Ended(generation));
            Ok(())
        }))?;
        let failed = outcome.clone();
        inner.MediaFailed(&TypedEventHandler::new(move |_, _| {
            failed.store(2, Ordering::Release);
            let _ = sender.try_send(Command::Failed(generation));
            Ok(())
        }))?;
        let updater = controls.DisplayUpdater()?;
        updater.SetType(MediaPlaybackType::Music)?;
        updater.MusicProperties()?.SetTitle(&HSTRING::from(&track.title))?;
        updater.MusicProperties()?.SetArtist(&HSTRING::from(&track.artist))?;
        if !track.cover.is_empty() {
            updater.SetThumbnail(&RandomAccessStreamReference::CreateFromUri(&Uri::CreateUri(&HSTRING::from(
                &track.cover,
            ))?)?)?;
        }
        updater.Update()?;
        inner.SetSource(&MediaSource::CreateFromUri(&Uri::CreateUri(&HSTRING::from(url))?)?)?;
        inner.SetVolume(f64::from(volume.min(100)) / 100.0)?;
        inner.Play()?;
        Ok(Self { inner, outcome })
    }

    pub fn outcome(&self) -> u8 {
        self.outcome.swap(0, Ordering::AcqRel)
    }

    pub fn set_volume(&self, volume: u8) -> windows::core::Result<()> {
        self.inner.SetVolume(f64::from(volume.min(100)) / 100.0)
    }

    pub fn pause(&self) {
        let _ = self.inner.Pause();
    }
    pub fn play(&self) {
        let _ = self.inner.Play();
    }
    pub fn seek(&self, seconds: f32) {
        if seconds.is_finite()
            && let Ok(s) = self.inner.PlaybackSession()
        {
            let length = s.NaturalDuration().map_or(0.0, |v| v.Duration as f32 / 1e7);
            let _ = s.SetPosition(TimeSpan { Duration: (seconds.clamp(0.0, length.max(0.0)) as f64 * 1e7) as i64 });
        }
    }
    pub fn state(&self) -> (f32, f32, bool) {
        let Ok(s) = self.inner.PlaybackSession() else { return (0.0, 0.0, false) };
        let playing = s.PlaybackState().is_ok_and(|v| v == MediaPlaybackState::Playing);
        if let Ok(controls) = self.inner.SystemMediaTransportControls() {
            let _ = controls.SetPlaybackStatus(if playing {
                MediaPlaybackStatus::Playing
            } else {
                MediaPlaybackStatus::Paused
            });
        }
        (
            s.Position().map_or(0.0, |v| v.Duration as f32 / 1e7),
            s.NaturalDuration().map_or(0.0, |v| v.Duration as f32 / 1e7),
            playing,
        )
    }
}

type CoverJob = (String, u64, SyncSender<Command>);
static COVERS: OnceLock<SyncSender<CoverJob>> = OnceLock::new();

pub fn load_cover(url: String, generation: u64, sender: SyncSender<Command>) {
    let worker = COVERS.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<CoverJob>(2);
        std::thread::spawn(move || {
            unsafe {
                let _ =
                    windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED);
            }
            while let Ok(mut job) = rx.recv() {
                while let Ok(next) = rx.try_recv() {
                    job = next;
                }
                if let Some(cover) = crate::platform::media::cover_url(&job.0) {
                    let _ = job.2.try_send(Command::Cover(job.1, cover));
                }
            }
        });
        tx
    });
    let _ = worker.try_send((url, generation, sender));
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.inner.Pause();
        if let Ok(c) = self.inner.SystemMediaTransportControls() {
            let _ = c.SetIsEnabled(false);
        }
        let _ = self.inner.Close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    #[test]
    #[ignore = "Uses the Windows audio engine with a generated silent WAV"]
    fn native_wave_player_plays_pauses_seeks_and_reports_end() -> windows::core::Result<()> {
        unsafe {
            windows::Win32::System::WinRT::RoInitialize(windows::Win32::System::WinRT::RO_INIT_MULTITHREADED)?;
        }
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("night-island-wave-test-{stamp}.wav"));
        let bytes = 64_000u32;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(bytes + 36).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&16_000u32.to_le_bytes());
        wav.extend_from_slice(&32_000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&bytes.to_le_bytes());
        wav.resize(wav.len() + bytes as usize, 0);
        std::fs::write(&path, wav).unwrap();
        let url = format!("file:///{}", path.to_string_lossy().replace('\\', "/"));
        let (tx, rx) = std::sync::mpsc::sync_channel(16);
        let track = Track {
            id: "test".into(),
            title: "Night Island audio test".into(),
            artist: "Silent fixture".into(),
            cover: String::new(),
            batch: String::new(),
            session: String::new(),
            palette: Default::default(),
        };
        let player = Player::open(&url, &track, 73, tx, 37)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while !player.state().2 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(30));
        }
        assert!(player.state().2, "native player did not start");
        assert!(player.state().1 > 1.5);
        assert!((player.inner.Volume()? - 0.37).abs() < 0.001);
        player.set_volume(0)?;
        assert_eq!(player.inner.Volume()?, 0.0);
        player.set_volume(27)?;
        assert!((player.inner.Volume()? - 0.27).abs() < 0.001);
        let manager =
            windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.join()?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let session = manager.GetSessions()?.into_iter().find(|session| {
                session
                    .TryGetMediaPropertiesAsync()
                    .and_then(|p| p.join())
                    .and_then(|p| p.Title())
                    .is_ok_and(|title| title == "Night Island audio test")
            });
            if let Some(session) = session {
                let id = session.SourceAppUserModelId()?.to_string();
                assert!(
                    crate::platform::media::own_session(&id),
                    "native player has unexpected session identity: {id}"
                );
                break;
            }
            assert!(Instant::now() < deadline, "native player was not published to Windows media sessions");
            std::thread::sleep(Duration::from_millis(50));
        }
        player.pause();
        std::thread::sleep(Duration::from_millis(100));
        assert!(!player.state().2);
        player.seek(1.4);
        player.play();
        let end = rx.recv_timeout(Duration::from_secs(5)).expect("missing ended event");
        assert!(matches!(end, Command::Ended(73)));
        drop(player);
        let _ = std::fs::remove_file(path);
        unsafe {
            windows::Win32::System::WinRT::RoUninitialize();
        }
        Ok(())
    }
}
