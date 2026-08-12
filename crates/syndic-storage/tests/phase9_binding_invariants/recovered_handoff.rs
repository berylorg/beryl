use super::*;

struct RecoveredFixture {
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    selected: SelectedPathProof,
    proof: RecoveredInjectionProof,
    injection_generation: CasLoadedSessionGeneration,
    current_generation: CasLoadedSessionGeneration,
    snapshot: SyndicExecutionSnapshotId,
}

fn establish_recovered_valid(
    store: &HomeStore,
    storage: SyndicStorage,
    cas_thread_name: &str,
    snapshot: SyndicExecutionSnapshotId,
) -> RecoveredFixture {
    let (thread, parent, turn, selected) = non_root_pending(store, storage);
    let parent = storage.turn(store, parent, point_limit()).unwrap().unwrap();
    let represented = CasRepresentedPrefixProof::new(
        Some(parent.id()),
        selected.thread_revision(),
        parent.chain_digest(),
    );
    let injection_generation = loaded_generation(7, 11);
    let current_generation = loaded_generation(7, 12);
    let proof = RecoveredInjectionProof::new(
        RecoveryProjectionVersion::V1,
        represented,
        RecoveryItemSequenceDigest::from_bytes([67; 32]),
        RecoveryItemCount::new(1).unwrap(),
        RecoveryUtf8ByteCount::new(5).unwrap(),
        timestamp(6),
        injection_generation,
    )
    .unwrap();
    publish_valid(
        store,
        storage,
        valid_request(
            store,
            storage,
            thread,
            selected,
            CasThreadId::new(cas_thread_name).unwrap(),
            represented,
            CasLineageProof::recovered(proof),
        ),
    );
    RecoveredFixture {
        thread,
        turn,
        selected,
        proof,
        injection_generation,
        current_generation,
        snapshot,
    }
}

fn activate_recovered(store: &HomeStore, storage: SyndicStorage, fixture: &RecoveredFixture) {
    execute(
        store,
        storage.activate_binding(
            storage.revision(store).unwrap(),
            ActivateBinding::new(
                fixture.thread,
                current_binding_revision(store, storage, fixture.thread),
                current_gate_revision(store, storage, fixture.thread),
                fixture.selected,
                fixture.snapshot,
                fixture.turn,
                fixture.current_generation,
                timestamp(8),
            ),
        ),
    );
}

fn stale_from_usable(
    usable: &UsableCasBinding,
    generation: CasLoadedSessionGeneration,
) -> StaleCasBinding {
    StaleCasBinding::new(
        usable.execution().clone(),
        usable.cas_thread_id().clone(),
        Some(usable.tool_profile()),
        Some(usable.represented_prefix()),
        Some(usable.lineage()),
        Some(usable.native_turn_count()),
        Some(generation),
        "recovered projection handoff lost",
        timestamp(9),
    )
    .unwrap()
}

