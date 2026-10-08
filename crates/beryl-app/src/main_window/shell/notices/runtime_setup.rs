use super::*;
use crate::main_window::{NoticeConditionId, NoticeDismissal, NoticeKind, NoticeVariant};

impl MainWindowShellRoot {
    pub(in crate::main_window::shell) fn publish_runtime_setup_failure(
        &mut self,
        previous: Option<NoticeRecordToken>,
        error: &str,
        terminal: bool,
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
                if terminal {
                    NoticeDismissal::Persistent
                } else {
                    NoticeDismissal::Dismissible
                },
                if terminal {
                    "Runtime setup is unavailable"
                } else {
                    "Runtime setup could not complete"
                },
                error,
            ),
        };
        match self.notices.arbiter.admit(record) {
            NoticeAdmission::Admitted(token) | NoticeAdmission::Updated(token) => Some(token),
            _ => None,
        }
    }
}
