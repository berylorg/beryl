use super::*;

use beryl_app::composer_host::ComposerHostServiceDisposalCompletion;

#[derive(Clone, Copy)]
enum CancellationCut {
    Capture,
    Materialization,
    FreeSpaceAdmission,
    BeforeFinalCommand,
}

#[test]
fn caller_cancellation_at_capture_preserves_exact_editor_state_and_sends_nothing() {
    prove_cancellation_cut(CancellationCut::Capture, "phase175-cancel-capture", 31);
}

#[test]
fn caller_cancellation_at_materialization_preserves_exact_editor_state_and_sends_nothing() {
    prove_cancellation_cut(
        CancellationCut::Materialization,
        "phase175-cancel-materialization",
        41,
    );
}

#[test]
fn caller_cancellation_at_free_space_admission_preserves_exact_editor_state_and_sends_nothing() {
    prove_cancellation_cut(
        CancellationCut::FreeSpaceAdmission,
        "phase175-cancel-free-space",
        51,
    );
}

#[test]
fn caller_cancellation_before_final_home_command_preserves_exact_editor_state_and_sends_nothing() {
    prove_cancellation_cut(
        CancellationCut::BeforeFinalCommand,
        "phase175-cancel-final-command",
        61,
    );
}

#[test]
fn materializer_indeterminate_custody_is_joined_and_released_by_service_disposal() {
    let (_home, mut store, storage, thread, faults) =
        base::fault_fixture("phase175-dispose-materializer", 71);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, empty) = activated(storage.clone(), &store, thread, 72, 73);
    commit_text(&mut host, &store, empty, 1, 0, 0, "materializer", 12, 1);
    let ticket = host.begin_submission(request(74)).unwrap();
    advance_until_stage(
        &mut host,
        &store,
        assets.clone(),
        &seals,
        ticket,
        ComposerHostSubmissionStage::Materializing,
        75,
        None,
    );
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    assert_eq!(
        advance(&mut host, &store, assets.clone(), &seals, ticket, 75).unwrap(),
        ComposerHostSubmissionAdvance::ReconciliationPending
    );
    assert_eq!(store.pending_reconciliations().len(), 1);

    assert_eq!(
        host.dispose_composer_service(&store).unwrap(),
        ComposerHostServiceDisposalCompletion::Disposed
    );
    assert_disposed_submission(&mut host, &store, assets, &seals, ticket, 75);
}

#[test]
fn acceptance_indeterminate_custody_is_joined_and_released_by_service_disposal() {
    let (_home, mut store, storage, thread, faults) =
        base::fault_fixture("phase175-dispose-acceptance", 81);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, empty) = activated(storage.clone(), &store, thread, 82, 83);
    commit_text(&mut host, &store, empty, 1, 0, 0, "acceptance", 10, 1);
    let ticket = host.begin_submission(request(84)).unwrap();
    advance_until_stage(
        &mut host,
        &store,
        assets.clone(),
        &seals,
        ticket,
        ComposerHostSubmissionStage::Accepting,
        85,
        None,
    );
    let injected = faults.clone();
    host.test_arm_submission_before_execute_fault(move |_, _| {
        injected.fail_next(FaultPoint::AfterCommitBeforePersist);
    });
    assert_eq!(
        advance(&mut host, &store, assets.clone(), &seals, ticket, 85).unwrap(),
        ComposerHostSubmissionAdvance::ReconciliationPending
    );
    assert_eq!(store.pending_reconciliations().len(), 1);

    assert_eq!(
        host.dispose_composer_service(&store).unwrap(),
        ComposerHostServiceDisposalCompletion::Disposed
    );
    assert_disposed_submission(&mut host, &store, assets, &seals, ticket, 85);
}

