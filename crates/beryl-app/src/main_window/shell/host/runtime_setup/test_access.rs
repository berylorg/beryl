use super::*;

impl MainWindowShellRoot {
    pub(crate) fn test_observe_selected_disposal_execution(
        &self,
        expected: crate::main_window::MainWindowComposerSelectionIdentity,
        observation: crate::composer_host::ComposerHostDisposalExecutionObservation,
        app: &App,
    ) -> Result<(), String> {
        let controller = self
            .controller
            .as_ref()
            .ok_or("original controller is missing")?;
        if controller.window_id() != expected.window_id() {
            return Err("original disposal observation window differs".into());
        }
        let mount = controller
            .composer_mount
            .as_ref()
            .ok_or("original mount is missing")?
            .read(app);
        let resident = mount.contribution().ok_or("original editor is missing")?;
        if resident.read(app).selection_identity() != expected {
            return Err("original disposal observation selection differs".into());
        }
        mount
            .claim_publication_service()?
            .test_observe_selected_disposal_execution(expected, observation)
    }

    #[cfg(feature = "test-faults")]
    pub(crate) fn test_after_selected_publication_execute(
        &self,
        expected: crate::main_window::MainWindowComposerSelectionIdentity,
        hook: Box<
            dyn FnOnce(&beryl_home_store::HomeStore, &beryl_home_store::CommandOutcome) + Send,
        >,
        app: &App,
    ) -> Result<(), String> {
        let controller = self
            .controller
            .as_ref()
            .ok_or("original controller is missing")?;
        if controller.window_id() != expected.window_id() {
            return Err("original publication outcome window differs".into());
        }
        let mount = controller
            .composer_mount
            .as_ref()
            .ok_or("original mount is missing")?
            .read(app);
        let resident = mount.contribution().ok_or("original editor is missing")?;
        if resident.read(app).selection_identity() != expected {
            return Err("original publication outcome selection differs".into());
        }
        mount
            .claim_publication_service()?
            .test_after_selected_publication_execute(expected, hook)
    }
    pub(crate) fn test_observe_selected_publication_execution(
        &self,
        expected: crate::main_window::MainWindowComposerSelectionIdentity,
        observation: crate::composer_host::ComposerHostPublicationExecutionObservation,
        app: &App,
    ) -> Result<(), String> {
        let controller = self
            .controller
            .as_ref()
            .ok_or("original controller is missing")?;
        if controller.window_id() != expected.window_id() {
            return Err("original publication observation window differs".into());
        }
        let mount = controller
            .composer_mount
            .as_ref()
            .ok_or("original mount is missing")?;
        let mount = mount.read(app);
        let resident = mount.contribution().ok_or("original editor is missing")?;
        if resident.read(app).selection_identity() != expected {
            return Err("original publication observation selection differs".into());
        }
        mount
            .claim_publication_service()?
            .test_observe_selected_publication_execution(expected, observation)
    }

    pub(crate) fn test_running_transcript_snapshot(
        &self,
        app: &App,
    ) -> Arc<crate::syndic_transcript::ResidentTranscriptSnapshot> {
        self.running_threads.transcript.read(app).snapshot()
    }

    pub(crate) fn test_primary_thread_command(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_primary_thread(window, cx);
    }

    pub(crate) fn test_primary_thread_reason(&self, cx: &App) -> Option<String> {
        self.primary_thread_reason(cx)
    }

    pub(crate) fn test_new_thread_focus(&self, primary: bool) -> gpui::FocusHandle {
        if primary {
            self.runtime_setup.primary_focus.clone()
        } else {
            self.runtime_setup.focus.clone()
        }
    }

