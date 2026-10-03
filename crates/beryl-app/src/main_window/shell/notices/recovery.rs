use super::*;
use crate::main_window::{NoticeConditionId, NoticeDismissal, NoticeKind, NoticeVariant};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum MainWindowHomeRecoveryNoticeState {
    Failed,
    Recovering,
    Retrying,
    Cancelled,
    Unavailable,
    Recovered,
}

#[derive(Default)]
pub(super) struct HomeRecoveryNoticeContribution {
    condition: Option<NoticeConditionId>,
    pub(super) state: Option<MainWindowHomeRecoveryNoticeState>,
    record: Option<NoticeRecordToken>,
    revision: u64,
}

impl MainWindowShellRoot {
    pub(crate) fn project_running_home_recovery_notice(
        &mut self,
        condition: &NoticeConditionId,
        state: MainWindowHomeRecoveryNoticeState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.notices.retired
            || (!self.notices.scope_current() && !self.notices.recovery_display_current())
            || (self.notices.recovery.condition.as_ref() == Some(condition)
                && self.notices.recovery.state == Some(state))
        {
            return;
        }
        let recovered = state == MainWindowHomeRecoveryNoticeState::Recovered;
        if self.notices.recovery.condition.as_ref() != Some(condition) {
            if let Some(token) = self.notices.recovery.record.take() {
                let _ = self.notices.arbiter.remove(&token);
            }
            self.notices.recovery.revision = 0;
        }
        self.notices.recovery.revision += 1;
        let detail = match state {
            MainWindowHomeRecoveryNoticeState::Failed => {
                "This home is unavailable. Your open windows and unsaved work remain retained while home operations are unavailable."
            }
            MainWindowHomeRecoveryNoticeState::Recovering => {
                "Beryl is recovering this home. Your open windows and unsaved work are retained while home operations are unavailable."
            }
            MainWindowHomeRecoveryNoticeState::Retrying => {
                "Beryl is retrying recovery of this home. Your open windows and unsaved work remain retained while home operations are unavailable."
            }
            MainWindowHomeRecoveryNoticeState::Cancelled => {
                "Home recovery was cancelled. Your open windows and unsaved work remain retained, and home operations are unavailable."
            }
            MainWindowHomeRecoveryNoticeState::Unavailable => {
                "This home could not be recovered. Your open windows and unsaved work remain retained, and home operations are unavailable."
            }
            MainWindowHomeRecoveryNoticeState::Recovered => {
                "Beryl recovered and validated this home. Your open windows and retained work are available again."
            }
        };
        let record = NoticeRecord {
            window_id: self.notices.window_id,
            condition: condition.clone(),
            revision: self.notices.recovery.revision,
            kind: if recovered {
                NoticeKind::Recovery
            } else {
                NoticeKind::HomeFailure
            },
            content: NoticeContent::new(
                if recovered {
                    NoticeVariant::Info
                } else {
                    NoticeVariant::Error
                },
                if recovered {
                    NoticeDismissal::Dismissible
                } else {
                    NoticeDismissal::Persistent
                },
                if recovered {
                    "Home recovered"
                } else {
                    "Home unavailable"
                },
                detail,
            ),
        };
        if let NoticeAdmission::Admitted(token) | NoticeAdmission::Updated(token) =
            self.notices.arbiter.admit(record)
        {
            self.notices.recovery.record = Some(token);
            self.notices.recovery.condition = Some(condition.clone());
            self.notices.recovery.state = Some(state);
        }
        self.sync_notices(window, cx);
    }
}
