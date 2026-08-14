use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread,
};

use tracing::warn;

const NOTIFICATION_SOUND_QUEUE_CAPACITY: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TurnCompletionSoundCandidate {
    pub(super) thread_id: Option<String>,
    pub(super) turn_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LifecycleNotificationKind {
    OperatorAttention,
    PlanComplete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LifecycleNotificationCandidate {
    pub(super) thread_id: Option<String>,
    pub(super) turn_id: Option<String>,
    pub(super) kind: LifecycleNotificationKind,
}

impl TurnCompletionSoundCandidate {
    pub(super) fn new(thread_id: Option<String>, turn_id: Option<String>) -> Self {
        Self { thread_id, turn_id }
    }
}

impl LifecycleNotificationCandidate {
    pub(super) fn new(
        thread_id: Option<String>,
        turn_id: Option<String>,
        kind: LifecycleNotificationKind,
    ) -> Self {
        Self {
            thread_id,
            turn_id,
            kind,
        }
    }
}

#[derive(Clone)]
pub(super) struct NotificationSoundPlayer {
    sender: Option<SyncSender<PathBuf>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NotificationSoundEnqueueResult {
    Enqueued,
    QueueFull,
    WorkerStopped,
    UnsupportedPlatform,
}

impl NotificationSoundPlayer {
    pub(super) fn spawn() -> Self {
        spawn_notification_sound_player()
    }

    pub(super) fn enqueue_end_turn_sound(&self, path: PathBuf) {
        let result = match self.sender.as_ref() {
            Some(sender) => try_enqueue_sound_path(sender, path),
            None => NotificationSoundEnqueueResult::UnsupportedPlatform,
        };

        match result {
            NotificationSoundEnqueueResult::Enqueued => {}
            NotificationSoundEnqueueResult::QueueFull => {
                warn!("notification sound queue is full; dropping end-turn sound request");
            }
            NotificationSoundEnqueueResult::WorkerStopped => {
                warn!("notification sound worker has stopped; dropping end-turn sound request");
            }
            NotificationSoundEnqueueResult::UnsupportedPlatform => {}
        }
    }

    pub(super) fn enqueue_lifecycle_notification_sound(
        &self,
        kind: LifecycleNotificationKind,
        path: PathBuf,
    ) {
        let result = match self.sender.as_ref() {
            Some(sender) => try_enqueue_sound_path(sender, path),
            None => NotificationSoundEnqueueResult::UnsupportedPlatform,
        };

        match result {
            NotificationSoundEnqueueResult::Enqueued => {}
            NotificationSoundEnqueueResult::QueueFull => {
                warn!(
                    ?kind,
                    "notification sound queue is full; dropping lifecycle notification sound request"
                );
            }
            NotificationSoundEnqueueResult::WorkerStopped => {
                warn!(
                    ?kind,
                    "notification sound worker has stopped; dropping lifecycle notification sound request"
                );
            }
            NotificationSoundEnqueueResult::UnsupportedPlatform => {}
        }
    }
}

pub(super) fn try_enqueue_sound_path(
    sender: &SyncSender<PathBuf>,
    path: PathBuf,
) -> NotificationSoundEnqueueResult {
    match sender.try_send(path) {
        Ok(()) => NotificationSoundEnqueueResult::Enqueued,
        Err(TrySendError::Full(_)) => NotificationSoundEnqueueResult::QueueFull,
        Err(TrySendError::Disconnected(_)) => NotificationSoundEnqueueResult::WorkerStopped,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DefaultOutputDeviceIdentity(String);

impl DefaultOutputDeviceIdentity {
    pub(super) fn new(stable_id: impl Into<String>) -> Self {
        Self(stable_id.into())
    }
}

pub(super) fn should_reopen_for_default_output_device(
    cached: Option<&DefaultOutputDeviceIdentity>,
    current: Option<&DefaultOutputDeviceIdentity>,
) -> bool {
    current.is_some_and(|current| cached != Some(current))
}

#[cfg(target_os = "windows")]
fn spawn_notification_sound_player() -> NotificationSoundPlayer {
    let (sender, receiver) = mpsc::sync_channel::<PathBuf>(NOTIFICATION_SOUND_QUEUE_CAPACITY);
    thread::spawn(move || {
        let mut playback = WindowsNotificationPlayback::default();
        while let Ok(path) = receiver.recv() {
            playback.play_wav(&path);
        }
    });
    NotificationSoundPlayer {
        sender: Some(sender),
    }
}

#[cfg(not(target_os = "windows"))]
fn spawn_notification_sound_player() -> NotificationSoundPlayer {
    NotificationSoundPlayer { sender: None }
}

#[cfg(target_os = "windows")]
#[derive(Default)]
struct WindowsNotificationPlayback {
    sink: Option<WindowsNotificationSink>,
}

#[cfg(target_os = "windows")]
struct WindowsNotificationSink {
    _sink: rodio::MixerDeviceSink,
    player: rodio::Player,
    stream_failed: Arc<AtomicBool>,
    default_device_identity: Option<DefaultOutputDeviceIdentity>,
}

#[cfg(target_os = "windows")]
type WindowsNotificationSinkResult =
    Result<WindowsNotificationSink, rodio::stream::DeviceSinkError>;

#[cfg(target_os = "windows")]
struct DefaultOutputDevice {
    device: rodio::cpal::Device,
    identity: Option<DefaultOutputDeviceIdentity>,
}

#[cfg(target_os = "windows")]
impl WindowsNotificationPlayback {
    fn play_wav(&mut self, path: &Path) {
        if !path.exists() {
            warn!(
                path = %path.display(),
                "notification sound file is missing"
            );
            return;
        }
        if !path.is_file() {
            warn!(
                path = %path.display(),
                "notification sound path is not a file"
            );
            return;
        }

        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) => {
                warn!(
                    path = %path.display(),
                    error = %error,
                    "notification sound file could not be opened"
                );
                return;
            }
        };
        let decoder = match catch_unwind(AssertUnwindSafe(|| rodio::Decoder::new_wav(file))) {
            Ok(Ok(decoder)) => decoder,
            Ok(Err(error)) => {
                warn!(
                    path = %path.display(),
                    error = %error,
                    "notification sound WAV could not be decoded"
                );
                return;
            }
            Err(_) => {
                warn!(
                    path = %path.display(),
                    "notification sound WAV decoder panicked"
                );
                return;
            }
        };
        let Some(sink) = self.default_sink(path) else {
            return;
        };
        sink.player.append(decoder);
        sink.player.sleep_until_end();
    }

    fn default_sink(&mut self, path: &Path) -> Option<&mut WindowsNotificationSink> {
        if self
            .sink
            .as_ref()
            .is_some_and(|sink| sink.stream_failed.load(Ordering::SeqCst))
        {
            self.sink = None;
        }

        if let Some(sink) = self.sink.as_ref() {
            let current_identity = current_default_output_device_identity();
            if should_reopen_for_default_output_device(
                sink.default_device_identity.as_ref(),
                current_identity.as_ref(),
            ) {
                self.sink = None;
            }
        }

        if self.sink.is_none() {
            match open_default_notification_sink() {
                Ok(sink) => self.sink = Some(sink),
                Err(error) => {
                    warn!(
                        path = %path.display(),
                        error = %error,
                        "notification sound audio device could not be opened"
                    );
                    return None;
                }
            }
        }

        self.sink.as_mut()
    }
}

#[cfg(target_os = "windows")]
fn open_default_notification_sink() -> WindowsNotificationSinkResult {
    let Some(default_device) = current_default_output_device() else {
        return Err(rodio::stream::DeviceSinkError::NoDevice);
    };
    let stream_failed = Arc::new(AtomicBool::new(false));
    let error_flag = Arc::clone(&stream_failed);
    let mut sink = rodio::DeviceSinkBuilder::from_device(default_device.device)
        .map(|builder| {
            builder.with_error_callback(move |error| {
                error_flag.store(true, Ordering::SeqCst);
                notification_audio_stream_error(error);
            })
        })
        .and_then(|builder| builder.open_sink_or_fallback())?;
    sink.log_on_drop(false);
    let player = rodio::Player::connect_new(sink.mixer());
    Ok(WindowsNotificationSink {
        _sink: sink,
        player,
        stream_failed,
        default_device_identity: default_device.identity,
    })
}

#[cfg(target_os = "windows")]
fn notification_audio_stream_error(error: rodio::cpal::StreamError) {
    warn!(
        error = %error,
        "notification sound audio stream reported an error"
    );
}

#[cfg(target_os = "windows")]
fn current_default_output_device() -> Option<DefaultOutputDevice> {
    use rodio::cpal::traits::{DeviceTrait, HostTrait};

    let device = rodio::cpal::default_host().default_output_device()?;
    let identity = match device.id() {
        Ok(id) => Some(DefaultOutputDeviceIdentity::new(id.to_string())),
        Err(error) => {
            warn!(
                error = %error,
                "notification sound default audio device identity could not be read"
            );
            None
        }
    };
    Some(DefaultOutputDevice { device, identity })
}

#[cfg(target_os = "windows")]
fn current_default_output_device_identity() -> Option<DefaultOutputDeviceIdentity> {
    current_default_output_device().and_then(|default_device| default_device.identity)
}