    pub(crate) fn test_thread_confirmation_visible_transcript_claim(
        &self,
    ) -> Option<beryl_state::WindowClaimSelection> {
        self.running_threads.transcript_claim
    }
    pub(crate) fn test_thread_confirmation_visible_identity(
        &self,
        app: &App,
    ) -> (gpui::EntityId, usize, Option<beryl_state::RememberedTarget>) {
        let controller = self.controller.as_ref().unwrap();
        let remembered = match &controller.content {
            ShellContent::Selected { window, .. } => window.remembered_target(),
            ShellContent::Acquired { custody, .. } => Some(custody.acquisition.target()),
            ShellContent::Restored { custody, .. } => {
                custody.composer.recovery_window().remembered_target()
            }
            ShellContent::Threadless { source, .. } => source.recovery_window().remembered_target(),
            ShellContent::RecoveredThreadless { source, .. } => source.window().remembered_target(),
            ShellContent::Retired { .. } => None,
        };
        let transcript = self.running_threads.transcript.read(app).snapshot();
        (
            controller.composer_mount.as_ref().unwrap().entity_id(),
            Arc::as_ptr(&transcript) as usize,
            remembered,
        )
    }
    pub(crate) fn test_bound_running_window_command(
        &self,
    ) -> Option<crate::startup_owner::RunningWindowExit> {
        self.running_command.clone()
    }
    pub(crate) fn test_thread_creation_picker_open_diagnostics(&self) -> String {
        format!(
            "enabled={}, pending={}, picker={}, unavailable={:?}, suspended={}, retired={}, shutdown_gated={}, activation_pending={}, services_current={}",
            self.setup_enabled(),
            self.runtime_setup.pending(),
            self.runtime_setup.picker.is_some(),
            self.runtime_setup.unavailable,
            self.runtime_setup.suspended,
            self.runtime_setup.retired,
            self.shutdown_interaction_gated,
            self.running_threads.pending_activation.is_some(),
            self.runtime_setup
                .services
                .as_ref()
                .is_some_and(|services| services.current())
        )
    }

    pub(crate) fn test_reject_thread_creation_recovery_mount(&mut self) {
        self.running_threads.fixture_reject_creation_recovery_mount = true;
    }

    pub(crate) fn test_thread_creation_recovery_mount(
        &self,
        app: &App,
    ) -> Option<(
        gpui::EntityId,
        crate::main_window::MainWindowComposerSelectionIdentity,
        bool,
    )> {
        let mount = self.controller.as_ref()?.composer_mount.as_ref()?;
        Some((
            mount.entity_id(),
            mount.read(app).fresh_recovery_ticket()?.selection(),
            self.shutdown_interaction_gated,
        ))
    }
    pub(crate) fn test_thread_creation_composer_adopted_custody_items(&self, cx: &App) -> usize {
        self.controller
            .as_ref()
            .and_then(|controller| controller.composer_mount.as_ref())
            .and_then(|mount| mount.read(cx).contribution())
            .map_or(0, |resident| {
                resident
                    .read(cx)
                    .gpui_input()
                    .read(cx)
                    .realization_diagnostics()
                    .adopted_custody_items
            })
    }
    pub(crate) fn test_thread_creation_composer_selection(
        &self,
        cx: &App,
    ) -> Option<gpui_text_input::RangeSourceSelection> {
        let composer = self
            .controller
            .as_ref()?
            .composer_mount
            .as_ref()?
            .read(cx)
            .contribution()?;
        let input = composer.read(cx).gpui_input();
        input.read(cx).surface().map(|surface| surface.selection())
    }
    pub(crate) fn test_thread_creation_recovery_widget_release_counts(&self) -> (usize, usize) {
        self.running_threads.fixture_creation_release_counts
    }
    pub(crate) fn test_thread_creation_prepublication_page_release_acknowledgements(
        &self,
    ) -> usize {
        self.running_threads
            .fixture_creation_page_release_acknowledgements
    }
    pub(crate) fn test_thread_confirmation_lease(
        &mut self,
        lease: Arc<crate::window_acquisition::WindowSelectionLease>,
    ) {
        self.runtime_setup.fixture_thread_lease = Some(lease);
    }

