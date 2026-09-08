use super::*;
use crate::main_window::{
    MainWindowNoticeArbiter, MainWindowNoticeDiagnosticKey, MainWindowNoticeOverlayAllocation,
    MainWindowNoticeWidget, MainWindowNoticeWidgetEvent, MainWindowNoticeWidgetRecord,
    NoticeAdmission, NoticeCommandId, NoticeCommandState, NoticeContent, NoticeDiagnostics,
    NoticeProjection, NoticeRecord, NoticeRecordToken, NoticeRejection, NoticeVisibleToken,
};
use crate::theme_runtime::{AppearancePublicationTarget, GpuiAppearancePublicationTarget};
use gpui::Focusable;

mod composer;

#[derive(Clone)]
pub struct MainWindowNoticeIngress {
    window: WindowHandle<MainWindowShellRoot>,
    window_id: beryl_model::WindowId,
    home: beryl_state::ThemeHomeIdentity,
    lifetime: Rc<()>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MainWindowNoticeOwnerCommand {
    pub token: NoticeVisibleToken,
    pub command: NoticeCommandId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainWindowNoticeRouteRejection {
    Notice(NoticeRejection),
    Inert,
    CommandUnavailable,
}

impl gpui::EventEmitter<MainWindowNoticeOwnerCommand> for MainWindowShellRoot {}

impl MainWindowNoticeIngress {
    fn validate(&self, root: &MainWindowShellRoot) -> Result<(), NoticeRejection> {
        let mount = &root.notices;
        if mount.retired || !mount.scope_current() || !Rc::ptr_eq(&self.lifetime, &mount.lifetime) {
            return Err(NoticeRejection::Disposed);
        }
        let controller = root.controller.as_ref().ok_or(NoticeRejection::Disposed)?;
        if self.window_id != controller.window_id() {
            return Err(NoticeRejection::WrongWindow);
        }
        if self.home != controller.appearance().prepared().home() || self.home != mount.home {
            return Err(NoticeRejection::Disposed);
        }
        Ok(())
    }

    pub fn admit(&self, record: NoticeRecord, app: &mut App) -> NoticeAdmission {
        self.window
            .update(app, |root, window, cx| {
                if let Err(reason) = self.validate(root) {
                    return NoticeAdmission::Rejected(reason);
                }
                let result = root.notices.arbiter.admit(record);
                root.sync_notices(window, cx);
                result
            })
            .unwrap_or(NoticeAdmission::Rejected(NoticeRejection::Disposed))
    }

    pub fn update(
        &self,
        expected: &NoticeRecordToken,
        revision: u64,
        content: NoticeContent,
        app: &mut App,
    ) -> Result<NoticeRecordToken, NoticeRejection> {
        self.window
            .update(app, |root, window, cx| {
                self.validate(root)?;
                let result = root.notices.arbiter.update(expected, revision, content);
                root.sync_notices(window, cx);
                result
            })
            .unwrap_or(Err(NoticeRejection::Disposed))
    }

    pub fn replace_protected(
        &self,
        expected: &NoticeRecordToken,
        replacement: NoticeRecord,
        app: &mut App,
    ) -> Result<NoticeRecordToken, NoticeRejection> {
        self.window
            .update(app, |root, window, cx| {
                self.validate(root)?;
                let result = root
                    .notices
                    .arbiter
                    .replace_protected(expected, replacement);
                root.sync_notices(window, cx);
                result
            })
            .unwrap_or(Err(NoticeRejection::Disposed))
    }

    pub fn remove(
        &self,
        expected: &NoticeRecordToken,
        app: &mut App,
    ) -> Result<(), NoticeRejection> {
        self.window
            .update(app, |root, window, cx| {
                self.validate(root)?;
                let result = root.notices.arbiter.remove(expected);
                root.sync_notices(window, cx);
                result
            })
            .unwrap_or(Err(NoticeRejection::Disposed))
    }

    pub fn dispatch(
        &self,
        event: MainWindowNoticeWidgetEvent,
        app: &mut App,
    ) -> Result<(), MainWindowNoticeRouteRejection> {
        self.window
            .update(app, |root, window, cx| {
                self.validate(root)
                    .map_err(MainWindowNoticeRouteRejection::Notice)?;
                root.route_notice_event(event, window, cx)
            })
            .unwrap_or(Err(MainWindowNoticeRouteRejection::Notice(
                NoticeRejection::Disposed,
            )))
    }
}

pub(super) struct MainWindowShellNotices {
    arbiter: MainWindowNoticeArbiter,
    composer: composer::ComposerNoticeContribution,
    window_id: beryl_model::WindowId,
    pub(super) widget: Entity<MainWindowNoticeWidget>,
    home: beryl_state::ThemeHomeIdentity,
    publication: Arc<GpuiAppearancePublicationTarget>,
    lifetime: Rc<()>,
    subscription: Option<gpui::Subscription>,
    projected: Option<NoticeVisibleToken>,
    allocation: Option<MainWindowNoticeOverlayAllocation>,
    diagnostic_sequence: u64,
    diagnostic_key: MainWindowNoticeDiagnosticKey,
    inert: bool,
    retired: bool,
}

impl MainWindowShellNotices {
    pub(super) fn new(
        controller: &MainWindowShellController,
        publication: Arc<GpuiAppearancePublicationTarget>,
        safe_focus: gpui::FocusHandle,
        cx: &mut Context<MainWindowShellRoot>,
    ) -> Self {
        let appearance = controller.appearance().clone();
        Self {
            arbiter: MainWindowNoticeArbiter::new(controller.window_id()),
            composer: composer::ComposerNoticeContribution::default(),
            window_id: controller.window_id(),
            home: appearance.prepared().home(),
            publication,
            widget: cx.new(|cx| MainWindowNoticeWidget::new(appearance, safe_focus, |_| {}, cx)),
            lifetime: Rc::new(()),
            subscription: None,
            projected: None,
            allocation: None,
            diagnostic_sequence: 0,
            diagnostic_key: MainWindowNoticeDiagnosticKey::from_opaque_bytes([0; 32]),
            inert: false,
            retired: false,
        }
    }

    fn scope_current(&self) -> bool {
        let current = self.publication.snapshot();
        current.active && current.current.prepared().home() == self.home
    }
}

impl MainWindowShellRoot {
    pub fn notice_ingress(&self, window: &Window, _: &Context<Self>) -> MainWindowNoticeIngress {
        MainWindowNoticeIngress {
            window: window
                .window_handle()
                .downcast::<Self>()
                .expect("main-window shell root"),
            window_id: self.notices.window_id,
            home: self.notices.home,
            lifetime: self.notices.lifetime.clone(),
        }
    }

    pub fn notice_widget(&self) -> Entity<MainWindowNoticeWidget> {
        self.notices.widget.clone()
    }

    pub fn notice_projection(&self) -> Option<NoticeProjection<'_>> {
        self.notices.arbiter.active()
    }

    pub fn notice_diagnostics(&self) -> NoticeDiagnostics {
        self.notices.arbiter.diagnostics()
    }

    pub fn notice_safe_focus(&self, app: &App) -> gpui::FocusHandle {
        self.controller
            .as_ref()
            .and_then(|controller| controller.composer_mount())
            .and_then(|mount| mount.read(app).contribution())
            .map(|composer| composer.read(app).gpui_input())
            .filter(|input| input.read(app).is_surface_current_and_interactive())
            .map(|input| input.read(app).focus_handle(app))
            .unwrap_or_else(|| self.shell_focus.clone())
    }

    fn refresh_notice_safe_focus(&self, cx: &mut Context<Self>) {
        let focus = self.notice_safe_focus(cx);
        self.notices
            .widget
            .update(cx, |widget, _| widget.set_safe_focus(focus));
    }

    pub fn set_notices_inert(&mut self, inert: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.notices.retired {
            return;
        }
        self.refresh_notice_safe_focus(cx);
        self.notices.inert = inert;
        self.notices
            .widget
            .update(cx, |widget, cx| widget.set_inert(inert, window, cx));
    }

    pub fn retire_notices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.notices.retired {
            return;
        }
        self.refresh_notice_safe_focus(cx);
        self.notices.retired = true;
        self.notices.subscription = None;
        self.notices.composer = composer::ComposerNoticeContribution::default();
        self.notices.arbiter.dispose();
        self.notices.projected = None;
        self.notices.allocation = None;
        self.notices.widget.update(cx, |widget, cx| {
            widget.replace(None, window, cx);
            widget.set_inert(true, window, cx);
        });
        cx.notify();
    }

