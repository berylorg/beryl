use super::*;
use crate::main_window::{NoticeConditionId, NoticeDismissal, NoticeKind, NoticeVariant};

impl MainWindowShellRoot {
    pub(in crate::main_window::shell) fn remove_running_activation_failure(
        &mut self,
        previous: Option<NoticeRecordToken>,
    ) {
        if let Some(previous) = previous {
            let _ = self.notices.arbiter.remove(&previous);
        }
    }

    pub(in crate::main_window::shell) fn publish_running_activation_failure(
        &mut self,
        previous: Option<NoticeRecordToken>,
        error: &str,
    ) -> Option<NoticeRecordToken> {
        if self.notices.retired || self.notices.inert || !self.notices.scope_current() {
            return None;
        }
        if let Some(previous) = previous {
            let _ = self.notices.arbiter.remove(&previous);
        }
        let record = NoticeRecord {
            window_id: self.notices.window_id,
            condition: NoticeConditionId::new(),
            revision: 1,
            kind: NoticeKind::Error,
            content: NoticeContent::new(
                NoticeVariant::Error,
                NoticeDismissal::Dismissible,
                "Thread could not be opened",
                error,
            ),
        };
        match self.notices.arbiter.admit(record) {
            NoticeAdmission::Admitted(token) | NoticeAdmission::Updated(token) => Some(token),
            _ => None,
        }
    }
}
