use super::*;
use crate::cas_projection::ExactStopFeedbackIdentity;

pub(super) struct AcknowledgedStopFeedback {
    pub(super) identity: ExactStopFeedbackIdentity,
    origin: ExactOperationOrigin,
    volatile: bool,
}

impl ExactStatusControls {
    pub(super) fn feedback_budget(&self) -> usize {
        self.feedback.len() + self.acknowledged.len()
    }

    pub(super) fn volatile_refused(&self) -> bool {
        self.snapshot.origin.as_ref().is_some_and(|origin| {
            self.acknowledged
                .iter()
                .any(|entry| entry.volatile && &entry.origin == origin)
        })
    }

    pub(super) fn prune_acknowledgments(&mut self) {
        let worker = self.worker.as_ref();
        let snapshot = &self.snapshot;
        self.acknowledged.retain_mut(|entry| {
            if entry.volatile
                && (worker.is_some_and(|worker| {
                    !entry.origin.belongs_to_service(worker.worker_identity())
                }) || snapshot.origin.as_ref().is_some_and(|origin| {
                    entry.origin.same_selected_thread(origin) && origin != &entry.origin
                }))
            {
                entry.volatile = false;
            }
            entry.volatile || entry.identity.is_live()
        });
    }
}

impl MainWindowShellRoot {
    pub(in crate::main_window::shell) fn stop_feedback_popup_safe(
        &self,
        feedback: &ExactStopFeedback,
    ) -> bool {
        self.status_controls
            .worker
            .as_ref()
            .is_some_and(|worker| worker.publication_current())
            && self.status_controls.snapshot.operation_active
            && self.status_controls.menu_available()
            && self.status_controls.feedback() == Some(feedback)
    }

    pub(in crate::main_window::shell) fn stop_feedback_fresh_eligibility(
        &self,
        feedback: &ExactStopFeedback,
    ) -> bool {
        self.status_controls.snapshot.origin.as_ref() == feedback.operation_origin().as_ref()
            && self.status_controls.command_enabled()
            && self
                .status_controls
                .worker
                .as_ref()
                .is_some_and(|worker| worker.publication_current())
    }

    pub(in crate::main_window::shell) fn acknowledge_stop_feedback(
        &mut self,
        feedback: &ExactStopFeedback,
    ) {
        if feedback.snapshot().state == ExactStopFeedbackState::Waiting {
            return;
        }
        let Some(index) = self
            .status_controls
            .feedback
            .iter()
            .position(|entry| &entry.feedback == feedback)
        else {
            return;
        };
        let entry = self.status_controls.feedback.remove(index);
        if self.status_controls.snapshot.origin.as_ref() == Some(&entry.origin) {
            self.status_controls
                .generation
                .fetch_add(1, Ordering::AcqRel);
            self.status_controls.snapshot.stop =
                ExactSoftStopAvailability::Unavailable(ExactSoftStopUnavailable::RequestInProgress);
        }
        self.status_controls
            .acknowledged
            .push(AcknowledgedStopFeedback {
                identity: feedback.weak_identity(),
                origin: entry.origin,
                volatile: feedback.snapshot().state == ExactStopFeedbackState::VolatileNondispatch,
            });
    }
}
