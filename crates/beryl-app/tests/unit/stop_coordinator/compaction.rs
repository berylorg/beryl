use super::*;
use beryl_model::{
    CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasNativeTurnCount, CasProcessGeneration,
};
use syndic_storage::{
    ActivityQuerySource, CasLineageProof, CasRepresentedPrefixProof, ClaimCompactionDispatch,
    CompactionAdmissionRead, CompactionAttemptNonce, CompactionOperationNonce,
    CompactionProviderEvent, CompactionProviderSequence, CompactionThreadStatus, NativeCasLineage,
    PublishCompactionProviderEvent, PublishValidBinding, TurnLifecycle, empty_selected_path_digest,
};

#[test]
fn first_work_compaction_stop_abandons_without_activity_enrollment() {
    let fixture = compaction_fixture(216);
    let activity_before = fixture
        .storage
        .activity_query_head(&fixture.home, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(activity_before.source(), None);
    assert!(!activity_before.source_active());
    assert_eq!(
        fixture
            .runtime_source
            .interest()
            .unwrap()
            .with_activity_for_test(
                ActivityQuerySource::new(fixture.thread, fixture.turn),
                |_| (),
            ),
        None
    );
    let owner = match fixture
        .coordinator
        .coordinate(
            &fixture.router,
            fixture.proof.clone(),
            StopCause::InterruptingApproval,
            &fixture.runtime_source,
        )
        .unwrap()
    {
        StopOwnership::Primary(owner) => owner,
        StopOwnership::Joined { .. } => panic!("first provider stop owns dispatch"),
    };
    let operation = owner.operation_id();
    assert!(
        matches!(owner.settle_before_dispatch().unwrap(), StopDispatchSettlement::Abandoned(id) if id == operation)
    );
    let stop = fixture
        .storage
        .stop_operation(&fixture.home, operation, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(stop.state(), StopOperationState::Abandoned(_)));
    let turn = fixture
        .storage
        .turn_state(&fixture.home, fixture.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(turn.lifecycle(), TurnLifecycle::Incomplete);
    assert_eq!(
        fixture
            .storage
            .activity_query_head(&fixture.home, fixture.thread, point_limit())
            .unwrap()
            .unwrap(),
        activity_before
    );
    assert_eq!(
        fixture
            .runtime_source
            .interest()
            .unwrap()
            .with_activity_for_test(
                ActivityQuerySource::new(fixture.thread, fixture.turn),
                |_| (),
            ),
        None
    );
    fixture
        .home
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    assert!(
        fixture
            .coordinator
            .abandon_for_authority_loss(fixture.thread, fixture.turn)
            .unwrap()
    );
}

fn compaction_fixture(seed: u8) -> StopFixture {
    let directory = tempfile::tempdir().unwrap();
    let mut candidate = beryl_home_store::HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let owner = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let thread = SyndicThreadId::from_bytes([seed; 16]);
    execute(
        &owner,
        storage.create_thread(
            storage.revision(&owner).unwrap(),
            CreateThread::ordinary(
                thread,
                SyndicDraftId::from_bytes([seed.wrapping_add(1); 16]),
                exact_cas::execution_binding(),
                timestamp(1),
                DraftEditHistoryPolicyV1::new(65_536, 1).unwrap(),
            ),
        ),
    );
    let current = storage
        .current_binding(&owner, thread, point_limit())
        .unwrap()
        .unwrap();
    let selected = current.binding().selected_path();
    let represented = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let lineage = CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap();
    execute(
        &owner,
        storage.publish_valid_binding(
            storage.revision(&owner).unwrap(),
            PublishValidBinding::new(
                thread,
                current.binding().revision(),
                selected,
                exact_cas::execution_binding(),
                CasThreadId::new(format!("first-compaction-{seed}")).unwrap(),
                represented,
                CasNativeTurnCount::ZERO,
                exact_cas::tool_profile(),
                lineage,
            ),
        ),
    );
    let CompactionAdmissionRead::Admissible(admissible) = storage
        .compaction_admission_read(&owner, thread, point_limit())
        .unwrap()
    else {
        panic!("empty idle thread permits compaction")
    };
    let attempt = CompactionAttemptNonce::from_bytes([seed.wrapping_add(2); 16]);
    let admission = admissible.admission(
        CompactionOperationNonce::from_bytes([seed.wrapping_add(3); 16]),
        attempt,
        CasLoadedSessionGeneration::new(
            CasProcessGeneration::new(1).unwrap(),
            CasLoadedThreadGeneration::new(1).unwrap(),
        ),
        timestamp(2),
    );
    let compaction = admission.operation_id();
    committed(owner.execute_current(storage.current_admit_compaction_operation(admission)));
    let operation = storage
        .compaction_operation(&owner, compaction, point_limit())
        .unwrap()
        .unwrap();
    committed(
        owner.execute_current(storage.current_claim_compaction_dispatch(
            ClaimCompactionDispatch::new(compaction, operation.revision(), attempt),
        )),
    );
    for (offset, event) in [
        CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Active),
        CompactionProviderEvent::TurnStarted(CasTurnId::new("first-compaction-turn").unwrap()),
    ]
    .into_iter()
    .enumerate()
    {
        let operation = storage
            .compaction_operation(&owner, compaction, point_limit())
            .unwrap()
            .unwrap();
        let sequence = operation
            .provider_frontier()
            .map_or(CompactionProviderSequence::FIRST, |prior| {
                prior.checked_next().unwrap()
            });
        committed(
            owner.execute_current(storage.current_publish_compaction_provider_event(
                PublishCompactionProviderEvent::new(
                    compaction,
                    operation.revision(),
                    sequence,
                    event,
                    timestamp(3 + offset as u64),
                ),
            )),
        );
    }
    let StopAdmissionRead::Admissible(admissible) = storage
        .stop_admission_read(&owner, thread, point_limit())
        .unwrap()
    else {
        panic!("known provider turn permits stop")
    };
    let target = admissible.target().clone();
    let turn = target.turn_id();
    let (runtime_harness, interest) = stop_runtime(&owner);
    let (runtime_source, custody) =
        crate::cas_projection::service_config::ConnectionRuntimeInterestSource::retained_for_test(
            interest,
        );
    let home = Arc::new(owner.service_reference());
    let command_gate = crate::cas_projection::persistent_failure::MasterCommandGate::new(
        Default::default(),
        crate::cas_projection::persistent_failure::ProjectionServiceGeneration::allocate().unwrap(),
        None,
    );
    let coordinator = Arc::new(StopCoordinator::new(
        &home,
        owner.home_id(),
        owner.health().generation().unwrap(),
        storage.clone(),
        command_gate.authorizer(),
        crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal::new(),
    ));
    let router = Arc::new(
        EventRouter::new_with_scheduler(
            target.runtime_id(),
            target.loaded_generation().process(),
            NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed),
            crate::cas_projection::accepted_input_scheduler::AcceptedInputSchedulerSignal::new(),
            command_gate.authorizer(),
            None,
        )
        .unwrap(),
    );
    let command = router.authorize_command_for_test().unwrap();
    router
        .register(
            &command,
            LoadedThreadKey {
                runtime_id: target.runtime_id(),
                process_generation: target.loaded_generation().process(),
                cas_thread_id: target.cas_thread_id().clone(),
            },
            thread,
            target.loaded_generation(),
            owner.health().generation().unwrap().get(),
            Duration::from_secs(1),
            TargetTurnRegistration::ContextCompaction(
                crate::cas_projection::context_compaction::ContextCompactionTargetAuthority::new(
                    compaction, turn,
                ),
            ),
        )
        .unwrap();
    drop(command);
    router.activate_stop_target_for_test(target.cas_thread_id(), target.cas_turn_id().clone());
    let proof = router
        .stop_target(thread, target.cas_thread_id(), target.cas_turn_id())
        .unwrap();
    StopFixture {
        _directory: directory,
        owner,
        home,
        storage,
        coordinator,
        command_gate,
        router,
        thread,
        turn,
        target,
        proof,
        runtime_source,
        runtime_harness,
        _runtime_custody: Box::new(custody),
    }
}

fn committed(outcome: CommandOutcome) {
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "compaction fixture command: {outcome:?}"
    );
}
