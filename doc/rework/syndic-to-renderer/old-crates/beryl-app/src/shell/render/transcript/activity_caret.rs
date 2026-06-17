use std::time::Duration;

#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::GetCaretBlinkTime;

#[cfg(not(target_os = "windows"))]
const DEFAULT_CARET_BLINK_INTERVAL: Duration = Duration::from_millis(530);

pub(crate) fn platform_caret_blink_interval() -> Option<Duration> {
    #[cfg(target_os = "windows")]
    {
        return windows_caret_blink_interval();
    }

    #[cfg(not(target_os = "windows"))]
    {
        Some(DEFAULT_CARET_BLINK_INTERVAL)
    }
}

#[cfg(target_os = "windows")]
fn windows_caret_blink_interval() -> Option<Duration> {
    windows_caret_blink_interval_from_millis(unsafe { GetCaretBlinkTime() })
}

pub(crate) fn windows_caret_blink_interval_from_millis(millis: u32) -> Option<Duration> {
    if millis == 0 || millis == u32::MAX {
        None
    } else {
        Some(Duration::from_millis(u64::from(millis)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActivityCaretMotion {
    Blink { interval: Duration },
    Steady,
}

impl ActivityCaretMotion {
    pub(crate) fn for_blink_interval(interval: Option<Duration>) -> Self {
        match interval.filter(|interval| *interval > Duration::ZERO) {
            Some(interval) => Self::Blink { interval },
            None => Self::Steady,
        }
    }

    fn interval(self) -> Option<Duration> {
        match self {
            Self::Blink { interval } => Some(interval),
            Self::Steady => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ActivityCaretBlinkSchedule {
    pub(crate) generation: u64,
    pub(crate) interval: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ActivityCaretBlinkState {
    generation: u64,
    active: bool,
    visible: bool,
    motion: ActivityCaretMotion,
}

impl Default for ActivityCaretBlinkState {
    fn default() -> Self {
        Self {
            generation: 0,
            active: false,
            visible: true,
            motion: ActivityCaretMotion::Steady,
        }
    }
}

impl ActivityCaretBlinkState {
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn sync(&mut self, active: bool, motion: ActivityCaretMotion) -> bool {
        if !active {
            let changed = self.active || !self.visible;
            if changed {
                self.generation = self.generation.saturating_add(1);
            }
            self.active = false;
            self.visible = true;
            self.motion = motion;
            return changed;
        }

        let changed = !self.active || self.motion != motion;
        if changed {
            self.generation = self.generation.saturating_add(1);
            self.visible = true;
        }
        self.active = true;
        self.motion = motion;
        changed
    }

    pub(crate) fn advance(&mut self, generation: u64) -> bool {
        if self.generation != generation || !self.active || self.motion.interval().is_none() {
            return false;
        }
        self.visible = !self.visible;
        true
    }

    pub(crate) fn blink_schedule(&self) -> Option<ActivityCaretBlinkSchedule> {
        Some(ActivityCaretBlinkSchedule {
            generation: self.generation,
            interval: self.active.then(|| self.motion.interval()).flatten()?,
        })
    }

    pub(crate) fn opacity(&self) -> f32 {
        if !self.active || self.motion.interval().is_none() || self.visible {
            1.0
        } else {
            0.0
        }
    }
}
