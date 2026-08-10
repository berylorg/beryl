use std::time::Duration;

use gpui::{AsyncApp, Context, WeakEntity};
use tracing::warn;

use super::ShellView;
use crate::activity_diagnostic_file_capture::{
    ActivityDiagnosticCaptureErrorCategory, ActivityDiagnosticCaptureRuntimeState,
    ActivityDiagnosticCaptureStatus,
};

const ACTIVITY_DIAGNOSTIC_CAPTURE_TRANSITION_STATUS_POLL_INTERVAL: Duration =
    Duration::from_millis(16);
const ACTIVITY_DIAGNOSTIC_CAPTURE_ACTIVE_STATUS_POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActivityDiagnosticCaptureStatusPollMode {
    Transition,
    Active,
}

impl ActivityDiagnosticCaptureStatusPollMode {
    fn interval(self) -> Duration {
        match self {
            Self::Transition => ACTIVITY_DIAGNOSTIC_CAPTURE_TRANSITION_STATUS_POLL_INTERVAL,
            Self::Active => ACTIVITY_DIAGNOSTIC_CAPTURE_ACTIVE_STATUS_POLL_INTERVAL,
        }
    }

    fn is_slow(self) -> bool {
        matches!(self, Self::Active)
    }
}

impl ShellView {
    pub(super) fn activity_diagnostic_capture_status(&self) -> ActivityDiagnosticCaptureStatus {
        let configured = self
            .settings_state
            .active_preferences_snapshot()
            .diagnostics
            .activity_diagnostic_capture_enabled;
        let Some(controller) = self.activity_diagnostic_file_capture.as_ref() else {
            let mut status = ActivityDiagnosticCaptureStatus::default();
            status.configured = configured;
            if configured {
                status.runtime_state = ActivityDiagnosticCaptureRuntimeState::Failed;
                status.error_category =
                    Some(ActivityDiagnosticCaptureErrorCategory::WriterDisconnected);
            }
            return status;
        };

        let mut status = controller.status();
        status.configured = configured;
        status
    }

    pub(super) fn reconcile_activity_diagnostic_capture(&mut self, cx: &mut Context<Self>) {
        let configured = self
            .settings_state
            .active_preferences_snapshot()
            .diagnostics
            .activity_diagnostic_capture_enabled;
        if let Some(controller) = self.activity_diagnostic_file_capture.as_ref() {
            let runtime_configured = controller.status().configured;
            let transition = match (runtime_configured, configured) {
                (false, true) => controller.enable(Some(crate::build_identity::build_identity())),
                (true, false) => controller.disable().map(|()| 0),
                _ => Ok(0),
            };
            if let Err(error) = transition {
                warn!(category = %error, "Activity diagnostic capture transition failed");
            }
        }

        self.refresh_activity_diagnostic_capture_status();
        self.schedule_activity_diagnostic_capture_status_refresh(cx);
    }

    pub(super) fn refresh_activity_diagnostic_capture_status(&mut self) {
        let status = self.activity_diagnostic_capture_status();
        self.settings_state
            .set_activity_diagnostic_capture_status(status);
    }

    pub(super) fn schedule_activity_diagnostic_capture_status_refresh(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(poll_mode) = self.activity_diagnostic_capture_status_poll_mode(cx) else {
            self.cancel_activity_diagnostic_capture_status_refresh();
            return;
        };

        let slow = poll_mode.is_slow();
        if self.activity_diagnostic_capture_status_poll_scheduled
            && self.activity_diagnostic_capture_status_poll_slow == slow
        {
            return;
        }

        self.activity_diagnostic_capture_status_poll_generation = self
            .activity_diagnostic_capture_status_poll_generation
            .wrapping_add(1);
        let generation = self.activity_diagnostic_capture_status_poll_generation;
        self.activity_diagnostic_capture_status_poll_slow = slow;
        self.activity_diagnostic_capture_status_poll_scheduled = true;
        cx.spawn(move |view: WeakEntity<Self>, cx: &mut AsyncApp| {
            let mut cx = cx.clone();
            async move {
                cx.background_executor().timer(poll_mode.interval()).await;
                let _ = view.update(&mut cx, |view, cx| {
                    if !view.activity_diagnostic_capture_status_poll_scheduled
                        || view.activity_diagnostic_capture_status_poll_generation != generation
                        || view.activity_diagnostic_capture_status_poll_slow != slow
                    {
                        return;
                    }
                    view.activity_diagnostic_capture_status_poll_scheduled = false;
                    view.sync_settings_window_model(cx);
                });
            }
        })
        .detach();
    }

    fn activity_diagnostic_capture_status_poll_mode(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<ActivityDiagnosticCaptureStatusPollMode> {
        match self.activity_diagnostic_capture_status().runtime_state {
            ActivityDiagnosticCaptureRuntimeState::Starting
            | ActivityDiagnosticCaptureRuntimeState::Stopping => {
                Some(ActivityDiagnosticCaptureStatusPollMode::Transition)
            }
            ActivityDiagnosticCaptureRuntimeState::Active
                if self.settings_state.diagnostics_page_selected()
                    && self.settings_window.is_visible(cx).unwrap_or(false) =>
            {
                Some(ActivityDiagnosticCaptureStatusPollMode::Active)
            }
            ActivityDiagnosticCaptureRuntimeState::Disabled
            | ActivityDiagnosticCaptureRuntimeState::Active
            | ActivityDiagnosticCaptureRuntimeState::Unavailable
            | ActivityDiagnosticCaptureRuntimeState::Failed => None,
        }
    }

    fn cancel_activity_diagnostic_capture_status_refresh(&mut self) {
        if !self.activity_diagnostic_capture_status_poll_scheduled {
            return;
        }
        self.activity_diagnostic_capture_status_poll_generation = self
            .activity_diagnostic_capture_status_poll_generation
            .wrapping_add(1);
        self.activity_diagnostic_capture_status_poll_scheduled = false;
        self.activity_diagnostic_capture_status_poll_slow = false;
    }
}
