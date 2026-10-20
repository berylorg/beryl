use super::*;
use crate::main_window::{
    MainWindowComposerSelectionIdentity, NoticeConditionId, NoticeDismissal, NoticeKind,
    NoticeVariant,
};

#[derive(Default)]
pub(super) struct DiscussionNoticeContribution {
    scope: Option<(
        gpui::EntityId,
        beryl_state::WindowClaimSelection,
        Option<crate::discussion_presentation::DiscussionAttemptIdentity>,
    )>,
    condition: Option<NoticeConditionId>,
    token: Option<NoticeRecordToken>,
    detail: Option<String>,
    retry_failure: Option<(gpui::EntityId, MainWindowComposerSelectionIdentity, String)>,
    revision: u64,
}

impl MainWindowShellRoot {
    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_discussion_notice_token(&self) -> Option<NoticeRecordToken> {
        self.notices
            .discussion
            .token
            .clone()
            .filter(|token| self.notices.arbiter.contains(token))
    }
    pub(in crate::main_window::shell) fn retain_discussion_retry_failure(
        &mut self,
        owner: gpui::EntityId,
        selection: MainWindowComposerSelectionIdentity,
        detail: String,
        cx: &App,
    ) {
        if self.discussion_composer(cx).is_some_and(|composer| {
            composer.entity_id() == owner && composer.read(cx).selection_identity() == selection
        }) {
            self.notices.discussion.retry_failure =
                Some((owner, selection, bounded_detail(&detail)));
        }
    }

    pub(in crate::main_window::shell) fn clear_discussion_retry_failure(&mut self) {
        self.notices.discussion.retry_failure = None;
    }

    pub(super) fn sync_discussion_notice(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.selected_discussion_snapshot(cx);
        let scope = self.discussion_composer(cx).map(|composer| {
            (
                composer.entity_id(),
                composer.read(cx).selection_identity().claim(),
                snapshot.as_ref().and_then(|snapshot| snapshot.attempt()),
            )
        });
        if self.notices.discussion.scope != scope {
            if let Some(token) = self.notices.discussion.token.take() {
                let _ = self.notices.arbiter.remove(&token);
            }
            self.notices.discussion = DiscussionNoticeContribution {
                scope,
                condition: scope.map(|_| NoticeConditionId::new()),
                ..Default::default()
            };
        }
        if snapshot.as_ref().is_some_and(|snapshot| {
            !matches!(
                snapshot.status(),
                Some(
                    crate::discussion_presentation::DiscussionStatus::HandoffFailed
                        | crate::discussion_presentation::DiscussionStatus::Unavailable
                )
            )
        }) {
            self.notices.discussion.retry_failure = None;
        }
        let retry_failure = self
            .notices
            .discussion
            .retry_failure
            .as_ref()
            .filter(|(owner, selection, _)| {
                scope.is_some_and(|(current, selected, _)| {
                    current == *owner && selected == selection.claim()
                })
            })
            .map(|(_, _, detail)| detail.as_str());
        let durable_detail = snapshot.as_ref().and_then(|snapshot| snapshot.detail());
        let detail = match (retry_failure, durable_detail) {
            (Some(failure), Some(durable)) => {
                Some(bounded_detail(&format!("{failure}\n\n{durable}")))
            }
            (Some(detail), None) | (None, Some(detail)) => Some(bounded_detail(detail)),
            (None, None) => None,
        };
        let contribution = &mut self.notices.discussion;
        if contribution.detail != detail {
            contribution.detail = detail;
            contribution.revision = contribution.revision.saturating_add(1);
        }
        let Some(detail) = &contribution.detail else {
            if let Some(token) = contribution.token.take() {
                let _ = self.notices.arbiter.remove(&token);
            }
            return;
        };
        let Some(condition) = &contribution.condition else {
            return;
        };
        if let Some(token) = &contribution.token {
            if self.notices.arbiter.contains(token) && token.revision() == contribution.revision {
                return;
            }
        }
        let record = NoticeRecord {
            window_id: self.notices.window_id,
            condition: condition.clone(),
            revision: contribution.revision,
            kind: NoticeKind::Error,
            content: NoticeContent::new(
                NoticeVariant::Error,
                NoticeDismissal::Persistent,
                "Discussion handoff",
                detail,
            ),
        };
        contribution.token = match self.notices.arbiter.admit(record) {
            NoticeAdmission::Admitted(token) | NoticeAdmission::Updated(token) => Some(token),
            NoticeAdmission::Omitted | NoticeAdmission::Rejected(_) => None,
        };
    }
}

fn bounded_detail(detail: &str) -> String {
    const LIMIT: usize = 2048;
    const SUFFIX: &str = "… [truncated]";
    if detail.len() <= LIMIT {
        return detail.to_owned();
    }
    let mut end = LIMIT - SUFFIX.len();
    while !detail.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{SUFFIX}", &detail[..end])
}