    pub(super) fn subscribe_notices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ingress = self.notice_ingress(window, cx);
        self.notices.subscription = Some(cx.subscribe_in(
            &self.notices.widget,
            window,
            move |root, _, event: &MainWindowNoticeWidgetEvent, window, cx| {
                if ingress.validate(root).is_ok() {
                    let _ = root.route_notice_event(event.clone(), window, cx);
                }
            },
        ));
    }

    fn route_notice_event(
        &mut self,
        event: MainWindowNoticeWidgetEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), MainWindowNoticeRouteRejection> {
        use MainWindowNoticeRouteRejection as Rejection;
        if self.notices.retired || !self.notices.scope_current() {
            return Err(Rejection::Notice(NoticeRejection::Disposed));
        }
        if self.notices.inert {
            return Err(Rejection::Inert);
        }
        match event {
            MainWindowNoticeWidgetEvent::Dismiss(token) => {
                self.notices
                    .arbiter
                    .dismiss(&token)
                    .map_err(Rejection::Notice)?;
                self.sync_notices(window, cx);
            }
            MainWindowNoticeWidgetEvent::Command { token, command } => {
                let current = self
                    .notices
                    .arbiter
                    .active()
                    .ok_or(Rejection::Notice(NoticeRejection::StaleVisibility))?;
                if current.token != token {
                    return Err(Rejection::Notice(NoticeRejection::StaleVisibility));
                }
                if !current.content.commands().any(|candidate| {
                    candidate.id == command && candidate.state() == NoticeCommandState::Enabled
                }) {
                    return Err(Rejection::CommandUnavailable);
                }
                cx.emit(MainWindowNoticeOwnerCommand { token, command });
            }
        }
        Ok(())
    }

    pub(super) fn notice_chrome_height(&self) -> f32 {
        if self.creation.is_some() { 44. } else { 0. }
    }

    pub(super) fn sync_notices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.notices.retired {
            return;
        }
        if !self.notices.scope_current() {
            self.retire_notices(window, cx);
            return;
        }
        self.sync_composer_notice(cx);
        self.refresh_notice_safe_focus(cx);
        let viewport = window.viewport_size();
        let chrome = self
            .notice_chrome_height()
            .min(f32::from(viewport.height).max(0.));
        let inset_top = 12_f32.min((f32::from(viewport.height) - chrome).max(0.));
        let inset_right = 12_f32.min(f32::from(viewport.width).max(0.));
        let allocation = MainWindowNoticeOverlayAllocation::new(
            chrome,
            inset_top,
            inset_right,
            (f32::from(viewport.width) - inset_right).max(0.),
            (f32::from(viewport.height) - chrome - inset_top).max(0.),
        );
        let projection = self.notices.arbiter.active();
        let token = projection
            .as_ref()
            .map(|projection| projection.token.clone());
        if self.notices.projected == token && self.notices.allocation == Some(allocation) {
            return;
        }
        let same_identity = self
            .notices
            .projected
            .as_ref()
            .zip(token.as_ref())
            .is_some_and(|(old, new)| old.record().same_identity(new.record()));
        if token.is_some() && !same_identity {
            self.notices.diagnostic_sequence = self.notices.diagnostic_sequence.saturating_add(1);
            let mut bytes = [0; 32];
            bytes[..8].copy_from_slice(&cx.entity_id().as_u64().to_le_bytes());
            bytes[8..16].copy_from_slice(&self.notices.diagnostic_sequence.to_le_bytes());
            self.notices.diagnostic_key = MainWindowNoticeDiagnosticKey::from_opaque_bytes(bytes);
        }
        let record = projection.map(|projection| {
            MainWindowNoticeWidgetRecord::from_projection(
                projection,
                self.notices.diagnostic_key,
                allocation,
            )
        });
        self.notices.projected = token;
        self.notices.allocation = Some(allocation);
        self.notices
            .widget
            .update(cx, |widget, cx| widget.replace(record, window, cx));
        cx.notify();
    }
}
