use super::*;
use crate::cas_projection::{ExactStopFeedback, ExactStopFeedbackSnapshot, ExactStopFeedbackState};
use crate::main_window::{NoticeConditionId, NoticeDismissal, NoticeKind, NoticeVariant};

#[derive(Default)]
pub(super) struct StopFeedbackNoticeContribution {
    entries: Vec<FeedbackNotice>,
    selected: Option<NoticeConditionId>,
}

struct FeedbackNotice {
    feedback: ExactStopFeedback,
    condition: NoticeConditionId,
    token: Option<NoticeRecordToken>,
    revision: u64,
    projected: Option<(ExactStopFeedbackSnapshot, bool)>,
}

impl MainWindowShellRoot {
    pub(super) fn sync_stop_feedback_notice(&mut self) {
        let observations = self
            .exact_stop_feedback_handoff()
            .iter()
            .map(|entry| {
                (
                    entry.feedback.clone(),
                    self.stop_feedback_popup_safe(&entry.feedback),
                    self.stop_feedback_fresh_eligibility(&entry.feedback),
                    entry.feedback.snapshot(),
                )
            })
            .collect::<Vec<_>>();
        let contribution = &mut self.notices.stop_feedback;
        let arbiter = &mut self.notices.arbiter;
        contribution.entries.retain_mut(|entry| {
            if observations
                .iter()
                .any(|(feedback, _, _, _)| feedback == &entry.feedback)
            {
                return true;
            }
            if let Some(token) = entry.token.take()
                && arbiter.contains(&token)
            {
                let _ = arbiter.remove(&token);
            }
            false
        });
        for (feedback, _, _, _) in &observations {
            if !contribution
                .entries
                .iter()
                .any(|entry| &entry.feedback == feedback)
            {
                contribution.entries.push(FeedbackNotice {
                    feedback: feedback.clone(),
                    condition: NoticeConditionId::new(),
                    token: None,
                    revision: 0,
                    projected: None,
                });
            }
        }
        let unsafe_entry = |entry: &FeedbackNotice| {
            observations
                .iter()
                .any(|(feedback, safe, _, _)| feedback == &entry.feedback && !safe)
        };
        let previous = contribution.selected.as_ref().and_then(|condition| {
            contribution
                .entries
                .iter()
                .position(|entry| &entry.condition == condition)
        });
        let selected = previous
            .filter(|index| unsafe_entry(&contribution.entries[*index]))
            .or_else(|| contribution.entries.iter().position(unsafe_entry));
        for (index, entry) in contribution.entries.iter_mut().enumerate() {
            if Some(index) != selected
                && let Some(token) = entry.token.take()
                && arbiter.contains(&token)
            {
                let _ = arbiter.remove(&token);
            }
        }
        contribution.selected = selected.map(|index| contribution.entries[index].condition.clone());
        let Some(index) = selected else {
            return;
        };
        let entry = &mut contribution.entries[index];
        let (_, _, fresh, snapshot) = observations
            .iter()
            .find(|(feedback, _, _, _)| feedback == &entry.feedback)
            .unwrap();
        let projected = (*snapshot, *fresh);
        if entry.projected == Some(projected)
            && entry
                .token
                .as_ref()
                .is_some_and(|token| arbiter.contains(token))
        {
            return;
        }
        let revision = entry
            .revision
            .checked_add(1)
            .expect("bounded feedback notice revisions");
        let content = content(snapshot.state, *fresh);
        let result = if let Some(token) =
            entry.token.as_ref().filter(|token| arbiter.contains(token))
        {
            arbiter.update(token, revision, content).ok()
        } else {
            match arbiter.admit(NoticeRecord {
                window_id: self.notices.window_id,
                condition: entry.condition.clone(),
                revision,
                kind: NoticeKind::ExactStopFeedback,
                content,
            }) {
                NoticeAdmission::Admitted(token) | NoticeAdmission::Updated(token) => Some(token),
                NoticeAdmission::Omitted | NoticeAdmission::Rejected(_) => None,
            }
        };
        if let Some(token) = result {
            entry.token = Some(token);
            entry.revision = revision;
            entry.projected = Some(projected);
        }
    }

    pub(super) fn acknowledge_stop_notice(&mut self, token: &NoticeVisibleToken) {
        let Some(index) = self
            .notices
            .stop_feedback
            .entries
            .iter()
            .position(|entry| entry.token.as_ref() == Some(token.record()))
        else {
            return;
        };
        let entry = self.notices.stop_feedback.entries.remove(index);
        self.acknowledge_stop_feedback(&entry.feedback);
        self.notices.stop_feedback.selected = None;
    }
}

fn content(state: ExactStopFeedbackState, fresh: bool) -> NoticeContent {
    use ExactStopFeedbackState as State;
    let (variant, dismissal, title, detail) = match state {
        State::Waiting => (
            NoticeVariant::Warning,
            NoticeDismissal::Persistent,
            "Soft stop requested",
            "Awaiting the exact operation's outcome.",
        ),
        State::DurableNondispatch => (
            NoticeVariant::Error,
            NoticeDismissal::Dismissible,
            "Soft stop was not dispatched",
            if fresh {
                "The durably admitted stop was proven not dispatched. Soft stop is available again for this same operation."
            } else {
                "The durably admitted stop was proven not dispatched."
            },
        ),
        State::VolatileNondispatch => (
            NoticeVariant::Error,
            NoticeDismissal::Dismissible,
            "Soft stop request failed",
            "The volatile stop request was not dispatched. It cannot be retried.",
        ),
        State::RequestNotAdmitted => (
            NoticeVariant::Error,
            NoticeDismissal::Dismissible,
            "Soft stop request was not admitted",
            "The exact request was not admitted. No terminal outcome is claimed.",
        ),
        State::Interrupted => (
            NoticeVariant::Info,
            NoticeDismissal::Dismissible,
            "Operation interrupted",
            "The exact operation was interrupted.",
        ),
        State::Completed => (
            NoticeVariant::Info,
            NoticeDismissal::Dismissible,
            "Operation completed",
            "The exact operation completed successfully.",
        ),
        State::Failed => (
            NoticeVariant::Error,
            NoticeDismissal::Dismissible,
            "Operation failed",
            "The exact operation ended with failure.",
        ),
        State::UnknownTerminal => (
            NoticeVariant::Warning,
            NoticeDismissal::Dismissible,
            "Operation outcome is unknown",
            "The exact operation reached an unknown terminal outcome.",
        ),
        State::AuthorityLost => (
            NoticeVariant::Warning,
            NoticeDismissal::Dismissible,
            "Soft stop authority was lost",
            "Exact operation authority was lost before its outcome could be confirmed.",
        ),
    };
    NoticeContent::new(variant, dismissal, title, detail)
}
