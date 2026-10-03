use super::*;
use crate::main_window::{NoticeConditionId, NoticeDismissal, NoticeKind, NoticeVariant};
use std::time::Duration;

struct SuccessfulHomeOpen {
    startup: Arc<()>,
    best_effort: bool,
}

impl gpui::Global for SuccessfulHomeOpen {}

pub(crate) fn publish_home_open_notice_classification(best_effort: bool, app: &mut App) {
    app.set_global(SuccessfulHomeOpen {
        startup: Arc::new(()),
        best_effort,
    });
}

#[cfg(feature = "test-faults")]
pub fn test_publish_home_open_notice_classification(best_effort: bool, app: &mut App) {
    publish_home_open_notice_classification(best_effort, app);
}

#[derive(Default)]
pub(super) struct HomeWarning {
    record: Option<NoticeRecordToken>,
    armed: Option<NoticeVisibleToken>,
    task: Option<gpui::Task<()>>,
    arm: Option<Rc<()>>,
}

impl MainWindowShellRoot {
    pub(in crate::main_window::shell) fn admit_best_effort_home_warning(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = cx.try_global::<SuccessfulHomeOpen>() else {
            return;
        };
        if !open.best_effort
            || self
                .home_warning_startup
                .as_ref()
                .is_some_and(|startup| Arc::ptr_eq(startup, &open.startup))
            || self.notices.retired
            || !self.notices.scope_current()
        {
            return;
        }
        self.home_warning_startup = Some(open.startup.clone());
        let record = NoticeRecord {
            window_id: self.notices.window_id,
            condition: NoticeConditionId::new(),
            revision: 1,
            kind: NoticeKind::Warning,
            content: NoticeContent::new(
                NoticeVariant::Warning,
                NoticeDismissal::Dismissible,
                "Reduced home durability",
                "Beryl acquired exclusive ownership of this home, but its filesystem provides reduced durability guarantees compared with native local NTFS.",
            ),
        };
        if let NoticeAdmission::Admitted(token) = self.notices.arbiter.admit(record) {
            self.notices.home_warning.record = Some(token);
        }
        self.sync_notices(window, cx);
    }

    pub(super) fn sync_home_warning_timer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let token = if self.notices.retired {
            None
        } else {
            self.notices.arbiter.active().and_then(|projection| {
                self.notices
                    .home_warning
                    .record
                    .as_ref()
                    .filter(|record| record.same_identity(projection.token.record()))
                    .map(|_| projection.token.clone())
            })
        };
        if self.notices.home_warning.armed == token {
            return;
        }
        self.notices.home_warning.task = None;
        self.notices.home_warning.arm = None;
        self.notices.home_warning.armed = token.clone();
        let Some(token) = token else {
            return;
        };
        let arm = Rc::new(());
        self.notices.home_warning.arm = Some(arm.clone());
        let sleeper = cx.background_executor().timer(Duration::from_secs(5));
        self.notices.home_warning.task = Some(cx.spawn_in(window, async move |this, cx| {
            sleeper.await;
            let _ = this.update_in(cx, |root, window, cx| {
                root.expire_home_warning(&token, &arm, window, cx);
            });
        }));
    }

    fn expire_home_warning(
        &mut self,
        token: &NoticeVisibleToken,
        arm: &Rc<()>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.notices.retired
            || !self.notices.scope_current()
            || self.notices.home_warning.armed.as_ref() != Some(token)
            || !self
                .notices
                .home_warning
                .arm
                .as_ref()
                .is_some_and(|current| Rc::ptr_eq(current, arm))
        {
            return;
        }
        if self.notices.arbiter.dismiss(token).is_ok() {
            self.sync_notices(window, cx);
        }
    }

    #[cfg(feature = "test-faults")]
    pub fn test_repeat_home_warning_trigger(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.admit_best_effort_home_warning(window, cx);
    }

    #[cfg(feature = "test-faults")]
    pub fn test_home_warning_timer(&self) -> Option<BestEffortHomeWarningTimer> {
        Some(BestEffortHomeWarningTimer {
            token: self.notices.home_warning.armed.clone()?,
            arm: self.notices.home_warning.arm.clone()?,
        })
    }

    #[cfg(feature = "test-faults")]
    pub fn test_expire_home_warning(
        &mut self,
        timer: &BestEffortHomeWarningTimer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.expire_home_warning(&timer.token, &timer.arm, window, cx);
    }
}

#[cfg(feature = "test-faults")]
#[derive(Clone)]
pub struct BestEffortHomeWarningTimer {
    token: NoticeVisibleToken,
    arm: Rc<()>,
}
