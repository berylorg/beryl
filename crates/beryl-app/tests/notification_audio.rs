#![allow(dead_code)]
#![cfg(target_os = "windows")]

#[path = "../src/notification_audio.rs"]
mod notification_audio;

use notification_audio::{
    Admission, AudioIngress, Backend, Control, NotificationAudioLane, SoundEvent, SoundKind,
};
use std::{
    path::Path,
    sync::{Arc, mpsc},
    time::Duration,
};

struct ControlledBackend {
    started: mpsc::Sender<(String, Arc<Control>)>,
    release: mpsc::Receiver<()>,
}

impl Backend for ControlledBackend {
    fn attempt(&mut self, event: SoundEvent, control: &Arc<Control>) -> Result<(), String> {
        self.started
            .send((
                event
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                control.clone(),
            ))
            .unwrap();
        loop {
            control.check()?;
            match self.release.recv_timeout(Duration::from_millis(10)) {
                Ok(()) => return Ok(()),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err("controlled output unavailable".into()),
            }
        }
    }
}

fn controlled() -> (
    NotificationAudioLane,
    mpsc::Receiver<(String, Arc<Control>)>,
    mpsc::Sender<()>,
) {
    let (started, receiver) = mpsc::channel();
    let (release, sender) = mpsc::channel();
    (
        NotificationAudioLane::with_backend(ControlledBackend {
            started,
            release: sender,
        }),
        receiver,
        release,
    )
}

