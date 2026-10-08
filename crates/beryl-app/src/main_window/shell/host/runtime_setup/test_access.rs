use super::*;

impl MainWindowShellRoot {
    pub(crate) fn test_runtime_setup_query(&self) -> &str {
        self.runtime_setup.query.as_str()
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
