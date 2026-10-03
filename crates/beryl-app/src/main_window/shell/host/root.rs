use super::*;

impl MainWindowShellRoot {
    pub(super) fn new(
        controller: MainWindowShellController,
        construction_error: Option<String>,
        publication: Arc<crate::theme_runtime::GpuiAppearancePublicationTarget>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let shell_focus = cx.focus_handle();
        let notices =
            notices::MainWindowShellNotices::new(&controller, publication, shell_focus.clone(), cx);
        let mut root = Self {
            startup_interaction: None,
            shutdown_interaction_gated: false,
            exit_disabled_reason: None,
            running_command: None,
            ordinary_close_interaction_gated: false,
            #[cfg(target_os = "windows")]
            blocked_shutdown: None,
            controller: Some(controller),
            construction_error,
            composer_observer: None,
            creation: None,
            creation_observer: None,
            appearance_release: None,
            command_focus: cx.focus_handle(),
            exit_focus: cx.focus_handle(),
            shell_focus,
            notices,
            home_warning_startup: None,
        };
        root.subscribe_notices(window, cx);
        root
    }

    #[must_use]
    pub fn controller(&self) -> Option<&MainWindowShellController> {
        self.controller.as_ref()
    }

    pub(crate) fn diagnostic_selected_thread(
        &self,
        app: &App,
    ) -> Option<beryl_model::SyndicThreadId> {
        self.creation_target(app)
            .map(|(selection, _)| selection.claim().thread_id())
    }

    pub(in crate::main_window) fn creation_target(
        &self,
        app: &App,
    ) -> Option<(
        crate::main_window::MainWindowComposerSelectionIdentity,
        beryl_state::RememberedTarget,
    )> {
        let controller = self.controller.as_ref()?;
        let (selection, target) = match &controller.content {
            ShellContent::Recovered {
                window, selection, ..
            } => (*selection, window.remembered_target()?),
            ShellContent::Acquired { custody, selection } => {
                (*selection, custody.acquisition.target())
            }
            ShellContent::Restored { custody, selection } => {
                (*selection, custody.composer.target())
            }
            ShellContent::Threadless { .. }
            | ShellContent::RecoveredThreadless { .. }
            | ShellContent::Retired { .. } => return None,
        };
        let mount = controller.composer_mount.as_ref()?.read(app);
        let composer = mount.contribution()?.read(app);
        (composer.selection_identity() == selection && mount.selected_first_presentable(app))
            .then_some((selection, target))
    }

    pub fn new_window_disabled_reason(&self, app: &App) -> Option<String> {
        if self.ordinary_close_interaction_gated {
            return Some("This window is waiting for its draft and durable close state.".into());
        }
        if self.shutdown_interaction_gated {
            return Some(
                "Application Exit is waiting for active work and durable state.".to_owned(),
            );
        }
        if self.startup_interaction_gated() {
            return Some("Beryl is preparing its windows.".to_owned());
        }
        if self
            .controller
            .as_ref()
            .is_some_and(|controller| controller.is_threadless())
        {
            return Some("Add a runtime using the New Thread ellipsis segment before opening another window.".to_owned());
        }
        let Some(owner) = self.creation.as_ref().and_then(|owner| owner.upgrade()) else {
            return Some("New Window is not available.".to_owned());
        };
        let Some(controller) = self.controller.as_ref() else {
            return Some("The selected thread is unavailable.".to_owned());
        };
        if let Some(reason) = owner.read(app).disabled_reason(controller.window_id()) {
            return Some(reason.to_owned());
        }
        self.creation_target(app)
            .is_none()
            .then(|| "The selected thread is not ready for New Window.".to_owned())
    }

    pub(in crate::main_window) fn invoke_new_window(&mut self, cx: &mut Context<Self>) {
        if self.new_window_disabled_reason(cx).is_some() {
            return;
        }
        let Some(owner) = self.creation.as_ref().and_then(|owner| owner.upgrade()) else {
            return;
        };
        let Some((selection, target)) = self.creation_target(cx) else {
            return;
        };
        let source = cx.weak_entity();
        let _ = owner.update(cx, |owner, cx| {
            owner.activate_captured(source, selection, target, cx)
        });
    }

    #[cfg(test)]
    pub(crate) fn test_activate_new_window_command(&mut self, cx: &mut Context<Self>) {
        self.invoke_new_window(cx);
    }
}

impl Render for MainWindowShellRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_notices(window, cx);
        let Some(controller) = self.controller.as_ref() else {
            return div().id("main-window-shell-empty").into_any_element();
        };
        let appearance = &controller.appearance;
        let composer = controller.composer_mount();
        let content_height = composer
            .as_ref()
            .and_then(|mount| mount.read(cx).contribution())
            .and_then(|composer| {
                composer
                    .read(cx)
                    .gpui_input()
                    .read(cx)
                    .surface()
                    .map(|surface| surface.content_height())
            })
            .unwrap_or(px(0.));
        let minimum_panel_height = controller.minimum_size.height * 0.5;
        let composer_height = (content_height + px(22.))
            .max(minimum_panel_height)
            .min((window.viewport_size().height * 0.5).max(minimum_panel_height));
        let command = (self.creation.is_some() || controller.is_threadless())
            .then(|| crate::main_window::creation::command::render(self, &self.command_focus, cx));
        let exit = exit_command::render(self, window, cx);
        let input_panel = composer.map(|composer| {
            div()
                .id("main-window-user-input-panel")
                .debug_selector(|| "main-window-user-input-panel".to_owned())
                .flex()
                .h(composer_height)
                .min_h(minimum_panel_height)
                .px(px(12.))
                .py(px(10.))
                .border_1()
                .flex_none()
                .bg(appearance.input_panel)
                .border_color(appearance.separator)
                .child(composer)
        });
        div()
            .id("main-window-shell")
            .relative()
            .track_focus(&self.shell_focus)
            .key_context("MainWindow")
            .on_action(cx.listener(|root, _: &NewWindow, _, cx| root.invoke_new_window(cx)))
            .size_full()
            .flex()
            .flex_col()
            .bg(appearance.background)
            .child(
                div()
                    .id("main-window-toolbar")
                    .h(px(self.notice_chrome_height()))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(6.))
                    .px(px(12.))
                    .flex_none()
                    .bg(appearance.toolbar)
                    .children(command)
                    .child(exit),
            )
            .child(
                div()
                    .id("main-window-conversation-body")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id("main-window-transcript-region")
                            .debug_selector(|| "main-window-transcript-region".to_owned())
                            .flex_1()
                            .min_h_0(),
                    )
                    .children(input_panel),
            )
            .child(
                div()
                    .id("main-window-status-line")
                    .h(px(0.))
                    .flex_none()
                    .bg(appearance.status),
            )
            .child(self.notices.widget.clone())
            .into_any_element()
    }
}
