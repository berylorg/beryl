use super::*;
use crate::main_window::{NoticeConditionId, NoticeDismissal, NoticeKind, NoticeVariant};

impl MainWindowShellRoot {
    pub(in crate::main_window::shell) fn activity_height_notice(
        &mut self,
        condition: NoticeConditionId,
        revision: u64,
        title: &'static str,
        detail: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.notices.arbiter.admit(NoticeRecord {
            window_id: self.notices.window_id,
            condition,
            revision,
            kind: NoticeKind::Error,
            content: NoticeContent::new(
                NoticeVariant::Error,
                NoticeDismissal::Dismissible,
                title,
                detail,
            ),
        });
        self.sync_notices(window, cx);
        cx.notify();
    }
}
