use std::sync::{Condvar, Mutex, atomic::AtomicBool};

pub(crate) struct HeadGate {
    entered: AtomicBool,
    released: Mutex<bool>,
    changed: Condvar,
}

impl HeadGate {
    fn new() -> Self {
        Self {
            entered: AtomicBool::new(false),
            released: Mutex::new(false),
            changed: Condvar::new(),
        }
    }
    pub(crate) fn entered(&self) -> bool {
        self.entered.load(Ordering::Acquire)
    }
    pub(crate) fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.changed.notify_all();
    }
    pub(super) fn wait(&self, cancel: &ProjectionCancellationToken) {
        self.entered.store(true, Ordering::Release);
        let mut released = self.released.lock().unwrap();
        while !*released && !cancel.is_cancelled() {
            released = self
                .changed
                .wait_timeout(released, Duration::from_millis(10))
                .unwrap()
                .0;
        }
    }
}

impl MainWindowShellRoot {
    pub(crate) fn test_lineage_readiness(&self, app: &gpui::App) -> String {
        let lineage = &self.running_threads.lineage;
        format!(
            "cached_claim={:?}, selected_claim={:?}, selected_identity_current={}, observation_current={}, query_current={}, title={:?}, transcript_claim={:?}, transcript_source_present={}, head={:?}, head_pending={}, page_pending={}, observed={}, enabled={}, custody={}, suspended={}, workers={}, widget={}, pending_claim={}",
            self.cached_running_selection(app)
                .map(|(identity, _)| identity.claim()),
            lineage.selected.map(|identity| identity.claim()),
            self.cached_running_selection(app)
                .map(|(identity, _)| identity)
                == lineage.selected,
            lineage.observation.as_ref().is_some_and(|observed| {
                self.running_threads
                    .reader
                    .as_ref()
                    .is_some_and(|reader| reader.elect(observed, || ()).is_ok())
            }),
            lineage
                .widget
                .as_ref()
                .is_some_and(|widget| { self.lineage_current(widget.read(app).query(), app) }),
            lineage.title,
            self.running_threads.transcript_claim,
            self.running_threads.transcript_source.is_some(),
            lineage.head,
            lineage.head_job.is_some(),
            lineage.page_jobs.len(),
            lineage.observation.is_some(),
            self.running_threads_enabled(),
            self.running_threads.has_activation_custody(),
            self.running_threads.reads_suspended,
            self.running_threads.workers.retained(),
            lineage.widget.as_ref().map_or_else(
                || "absent".into(),
                |widget| widget.read(app).test_readiness()
            ),
            self.controller
                .as_ref()
                .and_then(|controller| controller.composer_mount())
                .map_or_else(
                    || "absent".into(),
                    |mount| mount.read(app).test_claim_pending_state(app)
                ),
        )
    }
    pub(crate) fn test_hold_lineage_head(&mut self) -> Arc<HeadGate> {
        let gate = Arc::new(HeadGate::new());
        self.running_threads.lineage.head_gate = Some(gate.clone());
        gate
    }
    pub(crate) fn test_thread_lineage_view(&self) -> Option<Entity<ThreadLineage>> {
        self.running_threads.lineage.widget.clone()
    }
    pub(crate) fn test_thread_lineage_focus(
        &self,
        selected: beryl_model::SyndicThreadId,
        parent: beryl_model::SyndicThreadId,
        window: &Window,
        app: &gpui::App,
    ) -> bool {
        self.running_threads
            .lineage
            .widget
            .as_ref()
            .is_some_and(|widget| {
                let widget = widget.read(app);
                widget.query().selected == selected && widget.test_logical_focus(parent, window)
            })
    }
}