fn offer(ingress: &AudioIngress, file: &str) -> Admission {
    let path = Path::new("C:\\notification-sounds").join(file);
    for _ in 0..100 {
        let admission = ingress.offer(SoundKind::EndTurn, &path);
        if admission != Admission::Busy {
            return admission;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("audio ingress remained busy");
}

#[test]
fn active_attempt_survives_latest_waiting_replacement_and_releases_reservations() {
    let (mut lane, started, release) = controlled();
    let ingress = lane.ingress();
    assert_eq!(offer(&ingress, "active.wav"), Admission::Accepted);
    let (name, control) = started.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(name, "active.wav");
    assert_eq!(
        lane.charges(),
        (
            notification_audio::MAX_ENCODED_BYTES,
            notification_audio::MAX_DECODED_BYTES
        )
    );
    assert_eq!(offer(&ingress, "replaced.wav"), Admission::Accepted);
    assert_eq!(offer(&ingress, "latest.wav"), Admission::ReplacedWaiting);
    assert!(!control.cancelled());
    assert!(started.try_recv().is_err());
    release.send(()).unwrap();
    assert_eq!(
        started.recv_timeout(Duration::from_secs(2)).unwrap().0,
        "latest.wav"
    );
    lane.finish();
    assert_eq!(lane.charges(), (0, 0));
    assert!(lane.is_finished());
    assert_eq!(offer(&ingress, "late.wav"), Admission::Closed);
    assert!(started.try_recv().is_err());
}

#[test]
fn active_and_waiting_shutdown_cancels_attempt_and_never_restarts() {
    let (mut lane, started, _release) = controlled();
    let ingress = lane.ingress();
    offer(&ingress, "active.wav");
    let (_, control) = started.recv_timeout(Duration::from_secs(2)).unwrap();
    offer(&ingress, "waiting.wav");
    lane.close();
    assert!(control.cancelled());
    let mut drain = lane.take_worker();
    assert!(lane.ingress().same_lane(&ingress));
    drain.finish();
    assert_eq!(lane.charges(), (0, 0));
    assert_eq!(offer(&ingress, "late.wav"), Admission::Closed);
    assert!(started.try_recv().is_err());
}

#[test]
fn idle_shutdown_and_owner_drop_drain_every_worker() {
    let (mut lane, started, _release) = controlled();
    let ingress = lane.ingress();
    lane.finish();
    assert!(lane.is_finished());
    assert_eq!(offer(&ingress, "late.wav"), Admission::Closed);
    assert!(started.try_recv().is_err());
    let (lane, started, _release) = controlled();
    let ingress = lane.ingress();
    offer(&ingress, "active.wav");
    let (_, control) = started.recv_timeout(Duration::from_secs(2)).unwrap();
    drop(lane);
    assert!(control.cancelled());
    assert_eq!(offer(&ingress, "late.wav"), Admission::Closed);
}

#[test]
fn path_metadata_rejects_relative_and_oversized_input() {
    let (mut lane, _, _release) = controlled();
    assert_eq!(
        lane.ingress()
            .offer(SoundKind::EndTurn, Path::new("relative.wav")),
        Admission::InvalidPath
    );
    let oversized = format!("C:\\{}", "x".repeat(notification_audio::MAX_PATH_BYTES));
    assert_eq!(
        lane.ingress()
            .offer(SoundKind::EndTurn, Path::new(&oversized)),
        Admission::InvalidPath
    );
    lane.finish();
}

#[test]
fn device_names_and_namespaces_are_rejected_before_file_acquisition() {
    let (mut lane, _, _release) = controlled();
    for path in [
        "C:\\sounds\\CON.wav",
        "C:\\sounds\\nul",
        "C:\\sounds\\COM1.wav",
        "C:\\sounds\\COM¹.wav",
        "\\\\.\\pipe\\sound",
        "\\\\?\\GLOBALROOT\\Device\\sound",
    ] {
        assert_eq!(
            lane.ingress().offer(SoundKind::EndTurn, Path::new(path)),
            Admission::InvalidPath
        );
    }
    lane.finish();
}

fn wave(tag: u16, bits: u16, channels: u16, rate: u32, data: &[u8]) -> Vec<u8> {
    let align = channels * (bits / 8);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&tag.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * u32::from(align)).to_le_bytes());
    bytes.extend_from_slice(&align.to_le_bytes());
    bytes.extend_from_slice(&bits.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(data);
    if data.len() % 2 != 0 {
        bytes.push(0);
        bytes[4..8].copy_from_slice(&(36 + data.len() as u32 + 1).to_le_bytes());
    }
    bytes
}

#[test]
fn real_decoder_handles_supported_samples_and_rejects_invalid_headers_and_limits() {
    let (mut lane, started, _release) = controlled();
    offer(&lane.ingress(), "control.wav");
    let (_, control) = started.recv_timeout(Duration::from_secs(2)).unwrap();
    for bits in [8, 16, 24, 32] {
        let samples = notification_audio::wav::decode(
            wave(1, bits, 1, 8000, &vec![0; usize::from(bits / 8) * 2]),
            &control,
        )
        .unwrap();
        assert_eq!(samples.data.len(), 2);
    }
    assert_eq!(
        notification_audio::wav::decode(wave(3, 32, 1, 8000, &0.5f32.to_le_bytes()), &control)
            .unwrap()
            .data,
        [0.5]
    );
    let valid = wave(1, 16, 1, 8000, &[0, 0]);
    assert_eq!(
        notification_audio::wav::decode(wave(1, 8, 1, 8000, &[128]), &control)
            .unwrap()
            .data,
        [0.0]
    );
    let mut unpadded = wave(1, 8, 1, 8000, &[128]);
    unpadded.pop();
    unpadded[4..8].copy_from_slice(&37u32.to_le_bytes());
    assert!(notification_audio::wav::decode(unpadded, &control).is_err());
    for length in 0..valid.len() {
        assert!(notification_audio::wav::decode(valid[..length].to_vec(), &control).is_err());
    }
    assert!(
        notification_audio::wav::decode(wave(3, 32, 1, 8000, &f32::NAN.to_le_bytes()), &control)
            .is_err()
    );
    assert!(notification_audio::wav::decode(wave(1, 16, 3, 8000, &[0; 6]), &control).is_err());
    assert!(notification_audio::wav::decode(wave(1, 16, 1, 0, &[0; 2]), &control).is_err());
    assert!(
        notification_audio::wav::decode(wave(1, 8, 1, 8000, &vec![0; 8000 * 31]), &control)
            .is_err()
    );
    assert!(
        notification_audio::wav::decode(
            wave(
                1,
                8,
                2,
                192000,
                &vec![0; notification_audio::MAX_DECODED_BYTES / 4 + 2]
            ),
            &control
        )
        .is_err()
    );
    let mut overflow = valid.clone();
    overflow[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(notification_audio::wav::decode(overflow, &control).is_err());
    lane.close();
    assert!(notification_audio::wav::decode(valid, &control).is_err());
    lane.finish();
}

#[test]
fn production_file_acquisition_rejects_missing_invalid_and_oversized_regular_files() {
    let (mut lane, started, _release) = controlled();
    offer(&lane.ingress(), "control.wav");
    let (_, control) = started.recv_timeout(Duration::from_secs(2)).unwrap();
    let fixture = tempfile::NamedTempFile::new().unwrap();
    let path = fixture.path();
    assert!(
        notification_audio::wav::acquire(&path.with_extension("missing-sound"), &control).is_err()
    );
    std::fs::write(&path, wave(1, 16, 1, 8000, &[0, 0, 255, 127])).unwrap();
    assert_eq!(
        notification_audio::wav::acquire(&path, &control)
            .unwrap()
            .data
            .len(),
        2
    );
    std::fs::write(&path, b"invalid").unwrap();
    assert!(notification_audio::wav::acquire(&path, &control).is_err());
    std::fs::File::create(&path)
        .unwrap()
        .set_len(notification_audio::MAX_ENCODED_BYTES as u64 + 1)
        .unwrap();
    assert!(notification_audio::wav::acquire(&path, &control).is_err());
    lane.close();
    assert!(notification_audio::wav::acquire(&path, &control).is_err());
    lane.finish();
}

#[test]
fn acquisition_cancellation_releases_file_on_every_owned_cut() {
    let fixture = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(fixture.path(), wave(1, 16, 1, 8000, &[0; 4])).unwrap();
    for selected in [
        "before-open",
        "after-open",
        "after-metadata",
        "after-read",
        "before-decode",
    ] {
        let (mut lane, started, _release) = controlled();
        offer(&lane.ingress(), "control.wav");
        let (_, control) = started.recv_timeout(Duration::from_secs(2)).unwrap();
        let mut observed = false;
        let result = notification_audio::wav::acquire_with_cut(fixture.path(), &control, |cut| {
            if cut == selected {
                observed = true;
                lane.close();
            }
        });
        assert!(observed, "missing cancellation cut {selected}");
        assert!(result.is_err());
        lane.finish();
        assert_eq!(lane.charges(), (0, 0));
        std::fs::OpenOptions::new()
            .write(true)
            .open(fixture.path())
            .unwrap();
    }
}

#[test]
fn playback_polling_ends_on_cancellation_device_failure_and_deadline() {
    let (mut lane, started, _release) = controlled();
    offer(&lane.ingress(), "control.wav");
    let (_, control) = started.recv_timeout(Duration::from_secs(2)).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    assert!(notification_audio::playback::wait(&control, deadline, || false, || true).is_err());
    assert!(
        notification_audio::playback::wait(&control, std::time::Instant::now(), || false, || false)
            .is_err()
    );
    assert!(notification_audio::playback::wait(&control, deadline, || true, || false).is_ok());
    lane.close();
    assert!(
        control
            .admit_output(|| panic!("closed lane started output"))
            .is_err()
    );
    assert!(notification_audio::playback::wait(&control, deadline, || true, || false).is_err());
    lane.finish();
}

struct UnavailableDevice(mpsc::Sender<()>);
impl Backend for UnavailableDevice {
    fn attempt(&mut self, _: SoundEvent, _: &Arc<Control>) -> Result<(), String> {
        self.0.send(()).unwrap();
        Err("controlled device unavailable".into())
    }
}

#[test]
fn unavailable_device_is_nonfatal_and_later_attempt_may_retry() {
    let (sent, received) = mpsc::channel();
    let mut lane = NotificationAudioLane::with_backend(UnavailableDevice(sent));
    let ingress = lane.ingress();
    offer(&ingress, "first.wav");
    received.recv_timeout(Duration::from_secs(2)).unwrap();
    offer(&ingress, "second.wav");
    received.recv_timeout(Duration::from_secs(2)).unwrap();
    lane.finish();
    assert_eq!(lane.charges(), (0, 0));
}
