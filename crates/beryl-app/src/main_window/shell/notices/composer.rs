use super::*;
use crate::main_window::{
    MainWindowComposerMutationFeedback, MainWindowComposerMutationFeedbackKind,
    MainWindowConversationComposer, NoticeConditionId, NoticeDismissal, NoticeKind, NoticeVariant,
};

#[derive(Default)]
pub(super) struct ComposerNoticeContribution {
    mount: Option<gpui::EntityId>,
    mount_subscription: Option<gpui::Subscription>,
    owner: Option<gpui::WeakEntity<MainWindowConversationComposer>>,
    owner_subscription: Option<gpui::Subscription>,
    feedback: Option<ComposerNoticeFeedback>,
}

struct ComposerNoticeFeedback {
    source: MainWindowComposerMutationFeedback,
    condition: NoticeConditionId,
    token: Option<NoticeRecordToken>,
    attempted: bool,
    last_arbiter_change: Option<(usize, u64, u64, u64, u64, u64)>,
}

impl MainWindowShellRoot {
    pub(super) fn sync_composer_notice(&mut self, cx: &mut Context<Self>) {
        let mount = self
            .controller
            .as_ref()
            .and_then(|controller| controller.composer_mount());
        let mount_id = mount.as_ref().map(|mount| mount.entity_id());
        if self.notices.composer.mount != mount_id {
            self.notices.composer.mount = mount_id;
            self.notices.composer.mount_subscription = mount
                .as_ref()
                .map(|mount| cx.observe(mount, |_, _, cx| cx.notify()));
        }
        let owner = mount.and_then(|mount| mount.read(cx).contribution());
        let old_owner = self
            .notices
            .composer
            .owner
            .as_ref()
            .and_then(|owner| owner.upgrade());
        let owner_changed = old_owner.as_ref().map(|owner| owner.entity_id())
            != owner.as_ref().map(|owner| owner.entity_id())
            || (owner.is_none() && self.notices.composer.owner.is_some());
        if owner_changed {
            self.clear_composer_notice();
            self.notices.composer.owner = owner.as_ref().map(|owner| owner.downgrade());
            self.notices.composer.owner_subscription = owner
                .as_ref()
                .map(|owner| cx.observe(owner, |_, _, cx| cx.notify()));
        }
        let source = owner.and_then(|owner| owner.read(cx).mutation_feedback());
        if self
            .notices
            .composer
            .feedback
            .as_ref()
            .map(|feedback| feedback.source)
            != source
        {
            self.clear_composer_notice();
            self.notices.composer.feedback = source.map(|source| ComposerNoticeFeedback {
                source,
                condition: NoticeConditionId::new(),
                token: None,
                attempted: false,
                last_arbiter_change: None,
            });
        }
        let Some(feedback) = self.notices.composer.feedback.as_mut() else {
            return;
        };
        let persistent = matches!(
            feedback.source.kind,
            MainWindowComposerMutationFeedbackKind::Unavailable
                | MainWindowComposerMutationFeedbackKind::AdmittedWorkUnavailable
                | MainWindowComposerMutationFeedbackKind::CommittedUnavailable
        );
        if feedback
            .token
            .as_ref()
            .is_some_and(|token| self.notices.arbiter.contains(token))
        {
            return;
        }
        if feedback.attempted && !persistent {
            return;
        }
        let change = arbiter_change(self.notices.arbiter.diagnostics());
        if feedback.last_arbiter_change == Some(change) {
            return;
        }
        let (title, detail) = feedback_text(feedback.source.kind);
        let record = NoticeRecord {
            window_id: self.notices.window_id,
            condition: feedback.condition.clone(),
            revision: 1,
            kind: NoticeKind::Error,
            content: NoticeContent::new(
                NoticeVariant::Error,
                if persistent {
                    NoticeDismissal::Persistent
                } else {
                    NoticeDismissal::Dismissible
                },
                title,
                detail,
            ),
        };
        feedback.token = match self.notices.arbiter.admit(record) {
            NoticeAdmission::Admitted(token) | NoticeAdmission::Updated(token) => Some(token),
            NoticeAdmission::Omitted | NoticeAdmission::Rejected(_) => None,
        };
        feedback.attempted = true;
        feedback.last_arbiter_change = Some(arbiter_change(self.notices.arbiter.diagnostics()));
    }

    fn clear_composer_notice(&mut self) {
        if let Some(feedback) = self.notices.composer.feedback.take()
            && let Some(token) = feedback.token
            && self.notices.arbiter.contains(&token)
        {
            let _ = self.notices.arbiter.remove(&token);
        }
    }
}

fn arbiter_change(diagnostics: NoticeDiagnostics) -> (usize, u64, u64, u64, u64, u64) {
    (
        diagnostics.retained_records,
        diagnostics.admitted,
        diagnostics.updated,
        diagnostics.replaced,
        diagnostics.dismissed,
        diagnostics.removed,
    )
}

fn feedback_text(kind: MainWindowComposerMutationFeedbackKind) -> (&'static str, &'static str) {
    use MainWindowComposerMutationFeedbackKind as Kind;
    match kind {
        Kind::OperationTooLarge => (
            "Image edit is too large",
            "Use a smaller selection. Waiting and retrying the same operation will not make it fit. Your draft is unchanged.",
        ),
        Kind::CapacityUnavailable => (
            "Image edit capacity is temporarily unavailable",
            "Try the operation again later, after capacity is released. Your draft is unchanged.",
        ),
        Kind::Storage => (
            "Image edit storage failure",
            "The image edit could not be stored. Your draft is unchanged.",
        ),
        Kind::Refused => (
            "Image edit was refused",
            "The image edit could not be admitted. Your draft is unchanged.",
        ),
        Kind::Unavailable => (
            "Composer request is unavailable",
            "The exact request cannot be completed safely. The retained draft is context only. This request cannot be retried. Other healthy conversations remain available.",
        ),
        Kind::CommittedUnavailable => (
            "Committed composer request is unavailable",
            "The request committed, but its editor result is unavailable. The retained draft is context only. This request cannot be repeated. Other healthy conversations remain available.",
        ),
        Kind::AdmittedWorkUnavailable => (
            "Composer request is unavailable",
            "Some work for this request was stored, but the edit outcome is unavailable. The retained draft is context only. This request cannot be retried. Other healthy conversations remain available.",
        ),
    }
}
