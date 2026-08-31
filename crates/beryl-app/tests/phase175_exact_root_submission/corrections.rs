use super::*;

use beryl_app::{
    composer_host::{
        ComposerHostSubmissionError, ComposerHostSubmissionFaultPoint,
        ComposerHostSubmissionTicket, SyndicComposerHost,
    },
    input_admission::accepted_input_promotion_command,
};
use beryl_home_store::{
    CommandOutcome, CursorReadLimits, FreeSpaceOutcome,
    test_faults::{FaultPoint, FreeSpaceTestObservation},
};
use beryl_model::SyndicTurnId;
use beryl_state::AssetOwner;
use syndic_storage::{ACCEPTED_NEXT_PAGE_MAX_BYTES, PromoteAcceptedInput, SyndicStorage};

#[test]
fn flush_error_restores_the_exact_stage_and_retry_converges() {
    let (_home, mut store, storage, thread, _faults) =
        base::fault_fixture("phase175-flush-fault", 141);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, empty) = activated(storage.clone(), &store, thread, 142, 143);
    commit_text(&mut host, &store, empty, 1, 0, 0, "flush", 5, 1);
    let ticket = host.begin_submission(request(144)).unwrap();
    let before = host.submission_diagnostics();
    assert_eq!(before.stage(), Some(ComposerHostSubmissionStage::Flushing));
    host.test_arm_submission_transition_fault(ComposerHostSubmissionFaultPoint::Flush);
    assert!(matches!(
        advance(&mut host, &store, assets.clone(), &seals, ticket, 145).unwrap_err(),
        ComposerHostSubmissionError::InjectedFault(ComposerHostSubmissionFaultPoint::Flush)
    ));
    assert_eq!(host.submission_diagnostics(), before);
    assert!(matches!(
        drive_submission(&mut host, &store, assets, &seals, ticket, operation_id(145)),
        ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Idle { .. })
    ));
}

#[test]
fn materializer_error_restores_root_custody_and_retry_converges() {
    let (_home, mut store, storage, thread, _faults) =
        base::fault_fixture("phase175-materializer-fault", 151);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, empty) = activated(storage.clone(), &store, thread, 152, 153);
    commit_text(&mut host, &store, empty, 1, 0, 0, "materialize", 11, 1);
    let ticket = host.begin_submission(request(154)).unwrap();
    advance_until_stage(
        &mut host,
        &store,
        assets.clone(),
        &seals,
        ticket,
        ComposerHostSubmissionStage::Materializing,
        155,
        None,
    );
    let before = host.submission_diagnostics();
    assert_eq!(before.retained_roots(), 1);
    host.test_arm_submission_transition_fault(ComposerHostSubmissionFaultPoint::Materializer);
    assert!(matches!(
        advance(&mut host, &store, assets.clone(), &seals, ticket, 155).unwrap_err(),
        ComposerHostSubmissionError::InjectedFault(ComposerHostSubmissionFaultPoint::Materializer)
    ));
    assert_eq!(host.submission_diagnostics(), before);
    assert!(matches!(
        drive_submission(&mut host, &store, assets, &seals, ticket, operation_id(155)),
        ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Idle { .. })
    ));
}

#[test]
fn pre_attempt_error_restores_materialization_custody_without_admission() {
    let (_home, mut store, storage, thread, _faults) =
        base::fault_fixture("phase175-pre-acceptance-fault", 161);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, empty) = activated(storage.clone(), &store, thread, 162, 163);
    let edited = commit_text(&mut host, &store, empty, 1, 0, 0, "accept", 6, 1);
    let ticket = host.begin_submission(request(164)).unwrap();
    advance_until_stage(
        &mut host,
        &store,
        assets.clone(),
        &seals,
        ticket,
        ComposerHostSubmissionStage::Accepting,
        165,
        None,
    );
    let before = host.submission_diagnostics();
    host.test_arm_submission_transition_fault(
        ComposerHostSubmissionFaultPoint::AcceptanceBeforeAttempt,
    );
    assert!(matches!(
        advance(&mut host, &store, assets.clone(), &seals, ticket, 165).unwrap_err(),
        ComposerHostSubmissionError::InjectedFault(
            ComposerHostSubmissionFaultPoint::AcceptanceBeforeAttempt
        )
    ));
    assert_eq!(host.submission_diagnostics(), before);
    assert!(!host.submission_diagnostics().command_attempted());
    assert!(
        storage
            .draft(&store, edited.candidate().draft_id(), point_limit())
            .unwrap()
            .is_some()
    );
    assert!(matches!(
        drive_submission(&mut host, &store, assets, &seals, ticket, operation_id(165)),
        ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Idle { .. })
    ));
}

