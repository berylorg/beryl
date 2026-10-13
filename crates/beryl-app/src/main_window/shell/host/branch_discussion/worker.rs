use super::*;

impl MainWindowShellRoot {
    pub(super) fn qualify_captured_branch(
        &mut self,
        source: Entity<SyndicTranscriptPanel>,
        captured: DiscussionCreationRequested,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.running_read_capacity_available() {
            source.update(cx, |panel, cx| panel.accept_discussion_source_qualification(&captured, Err("Branch source is busy. Reopen this menu after current transcript work settles."), window, cx));
            return;
        }
        let Some(reader) = self.running_threads.reader.clone() else {
            return;
        };
        let Some((selection, _)) = self.cached_running_selection(cx) else {
            return;
        };
        if selection.claim().thread_id() != captured.captured.source.thread_id() {
            return;
        }
        let worker_reader = reader.clone();
        let worker_capture = captured.captured.clone();
        let work = self
            .running_threads
            .workers
            .track(move || worker_reader.qualify_captured_discussion(&worker_capture));
        let job = cx.background_executor().spawn(async move { work.run() });
        self.branch_discussion.qualification_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = job.await;
            let available = result.is_ok();
            let _ = this.update_in(cx, |root, window, cx| {
                if source != root.running_threads.transcript || root.running_threads.reads_suspended || root.running_threads.views_retired || !reader.current()
                    || root.cached_running_selection(cx).is_none_or(|(current, _)| current.claim() != selection.claim()) { return; }
                root.branch_discussion.qualification_task = None;
                source.update(cx, |panel, cx| panel.accept_discussion_source_qualification(&captured, if available { Ok(()) } else { Err("This selection has no current finalized assistant source from a terminal reply.") }, window, cx));
                root.sync_captured_branch_menu(cx);
            });
        }));
    }

    pub(super) fn run_captured_branch(
        &mut self,
        operation: Arc<Mutex<CapturedBranchOperation>>,
        reconcile: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.running_threads.reads_suspended || self.running_threads.views_retired {
            return;
        }
        if !self.running_read_capacity_available() {
            if reconcile {
                self.branch_discussion.task = Some(cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(250))
                        .await;
                    let _ = this.update_in(cx, |root, window, cx| {
                        root.branch_discussion.task = None;
                        root.run_captured_branch(operation, true, window, cx);
                    });
                }));
            } else {
                operation
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .outcome = Some(Err(
                    "Transcript work is busy; the discussion was not created.".into(),
                ));
                self.settle_captured_branch(operation, window, cx);
            }
            return;
        }
        let work = operation.clone();
        let work = self.running_threads.workers.track(move || {
            let (reader, captured, request, cancellation, audit) = {
                let operation = work.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                (
                    operation.reader.clone(),
                    operation.captured.captured.clone(),
                    operation.request,
                    operation.cancellation.clone(),
                    match &operation.outcome {
                        Some(Ok(DiscussionCreationOutcome::Indeterminate { audit, .. })) => {
                            Some(audit.clone())
                        }
                        _ => None,
                    },
                )
            };
            let Some(reader) = reader else {
                return;
            };
            if reconcile {
                let Some(audit) = audit else {
                    return;
                };
                #[cfg(all(test, feature = "test-faults"))]
                let hook = work.lock().unwrap().before_reconciliation.take();
                #[cfg(all(test, feature = "test-faults"))]
                if let Some(hook) = hook {
                    hook(audit.identity());
                }
                let result = reader.reconcile_captured_discussion(&audit);
                work.lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .resolution = Some(result);
            } else {
                #[cfg(all(test, feature = "test-faults"))]
                let hook = work.lock().unwrap().before_creation.take();
                let result = reader.create_captured_discussion(
                    &captured,
                    request,
                    cancellation,
                    #[cfg(all(test, feature = "test-faults"))]
                    hook,
                );
                work.lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .outcome = Some(result);
            }
        });
        let job = cx.background_executor().spawn(async move { work.run() });
        self.branch_discussion.task = Some(cx.spawn_in(window, async move |this, cx| {
            job.await;
            let _ = this.update_in(cx, |root, window, cx| {
                if root
                    .branch_discussion
                    .operation
                    .as_ref()
                    .is_none_or(|current| !Arc::ptr_eq(current, &operation))
                {
                    return;
                }
                root.branch_discussion.task = None;
                if root.running_threads.reads_suspended
                    || root.running_threads.views_retired
                    || root.shutdown_interaction_gated
                    || root.ordinary_close_interaction_gated
                    || root.startup_interaction_gated()
                    || root
                        .running_threads
                        .reader
                        .as_ref()
                        .is_none_or(|reader| !reader.current())
                {
                    return;
                }
                root.settle_captured_branch(operation, window, cx);
            });
        }));
    }

    pub(super) fn settle_captured_branch(
        &mut self,
        operation: Arc<Mutex<CapturedBranchOperation>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut state = operation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (stage, created, error) = if let Some(resolution) = state.resolution.take() {
            match resolution {
                Ok(DiscussionCreationAuditOutcome::Created(created)) => (BranchStage::Activating, Some(created), None),
                Ok(DiscussionCreationAuditOutcome::NotCommitted) => (BranchStage::NotCommitted, None, Some("The discussion was not created.".to_owned())),
                Ok(DiscussionCreationAuditOutcome::Collision) => (BranchStage::Unavailable, None, Some("Discussion creation is unavailable. The exact captured selection and outcome evidence are retained. Use established same-home recovery or bounded diagnostic reporting.".to_owned())),
                Ok(DiscussionCreationAuditOutcome::Pending) | Err(_) => (BranchStage::Reconciling, None, None),
            }
        } else {
            match state.outcome.as_ref() {
                Some(Ok(DiscussionCreationOutcome::Committed { created, .. })) => {
                    (BranchStage::Activating, Some(*created), None)
                }
                Some(Ok(DiscussionCreationOutcome::Indeterminate { .. })) => {
                    (BranchStage::Reconciling, None, None)
                }
                Some(Ok(DiscussionCreationOutcome::NotCommitted { evidence })) => {
                    (BranchStage::NotCommitted, None, Some(evidence.to_string()))
                }
                Some(Err(error)) => (BranchStage::NotCommitted, None, Some(error.clone())),
                None => return,
            }
        };
        state.stage = stage;
        state.created = created;
        state.error = error.clone();
        drop(state);
        self.publish_captured_branch_state(stage, cx);
        if matches!(stage, BranchStage::NotCommitted | BranchStage::Unavailable) {
            self.running_threads.transcript.update(cx, |panel, _| {
                panel.finish_captured_discussion_selection(false, window)
            });
        }
        if stage == BranchStage::NotCommitted {
            self.running_threads.transcript.update(cx, |panel, _| {
                panel.finish_captured_discussion_noncommit(window)
            });
        }
        if let Some(error) = error {
            self.report_captured_branch_failure(&error, window, cx);
        }
        match stage {
            BranchStage::Activating => self.activate_captured_branch(window, cx),
            BranchStage::Reconciling => {
                self.branch_discussion.task = Some(cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(250))
                        .await;
                    let _ = this.update_in(cx, |root, window, cx| {
                        root.branch_discussion.task = None;
                        root.run_captured_branch(operation, true, window, cx);
                    });
                }));
            }
            _ => {}
        }
    }
}
