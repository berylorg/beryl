use super::*;
use gpui::prelude::FluentBuilder;

impl MainWindowShellRoot {
    pub(super) fn selected_history_loading(&self, app: &App) -> bool {
        self.cached_running_selection(app)
            .is_some_and(|(selection, _)| {
                self.running_threads.transcript_claim != Some(selection.claim())
            })
    }

    #[cfg(test)]
    pub(crate) fn test_progressive_readiness(&self, app: &App) -> (bool, bool) {
        (
            self.thread_switcher.catalog_ready(),
            !self.selected_history_loading(app),
        )
    }

    #[cfg(test)]
    pub(crate) fn test_suspend_code_saves_for_close(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ordinary_close_interaction_gated = true;
        self.suspend_running_thread_reads(window, cx);
    }

    #[cfg(test)]
    pub(crate) fn test_resume_code_saves_after_close(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ordinary_close_interaction_gated = false;
        self.resume_running_thread_reads(window, cx);
    }
    #[cfg(test)]
    pub(crate) fn test_hold_indexed_code_source_read(
        &mut self,
        revision: crate::syndic_transcript::ProviderRevision,
    ) -> Arc<crate::transcript_provider::IndexedReadGate> {
        let gate = crate::transcript_provider::IndexedReadGate::source(revision);
        self.running_threads
            .transcript_provider
            .as_mut()
            .unwrap()
            .set_indexed_read_gate_for_test(gate.clone());
        gate
    }
    #[cfg(test)]
    pub(crate) fn test_hold_indexed_code_read(
        &mut self,
        revision: crate::syndic_transcript::ProviderRevision,
    ) -> Arc<crate::transcript_provider::IndexedReadGate> {
        let gate = crate::transcript_provider::IndexedReadGate::new(revision);
        self.running_threads
            .transcript_provider
            .as_mut()
            .unwrap()
            .set_indexed_read_gate_for_test(gate.clone());
        gate
    }

    pub(super) fn synchronize_transcript_appearance(&self, cx: &mut Context<Self>) {
        if let Some(controller) = &self.controller {
            let appearance = controller.appearance.generation.clone();
            self.running_threads
                .transcript
                .update(cx, |panel, _| panel.set_appearance(appearance));
        }
    }

    #[cfg(test)]
    pub(crate) fn test_transcript_panel(
        &self,
    ) -> Entity<crate::syndic_transcript::SyndicTranscriptPanel> {
        self.running_threads.transcript.clone()
    }

    pub(super) fn transcript_attachment_allocation(
        &self,
        mount: Option<&Entity<MainWindowConversationComposerMount>>,
        lineage_present: bool,
        window: &Window,
        cx: &App,
    ) -> gpui::Size<gpui::Pixels> {
        let viewport = window.viewport_size();
        let mut metrics = self
            .controller
            .as_ref()
            .expect("mounted shell")
            .metrics
            .clone();
        metrics.lineage = lineage_present;
        if let Some(panel) = mount.and_then(|mount| mount.read(cx).panel_minimum_size(cx)) {
            metrics.panel = panel;
        }
        let input_height = mount
            .map(|mount| {
                metrics.composer_height(
                    mount
                        .read(cx)
                        .presentation_content_height(cx)
                        .unwrap_or(px(0.0)),
                    viewport.height,
                )
            })
            .unwrap_or(px(0.0));
        let lineage_height = if lineage_present {
            px(crate::thread_lineage::THREAD_LINEAGE_HEIGHT_PX)
        } else {
            px(0.0)
        };
        gpui::size(
            viewport.width,
            (viewport.height
                - px(self.notice_chrome_height())
                - px(status_controls::STATUS_STRIP_HEIGHT_PX)
                - input_height
                - lineage_height)
                .max(px(0.0)),
        )
    }

    pub(super) fn new(
        controller: MainWindowShellController,
        construction_error: Option<String>,
        publication: Arc<crate::theme_runtime::GpuiAppearancePublicationTarget>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let minimum_protection = controller.minimum_size;
        let shell_focus = cx.focus_handle();
        let notices =
            notices::MainWindowShellNotices::new(&controller, publication, shell_focus.clone(), cx);
        let mut root = Self {
            thread_switcher: thread_switcher::ThreadSwitcherContribution::new(cx),
            thread_navigation: thread_navigation::ThreadNavigationControls::new(cx),
            runtime_setup: runtime_setup::RuntimeSetupContribution::new(cx),
            running_threads: running_threads::RunningThreadsContribution::new(cx),
            status_controls: status_controls::ExactStatusControls::new(cx),
            model_controls: model_controls::ModelControls::new(cx),
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
            minimum_flight: None,
            minimum_protection,
            minimum_viewport: window.viewport_size(),
            appearance_registration: Rc::new(std::cell::Cell::new(false)),
            command_focus: cx.focus_handle(),
            exit_focus: cx.focus_handle(),
            shell_focus,
            notices,
            home_warning_startup: None,
            transcript_image_preview: None,
            branch_discussion: branch_discussion::BranchDiscussionContribution::default(),
        };
        root.subscribe_notices(window, cx);
        cx.observe_window_bounds(window, |root, window, cx| {
            root.minimum_viewport = window.viewport_size();
            root.observe_minimum_restore(window);
            cx.notify();
        })
        .detach();
        root.subscribe_quote(window, cx);
        cx.observe_window_activation(window, |root, window, cx| {
            root.observe_minimum_restore(window);
            #[cfg(target_os = "windows")]
            crate::parent_completion_sound::refresh_focus(cx);
            cx.notify();
        })
        .detach();
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
        self.coherent_viewed_thread(app)
    }

