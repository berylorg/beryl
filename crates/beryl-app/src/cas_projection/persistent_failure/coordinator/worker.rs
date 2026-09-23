use super::*;

pub(super) struct WorkerExitSignal(
    pub(super) Arc<(Mutex<CoordinatorState>, Condvar)>,
    pub(super) Arc<crate::cas_projection::outage_buffer::OutageInventory>,
);

impl Drop for WorkerExitSignal {
    fn drop(&mut self) {
        let mut state = self.0.0.lock().unwrap_or_else(|poison| poison.into_inner());
        state.worker_exited = true;
        if matches!(
            state.phase,
            PersistentFailureCutState::Armed | PersistentFailureCutState::Cutting
        ) {
            self.1.retire();
            state.phase = if std::thread::panicking() {
                PersistentFailureCutState::Incomplete
            } else {
                PersistentFailureCutState::Stopped
            };
        }
        self.0.1.notify_all();
    }
}

impl Drop for PersistentFailureCoordinator {
    fn drop(&mut self) {
        self.stop_requested.store(true, Ordering::Release);
        self.notification.wake_worker();
        // Explicit service shutdown owns the join. An implicit drop can run while
        // another teardown owner is still unwinding, so it may only request stop
        // and detach the worker.
        let _ = self
            .handle
            .get_mut()
            .unwrap_or_else(|poison| poison.into_inner())
            .take();
    }
}

pub(super) fn run_worker(receiver: mpsc::Receiver<()>, context: WorkerContext) {
    while receiver.recv().is_ok() {
        if context.stop_requested.load(Ordering::Acquire) {
            finish_worker(
                &context,
                PersistentFailureCutState::Stopped,
                None,
                CutResultCounts::default(),
            );
            return;
        }
        if !context.notification.failure_observed() {
            continue;
        }
        let identity = PersistentFailureCutIdentity::new(
            context.home_id,
            context.home_generation,
            context.service_generation,
            PersistentFailureGeneration::FIRST,
        );
        {
            let mut state = context
                .state
                .0
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            if state.phase != PersistentFailureCutState::Armed {
                continue;
            }
            state.phase = PersistentFailureCutState::Cutting;
            state.failure_generation = Some(identity.failure_generation);
            context.state.1.notify_all();
        }
        if !context
            .gate
            .close_for_persistent_failure(identity.failure_generation)
            .unwrap_or(false)
        {
            finish_worker(
                &context,
                PersistentFailureCutState::Stopped,
                Some(identity.failure_generation),
                CutResultCounts::default(),
            );
            return;
        }
        context.notification.mark_cut_elected();
        let stop_freeze_failed = context
            .stop_coordinator
            .freeze_for_persistent_failure(identity)
            .is_err();
        let drain_failed = context.gate.wait_until_drained().is_err();
        if stop_freeze_failed || drain_failed {
            finish_worker(
                &context,
                PersistentFailureCutState::Incomplete,
                Some(identity.failure_generation),
                CutResultCounts::default(),
            );
            return;
        }
        let Ok(results) =
            freeze_and_dispatch_targets(identity, &context.connections, &context.outage_inventory)
        else {
            finish_worker(
                &context,
                PersistentFailureCutState::Incomplete,
                Some(identity.failure_generation),
                CutResultCounts::default(),
            );
            return;
        };
        finish_worker(
            &context,
            PersistentFailureCutState::Finished,
            Some(identity.failure_generation),
            results,
        );
        return;
    }
    finish_worker(
        &context,
        PersistentFailureCutState::Stopped,
        None,
        CutResultCounts::default(),
    );
}

