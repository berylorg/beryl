use super::*;
use crate::{
    discussion_handoff_limits::{HandoffScanConfiguration, HandoffScanLimits},
    discussion_settlement::{DiscussionSettlementOperations, HandoffCandidateConvergenceError},
};
use beryl_home_store::CommandCancellation;
use syndic_storage::SyndicTimestamp;

fn limits() -> HandoffScanLimits {
    HandoffScanLimits::try_from(HandoffScanConfiguration {
        handoff_recovery_page_items: 1,
        handoff_recovery_page_encoded_bytes: beryl_state::HANDOFF_LIVE_RECORD_MAX_ENCODED_BYTES,
        handoff_job_record_encoded_bytes: beryl_state::HANDOFF_JOB_RECORD_MAX_ENCODED_BYTES,
        handoff_reconcile_slots: 1,
        handoff_ready_job_items: 1,
    })
    .unwrap()
}

#[test]
fn recovery_discussion_handoff_stays_private_and_joins_before_abort() {
    for mode in [
        "transfer",
        "cancel",
        "duplicate",
        "precancelled",
        "slots",
        "read_failure",
    ] {
        let (directory, candidate, storage, _, faults) = fixture();
        let reference = candidate.service_reference();
        let generation = candidate.generation();
        let state = BerylState::reacquire_candidate(&candidate).unwrap();
        let probe = Arc::new(Probe::default());
        let mut provider = provider(&candidate, &probe);
        if mode == "read_failure" {
            provider.shutdown_state = HomeHealthState::Failed;
        }
        let process = crate::process_admission::ProcessAdmissionGate::new();
        let operations = DiscussionSettlementOperations::new(
            process.clone(),
            std::num::NonZeroUsize::new(if mode == "slots" { 2 } else { 1 }).unwrap(),
        );
        let prepared = PreparedRecoveryCasServices::prepare(
            process,
            candidate,
            storage,
            config(),
            Box::new(provider),
            &ProjectionCancellationToken::new(),
        )
        .unwrap_or_else(|_| panic!("CAS fixture preparation failed"));
        let signal = prepared.service.as_ref().unwrap().scheduler_signal.clone();
        let gate = prepared.initial_start.as_ref().unwrap().gate();
        let cancellation = CommandCancellation::new();
        if mode == "precancelled" {
            cancellation.cancel();
        }
        if mode == "read_failure" {
            faults.fail_next(FaultPoint::BeforeReadConfirmation);
        }
        let prepared = prepared.prepare_handoff(
            operations.clone(),
            state.clone(),
            limits(),
            SyndicTimestamp::from_unix_millis(1),
            cancellation,
        );
        let home = match prepared {
            Ok(prepared) => {
                assert!(matches!(mode, "transfer" | "cancel" | "duplicate"));
                assert_eq!(
                    prepared.handoff.as_ref().unwrap().test_completed_passes(),
                    0
                );
                assert_eq!(reference.health().state(), HomeHealthState::Reopening);
                assert_eq!(signal.diagnostics().pass_count(), 0);
                assert_eq!(probe.issued.load(Ordering::SeqCst), 0);
                if mode == "transfer" {
                    let (candidate, service, start, handoff) = prepared.into_recovery_parts();
                    assert_eq!(candidate.generation(), generation);
                    assert!(Arc::ptr_eq(&gate, &start.gate()));
                    let mut handoff = handoff.expect("prepared handoff must transfer");
                    assert_eq!(handoff.test_completed_passes(), 0);
                    drop(start);
                    handoff.shutdown().unwrap();
                    service.close().unwrap();
                    assert_eq!(reference.health().state(), HomeHealthState::Reopening);
                    candidate.abort()
                } else {
                    let failure = if mode == "duplicate" {
                        match prepared.prepare_handoff(
                            operations,
                            state,
                            limits(),
                            SyndicTimestamp::from_unix_millis(1),
                            CommandCancellation::new(),
                        ) {
                            Ok(_) => panic!("duplicate coordinator admitted"),
                            Err(failure) => {
                                assert!(matches!(
                                    failure.error(),
                                    CasPreparationError::HandoffAlreadyPrepared
                                ));
                                failure
                            }
                        }
                    } else {
                        prepared.cancel()
                    };
                    failure
                        .into_retry_parts()
                        .unwrap_or_else(|_| panic!("cleanup must permit retry"))
                        .0
                }
            }
            Err(failure) => {
                match mode {
                    "precancelled" => {
                        assert!(matches!(failure.error(), CasPreparationError::Cancelled))
                    }
                    "slots" => assert!(matches!(
                        failure.error(),
                        CasPreparationError::HandoffConvergence(
                            HandoffCandidateConvergenceError::SlotConfigurationMismatch
                        )
                    )),
                    "read_failure" => assert!(matches!(
                        failure.error(),
                        CasPreparationError::HandoffConvergence(_)
                    )),
                    _ => panic!("unexpected preparation failure: {}", failure.error()),
                }
                failure
                    .into_retry_parts()
                    .unwrap_or_else(|_| panic!("cleanup must permit retry"))
                    .0
            }
        };
        assert!(!gate.wait());
        assert!(signal.diagnostics().stopped());
        assert_eq!(probe.shutdown.load(Ordering::SeqCst), 1);
        assert_eq!(home.health().state(), HomeHealthState::Failed);
        assert!(
            HomeOpenCandidate::open(HomeOpenOptions::new(
                directory.path(),
                HomeSchemaVersion::CURRENT
            ))
            .is_err()
        );
        let candidate = home.recover_same_home().unwrap();
        assert_ne!(candidate.generation(), generation);
        candidate.abort().close().unwrap();
    }
}

