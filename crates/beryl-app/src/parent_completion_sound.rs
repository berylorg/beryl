use crate::notification_audio::{AudioIngress, SoundKind};
use beryl_home_store::{HomeGeneration, HomeHealthState, HomeServiceReference};
use beryl_state::{SettingKey, SettingsState};
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

mod platform_attention;
use platform_attention::{PlatformAttentionMonitor, PlatformAttentionReader};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AttentionTriggerState {
    Active,
    Inactive,
    Unknown,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlatformAttentionState {
    pub(crate) local_input_idle: AttentionTriggerState,
    pub(crate) session_locked: AttentionTriggerState,
    pub(crate) lid_closed: AttentionTriggerState,
    pub(crate) display_inactive: AttentionTriggerState,
}

impl PlatformAttentionState {
    fn has_active_trigger(self) -> bool {
        [
            self.local_input_idle,
            self.session_locked,
            self.lid_closed,
            self.display_inactive,
        ]
        .contains(&AttentionTriggerState::Active)
    }
}

struct DesktopFacts {
    closed: AtomicBool,
    focus: AtomicU8,
    platform: PlatformAttentionReader,
    #[cfg(test)]
    attention_override: std::sync::Mutex<Option<PlatformAttentionState>>,
    #[cfg(test)]
    attempts: std::sync::atomic::AtomicUsize,
}

pub(crate) struct ParentCompletionSoundOwner {
    facts: Arc<DesktopFacts>,
    monitor: PlatformAttentionMonitor,
}

#[derive(Clone)]
pub(crate) struct ParentSoundSink {
    facts: Weak<DesktopFacts>,
    audio: AudioIngress,
    settings: SettingsState,
    service_generation: crate::cas_projection::ProjectionServiceGeneration,
}

impl std::fmt::Debug for ParentSoundSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParentSoundSink").finish_non_exhaustive()
    }
}

struct DesktopFocus(Weak<DesktopFacts>);
impl gpui::Global for DesktopFocus {}

impl ParentCompletionSoundOwner {
    pub(crate) fn new(app: &mut gpui::App) -> Self {
        let monitor = PlatformAttentionMonitor::spawn();
        let facts = Arc::new(DesktopFacts {
            closed: AtomicBool::new(false),
            focus: AtomicU8::new(0),
            platform: monitor.reader(),
            #[cfg(test)]
            attention_override: std::sync::Mutex::new(None),
            #[cfg(test)]
            attempts: std::sync::atomic::AtomicUsize::new(0),
        });
        app.set_global(DesktopFocus(Arc::downgrade(&facts)));
        app.on_window_closed(refresh_focus).detach();
        app.observe_new::<gpui_settings_window::SettingsWindowView>(|_, window, cx| {
            if let Some(window) = window {
                cx.observe_window_activation(window, |_, _, cx| refresh_focus(cx))
                    .detach();
            }
            refresh_focus(cx);
        })
        .detach();
        for settings in app
            .windows()
            .into_iter()
            .filter_map(|window| window.downcast::<gpui_settings_window::SettingsWindowView>())
        {
            let _ = settings.update(app, |_, window, cx| {
                cx.observe_window_activation(window, |_, _, cx| refresh_focus(cx))
                    .detach();
            });
        }
        refresh_focus(app);
        Self { facts, monitor }
    }

    pub(crate) fn sink(
        &self,
        audio: AudioIngress,
        settings: SettingsState,
        service_generation: crate::cas_projection::ProjectionServiceGeneration,
    ) -> ParentSoundSink {
        ParentSoundSink {
            facts: Arc::downgrade(&self.facts),
            audio,
            settings,
            service_generation,
        }
    }

    pub(crate) fn close(&self) {
        self.facts.closed.store(true, Ordering::Release);
        self.monitor.close();
    }

    pub(crate) fn take_worker(&mut self) -> Option<std::thread::JoinHandle<()>> {
        self.close();
        self.monitor.take_worker()
    }

    #[cfg(test)]
    pub(crate) fn is_finished(&self) -> bool {
        self.monitor.is_finished()
    }

    #[cfg(test)]
    pub(crate) fn test_focus(&self) -> u8 {
        self.facts.focus.load(Ordering::Acquire)
    }
}

pub(crate) fn refresh_focus(app: &mut gpui::App) {
    let Some(facts) = app
        .try_global::<DesktopFocus>()
        .and_then(|focus| focus.0.upgrade())
    else {
        return;
    };
    let focus = match app.active_window() {
        Some(active)
            if active
                .downcast::<crate::main_window::MainWindowShellRoot>()
                .is_some()
                || active
                    .downcast::<gpui_settings_window::SettingsWindowView>()
                    .is_some() =>
        {
            1
        }
        Some(_) | None => 2,
    };
    facts.focus.store(focus, Ordering::Release);
}

impl ParentSoundSink {
    #[cfg(test)]
    pub(crate) fn test_bound_to(
        &self,
        owner: &ParentCompletionSoundOwner,
        audio: &AudioIngress,
    ) -> bool {
        self.facts.ptr_eq(&Arc::downgrade(&owner.facts)) && self.audio.same_lane(audio)
    }

    pub(crate) fn notify(
        &self,
        home: &HomeServiceReference,
        expected: HomeGeneration,
        authorizer: &crate::cas_projection::LiveCommandAuthorizer,
        kind: syndic_storage::TurnKind,
        source_current: impl Fn() -> bool,
    ) {
        use syndic_storage::TurnKind;
        if !matches!(
            kind,
            TurnKind::OrdinaryUser | TurnKind::BerylDiscussionHandoff
        ) {
            return;
        }
        let Some(facts) = self.facts.upgrade() else {
            return;
        };
        #[cfg(test)]
        facts.attempts.fetch_add(1, Ordering::AcqRel);
        let current = || {
            source_current()
                && !facts.closed.load(Ordering::Acquire)
                && authorizer.service_generation() == self.service_generation
                && authorizer.is_open()
                && home.health().state() == HomeHealthState::Healthy
                && home.health().generation() == Some(expected)
        };
        if !current() {
            return;
        }
        let record = match self.settings.setting(home, SettingKey::EndTurnSound) {
            Ok(Some(record)) => record,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(%error, "parent completion sound settings read failed");
                return;
            }
        };
        let Some(Some(path)) = record.value().as_end_turn_sound() else {
            return;
        };
        let attention = facts.platform.snapshot();
        #[cfg(test)]
        let attention = facts
            .attention_override
            .lock()
            .unwrap()
            .unwrap_or(attention);
        if facts.focus.load(Ordering::Acquire) != 2 && !attention.has_active_trigger() {
            return;
        }
        let Ok(_service) = authorizer.try_hold_work_open() else {
            return;
        };
        if !source_current()
            || facts.closed.load(Ordering::Acquire)
            || home.health().state() != HomeHealthState::Healthy
            || home.health().generation() != Some(expected)
        {
            return;
        }
        let _ = self
            .audio
            .offer(SoundKind::EndTurn, std::path::Path::new(path.as_str()));
    }
}

#[cfg(all(test, feature = "test-faults"))]
#[path = "../tests/unit/parent_completion_sound.rs"]
mod tests;
