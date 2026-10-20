use std::time::Duration;

use beryl_home_store::CommandCancellation;

use super::*;
use crate::settings_service::{
    SettingsApplyAction, SettingsApplyObservation, SettingsDraftBinding, SettingsServiceError,
};

pub(super) struct PendingApply {
    pub(super) binding: SettingsDraftBinding,
    pub(super) operation: u64,
}

impl SettingsWindowController {
    pub(super) fn apply(&mut self, hide_after_commit: bool, cx: &mut Context<Self>) {
        if self.retired || self.exit_is_gated() {
            return;
        }
        if self
            .draft
            .as_ref()
            .is_none_or(|draft| draft.modified().is_empty())
        {
            if hide_after_commit {
                self.cancel(cx);
            }
            return;
        }
        if !self.source_current || !self.home_available {
            return;
        }
        let Some(draft) = &self.draft else {
            return;
        };
        if !draft.can_apply() || !self.theme_is_qualified() {
            self.publish(cx);
            return;
        }
        let Ok(operation) = self.service.next_operation_id() else {
            self.source_current = false;
            self.publish(cx);
            return;
        };
        let action = if hide_after_commit {
            SettingsApplyAction::Ok
        } else {
            SettingsApplyAction::Apply
        };
        let binding = draft.binding();
        let draft = draft.clone();
        self.pending = Some(PendingApply { binding, operation });
        self.publish(cx);
        let service = self.service.clone();
        let work = cx.background_executor().spawn(async move {
            let mut prepared = service.prepare_apply(&draft, action, operation)?;
            service.prepare_theme(&mut prepared)?;
            service.execute(prepared, &CommandCancellation::new())
        });
        self.worker = Some(cx.spawn(async move |owner, cx| {
            let result = work.await;
            let _ = owner.update(cx, |owner, cx| {
                owner.complete_command(binding, operation, result, cx)
            });
        }));
    }

    fn matches_pending(&self, binding: SettingsDraftBinding, operation: u64) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| pending.binding == binding && pending.operation == operation)
            && self
                .draft
                .as_ref()
                .is_some_and(|draft| draft.binding() == binding)
    }

    fn complete_command(
        &mut self,
        binding: SettingsDraftBinding,
        operation: u64,
        result: Result<SettingsApplyObservation, SettingsServiceError>,
        cx: &mut Context<Self>,
    ) {
        self.worker = None;
        if !self.matches_pending(binding, operation) {
            return;
        }
        match result {
            Ok(observation) => self.consume_observation(observation, cx),
            Err(error) => {
                if let Ok(Some(observation)) = self.service.observation(operation) {
                    self.consume_observation(observation, cx);
                } else if self.service.pending_operation() == Some(operation) {
                    self.reconcile_original(cx);
                } else {
                    self.fail_unadmitted(error, cx);
                }
            }
        }
    }

    pub(super) fn consume_observation(
        &mut self,
        observation: SettingsApplyObservation,
        cx: &mut Context<Self>,
    ) {
        if !self.matches_pending(observation.binding(), observation.operation()) {
            return;
        }
        match &observation {
            SettingsApplyObservation::Reconciling { .. } => {
                self.publish(cx);
                self.reconcile_original(cx);
                return;
            }
            SettingsApplyObservation::Committed { action, theme, .. } => {
                let accepted = self
                    .draft
                    .as_mut()
                    .is_some_and(|draft| draft.accept_committed(&observation).unwrap_or(false));
                self.pending = None;
                if accepted {
                    self.feedback = std::array::from_fn(|_| None);
                    if theme.is_some() {
                        self.confirmed_theme = theme.clone();
                        self.appearance_busy = true;
                    }
                } else {
                    self.source_current = false;
                }
                self.publish(cx);
                if accepted && *action == SettingsApplyAction::Ok {
                    if self.appearance_busy {
                        self.close_after_appearance = true;
                        return;
                    }
                    self.deactivate_theme_editor(cx);
                    self.theme_state.editor = None;
                    self.editor_worker = None;
                    self.retire_theme_reads();
                    if let Some(window) = &self.window {
                        let _ = window.hide(cx);
                    }
                }
                return;
            }
            SettingsApplyObservation::NotCommitted { issue, .. } => {
                self.mark_modified_feedback(*issue)
            }
            SettingsApplyObservation::ScopedUnresolved { issue, .. } => {
                if let Some(draft) = &mut self.draft {
                    if draft.mark_scoped_unresolved(&observation).is_err() {
                        self.source_current = false;
                    }
                }
                self.mark_modified_feedback(*issue);
            }
        }
        self.pending = None;
        self.publish(cx);
    }

    fn reconcile_original(&mut self, cx: &mut Context<Self>) {
        if self.worker.is_some() {
            return;
        }
        let Some(pending) = &self.pending else {
            return;
        };
        let binding = pending.binding;
        let operation = pending.operation;
        let service = self.service.clone();
        self.worker = Some(cx.spawn(async move |owner, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(40))
                .await;
            let result = cx
                .background_executor()
                .spawn(async move { service.reconcile_original(operation) })
                .await;
            let _ = owner.update(cx, |owner, cx| {
                owner.complete_command(binding, operation, result, cx)
            });
        }));
    }

    pub(super) fn fail_unadmitted(&mut self, error: SettingsServiceError, cx: &mut Context<Self>) {
        let issue = match error {
            SettingsServiceError::Invalid { key, issue } => {
                self.set_feedback(key, presentation::issue_text(issue));
                None
            }
            SettingsServiceError::Stale | SettingsServiceError::Changed => {
                Some(SettingsIssue::Stale)
            }
            SettingsServiceError::Unavailable(_) => Some(SettingsIssue::Excluded),
            _ => Some(SettingsIssue::CommandRefused),
        };
        if let Some(issue) = issue {
            self.mark_modified_feedback(issue);
        }
        self.pending = None;
        self.publish(cx);
    }

    fn mark_modified_feedback(&mut self, issue: SettingsIssue) {
        if let Some(draft) = &self.draft {
            for key in draft.modified() {
                self.set_feedback(key, presentation::issue_text(issue));
            }
        }
    }
}