#[test]
fn direct_idle_admission_requires_one_immediate_sufficient_observation() {
    let cases = [
        (
            "phase175-below-reserve",
            171,
            FreeSpaceTestObservation::Observed {
                available_bytes: 0,
                total_free_bytes: 0,
                total_bytes: 1,
            },
        ),
        (
            "phase175-free-space-unavailable",
            181,
            FreeSpaceTestObservation::Unavailable,
        ),
        (
            "phase175-free-space-indeterminate",
            191,
            FreeSpaceTestObservation::Observed {
                available_bytes: 2,
                total_free_bytes: 1,
                total_bytes: 2,
            },
        ),
    ];
    for (name, seed, denied_observation) in cases {
        let (_home, mut store, storage, thread, faults) = base::fault_fixture(name, seed);
        let assets = BerylState::register(&mut store).unwrap().assets();
        let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
        let (mut host, empty) = activated(storage.clone(), &store, thread, seed + 1, seed + 2);
        let edited = commit_text(&mut host, &store, empty, 1, 0, 0, "reserve", 7, 1);
        let ticket = host.begin_submission(request(seed + 3)).unwrap();
        advance_until_stage(
            &mut host,
            &store,
            assets.clone(),
            &seals,
            ticket,
            ComposerHostSubmissionStage::Accepting,
            seed + 4,
            None,
        );
        faults.push_free_space_observation(denied_observation);
        let denied = advance(&mut host, &store, assets.clone(), &seals, ticket, seed + 4).unwrap();
        let expected = match denied_observation {
            FreeSpaceTestObservation::Observed {
                available_bytes: 0, ..
            } => FreeSpaceOutcome::BelowReserve {
                available_bytes: 0,
                reserve_bytes: admission_requirement().total_bytes(),
            },
            FreeSpaceTestObservation::Unavailable => FreeSpaceOutcome::Unavailable,
            FreeSpaceTestObservation::Observed { .. } => FreeSpaceOutcome::Indeterminate,
        };
        assert_eq!(
            denied,
            ComposerHostSubmissionAdvance::DirectAdmissionDenied(expected)
        );
        let diagnostics = host.submission_diagnostics();
        assert_eq!(
            diagnostics.stage(),
            Some(ComposerHostSubmissionStage::Accepting)
        );
        assert_eq!(diagnostics.retained_roots(), 1);
        assert_eq!(diagnostics.retained_materializations(), 1);
        assert!(!diagnostics.command_attempted());
        assert!(
            storage
                .draft(&store, edited.candidate().draft_id(), point_limit())
                .unwrap()
                .is_some()
        );
        let sufficient = admission_requirement().total_bytes();
        faults.push_free_space_observation(FreeSpaceTestObservation::Observed {
            available_bytes: sufficient,
            total_free_bytes: sufficient,
            total_bytes: sufficient,
        });
        assert!(matches!(
            advance(&mut host, &store, assets.clone(), &seals, ticket, seed + 4).unwrap(),
            ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Idle { .. })
        ));
        assert_eq!(faults.free_space_observation_count(), 2);
    }
}