    pub(crate) fn test_thread_confirmation_command(
        &mut self,
        key: PickerRowKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.setup_command(PickerCommand::Confirm(key), window, cx);
    }
    pub(crate) fn test_runtime_setup_query(&self) -> &str {
        self.runtime_setup.query.as_str()
    }
    pub(crate) fn test_runtime_setup_scope(&self) -> Option<beryl_model::RuntimeId> {
        self.runtime_setup.scope
    }
    pub(crate) fn test_runtime_setup_admission(
        &mut self,
        flight: Arc<RuntimeSetupFlight>,
        fail_root_page: Arc<std::sync::atomic::AtomicBool>,
    ) {
        self.runtime_setup.fixture_admission = Some(flight);
        self.runtime_setup.fixture_fail_root_page = Some(fail_root_page);
    }
    pub(crate) fn test_runtime_setup_page_delivery(
        &mut self,
        delivery: Option<Arc<dyn Fn(bool, u64) + Send + Sync>>,
    ) {
        self.runtime_setup.fixture_page_delivery = delivery;
    }

    pub(crate) fn test_runtime_setup_bootstrap(&self) -> (u64, bool, bool) {
        (
            self.runtime_setup.query_revision,
            self.runtime_setup.bootstrap_root_page.is_some(),
            self.runtime_setup.bootstrap_runtime_page.is_some(),
        )
    }

    pub(crate) fn test_runtime_setup_page_jobs(&self) -> usize {
        self.runtime_setup.page_jobs.len()
    }
    pub(crate) fn test_runtime_setup_fixture(
        &mut self,
        services: PublishedRuntimeSetupServices,
        members: Vec<beryl_model::WindowId>,
        reader: Option<crate::app_services::PublishedRunningThreadsReader>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.runtime_setup.fixture_services = Some(services.clone());
        self.runtime_setup.services = Some(services);
        self.runtime_setup.fixture_members = members;
        self.runtime_setup.fixture_native = true;
        self.runtime_setup.fixture_transcript_reader = reader;
        self.sync_runtime_setup(window, cx);
    }

    pub(crate) fn test_open_runtime_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_runtime_setup(window, cx);
    }

    pub(crate) fn test_runtime_setup_picker(&self) -> Option<Entity<ThreadRootPicker>> {
        self.runtime_setup.picker.clone()
    }

    pub(crate) fn test_runtime_setup_path_prompt(&self) -> Option<gpui::PathPromptOptions> {
        self.runtime_setup
            .fixture_path_prompt
            .as_ref()
            .map(|(options, _)| options.clone())
    }

    pub(crate) fn test_complete_runtime_setup_path(
        &mut self,
        paths: Option<Vec<std::path::PathBuf>>,
    ) {
        let (_, sender) = self
            .runtime_setup
            .fixture_path_prompt
            .take()
            .expect("exact native path request");
        sender.send(Ok(paths)).expect("live native path request");
    }

    pub(crate) fn test_fail_runtime_setup_path(&mut self, error: &str) {
        let (_, sender) = self
            .runtime_setup
            .fixture_path_prompt
            .take()
            .expect("exact native path request");
        sender
            .send(Err(error.to_owned()))
            .expect("live native path request");
    }

    pub(crate) fn test_attach_runtime_setup_flight(
        &mut self,
        flight: Arc<RuntimeSetupFlight>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        assert!(self.runtime_setup.flight.is_none());
        self.runtime_setup.flight = Some(flight);
        self.runtime_setup.command = Some(PickerCommand::AddRuntime);
        self.start_setup_poll(window, cx);
    }

    pub(crate) fn test_runtime_setup_state(&self) -> (bool, bool, bool, usize, usize) {
        (
            self.runtime_setup.pending(),
            self.runtime_setup.unavailable.is_some(),
            self.runtime_setup.mount.is_some(),
            self.runtime_setup.roots.len(),
            self.runtime_setup.runtimes.len(),
        )
    }

    pub(crate) fn test_runtime_setup_notice_persistent(&self) -> bool {
        self.runtime_setup.failure_notice.is_some()
    }

    pub(crate) fn test_hold_runtime_setup_mount_worker(
        &self,
        cx: &App,
    ) -> Box<dyn FnOnce() + Send> {
        self.runtime_setup
            .mount
            .as_ref()
            .expect("original staged first mount")
            .read(cx)
            .test_window_close_completion(|| ())
    }

    pub(crate) fn test_runtime_setup_root_scope(
        &mut self,
        scope: Option<beryl_model::RuntimeId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.setup_scope(scope, window, cx);
    }
}