fn freeze_and_dispatch_targets(
    identity: PersistentFailureCutIdentity,
    connections: &crate::cas_projection::service_registry::ProjectionServiceConnectionRegistry,
    outage_inventory: &crate::cas_projection::outage_buffer::OutageInventory,
) -> Result<CutResultCounts, ()> {
    let mut frozen = Vec::new();
    let mut results = CutResultCounts::default();
    connections
        .visit_connections::<crate::cas_projection::ConnectionWorkError>(|connection| {
            let workers = connection.retain_persistent_failure_workers();
            let batch = connection
                .freeze_original_failure_targets(identity, workers.as_ref())
                .map_err(|_| crate::cas_projection::ConnectionWorkError::SourceUnavailable)?;
            if let Some(batch) = batch {
                if let Some(workers) = workers {
                    frozen.push((Arc::clone(connection), workers, batch));
                } else {
                    for candidate in batch.into_candidates() {
                        let (_, proof) = candidate.into_parts();
                        results.record(PersistentFailureDriverResult::NoDispatch(match proof {
                            Ok(_) => PersistentFailureNoDispatchReason::DriverUnavailable,
                            Err(reason) => PersistentFailureNoDispatchReason::Router(reason),
                        }));
                    }
                }
            }
            Ok(())
        })
        .map_err(|_| ())?;
    let _ = outage_inventory.publish_frozen(
        identity,
        frozen.iter().flat_map(|(_, _, batch)| {
            batch.witnesses().map(|witness| {
                if witness.cut_identity() != identity {
                    return Err(());
                }
                witness.outage_target()
            })
        }),
    );
    let mut pending_results = Vec::new();
    let mut retained_workers = Vec::with_capacity(frozen.len());
    for (connection, workers, batch) in frozen {
        retained_workers.push(workers);
        let candidates = batch.into_candidates();
        let mut proofs = Vec::new();
        let mut proof_witnesses = Vec::new();
        for candidate in candidates {
            let (witness, proof) = candidate.into_parts();
            match proof {
                Ok(proof) => {
                    proof_witnesses.push(witness);
                    proofs.push(proof);
                }
                Err(reason) => {
                    drop(witness);
                    results.record(PersistentFailureDriverResult::NoDispatch(
                        PersistentFailureNoDispatchReason::Router(reason),
                    ));
                }
            }
        }
        match connection.install_persistent_failure_obligations(identity, proofs) {
            Ok(completions) if completions.len() == proof_witnesses.len() => {
                pending_results.extend(
                    completions
                        .into_iter()
                        .map(|completion| PendingPersistentFailureResult { completion }),
                );
            }
            Ok(_) | Err(()) => {
                for witness in proof_witnesses {
                    drop(witness);
                    results.record(PersistentFailureDriverResult::NoDispatch(
                        PersistentFailureNoDispatchReason::DriverUnavailable,
                    ));
                }
            }
        }
    }
    for pending in pending_results {
        results.record(pending.completion.wait());
    }
    drop(retained_workers);
    Ok(results)
}

#[derive(Default)]
struct CutResultCounts {
    target_count: usize,
    proven_nondispatch_count: usize,
    possible_dispatch_count: usize,
}

impl CutResultCounts {
    fn record(&mut self, result: PersistentFailureDriverResult) {
        self.target_count += 1;
        match result {
            PersistentFailureDriverResult::NoDispatch(_)
            | PersistentFailureDriverResult::Attempted {
                disposition:
                    PersistentFailureInterruptDisposition::RejectedBeforeCoreInterrupt
                    | PersistentFailureInterruptDisposition::ProvenNotDispatched,
                ..
            } => self.proven_nondispatch_count += 1,
            PersistentFailureDriverResult::Attempted {
                disposition:
                    PersistentFailureInterruptDisposition::RequestAccepted
                    | PersistentFailureInterruptDisposition::CompletionUnknown,
                ..
            } => self.possible_dispatch_count += 1,
        }
    }
}

fn finish_worker(
    context: &WorkerContext,
    phase: PersistentFailureCutState,
    failure_generation: Option<PersistentFailureGeneration>,
    results: CutResultCounts,
) {
    if phase != PersistentFailureCutState::Finished {
        context.outage_inventory.retire();
    }
    let mut state = context
        .state
        .0
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    state.phase = phase;
    state.failure_generation = failure_generation;
    state.target_count = results.target_count;
    state.proven_nondispatch_count = results.proven_nondispatch_count;
    state.possible_dispatch_count = results.possible_dispatch_count;
    context.state.1.notify_all();
}
