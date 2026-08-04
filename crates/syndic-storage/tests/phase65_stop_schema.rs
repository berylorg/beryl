use beryl_model::{
    BindingRevision, CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration,
    CasThreadId, CasTurnId, InputGateRevision, RuntimeId, SyndicExecutionSnapshotId,
    SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{
    AcceptedRouteGeneration, AcceptedRouteHeadProof, AcceptedRouteRevision, InputGateState,
    StopAbandonmentReason, StopAbandonmentWitness, StopAdmissionWitness, StopAttemptNonce,
    StopCause, StopCauseFirstRevisions, StopCauseFirstRevisionsError, StopCauseSet,
    StopCauseSetError, StopDispatchClaimWitness, StopDispositionSource,
    StopMatchingTerminalWitness, StopOperationId, StopOperationNonce, StopOperationRecord,
    StopOperationRecordError, StopOperationRevision, StopOperationState, StopOperationTarget,
    StopSafeReopenWitness, TurnKind, TurnStateRevision,
};

fn route(generation: u64, revision: u64) -> AcceptedRouteHeadProof {
    AcceptedRouteHeadProof::new(
        AcceptedRouteGeneration::new(generation).unwrap(),
        AcceptedRouteRevision::new(revision).unwrap(),
    )
}

fn admission() -> StopAdmissionWitness {
    StopAdmissionWitness::new(
        InputGateRevision::new(3).unwrap(),
        route(7, 4),
        InputGateRevision::new(4).unwrap(),
        route(7, 5),
    )
}

fn target(thread_id: SyndicThreadId) -> StopOperationTarget {
    StopOperationTarget::new(
        thread_id,
        SyndicTurnId::from_bytes([2; 16]),
        TurnKind::OrdinaryUser,
        BindingRevision::new(3).unwrap(),
        SyndicExecutionSnapshotId::from_bytes([4; 16]),
        RuntimeId::from_bytes([5; 16]),
        CasLoadedSessionGeneration::new(
            CasProcessGeneration::new(6).unwrap(),
            CasLoadedThreadGeneration::new(7).unwrap(),
        ),
        CasThreadId::new("cas-thread").unwrap(),
        CasTurnId::new("cas-turn").unwrap(),
    )
}

fn operation_id(thread_id: SyndicThreadId) -> StopOperationId {
    StopOperationId::new(thread_id, StopOperationNonce::from_bytes([8; 16]))
}

fn causes() -> StopCauseSet {
    StopCauseSet::from(StopCause::SelectedOperationControl).with(StopCause::DiagnosticControl)
}

#[test]
fn nonces_causes_and_stopping_gate_preserve_exact_canonical_values() {
    let operation_nonce = StopOperationNonce::from_bytes([11; 16]);
    let attempt_nonce = StopAttemptNonce::from_bytes([12; 16]);
    assert_eq!(operation_nonce.as_bytes(), &[11; 16]);
    assert_eq!(attempt_nonce.as_bytes(), &[12; 16]);

    assert_eq!(StopCauseSet::from_bits(0), Err(StopCauseSetError::Empty));
    assert_eq!(
        StopCauseSet::from_bits(0b0001_0000),
        Err(StopCauseSetError::UnknownBits { bits: 0b0001_0000 })
    );
    let all = StopCauseSet::from_bits(0b0000_1111).unwrap();
    assert_eq!(all.bits(), StopCauseSet::ALL_BITS);
    assert!(all.contains(StopCause::SelectedOperationControl));
    assert!(all.contains(StopCause::DiagnosticControl));
    assert!(all.contains(StopCause::HealthyHomeWindowClose));
    assert!(all.contains(StopCause::InterruptingApproval));

    let turn_id = SyndicTurnId::from_bytes([13; 16]);
    let gate = InputGateState::stopping(turn_id, operation_nonce);
    assert_eq!(gate.blocking_turn_id(), Some(turn_id));
    assert_eq!(gate.stop_operation_nonce(), Some(operation_nonce));
}

#[test]
fn admitted_record_retains_target_and_immutable_admission_witness() {
    let thread_id = SyndicThreadId::from_bytes([1; 16]);
    let id = operation_id(thread_id);
    let operation_target = target(thread_id);
    let admission = admission();
    let record =
        StopOperationRecord::admitted(id, operation_target.clone(), admission, causes()).unwrap();

    assert_eq!(record.id(), id);
    assert_eq!(record.target(), &operation_target);
    assert_eq!(record.admission(), admission);
    assert_eq!(record.revision(), StopOperationRevision::FIRST);
    assert_eq!(record.admission_causes(), causes());
    assert_eq!(
        record
            .cause_first_revisions()
            .first_revision(StopCause::DiagnosticControl),
        Some(StopOperationRevision::FIRST)
    );
    assert_eq!(record.dispatch_claim(), None);
    assert_eq!(record.attempt(), None);
    assert_eq!(record.state(), StopOperationState::Admitted);
    assert!(record.state().is_live());
}

#[test]
fn record_rejects_identity_admission_and_attempt_inconsistency() {
    let thread_id = SyndicThreadId::from_bytes([1; 16]);
    let id = operation_id(thread_id);
    let wrong_target = target(SyndicThreadId::from_bytes([9; 16]));
    assert_eq!(
        StopOperationRecord::admitted(id, wrong_target, admission(), causes()),
        Err(StopOperationRecordError::TargetThreadMismatch)
    );

    let valid_target = target(thread_id);
    let attempt = StopAttemptNonce::from_bytes([10; 16]);
    let claim = StopDispatchClaimWitness::new(StopOperationRevision::FIRST, attempt);
    let initial_causes = StopCauseFirstRevisions::for_admission(causes());
    assert_eq!(
        StopCauseFirstRevisions::new(None, None, None, None),
        Err(StopCauseFirstRevisionsError::MissingAdmissionCause)
    );
    assert_eq!(
        StopOperationRecord::new(
            id,
            valid_target.clone(),
            admission(),
            StopOperationRevision::new(2).unwrap(),
            initial_causes,
            Some(claim),
            StopOperationState::Admitted,
        ),
        Err(StopOperationRecordError::AdmittedClaimPresent)
    );
    assert_eq!(
        StopOperationRecord::new(
            id,
            valid_target.clone(),
            admission(),
            StopOperationRevision::new(2).unwrap(),
            initial_causes,
            None,
            StopOperationState::DispatchClaimed,
        ),
        Err(StopOperationRecordError::ClaimedWitnessMissing)
    );
    assert!(matches!(
        StopOperationRecord::new(
            id,
            valid_target.clone(),
            admission(),
            StopOperationRevision::FIRST,
            initial_causes,
            Some(claim),
            StopOperationState::DispatchClaimed,
        ),
        Err(StopOperationRecordError::ClaimPublicationFuture { .. })
    ));
    let claimed = StopOperationRecord::new(
        id,
        valid_target.clone(),
        admission(),
        StopOperationRevision::new(2).unwrap(),
        initial_causes,
        Some(claim),
        StopOperationState::DispatchClaimed,
    )
    .unwrap();
    assert!(syndic_storage::test_faults::stop_dispatch_claimed_first_codec_rejection(&claimed));

    let invalid_admission = StopAdmissionWitness::new(
        InputGateRevision::new(3).unwrap(),
        route(7, 4),
        InputGateRevision::new(5).unwrap(),
        route(7, 5),
    );
    assert!(matches!(
        StopOperationRecord::admitted(id, valid_target, invalid_admission, causes()),
        Err(StopOperationRecordError::AdmissionGateRevisionMismatch { .. })
    ));
}

#[test]
fn bounded_transition_ledger_rejects_future_duplicate_and_gapped_revisions() {
    let thread_id = SyndicThreadId::from_bytes([1; 16]);
    let id = operation_id(thread_id);
    let operation_target = target(thread_id);
    let first = Some(StopOperationRevision::FIRST);
    let second = Some(StopOperationRevision::new(2).unwrap());
    let third = Some(StopOperationRevision::new(3).unwrap());

    let future = StopCauseFirstRevisions::new(first, third, None, None).unwrap();
    assert!(matches!(
        StopOperationRecord::new(
            id,
            operation_target.clone(),
            admission(),
            StopOperationRevision::new(2).unwrap(),
            future,
            None,
            StopOperationState::Admitted,
        ),
        Err(StopOperationRecordError::CauseFirstRevisionFuture { .. })
    ));

    let duplicate = StopCauseFirstRevisions::new(first, second, second, None).unwrap();
    assert!(matches!(
        StopOperationRecord::new(
            id,
            operation_target.clone(),
            admission(),
            StopOperationRevision::new(3).unwrap(),
            duplicate,
            None,
            StopOperationState::Admitted,
        ),
        Err(StopOperationRecordError::DuplicateTransitionRevision { revision: 2 })
    ));

    let gap = StopCauseFirstRevisions::new(first, third, None, None).unwrap();
    assert!(matches!(
        StopOperationRecord::new(
            id,
            operation_target.clone(),
            admission(),
            StopOperationRevision::new(3).unwrap(),
            gap,
            None,
            StopOperationState::Admitted,
        ),
        Err(StopOperationRecordError::TransitionLedgerGap { .. })
    ));

    let attempt = StopAttemptNonce::from_bytes([10; 16]);
    let duplicate_claim = StopDispatchClaimWitness::new(StopOperationRevision::FIRST, attempt);
    assert!(matches!(
        StopOperationRecord::new(
            id,
            operation_target.clone(),
            admission(),
            StopOperationRevision::new(2).unwrap(),
            StopCauseFirstRevisions::new(first, second, None, None).unwrap(),
            Some(duplicate_claim),
            StopOperationState::DispatchClaimed,
        ),
        Err(StopOperationRecordError::DuplicateTransitionRevision { revision: 2 })
    ));

    let reordered_claim =
        StopDispatchClaimWitness::new(StopOperationRevision::new(2).unwrap(), attempt);
    assert!(matches!(
        StopOperationRecord::new(
            id,
            operation_target,
            admission(),
            StopOperationRevision::new(3).unwrap(),
            StopCauseFirstRevisions::new(first, None, None, None).unwrap(),
            Some(reordered_claim),
            StopOperationState::DispatchClaimed,
        ),
        Err(StopOperationRecordError::TransitionLedgerGap { .. })
    ));
}

#[test]
fn each_consumed_disposition_retains_a_bounded_exact_successor() {
    let thread_id = SyndicThreadId::from_bytes([1; 16]);
    let id = operation_id(thread_id);
    let operation_target = target(thread_id);
    let attempt = StopAttemptNonce::from_bytes([10; 16]);
    let source = StopDispositionSource::new(
        InputGateRevision::new(8).unwrap(),
        StopOperationRevision::new(4).unwrap(),
    );
    let successor_revision = StopOperationRevision::new(5).unwrap();
    let successor_gate_revision = InputGateRevision::new(9).unwrap();
    let retained_causes = StopCauseFirstRevisions::new(
        Some(StopOperationRevision::FIRST),
        Some(StopOperationRevision::FIRST),
        Some(StopOperationRevision::new(2).unwrap()),
        Some(StopOperationRevision::new(3).unwrap()),
    )
    .unwrap();
    let claim = Some(StopDispatchClaimWitness::new(
        StopOperationRevision::new(3).unwrap(),
        attempt,
    ));

    let safe = StopOperationRecord::new(
        id,
        operation_target.clone(),
        admission(),
        successor_revision,
        retained_causes,
        claim,
        StopOperationState::SafeReopened(StopSafeReopenWitness::new(
            source,
            successor_gate_revision,
            route(8, 1),
        )),
    )
    .unwrap();
    assert!(!safe.state().is_live());
    assert_eq!(safe.state().disposition_source(), Some(source));

    let terminal = StopOperationRecord::new(
        id,
        operation_target.clone(),
        admission(),
        successor_revision,
        retained_causes,
        claim,
        StopOperationState::MatchingTerminal(StopMatchingTerminalWitness::new(
            source,
            successor_gate_revision,
            TurnStateRevision::new(6).unwrap(),
        )),
    )
    .unwrap();
    assert_eq!(terminal.dispatch_claim(), claim);
    assert_eq!(terminal.attempt(), Some(attempt));

    let unclaimed_causes = StopCauseFirstRevisions::new(
        Some(StopOperationRevision::FIRST),
        Some(StopOperationRevision::new(2).unwrap()),
        Some(StopOperationRevision::new(3).unwrap()),
        Some(StopOperationRevision::new(4).unwrap()),
    )
    .unwrap();
    let abandoned = StopOperationRecord::new(
        id,
        operation_target,
        admission(),
        successor_revision,
        unclaimed_causes,
        None,
        StopOperationState::Abandoned(StopAbandonmentWitness::new(
            source,
            StopAbandonmentReason::StartupProcessGenerationLost,
            successor_gate_revision,
            BindingRevision::new(4).unwrap(),
            TurnStateRevision::new(6).unwrap(),
        )),
    )
    .unwrap();
    assert_eq!(abandoned.attempt(), None);

    let wrong_successor = StopOperationRecord::new(
        id,
        target(thread_id),
        admission(),
        StopOperationRevision::new(6).unwrap(),
        retained_causes,
        claim,
        StopOperationState::MatchingTerminal(StopMatchingTerminalWitness::new(
            source,
            successor_gate_revision,
            TurnStateRevision::new(6).unwrap(),
        )),
    );
    assert!(matches!(
        wrong_successor,
        Err(StopOperationRecordError::ConsumedStopRevisionMismatch { .. })
    ));
}

#[test]
fn direct_v1_codec_roundtrips_provenance_and_rejects_the_aggregate_predecessor() {
    use syndic_storage::test_faults::{
        StopProvenanceCodecCorruption, old_aggregate_stop_encoding_rejection,
        roundtrip_stop_operation_v1, stop_provenance_codec_rejection,
    };

    let thread_id = SyndicThreadId::from_bytes([1; 16]);
    let id = operation_id(thread_id);
    let operation_target = target(thread_id);
    let admitted =
        StopOperationRecord::admitted(id, operation_target.clone(), admission(), causes()).unwrap();
    assert_eq!(
        roundtrip_stop_operation_v1(&admitted),
        Some(admitted.clone())
    );
    assert!(old_aggregate_stop_encoding_rejection(&admitted));
    assert!(stop_provenance_codec_rejection(
        &admitted,
        StopProvenanceCodecCorruption::MissingAdmissionCause,
    ));
    assert!(stop_provenance_codec_rejection(
        &admitted,
        StopProvenanceCodecCorruption::FutureCause,
    ));

    let attempt = StopAttemptNonce::from_bytes([10; 16]);
    let claimed = StopOperationRecord::new(
        id,
        operation_target.clone(),
        admission(),
        StopOperationRevision::new(2).unwrap(),
        StopCauseFirstRevisions::for_admission(causes()),
        Some(StopDispatchClaimWitness::new(
            StopOperationRevision::FIRST,
            attempt,
        )),
        StopOperationState::DispatchClaimed,
    )
    .unwrap();
    assert_eq!(roundtrip_stop_operation_v1(&claimed), Some(claimed.clone()));
    assert!(stop_provenance_codec_rejection(
        &claimed,
        StopProvenanceCodecCorruption::ZeroClaimSource,
    ));
    assert!(stop_provenance_codec_rejection(
        &claimed,
        StopProvenanceCodecCorruption::FutureClaimPublication,
    ));

    let consumed = StopOperationRecord::new(
        id,
        operation_target,
        admission(),
        StopOperationRevision::new(3).unwrap(),
        StopCauseFirstRevisions::for_admission(causes()),
        claimed.dispatch_claim(),
        StopOperationState::SafeReopened(StopSafeReopenWitness::new(
            StopDispositionSource::new(
                InputGateRevision::new(8).unwrap(),
                StopOperationRevision::new(2).unwrap(),
            ),
            InputGateRevision::new(9).unwrap(),
            route(8, 1),
        )),
    )
    .unwrap();
    assert_eq!(
        roundtrip_stop_operation_v1(&consumed),
        Some(consumed.clone())
    );
    assert!(old_aggregate_stop_encoding_rejection(&consumed));

    let joined_then_claimed = StopOperationRecord::new(
        id,
        target(thread_id),
        admission(),
        StopOperationRevision::new(4).unwrap(),
        StopCauseFirstRevisions::new(
            Some(StopOperationRevision::FIRST),
            Some(StopOperationRevision::new(2).unwrap()),
            Some(StopOperationRevision::new(3).unwrap()),
            None,
        )
        .unwrap(),
        Some(StopDispatchClaimWitness::new(
            StopOperationRevision::new(3).unwrap(),
            attempt,
        )),
        StopOperationState::DispatchClaimed,
    )
    .unwrap();
    assert!(stop_provenance_codec_rejection(
        &joined_then_claimed,
        StopProvenanceCodecCorruption::DuplicateLaterCause,
    ));
    assert!(stop_provenance_codec_rejection(
        &joined_then_claimed,
        StopProvenanceCodecCorruption::GappedLaterCause,
    ));
}