#[test]
fn recovery_handoff_retry_transfers_the_convergence_command_outcome() {
    use crate::{discussion_settlement::DiscussionSettlementOutcome, support};
    use beryl_home_store::{CommandOutcome, HomeCommand};
    use beryl_model::{JobId, ResolutionIntentId};
    use beryl_state::{
        AdmitBranchHandoffJob, BranchHandoffJobAdmission, ParentQueueOrdinal,
        ResolutionAttemptOrdinal, ResolutionRequestIdentity, ResolutionText,
    };

    let (_directory, candidate, storage, _, faults) = fixture();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let home = candidate.publish().unwrap();
    support::seed_populated(&home, storage.clone());
    let mut request = support::discussion_handoff::active_request(
        &home,
        &storage,
        ResolutionIntentId::from_bytes([210; 16]),
        JobId::from_bytes([211; 16]),
    );
    let admission = BranchHandoffJobAdmission::new(
        request.intent_id,
        ResolutionAttemptOrdinal::FIRST,
        request.thread_id,
        request.parent.thread_id,
        request.context_owner,
        request.context_digest,
        request.resolving_target.pending().active_turn_id(),
        ResolutionRequestIdentity::new(
            request.resolving_target.pending().cas_thread_id().clone(),
            request.resolving_target.cas_turn_id().clone(),
            beryl_model::DynamicToolCallId::new("resolve").unwrap(),
        ),
        ParentQueueOrdinal::new(request.parent.accepted_high_water),
        ResolutionText::new("Resolution result").unwrap(),
    );
    let job = admission.job_id();
    request.job_id = job;
    let handoff = storage
        .prepare_discussion_handoff(
            &home,
            syndic_storage::DiscussionHandoffMutation::Admit(request),
        )
        .unwrap();
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command.add(handoff.contribution()).unwrap();
    command
        .add(state.durable_jobs().admit_branch_handoff(
            state.durable_jobs().revision(&home).unwrap(),
            AdmitBranchHandoffJob::new(admission),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    support::discussion_handoff::complete_resolving_turn(&home, &storage);
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    let candidate = home.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let probe = Arc::new(Probe::default());
    let provider = provider(&candidate, &probe);
    let process = crate::process_admission::ProcessAdmissionGate::new();
    let operations = DiscussionSettlementOperations::new(
        process.clone(),
        std::num::NonZeroUsize::new(1).unwrap(),
    );
    let prepared = PreparedRecoveryCasServices::prepare(
        process,
        candidate,
        storage,
        config(),
        Box::new(provider),
        &ProjectionCancellationToken::new(),
    )
    .unwrap_or_else(|_| panic!("CAS fixture preparation failed"));
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let failure = match prepared.prepare_handoff(
        operations,
        state,
        limits(),
        SyndicTimestamp::from_unix_millis(600),
        CommandCancellation::new(),
    ) {
        Ok(_) => panic!("uncertain convergence succeeded"),
        Err(failure) => failure,
    };
    let original = match failure.error() {
        CasPreparationError::HandoffConvergence(HandoffCandidateConvergenceError::Outcome {
            job_id,
            outcome,
        }) => {
            assert_eq!(*job_id, job);
            assert!(matches!(
                **outcome,
                DiscussionSettlementOutcome::Indeterminate { .. }
            ));
            &**outcome as *const DiscussionSettlementOutcome
        }
        error => panic!("expected command outcome: {error}"),
    };
    let (home, error) = failure
        .into_retry_parts()
        .unwrap_or_else(|_| panic!("cleanup must permit retry"));
    assert_eq!(probe.shutdown.load(Ordering::SeqCst), 1);
    let CasPreparationError::HandoffConvergence(HandoffCandidateConvergenceError::Outcome {
        outcome,
        ..
    }) = error
    else {
        panic!("outcome lost during transfer")
    };
    assert_eq!(&*outcome as *const DiscussionSettlementOutcome, original);
    let DiscussionSettlementOutcome::Indeterminate { audit, .. } = *outcome else {
        panic!("uncertain outcome changed")
    };
    let mut candidate = home.recover_same_home().unwrap();
    let state = BerylState::reacquire_candidate(&candidate).unwrap();
    let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
    let access = candidate.recovery_access().unwrap();
    assert_eq!(access.pending_reconciliations().len(), 1);
    assert!(audit.reconcile_candidate(&access, &storage, &state).is_ok());
    assert!(access.pending_reconciliations().is_empty());
    drop(audit);
    candidate.abort().close().unwrap();
}