#[test]
fn accepted_next_does_not_consume_a_direct_idle_space_observation() {
    let (_home, mut store, storage, thread, faults) =
        base::fault_fixture("phase175-accepted-no-space-check", 201);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut first_host, empty) = activated(storage.clone(), &store, thread, 202, 203);
    commit_text(&mut first_host, &store, empty, 1, 0, 0, "first", 5, 1);
    let first_ticket = first_host.begin_submission(request(204)).unwrap();
    assert!(matches!(
        drive_submission(
            &mut first_host,
            &store,
            assets.clone(),
            &seals,
            first_ticket,
            operation_id(205),
        ),
        ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Idle { .. })
    ));
    let (mut queued_host, empty) = activated(storage.clone(), &store, thread, 211, 212);
    commit_text(&mut queued_host, &store, empty, 1, 0, 0, "queued", 6, 1);
    let queued_ticket = queued_host.begin_submission(request(213)).unwrap();
    faults.push_free_space_observation(FreeSpaceTestObservation::Unavailable);
    assert_eq!(
        drive_submission_at(
            &mut queued_host,
            &store,
            assets.clone(),
            &seals,
            queued_ticket,
            operation_id(214),
            SyndicTimestamp::from_unix_millis(1_208),
        ),
        ComposerHostSubmissionAdvance::ExactSuccess(FirstAcceptanceKind::Accepted)
    );
    assert_eq!(faults.free_space_observation_count(), 1);
}

#[path = "corrections/lifecycle.rs"]
mod lifecycle;
#[path = "corrections/reconciliation.rs"]
mod reconciliation;
fn request(seed: u8) -> ComposerHostSubmissionRequest {
    ComposerHostSubmissionRequest::new(
        SyndicDraftId::from_bytes([seed; 16]),
        SyndicItemId::from_bytes([seed.wrapping_add(1); 16]),
        DraftComposerMaterializationOperationIdV1::from_bytes([seed.wrapping_add(2); 16]),
        DraftPieceOperationIdV1::from_bytes([seed.wrapping_add(3); 16]),
        SyndicTimestamp::from_unix_millis(u64::from(seed) + 1_000),
        admission_requirement(),
    )
}

fn advance(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    assets: AssetState,
    seals: &DraftMarkerSealService,
    ticket: ComposerHostSubmissionTicket,
    seed: u8,
) -> Result<ComposerHostSubmissionAdvance, ComposerHostSubmissionError> {
    advance_with_authority(host, store, assets.clone(), seals, ticket, seed, None)
}

fn advance_with_authority(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    assets: AssetState,
    seals: &DraftMarkerSealService,
    ticket: ComposerHostSubmissionTicket,
    seed: u8,
    authority: Option<beryl_app::composer_host::ComposerHostMarkerSealAuthority>,
) -> Result<ComposerHostSubmissionAdvance, ComposerHostSubmissionError> {
    host.advance_submission(
        store,
        ticket,
        assets.clone(),
        seals,
        operation_id(u64::from(seed) + 2_000),
        authority,
        SyndicTimestamp::from_unix_millis(u64::from(seed) + 995),
        &CommandCancellation::new(),
    )
}

#[allow(clippy::too_many_arguments)]
fn advance_until_stage(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    assets: AssetState,
    seals: &DraftMarkerSealService,
    ticket: ComposerHostSubmissionTicket,
    stage: ComposerHostSubmissionStage,
    seed: u8,
    authority: Option<beryl_app::composer_host::ComposerHostMarkerSealAuthority>,
) {
    for _ in 0..256 {
        if host.submission_diagnostics().stage() == Some(stage) {
            return;
        }
        let outcome =
            advance_with_authority(host, store, assets.clone(), seals, ticket, seed, authority)
                .unwrap();
        assert!(
            matches!(
                outcome,
                ComposerHostSubmissionAdvance::Progress(_)
                    | ComposerHostSubmissionAdvance::ReconciliationPending
            ),
            "unexpected submission outcome before {stage:?}: {outcome:?}"
        );
    }
    panic!("submission did not reach the requested correction stage")
}
