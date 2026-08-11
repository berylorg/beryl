use super::*;

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
                Vec::new(),
                Vec::new(),
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
                Vec::new(),
                Vec::new(),
            );
            return;
        }
        context.notification.mark_cut_elected();
        let stop_freeze_failed = context
            .stop_coordinator
            .freeze_for_persistent_failure(identity)
            .is_err();
        let drain_failed = context.gate.wait_until_drained().is_err();
        let connections = snapshot_connections(&context.connections);
        if stop_freeze_failed || drain_failed {
            finish_worker(
                &context,
                PersistentFailureCutState::Incomplete,
                Some(identity.failure_generation),
                connections,
                Vec::new(),
            );
            return;
        }
        let Ok(results) = freeze_and_dispatch_targets(identity, &connections) else {
            finish_worker(
                &context,
                PersistentFailureCutState::Incomplete,
                Some(identity.failure_generation),
                connections,
                Vec::new(),
            );
            return;
        };
        finish_worker(
            &context,
            PersistentFailureCutState::Finished,
            Some(identity.failure_generation),
            connections,
            results,
        );
        return;
    }
    finish_worker(
        &context,
        PersistentFailureCutState::Stopped,
        None,
        Vec::new(),
        Vec::new(),
    );
}

fn snapshot_connections(
    connections: &Arc<crate::cas_projection::service_registry::ProjectionServiceConnectionRegistry>,
) -> Vec<Arc<ProjectionConnection>> {
    let mut retained = connections
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    retained.retain(|connection| !connection.is_detached());
    retained.clone()
}

fn freeze_and_dispatch_targets(
    identity: PersistentFailureCutIdentity,
    connections: &[Arc<ProjectionConnection>],
) -> Result<Vec<PersistentFailureDriverResult>, ()> {
    let mut frozen = Vec::with_capacity(connections.len());
    for connection in connections {
        let candidates = connection
            .freeze_persistent_failure_targets(identity)
            .map_err(|_| ())?;
        frozen.push((connection, candidates));
    }
    let mut results = Vec::new();
    let mut pending_results = Vec::new();
    for (connection, batch) in frozen {
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
                    results.push(PersistentFailureDriverResult::NoDispatch(
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
                results.extend(proof_witnesses.into_iter().map(|witness| {
                    drop(witness);
                    PersistentFailureDriverResult::NoDispatch(
                        PersistentFailureNoDispatchReason::DriverUnavailable,
                    )
                }));
            }
        }
    }
    results.extend(
        pending_results
            .into_iter()
            .map(|pending| pending.completion.wait()),
    );
    Ok(results)
}

fn finish_worker(
    context: &WorkerContext,
    phase: PersistentFailureCutState,
    failure_generation: Option<PersistentFailureGeneration>,
    connections: Vec<Arc<ProjectionConnection>>,
    results: Vec<PersistentFailureDriverResult>,
) {
    drop(connections);
    let target_count = results.len();
    let proven_nondispatch_count = results
        .iter()
        .filter(|result| {
            matches!(
                result,
                PersistentFailureDriverResult::NoDispatch(_)
                    | PersistentFailureDriverResult::Attempted {
                        disposition:
                            PersistentFailureInterruptDisposition::RejectedBeforeCoreInterrupt
                                | PersistentFailureInterruptDisposition::ProvenNotDispatched,
                        ..
                    }
            )
        })
        .count();
    let possible_dispatch_count = results
        .iter()
        .filter(|result| {
            matches!(
                result,
                PersistentFailureDriverResult::Attempted {
                    disposition: PersistentFailureInterruptDisposition::RequestAccepted
                        | PersistentFailureInterruptDisposition::CompletionUnknown,
                    ..
                }
            )
        })
        .count();
    drop(results);
    let mut state = context
        .state
        .0
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    state.phase = phase;
    state.failure_generation = failure_generation;
    state.target_count = target_count;
    state.proven_nondispatch_count = proven_nondispatch_count;
    state.possible_dispatch_count = possible_dispatch_count;
    context.state.1.notify_all();
}
