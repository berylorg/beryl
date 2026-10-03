use super::{Control, wav::Samples};
use rodio::{ChannelCount, SampleRate, Source};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

struct OwnedSamples {
    samples: std::vec::IntoIter<f32>,
    channels: ChannelCount,
    rate: SampleRate,
    duration: Duration,
    silence: usize,
}

impl Iterator for OwnedSamples {
    type Item = f32;
    fn next(&mut self) -> Option<Self::Item> {
        self.samples.next().or_else(|| {
            if self.silence == 0 {
                None
            } else {
                self.silence -= 1;
                Some(0.0)
            }
        })
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.samples.len() + self.silence;
        (len, Some(len))
    }
}

impl Source for OwnedSamples {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.samples.len() + self.silence)
    }
    fn channels(&self) -> ChannelCount {
        self.channels
    }
    fn sample_rate(&self) -> SampleRate {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        Some(self.duration)
    }
}

pub(crate) fn play(samples: Samples, control: &Arc<Control>) -> Result<(), String> {
    control.check()?;
    let failed = Arc::new(AtomicBool::new(false));
    let error_flag = failed.clone();
    let mut device = rodio::DeviceSinkBuilder::from_default_device()
        .map_err(|e| format!("sound output device unavailable: {e}"))?
        .with_error_callback(move |error| {
            error_flag.store(true, Ordering::Release);
            tracing::warn!(%error, "notification audio output stream failed");
        })
        .open_sink_or_fallback()
        .map_err(|e| format!("sound output stream unavailable: {e}"))?;
    device.log_on_drop(false);
    control.check()?;
    let player = rodio::Player::connect_new(device.mixer());
    let silence = usize::from(samples.channels.get()) * samples.rate.get() as usize / 4;
    let duration = Duration::from_secs_f64(
        (samples.data.len() + silence) as f64
            / f64::from(samples.channels.get())
            / f64::from(samples.rate.get()),
    );
    let deadline = Instant::now() + duration + Duration::from_secs(2);
    control.admit_output(|| {
        player.append(OwnedSamples {
            samples: samples.data.into_iter(),
            channels: samples.channels,
            rate: samples.rate,
            duration,
            silence,
        })
    })?;
    let result = wait(
        control,
        deadline,
        || player.empty(),
        || failed.load(Ordering::Acquire),
    );
    player.stop();
    drop(player);
    drop(device);
    result
}

pub(crate) fn wait(
    control: &Arc<Control>,
    deadline: Instant,
    empty: impl Fn() -> bool,
    failed: impl Fn() -> bool,
) -> Result<(), String> {
    loop {
        if control.cancelled() {
            break Err("notification audio cancelled".into());
        }
        if failed() {
            break Err("sound output stream failed".into());
        }
        if empty() {
            break Ok(());
        }
        if Instant::now() >= deadline {
            break Err("sound playback exceeded its finite deadline".into());
        }
        control.wait_tick();
    }
}
