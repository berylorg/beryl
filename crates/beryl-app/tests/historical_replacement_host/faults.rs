use super::*;
use beryl_home_store::test_faults::{FaultController, FaultPoint};
use beryl_home_store::{HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion, HomeStore};
use beryl_model::{
    ExecutionBinding, PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId,
    SyndicThreadId,
};
use beryl_state::BerylState;

fn fault_fixture() -> (
    tempfile::TempDir,
    HomeStore,
    SyndicStorage,
    BerylState,
    FaultController,
    ComposerHostHistoricalTarget,
) {
    let home = tempfile::Builder::new()
        .prefix("beryl-historical-fault-")
        .tempdir()
        .unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(home.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(
            SyndicStorage::required_domains()
                .unwrap()
                .merge(BerylState::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let thread = SyndicThreadId::from_bytes([201; 16]);
    let execution = ExecutionBinding::new(
        RuntimeId::from_bytes([171; 16]),
        RootId::from_bytes([172; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\beryl-historical-fixture",
        )
        .unwrap(),
    );
    host_support::committed(host_support::execute(
        &store,
        storage.create_thread(
            storage.revision(&store).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([202; 16]),
                execution,
                SyndicTimestamp::from_unix_millis(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    ));
    let target = seed_source(
        &store,
        &storage,
        thread,
        &ComposerPayload::new(vec![
            ComposerAtom::text("retained historical input").unwrap(),
        ])
        .unwrap(),
    );
    (home, store, storage, state, faults, target)
}

#[test]
fn begin_noncommit_releases_only_its_own_unselected_attempt() {
    let (_home, store, storage, state, faults, target) = fault_fixture();
    let mut host = activate(&store, storage, target.thread_id);
    let original = host.binding().unwrap();
    let ticket = host
        .begin_historical_replacement(&store, original, target, operation(211))
        .unwrap();
    faults.fail_next(FaultPoint::BeforeCommit);
    assert_eq!(
        host.advance_historical_replacement(
            &store,
            ticket,
            &state.assets(),
            timestamp(),
            &CommandCancellation::new()
        )
        .unwrap(),
        ComposerHostHistoricalAdvance::Progress
    );
    assert_eq!(
        host.advance_historical_replacement(
            &store,
            ticket,
            &state.assets(),
            timestamp(),
            &CommandCancellation::new()
        )
        .unwrap(),
        ComposerHostHistoricalAdvance::Progress
    );
    assert_eq!(
        host.advance_historical_replacement(
            &store,
            ticket,
            &state.assets(),
            timestamp(),
            &CommandCancellation::new()
        )
        .unwrap(),
        ComposerHostHistoricalAdvance::NotCommitted
    );
    assert_eq!(host.binding(), Some(original));
    assert_eq!(
        host.submission_intent(),
        Some(DraftSubmissionIntent::Ordinary)
    );
    assert!(!host.historical_replacement_pending());
}

#[test]
fn start_outcome_loss_retains_original_scope_until_healthy_classification() {
    let (_home, store, storage, state, faults, target) = fault_fixture();
    let mut host = activate(&store, storage.clone(), target.thread_id);
    let original = host.binding().unwrap();
    let ticket = host
        .begin_historical_replacement(&store, original, target, operation(211))
        .unwrap();
    for _ in 0..1_000 {
        if host.historical_replacement_stage() == Some(ComposerHostHistoricalStage::Starting) {
            break;
        }
        assert_eq!(
            host.advance_historical_replacement(
                &store,
                ticket,
                &state.assets(),
                timestamp(),
                &CommandCancellation::new()
            )
            .unwrap(),
            ComposerHostHistoricalAdvance::Progress
        );
    }
    assert_eq!(
        host.historical_replacement_stage(),
        Some(ComposerHostHistoricalStage::Starting)
    );
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let classification_faults = faults.clone();
    host.test_arm_historical_after_execute_fault(move |store, outcome| {
        assert!(matches!(
            outcome,
            beryl_home_store::CommandOutcome::Indeterminate { .. }
        ));
        assert_eq!(
            store.health().state(),
            beryl_home_store::HomeHealthState::Healthy
        );
        classification_faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
    });
    assert!(
        host.advance_historical_replacement(
            &store,
            ticket,
            &state.assets(),
            timestamp(),
            &CommandCancellation::new()
        )
        .is_err()
    );
    assert!(host.historical_replacement_pending());
    assert_eq!(host.historical_replacement_ticket(), Some(ticket));
    assert_eq!(host.binding(), Some(original));
    assert_eq!(
        store.health().state(),
        beryl_home_store::HomeHealthState::Healthy
    );
    let before = store.home_revision().unwrap();
    let observation = host
        .observe_historical_replacement_recovery(&store, &storage, ticket)
        .unwrap();
    assert!(matches!(
        observation,
        ComposerHostHistoricalRecovery::OriginalOutcome(DraftReplacementCommandOutcomeV1::Started(
            _
        ))
    ));
    assert_eq!(store.home_revision().unwrap(), before);
    assert_eq!(host.binding(), Some(original));
    assert!(host.historical_replacement_pending());
    let completion = finish(&mut host, &store, &state.assets(), ticket);
    assert!(completion.requires_rebind());
    assert_eq!(store.home_revision().unwrap(), before);
    assert_eq!(host.historical_replacement_ticket(), Some(ticket));
    host.acknowledge_historical_replacement(ticket, completion.binding())
        .unwrap();
    assert!(!host.historical_replacement_pending());
}