fn prove_cancellation_cut(cut: CancellationCut, name: &str, seed: u8) {
    let (_home, mut store, storage, thread, faults) = base::fault_fixture(name, seed);
    let assets = BerylState::register(&mut store).unwrap().assets();
    let seals = service(&store, storage.clone(), assets.clone(), 1, 1);
    let (mut host, empty) = activated(storage.clone(), &store, thread, seed + 1, seed + 2);
    commit_text(&mut host, &store, empty, 1, 0, 0, "preserved", 9, 1);
    let request = request(seed + 3);
    let item = request.idle_user_item_id();
    let next_draft = request.next_draft_id();
    let ticket = host.begin_submission(request).unwrap();
    let cancellation = CommandCancellation::new();
    let target = match cut {
        CancellationCut::Capture => ComposerHostSubmissionStage::Capturing,
        CancellationCut::Materialization => ComposerHostSubmissionStage::Materializing,
        CancellationCut::FreeSpaceAdmission | CancellationCut::BeforeFinalCommand => {
            ComposerHostSubmissionStage::Accepting
        }
    };
    advance_until_stage_with_cancellation(
        &mut host,
        &store,
        assets.clone(),
        &seals,
        ticket,
        target,
        seed + 4,
        &cancellation,
    );
    let preserved = host.binding().unwrap();
    match cut {
        CancellationCut::Capture | CancellationCut::Materialization => cancellation.cancel(),
        CancellationCut::FreeSpaceAdmission => {
            push_sufficient_space(&faults);
            host.test_arm_submission_transition_fault(
                ComposerHostSubmissionFaultPoint::CancellationAfterFreeSpace,
            );
        }
        CancellationCut::BeforeFinalCommand => {
            push_sufficient_space(&faults);
            host.test_arm_submission_transition_fault(
                ComposerHostSubmissionFaultPoint::CancellationBeforeFinalCommand,
            );
        }
    }

    assert_eq!(
        advance_with_cancellation(
            &mut host,
            &store,
            assets.clone(),
            &seals,
            ticket,
            seed + 4,
            &cancellation,
        )
        .unwrap(),
        ComposerHostSubmissionAdvance::Cancelled
    );
    assert_eq!(host.binding(), Some(preserved));
    assert!(!host.is_unavailable());
    assert_released(&host, &store);
    assert_exact_unsent_state(&store, storage.clone(), thread, preserved, item, next_draft);
    assert_eq!(
        advance_with_cancellation(
            &mut host,
            &store,
            assets.clone(),
            &seals,
            ticket,
            seed + 4,
            &cancellation,
        )
        .unwrap(),
        ComposerHostSubmissionAdvance::Stale
    );
    assert_exact_unsent_state(&store, storage, thread, preserved, item, next_draft);
}

fn push_sufficient_space(faults: &beryl_home_store::test_faults::FaultController) {
    let sufficient = admission_requirement().total_bytes();
    faults.push_free_space_observation(FreeSpaceTestObservation::Observed {
        available_bytes: sufficient,
        total_free_bytes: sufficient,
        total_bytes: sufficient,
    });
}

#[allow(clippy::too_many_arguments)]
fn advance_until_stage_with_cancellation(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    assets: AssetState,
    seals: &DraftMarkerSealService,
    ticket: ComposerHostSubmissionTicket,
    stage: ComposerHostSubmissionStage,
    seed: u8,
    cancellation: &CommandCancellation,
) {
    for _ in 0..256 {
        if host.submission_diagnostics().stage() == Some(stage) {
            return;
        }
        let outcome = advance_with_cancellation(
            host,
            store,
            assets.clone(),
            seals,
            ticket,
            seed,
            cancellation,
        )
        .unwrap();
        assert!(matches!(
            outcome,
            ComposerHostSubmissionAdvance::Progress(_)
        ));
    }
    panic!("submission did not reach the requested cancellation stage")
}

fn advance_with_cancellation(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    assets: AssetState,
    seals: &DraftMarkerSealService,
    ticket: ComposerHostSubmissionTicket,
    seed: u8,
    cancellation: &CommandCancellation,
) -> Result<ComposerHostSubmissionAdvance, ComposerHostSubmissionError> {
    host.advance_submission(
        store,
        ticket,
        assets.clone(),
        seals,
        operation_id(u64::from(seed)),
        None,
        SyndicTimestamp::from_unix_millis(u64::from(seed)),
        cancellation,
    )
}

fn assert_exact_unsent_state(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    preserved: beryl_app::composer_host::ComposerHostBinding,
    item: SyndicItemId,
    next_draft: SyndicDraftId,
) {
    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(current.draft().id(), preserved.candidate().draft_id());
    assert_eq!(current.draft().root_history().root(), preserved.root());
    assert_eq!(
        current.draft().root_history().history(),
        preserved.history()
    );
    assert_ne!(current.draft().id(), next_draft);
    assert!(
        storage
            .canonical_item(store, item, point_limit())
            .unwrap()
            .is_none()
    );
    assert!(
        storage
            .accepted_input(
                store,
                preserved.candidate().draft_id().accepted_input_id(),
                point_limit(),
            )
            .unwrap()
            .is_none()
    );
}

fn assert_released(host: &SyndicComposerHost, store: &beryl_home_store::HomeStore) {
    let diagnostics = host.submission_diagnostics();
    assert!(!diagnostics.pending());
    assert_eq!(diagnostics.retained_roots(), 0);
    assert_eq!(diagnostics.retained_materializations(), 0);
    assert!(store.pending_reconciliations().is_empty());
}

fn assert_disposed_submission(
    host: &mut SyndicComposerHost,
    store: &beryl_home_store::HomeStore,
    assets: AssetState,
    seals: &DraftMarkerSealService,
    ticket: ComposerHostSubmissionTicket,
    seed: u8,
) {
    assert!(host.binding().is_none());
    assert_released(host, store);
    assert_eq!(
        advance(host, store, assets.clone(), seals, ticket, seed).unwrap(),
        ComposerHostSubmissionAdvance::Stale
    );
    assert!(matches!(
        host.begin_submission(request(seed.wrapping_add(1))),
        Err(ComposerHostSubmissionError::Host(
            beryl_app::composer_host::ComposerHostError::LifecycleBlocked
        ))
    ));
}