    pub(crate) fn coherent_viewed_thread(&self, app: &App) -> Option<beryl_model::SyndicThreadId> {
        self.cached_running_selection(app)
            .map(|(selection, _)| selection.claim().thread_id())
    }

    pub(in crate::main_window::shell::host) fn cached_running_selection(
        &self,
        app: &App,
    ) -> Option<(
        crate::main_window::MainWindowComposerSelectionIdentity,
        beryl_state::RememberedTarget,
    )> {
        let controller = self.controller.as_ref()?;
        let (selection, target) = match &controller.content {
            ShellContent::Selected {
                window, selection, ..
            } => (*selection, window.remembered_target()?),
            ShellContent::Acquired { custody, selection } => {
                (*selection, custody.acquisition.target())
            }
            ShellContent::Restored { custody, selection } => {
                (*selection, custody.composer.target())
            }
            _ => return None,
        };
        let composer = controller
            .composer_mount
            .as_ref()?
            .read(app)
            .contribution()?;
        let current = composer.read(app).selection_identity();
        (composer.read(app).is_live()
            && current.window_id() == selection.window_id()
            && current.claim() == selection.claim())
        .then_some((current, target))
    }

    pub(in crate::main_window) fn creation_target(
        &self,
        app: &App,
    ) -> Option<(
        crate::main_window::MainWindowComposerSelectionIdentity,
        beryl_state::RememberedTarget,
    )> {
        if self.running_selection_interaction_gated(app) {
            return None;
        }
        let controller = self.controller.as_ref()?;
        let (selection, target) = match &controller.content {
            ShellContent::Selected {
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
        if self.running_selection_interaction_gated(app) {
            return Some("A running thread is being opened in this window.".into());
        }
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
        #[cfg(target_os = "windows")]
        crate::parent_completion_sound::refresh_focus(cx);
        self.synchronize_transcript_appearance(cx);
        let _ = self.drive_recovered_transcript(window, cx);
        self.sync_status_controls(window, cx);
        self.sync_model_controls(window, cx);
        self.sync_running_threads(window, cx);
        self.sync_captured_branch(window, cx);
        self.sync_runtime_setup(window, cx);
        self.sync_thread_switcher(window, cx);
        self.sync_notices(window, cx);
        self.sync_historical_record_dimming(cx);
        let history_loading = self.selected_history_loading(cx);
        self.running_threads
            .transcript
            .update(cx, |panel, panel_cx| {
                panel.set_initial_history_loading(history_loading, window, panel_cx);
            });
        let image_preview = self.render_transcript_image_preview(window, cx);
        let lineage = self.render_selected_lineage(cx);
        if let Some(controller) = self.controller.as_ref() {
            let panel = if controller.composer_mount.is_some() || self.runtime_setup.mount.is_some()
            {
                controller.metrics.panel
            } else {
                gpui::size(px(0.), px(0.))
            };
            let candidate = self.candidate_metrics(
                &controller.appearance.generation,
                panel,
                lineage.is_some(),
                cx,
            );
            if let Ok(candidate) = candidate
                && candidate != controller.metrics
            {
                if let Ok(true) = self.ensure_minimum(candidate.clone(), window, cx) {
                    self.commit_metrics(candidate, window, cx);
                }
            }
        }
        let Some(controller) = self.controller.as_ref() else {
            return div().id("main-window-shell-empty").into_any_element();
        };
        let quote_colors = super::model_controls::menu_colors(self);
        let quote_menu = self
            .running_threads
            .transcript
            .clone()
            .update(cx, |panel, cx| {
                panel.render_quote_menu(quote_colors, window, cx)
            });
        let appearance = &controller.appearance;
        let composer = controller
            .composer_mount()
            .or_else(|| self.runtime_setup.mount.clone());
        let content_height = composer
            .as_ref()
            .and_then(|mount| mount.read(cx).presentation_content_height(cx))
            .unwrap_or(px(0.));
        let minimum_panel_height = controller.metrics.panel.height;
        let composer_height = controller
            .metrics
            .composer_height(content_height, window.viewport_size().height);
        let command = controller
            .metrics
            .new_window
            .then(|| crate::main_window::creation::command::render(self, &self.command_focus, cx));
        let exit = exit_command::render(self, window, cx);
        let running = running_threads::render_command(self, window, cx);
        let setup = runtime_setup::render_command(self, window, cx);
        let setup_picker = runtime_setup::render_picker(self, window);
        let running_picker = running_threads::render_picker(self, window);
        let switcher = thread_switcher::render_command(self, window, cx);
        let back = thread_navigation::render(self, false, cx);
        let forward = thread_navigation::render(self, true, cx);
        let switcher_picker = thread_switcher::render_picker(self, window);
        let status = status_controls::render_strip(self, cx);
        let stop_menu = status_controls::render_menu(self, window, cx);
        let model_menu = model_controls::render_menu(self, window, cx);
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
                    .debug_selector(|| "main-window-toolbar".to_owned())
                    .gap(px(sizing::TOOLBAR_GAP))
                    .px(px(sizing::TOOLBAR_PADDING))
                    .flex_none()
                    .bg(appearance.toolbar)
                    .child(setup)
                    .child(back)
                    .child(forward)
                    .child(switcher)
                    .child(running)
                    .children(command)
                    .child(exit),
            )
            .children(if controller.metrics.lineage {
                lineage
            } else {
                None
            })
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
                            .min_h_0()
                            .when(history_loading, |region| region.opacity(0.55))
                            .child(self.running_threads.transcript.clone()),
                    )
                    .children(input_panel),
            )
            .child(status)
            .children(stop_menu)
            .children(model_menu)
            .children(quote_menu)
            .children(running_picker)
            .children(setup_picker)
            .children(switcher_picker)
            .child(self.notices.widget.clone())
            .children(image_preview)
            .into_any_element()
    }
}
