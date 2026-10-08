use super::*;

impl MainWindowShellRoot {
    pub(in crate::main_window::shell::host) fn start_unviewed_running_activation(
        &mut self,
        reader: PublishedRunningThreadsReader,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.running_threads.activation_cancel.is_cancelled() || !self.running_threads_enabled()
        {
            return Err("Thread activation cancelled".into());
        }
        let owner = self
            .running_threads
            .prepared_activation
            .take()
            .ok_or("Thread selection preparation is missing")?;
        let target = owner.future_selection();
        let invoking = owner.future_window().window_id();
        #[cfg(all(test, feature = "test-faults"))]
        let fixture_lease = if self.running_threads.fixture_reader.is_some() {
            Some(
                self.running_threads
                    .selection_lease
                    .take()
                    .ok_or("Fixture selection lease is missing")?,
            )
        } else {
            None
        };
        #[cfg(target_os = "windows")]
        let admit = || {
            Ok::<_, String>(Arc::new(
                crate::running_owner::RunningProcessOwner::mounted_owner(cx)
                    .and_then(|owner| owner.upgrade())
                    .ok_or("Running threads process owner is unavailable")?
                    .borrow()
                    .admit_running_selection(invoking, cx)?,
            ))
        };
        #[cfg(all(target_os = "windows", test, feature = "test-faults"))]
        let lease = match fixture_lease {
            Some(lease) => lease,
            None => admit()?,
        };
        #[cfg(all(target_os = "windows", not(all(test, feature = "test-faults"))))]
        let lease = admit()?;
        #[cfg(not(target_os = "windows"))]
        let lease = self
            .running_threads
            .selection_lease
            .clone()
            .ok_or("Running threads process owner is unavailable")?;
        if lease.invoking() != owner.future_window().window_id() {
            return Err("Selection lease belongs to another window".into());
        }
        let mount = self
            .controller
            .as_ref()
            .and_then(|controller| controller.composer_mount.clone())
            .ok_or("Running threads composer is unavailable")?;
        let service = mount.read(cx).claim_publication_service()?;
        self.preflight_running_selection(&service, cx)?;
        let (assets, marker_seals) = mount.read(cx).claim_flush_resources()?;
        let settings = mount.read(cx).claim_autosave_settings();
        let prior = self
            .cached_running_selection(cx)
            .ok_or("Running threads prior selection is missing")?
            .0;
        self.suspend_running_thread_reads(window, cx);
        let provider = self.running_threads.transcript_provider.clone();
        let panel = self.running_threads.transcript.clone();
        let request_id = self
            .running_threads
            .transcript_request
            .checked_add(1)
            .ok_or("Transcript request identity exhausted")?;
        self.running_threads.transcript_request = request_id;
        let request = TranscriptAttachmentRequest {
            window_id: invoking,
            host: panel.read(cx).lifetime(),
            activation: 1,
            thread_id: target.thread_id(),
            request_id,
            placement: TranscriptActivationPlacement::Tail,
            purpose: TranscriptAttachmentPurpose::Attach,
        };
        self.running_threads.selection_lease = Some(lease.clone());
        #[cfg(all(test, feature = "test-faults"))]
        let mut fixture_hooks = if self.running_threads.fixture_reader.is_some() {
            self.running_threads
                .fixture_activation_hooks
                .take()
                .unwrap_or_default()
        } else {
            RunningActivationFixtureHooks::default()
        };
        self.running_threads.activation_operation = Some(UnviewedRunningActivation {
            creation: false,
            source: Arc::new(Mutex::new(ActivationSource {
                stage: Stage::Begin,
                reader,
                service,
                lease,
                owner: Some(owner),
                creation: None,
                outcome: None,
                committed: None,
                home: None,
                state: None,
                syndic: None,
                prior,
                expected: prior,
                target,
                receipt: None,
                presentation: None,
                pending_selection: None,
                advance: None,
                flush: None,
                flush_settled: false,
                widget_work: None,
                release: None,
                publication: None,
                completed_predecessor: None,
                completed_successor: None,
                completed_successor_progress: None,
                autosave: None,
                settings,
                assets,
                marker_seals,
                provider,
                transcript: None,
                request,
                transcript_cancel: AtomicBool::new(false),
                cancellation: self.running_threads.activation_cancel.clone(),
                error: None,
                installed: false,
                retired: false,
                gui_published: false,
                terminal_release_failure: false,
                transcript_retries: 0,
                source_retry_at: None,
                #[cfg(all(test, feature = "test-faults"))]
                before_commit: fixture_hooks.commit.take(),
                #[cfg(all(test, feature = "test-faults"))]
                before_save: fixture_hooks.save.take(),
                #[cfg(all(test, feature = "test-faults"))]
                before_disposal: fixture_hooks.disposal.take(),
            })),
            active: Arc::new(AtomicBool::new(false)),
            suspended: Arc::new(AtomicBool::new(false)),
            mount,
            panel,
        });
        self.resume_running_activation(window, cx);
        Ok(())
    }
}
