use super::*;

impl MainWindowShellRoot {
    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_thread_confirmation_workers_drained(&self) -> bool {
        self.running_threads.workers.retained() == 0
            && self.runtime_setup.workers.retained() == 0
            && self.running_threads.activation_task.is_none()
            && self
                .running_threads
                .activation_operation
                .as_ref()
                .is_some_and(|operation| {
                    !operation.active.load(Ordering::Acquire) && operation.source.try_lock().is_ok()
                })
    }
    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_thread_confirmation_failure_notice_retained(&self) -> bool {
        self.running_threads.failure_notice.is_some()
    }
    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_thread_confirmation_real_reconciliation_worker(&mut self) {
        assert!(!self.running_threads.has_activation_custody());
        self.running_threads.fixture_real_creation_reconciliation = true;
    }
    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_thread_confirmation_original_owner(&self) -> Option<(usize, usize)> {
        let operation = self
            .running_threads
            .activation_operation
            .as_ref()
            .filter(|operation| operation.creation)?;
        let lease = self.running_threads.selection_lease.as_ref()?;
        Some((
            Arc::as_ptr(&operation.source) as usize,
            Arc::as_ptr(lease) as usize,
        ))
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_thread_confirmation_original_outcome(&self) -> Option<&'static str> {
        let operation = self.running_threads.activation_operation.as_ref()?;
        let source = operation.source.try_lock().ok()?;
        Some(
            match source.creation.as_ref()?.operation.as_ref()?.outcome()? {
                crate::same_window_thread_acquisition::SameWindowThreadOutcome::Pending(_) => {
                    "Pending"
                }
                crate::same_window_thread_acquisition::SameWindowThreadOutcome::Unavailable(_) => {
                    "Unavailable"
                }
                crate::same_window_thread_acquisition::SameWindowThreadOutcome::Settled(_) => {
                    "Settled"
                }
                crate::same_window_thread_acquisition::SameWindowThreadOutcome::NotCommitted(_) => {
                    "NotCommitted"
                }
            },
        )
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_thread_confirmation_hooks(
        &mut self,
        commit: Option<FixtureHook>,
        save: Option<FixtureHook>,
        disposal: Option<FixtureHook>,
    ) {
        self.running_threads.fixture_activation_hooks = Some(RunningActivationFixtureHooks {
            commit,
            save,
            disposal,
        });
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_thread_confirmation_diagnostics(&self) -> Option<String> {
        self.running_threads
            .activation_operation
            .as_ref()
            .map(|operation| operation.diagnostics())
    }

    #[cfg(all(test, feature = "test-faults"))]
    pub(crate) fn test_suspend_thread_confirmation(&mut self, suspended: bool) {
        let operation = self.running_threads.activation_operation.as_ref().unwrap();
        if suspended {
            operation.suspend();
        } else {
            self.runtime_setup.unavailable = None;
            operation.resume();
        }
    }

    pub(in crate::main_window::shell::host) fn cancel_thread_confirmation(&mut self) {
        self.running_threads.activation_cancel.cancel();
    }

    pub(in crate::main_window::shell::host) fn start_thread_creation(
        &mut self,
        request: SameWindowThreadRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.running_threads.has_activation_custody() {
            return Err("This window already owns a thread selection request.".into());
        }
        let invoking = request.window_id();
        #[cfg(all(test, feature = "test-faults"))]
        let fixture = self
            .runtime_setup
            .fixture_transcript_reader
            .clone()
            .zip(self.runtime_setup.fixture_thread_lease.take());
        #[cfg(target_os = "windows")]
        let admit = || {
            let owner = crate::running_owner::RunningProcessOwner::mounted_owner(cx)
                .and_then(|owner| owner.upgrade())
                .ok_or("Thread creation process is unavailable")?;
            let owner = owner.borrow();
            Ok::<_, String>((
                owner
                    .thread_creation_reader()
                    .ok_or("Thread creation source is unavailable")?,
                Arc::new(owner.admit_thread_creation(invoking, cx)?),
            ))
        };
        #[cfg(all(test, feature = "test-faults", target_os = "windows"))]
        let (reader, lease) = match fixture {
            Some(fixture) => fixture,
            None => admit()?,
        };
        #[cfg(all(not(all(test, feature = "test-faults")), target_os = "windows"))]
        let (reader, lease) = admit()?;
        #[cfg(not(target_os = "windows"))]
        let (reader, lease) = {
            #[cfg(all(test, feature = "test-faults"))]
            {
                fixture.ok_or("Thread creation process is unavailable")?
            }
            #[cfg(not(all(test, feature = "test-faults")))]
            {
                return Err("Thread creation process is unavailable".into());
            }
        };
        if !reader.current() || lease.invoking() != invoking {
            return Err("Thread creation source or invoking window changed".into());
        }
        let mount = self
            .controller
            .as_ref()
            .and_then(|controller| controller.composer_mount.clone())
            .ok_or("Thread creation composer is unavailable")?;
        let service = mount.read(cx).claim_publication_service()?;
        self.preflight_running_selection(&service, cx)?;
        let prior = self
            .cached_running_selection(cx)
            .ok_or("Thread creation predecessor is unavailable")?
            .0;
        if request.selected() != Some(prior.claim()) || invoking != prior.window_id() {
            return Err("Thread creation predecessor changed".into());
        }
        let (assets, marker_seals) = mount.read(cx).claim_flush_resources()?;
        let settings = mount.read(cx).claim_autosave_settings();
        let panel = self.running_threads.transcript.clone();
        let request_id = self
            .running_threads
            .transcript_request
            .checked_add(1)
            .ok_or("Transcript request identity exhausted")?;
        self.running_threads.transcript_request = request_id;
        let transcript_request = TranscriptAttachmentRequest {
            window_id: invoking,
            host: panel.read(cx).lifetime(),
            activation: 1,
            thread_id: prior.claim().thread_id(),
            request_id,
            placement: TranscriptActivationPlacement::Tail,
            purpose: TranscriptAttachmentPurpose::Attach,
        };
        self.running_threads.reader = Some(reader.clone());
        self.running_threads.activation_cancel = beryl_home_store::CommandCancellation::new();
        self.running_threads.selection_lease = Some(lease.clone());
        #[cfg(all(test, feature = "test-faults"))]
        let mut hooks = self
            .running_threads
            .fixture_activation_hooks
            .take()
            .unwrap_or_default();
        self.suspend_running_thread_reads(window, cx);
        self.running_threads.activation_operation = Some(UnviewedRunningActivation {
            creation: true,
            source: Arc::new(Mutex::new(ActivationSource {
                stage: Stage::ThreadFence,
                reader,
                service,
                lease,
                owner: None,
                creation: Some(ThreadCreationSource {
                    request: Some(request),
                    operation: None,
                    current: None,
                    failure: None,
                    advance: None,
                    fenced: false,
                }),
                outcome: None,
                committed: None,
                home: None,
                state: None,
                syndic: None,
                prior,
                expected: prior,
                target: prior.claim(),
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
                provider: self.running_threads.transcript_provider.clone(),
                transcript: None,
                request: transcript_request,
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
                before_commit: hooks.commit.take(),
                #[cfg(all(test, feature = "test-faults"))]
                before_save: hooks.save.take(),
                #[cfg(all(test, feature = "test-faults"))]
                before_disposal: hooks.disposal.take(),
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