#[test]
fn recovered_stale_generation_may_advance_only_inside_the_injection_process() {
    let home = TestHome::new("phase13-recovered-stale-process");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = establish_recovered_valid(
        &store,
        storage,
        "phase13-recovered-stale-cas",
        SyndicExecutionSnapshotId::from_bytes([68; 16]),
    );
    let binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = binding.binding().state() else {
        panic!("recovered fixture binding is not valid");
    };

    let wrong_process = stale_from_usable(usable, loaded_generation(8, 12));
    let error = execute_outcome(
        &store,
        storage.publish_stale_binding(
            storage.revision(&store).unwrap(),
            PublishStaleBinding::new(
                fixture.thread,
                binding.binding().revision(),
                fixture.selected,
                wrong_process,
            ),
        ),
    );
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::BindingPathConflict
    ));

    let same_process = stale_from_usable(usable, fixture.current_generation);
    execute(
        &store,
        storage.publish_stale_binding(
            storage.revision(&store).unwrap(),
            PublishStaleBinding::new(
                fixture.thread,
                binding.binding().revision(),
                fixture.selected,
                same_process,
            ),
        ),
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let binding = storage
        .current_binding(&reopened, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Stale(stale) = binding.binding().state() else {
        panic!("reopened recovered binding is not stale");
    };
    assert_eq!(stale.loaded_generation(), Some(fixture.current_generation));
    assert_eq!(
        stale.observed_lineage(),
        Some(CasLineageProof::recovered(fixture.proof))
    );
    assert_eq!(
        fixture.proof.loaded_generation(),
        fixture.injection_generation
    );

    let corrupt = StaleCasBinding::new(
        stale.execution().clone(),
        stale.cas_thread_id().clone(),
        stale.observed_tool_profile(),
        stale.observed_prefix(),
        stale.observed_lineage(),
        stale.observed_native_turn_count(),
        Some(loaded_generation(8, 12)),
        stale.reason(),
        stale.observed_at(),
    )
    .unwrap();
    commit(
        &reopened,
        storage,
        batch([FixtureRecord::Binding(BindingRecord::new(
            binding.binding().thread_id(),
            binding.binding().revision(),
            binding.binding().selected_path(),
            BindingState::stale(corrupt),
        ))]),
    );
    reopened.close().unwrap();

    let mut invalid = open(home.path());
    let error = match SyndicStorage::register(&mut invalid) {
        Ok(_) => panic!("different-process stale recovery registered successfully"),
        Err(error) => error,
    };
    match error {
        DomainRegistrationError::Validation { domain, source } => {
            assert_eq!(domain, "syndic");
            assert_eq!(
                source.to_string(),
                "stale recovered lineage process generation disagrees"
            );
        }
        other => panic!("expected stale recovered-process rejection, got {other:?}"),
    }
    invalid.close().unwrap();
}

#[test]
fn recovered_abandonment_retains_exact_active_snapshot_generation() {
    let home = TestHome::new("phase13-recovered-abandonment-generation");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = establish_recovered_valid(
        &store,
        storage,
        "phase13-recovered-abandonment-cas",
        SyndicExecutionSnapshotId::from_bytes([69; 16]),
    );
    activate_recovered(&store, storage, &fixture);
    let binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Active(active) = binding.binding().state() else {
        panic!("recovered fixture binding is not active");
    };
    let snapshot = storage
        .execution_snapshot(&store, fixture.snapshot, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.loaded_generation(), fixture.current_generation);
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let target = AcceptedRouteLostTarget::AwaitingSteering(PendingSteeringTargetProof::new(
        binding.binding().revision(),
        active.snapshot_id(),
        active.turn_id(),
        active.usable().cas_thread_id().clone(),
    ));

    let injection_stale = stale_from_usable(active.usable(), fixture.injection_generation);
    let error = execute_outcome(
        &store,
        storage.abandon_active_binding(
            storage.revision(&store).unwrap(),
            AbandonActiveBinding::new(
                fixture.thread,
                binding.binding().revision(),
                gate.selected_route().unwrap().generation(),
                target.clone(),
                fixture.selected,
                injection_stale,
            ),
        ),
    );
    assert!(matches!(
        typed_error(&error),
        SyndicMutationError::BindingStateConflict
    ));
    assert!(matches!(
        storage
            .current_binding(&store, fixture.thread, point_limit())
            .unwrap()
            .unwrap()
            .binding()
            .state(),
        BindingState::Active(_)
    ));

    let current_stale = stale_from_usable(active.usable(), fixture.current_generation);
    execute(
        &store,
        storage.abandon_active_binding(
            storage.revision(&store).unwrap(),
            AbandonActiveBinding::new(
                fixture.thread,
                binding.binding().revision(),
                gate.selected_route().unwrap().generation(),
                target,
                fixture.selected,
                current_stale,
            ),
        ),
    );
    let binding = storage
        .current_binding(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Stale(stale) = binding.binding().state() else {
        panic!("recovered abandonment did not publish stale provenance");
    };
    assert_eq!(stale.loaded_generation(), Some(fixture.current_generation));
    assert_eq!(
        stale.observed_lineage(),
        Some(CasLineageProof::recovered(fixture.proof))
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let binding = storage
        .current_binding(&reopened, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Stale(stale) = binding.binding().state() else {
        panic!("reopened recovered abandonment is not stale");
    };
    assert_eq!(stale.loaded_generation(), Some(fixture.current_generation));
    assert_eq!(
        stale.observed_lineage(),
        Some(CasLineageProof::recovered(fixture.proof))
    );
    reopened.close().unwrap();
}
