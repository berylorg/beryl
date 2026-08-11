use beryl_home_store::{
    CommandOutcome, HomeOpenOptions, HomeSchemaVersion, HomeStore, StorageCommitState,
};
use beryl_model::{RuntimeMode, SyndicDraftId};
use beryl_state::AssetOwner;
use syndic_storage::{AcceptedInputAdmissionProof, AcceptedInputRecord};

use super::{
    AcceptedInputReplayContext, AcceptedInputReplayError,
    AcceptedInputReplayFactory, ProjectionCancellationToken,
    fixture::{self, Fixture, time},
};

#[test]
fn preparation_rejects_cancellation_identity_drift_and_missing_input() {
    let fixture = Fixture::new(40);
    let record = fixture.accept_text("exact durable accepted input");

    let cancelled = ProjectionCancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        fixture.replay_factory(record.clone(), &cancelled),
        Err(AcceptedInputReplayError::Cancelled)
    ));

    let changed = AcceptedInputRecord::new(
        record.id(),
        record.thread_id(),
        record.ordinal(),
        record.admission(),
        record.route_generation(),
        record.content(),
        record.asset_reference_set(),
        time(record.admitted_at().unix_millis() + 1),
    )
    .unwrap();
    assert!(matches!(
        fixture.replay_factory(changed, &ProjectionCancellationToken::new()),
        Err(AcceptedInputReplayError::AcceptedInputChanged { input_id })
            if input_id == record.id()
    ));

    let missing_source = SyndicDraftId::from_bytes([0xee; 16]);
    let missing_id = missing_source.accepted_input_id();
    let admission = record.admission();
    let missing_admission = AcceptedInputAdmissionProof::new(
        admission.expected_thread_revision(),
        missing_source,
        admission.expected_draft_revision(),
        admission.expected_gate_revision(),
        admission.replacement_draft_id(),
    )
    .unwrap();
    let missing = AcceptedInputRecord::new(
        missing_id,
        record.thread_id(),
        record.ordinal(),
        missing_admission,
        record.route_generation(),
        record.content(),
        record.asset_reference_set(),
        record.admitted_at(),
    )
    .unwrap();
    assert!(matches!(
        fixture.replay_factory(missing, &ProjectionCancellationToken::new()),
        Err(AcceptedInputReplayError::AcceptedInputMissing { input_id })
            if input_id == missing_id
    ));

    let other_directory = tempfile::tempdir().unwrap();
    let other = HomeStore::open(HomeOpenOptions::new(
        other_directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let owner_head = fixture
        .state
        .assets()
        .owner_head(&fixture.store, AssetOwner::AcceptedInput(record.id()))
        .unwrap();
    assert!(matches!(
        AcceptedInputReplayFactory::prepare(
            &fixture.store,
            fixture.storage,
            fixture.state.assets(),
            AcceptedInputReplayContext::new(
                other.home_id(),
                fixture.store.health().generation().unwrap(),
                RuntimeMode::host(),
            ),
            record,
            owner_head,
            &ProjectionCancellationToken::new(),
        ),
        Err(AcceptedInputReplayError::HomeIdentityMismatch { .. })
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn prepared_source_rechecks_the_exact_accepted_record_before_replay() {
    use beryl_backend::StreamedInputSourceError;
    use syndic_storage::test_faults::{FixtureBatch, FixtureRecord};

    use super::fixture::execute_one;

    let fixture = Fixture::new(80);
    let record = fixture.accept_text("durable drift after preparation");
    let factory = fixture
        .replay_factory(record.clone(), &ProjectionCancellationToken::new())
        .unwrap();
    let changed = AcceptedInputRecord::new(
        record.id(),
        record.thread_id(),
        record.ordinal(),
        record.admission(),
        record.route_generation(),
        record.content(),
        record.asset_reference_set(),
        time(record.admitted_at().unix_millis() + 1),
    )
    .unwrap();
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::AcceptedInput(changed)).unwrap();
    execute_one(
        &fixture.store,
        fixture.storage.fixture_contribution(
            fixture.storage.revision(&fixture.store).unwrap(),
            batch,
        ),
    );

    let mut source = factory.fresh_source();
    assert!(matches!(
        source.begin_pass(
            &fixture.store,
            &ProjectionCancellationToken::new()
        ),
        Err(StreamedInputSourceError::InvalidSource)
    ));
}

#[cfg(feature = "test-faults")]
#[test]
fn recovered_home_generation_invalidates_factory_and_preparation_context() {
    use beryl_backend::StreamedInputSourceError;
    use beryl_home_store::{
        HomeCommand,
        test_faults::{FaultController, FaultPoint},
    };
    use beryl_model::{SyndicDraftId, SyndicThreadId};
    use syndic_storage::CreateThread;

    let faults = FaultController::new();
    let fixture = Fixture::with_faults(100, faults.clone());
    let record = fixture.accept_text("generation-bound accepted input");
    let generation = fixture.store.health().generation().unwrap();
    let context = AcceptedInputReplayContext::new(
        fixture.store.home_id(),
        generation,
        RuntimeMode::host(),
    );
    let owner_head = fixture
        .state
        .assets()
        .owner_head(&fixture.store, AssetOwner::AcceptedInput(record.id()))
        .unwrap();
    let factory = AcceptedInputReplayFactory::prepare(
        &fixture.store,
        fixture.storage,
        fixture.state.assets(),
        context.clone(),
        record.clone(),
        owner_head.clone(),
        &ProjectionCancellationToken::new(),
    )
    .unwrap();

    let mut command = HomeCommand::new(fixture.store.home_revision().unwrap());
    command
        .add(fixture.storage.create_thread(
            fixture.storage.revision(&fixture.store).unwrap(),
            CreateThread::ordinary(
                SyndicThreadId::from_bytes([110; 16]),
                SyndicDraftId::from_bytes([111; 16]),
                fixture::execution_binding(110),
                time(90),
            ),
        ))
        .unwrap();
    faults.fail_next(FaultPoint::BeforeCommit);
    match fixture.store.execute(command) {
        CommandOutcome::NotCommitted { evidence } => {
            assert_eq!(
                evidence.storage_commit_state(),
                Some(StorageCommitState::NotCommitted)
            );
        }
        outcome @ CommandOutcome::Committed { .. } => panic!(
            "before-commit fault must not commit the command: {outcome:?}"
        ),
        outcome @ CommandOutcome::Indeterminate { .. } => {
            panic!("before-commit fault must not be indeterminate: {outcome:?}")
        }
    }
    faults.fail_next(FaultPoint::BeforeVerification);
    assert!(fixture.store.verify_health().is_err());
    let recovery = fixture.store.recover_same_home().unwrap();
    assert!(recovery.generation() > generation);

    assert!(matches!(
        AcceptedInputReplayFactory::prepare(
            &fixture.store,
            fixture.storage,
            fixture.state.assets(),
            context,
            record,
            owner_head,
            &ProjectionCancellationToken::new(),
        ),
        Err(AcceptedInputReplayError::HomeGenerationMismatch {
            expected,
            actual: Some(actual),
            ..
        }) if expected == generation && actual == recovery.generation()
    ));
    let mut source = factory.fresh_source();
    assert!(matches!(
        source.begin_pass(
            &fixture.store,
            &ProjectionCancellationToken::new()
        ),
        Err(StreamedInputSourceError::SourceIdentityMismatch { .. })
    ));
}
