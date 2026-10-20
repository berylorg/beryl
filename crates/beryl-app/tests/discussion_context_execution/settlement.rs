use super::*;

#[test]
fn successful_seed_cancelled_before_publication_keeps_success_proof_and_retires_target() {
    let mut fixture = Fixture::new(189);
    let child = create_discussion(&mut fixture, "cancelled successful source");
    let cancel = fixture.cancellation.clone();
    let server = WireServer::spawn(vec![
        fork(source_thread(&fixture, child)),
        Step::Inject {
            target: TARGET,
            items: vec![assistant(&frame(&fixture, child))],
            reply: Reply::SuccessAfter(Box::new(move || cancel.cancel())),
        },
        Step::Unsubscribe(TARGET),
    ]);
    let mut session = admit(&fixture, &server, 92_190);
    let observer = session.recovery_replay_diagnostics_observer();
    let error = obtain(&fixture, &mut session, child, 1_000_000).unwrap_err();
    assert!(
        matches!(error, ProjectionExecutionError::Cancelled),
        "post-success cancellation must preserve successful transport: {error:?}"
    );
    assert_target_retired(&fixture, child);
    let home = fixture.home();
    let binding = fixture
        .storage
        .current_binding(&home, child, point())
        .unwrap()
        .unwrap();
    let BindingState::Stale(stale) = binding.binding().state() else {
        panic!("cancelled target remained usable")
    };
    assert!(
        matches!(
            stale.observed_lineage(),
            Some(CasLineageProof::ContextAware(_))
        ),
        "exact successful seed must survive target disposal"
    );
    drop(home);
    let snapshot = observer.snapshot().unwrap();
    assert!(snapshot.released());
    assert_eq!(snapshot.logical_items(), 1);
    assert_eq!(snapshot.final_capacity().unwrap().pages().leased, 0);
    close(session, server);
}

#[test]
fn successful_seed_with_uncertain_publication_keeps_original_home_reconciliation() {
    use beryl_app::cas_projection::ProjectionPublicationFailure;
    use beryl_home_store::{
        ReconciliationResolution,
        test_faults::{FaultController, FaultPoint},
    };

    let faults = FaultController::new();
    let mut fixture = Fixture::with_faults(190, faults.clone());
    let child = create_discussion(&mut fixture, "uncertain publication source");
    let arm = faults.clone();
    let server = WireServer::spawn(vec![
        fork(source_thread(&fixture, child)),
        Step::Inject {
            target: TARGET,
            items: vec![assistant(&frame(&fixture, child))],
            reply: Reply::SuccessAfter(Box::new(move || {
                arm.fail_next(FaultPoint::AfterCommitBeforePersist)
            })),
        },
        Step::Unsubscribe(TARGET),
    ]);
    let mut session = admit(&fixture, &server, 92_191);
    let observer = session.recovery_replay_diagnostics_observer();
    let home = fixture.home();
    let error = obtain(&fixture, &mut session, child, 1_000_000).unwrap_err();
    let primary = match &error {
        ProjectionExecutionError::AbandonmentFailed { primary, .. } => primary.as_ref(),
        error => error,
    };
    assert!(
        matches!(primary,ProjectionExecutionError::Publication(error)
        if matches!(error.as_ref(),ProjectionPublicationFailure::CommandIndeterminate{..})),
        "original publication ambiguity must survive cleanup: {error:?}"
    );
    let original = home.pending_reconciliations();
    assert_eq!(original.len(), 1);
    let handle = &original[0];
    assert!(matches!(
        home.reconcile(handle).unwrap(),
        ReconciliationResolution::ExactNew { .. }
    ));
    assert!(home.pending_reconciliations().is_empty());
    let binding = fixture
        .storage
        .current_binding(&home, child, point())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = binding.binding().state() else {
        panic!("exact original publication did not reconcile as installed")
    };
    assert_eq!(usable.cas_thread_id().as_str(), TARGET);
    assert!(matches!(usable.lineage(), CasLineageProof::ContextAware(_)));
    drop(home);
    assert!(observer.snapshot().unwrap().released());
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|request| request["method"] == "thread/inject_items")
            .count(),
        1
    );
    close(session, server);
}

#[test]
fn successful_seed_with_injected_clock_error_retires_target_without_a_completion_proof() {
    use beryl_app::cas_projection::test_faults::{
        ProjectionCompletionClockFault, install_projection_completion_clock_fault,
    };
    use beryl_model::{CasProcessGeneration, CasThreadId};
    use syndic_storage::{NativeCasLineage, NativeProjectionPlan, NativeProjectionRequest};

    for (seed, fault) in [
        (191, ProjectionCompletionClockFault::BeforeUnixEpoch),
        (192, ProjectionCompletionClockFault::OutOfRange),
    ] {
        let mut fixture = Fixture::new(seed);
        let child = create_discussion(&mut fixture, "clock fault source");
        let server = WireServer::spawn(vec![
            fork(source_thread(&fixture, child)),
            super::seed(vec![assistant(&frame(&fixture, child))]),
            Step::Unsubscribe(TARGET),
        ]);
        let process = 94_000 + u64::from(seed);
        let mut session = admit(&fixture, &server, process);
        let observer = session.recovery_replay_diagnostics_observer();
        let home = fixture.home();
        let plan = fixture
            .storage
            .prepare_native_projection(
                &home,
                &NativeProjectionRequest::new(
                    child,
                    fixture.selected_path(child),
                    crate::syndic::execution_binding(),
                    beryl_app::conversation_tools::ConversationToolRegistry::canonical().profile(),
                ),
                point(),
            )
            .unwrap();
        let NativeProjectionPlan::Fork {
            basis,
            native_turn_count,
            ..
        } = plan
        else {
            panic!("clock fixture requires exact native fork")
        };
        assert_eq!(native_turn_count.get(), 1);
        let native_lineage =
            CasLineageProof::native(NativeCasLineage::Fork, basis.represented_prefix()).unwrap();
        let _fault = install_projection_completion_clock_fault(
            home.home_id(),
            child,
            CasThreadId::new(TARGET).unwrap(),
            CasProcessGeneration::new(process).unwrap(),
            fault,
        );
        let error = obtain(&fixture, &mut session, child, 1_000_000).unwrap_err();
        assert!(
            matches!(
                (fault, &error),
                (
                    ProjectionCompletionClockFault::BeforeUnixEpoch,
                    ProjectionExecutionError::SystemClockBeforeUnixEpoch(_)
                ) | (
                    ProjectionCompletionClockFault::OutOfRange,
                    ProjectionExecutionError::SystemClockOutOfRange
                )
            ),
            "exact injected clock result must survive successful target cleanup: {error:?}"
        );
        assert_target_retired(&fixture, child);
        let binding = fixture
            .storage
            .current_binding(&home, child, point())
            .unwrap()
            .unwrap();
        let BindingState::Stale(stale) = binding.binding().state() else {
            panic!("clock fault published usable authority")
        };
        assert_eq!(stale.observed_lineage(), Some(native_lineage));
        assert_eq!(stale.observed_prefix(), Some(basis.represented_prefix()));
        assert_eq!(stale.observed_native_turn_count(), Some(native_turn_count));
        assert!(stale.loaded_generation().is_some());
        let snapshot = observer.snapshot().unwrap();
        assert!(snapshot.released());
        assert_eq!(snapshot.logical_items(), 1);
        assert_eq!(snapshot.final_capacity().unwrap().pages().leased, 0);
        assert_eq!(
            server
                .requests()
                .iter()
                .filter(|request| request["method"] == "thread/inject_items")
                .count(),
            1
        );
        drop(home);
        close(session, server);
    }
}

#[test]
fn successful_seed_before_source_ordering_floor_retires_target_with_native_fork_provenance() {
    use beryl_model::{
        CasLoadedSessionGeneration, CasLoadedThreadGeneration, CasProcessGeneration, CasThreadId,
    };
    use syndic_storage::{
        ContextInjectionObservation, NativeCasLineage, NativeProjectionPlan,
        NativeProjectionRequest, PublishValidBinding, SyndicTimestamp, SyndicValueError,
    };

    let mut fixture = Fixture::new(194);
    let source = create_discussion(&mut fixture, "first context source");
    let process = 95_194;
    let future = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
    .checked_add(86_400_000)
    .unwrap();
    let future = SyndicTimestamp::from_unix_millis(future);
    let home = fixture.home();
    let request = NativeProjectionRequest::new(
        source,
        fixture.selected_path(source),
        crate::syndic::execution_binding(),
        beryl_app::conversation_tools::ConversationToolRegistry::canonical().profile(),
    );
    let NativeProjectionPlan::Fork {
        basis,
        native_turn_count,
        ..
    } = fixture
        .storage
        .prepare_native_projection(&home, &request, point())
        .unwrap()
    else {
        panic!("future source fixture requires a native seed")
    };
    let projection = fixture
        .storage
        .prepare_native_context_seed(&home, basis, Some(1_000_000))
        .unwrap();
    let observation = ContextInjectionObservation::new(
        projection.item_count(),
        projection.utf8_bytes(),
        projection.sequence_digest(),
        future,
    )
    .unwrap();
    let source_lineage = basis
        .context()
        .unwrap()
        .complete_native(
            NativeCasLineage::Fork,
            CasLoadedSessionGeneration::new(
                CasProcessGeneration::new(process).unwrap(),
                CasLoadedThreadGeneration::new(900).unwrap(),
            ),
            Some(observation),
        )
        .unwrap();
    command::execute_fixture_contribution(
        &home,
        fixture.storage.publish_valid_binding(
            fixture.storage.revision(&home).unwrap(),
            PublishValidBinding::from_native(
                basis,
                crate::syndic::execution_binding(),
                CasThreadId::new("future-context-source").unwrap(),
                native_turn_count,
                source_lineage,
            ),
        ),
    );
    let pending = fixture
        .storage
        .pending_dispatch_evidence(&home, source, point())
        .unwrap()
        .unwrap();
    let submitted = crate::syndic::SubmittedTurn {
        turn: pending.selected_path().tail().unwrap(),
        user_item: pending.item_id(),
    };
    drop(home);
    fixture.advance_clock_to(future.unix_millis() + 1);
    fixture.complete_with_assistant_on(source, submitted, "nested future source");
    fixture.thread = source;
    let child =
        create_discussion_from_completed(&mut fixture, submitted, "nested future source", 228);
    let source_binding = fixture.native_source(child);
    let CasLineageProof::ContextAware(source_proof) = source_binding.binding().lineage() else {
        panic!("nested source lost its prepared context proof")
    };
    assert_eq!(source_proof.ordering_floor(), future);
    assert_eq!(source_thread(&fixture, child), "future-context-source");

    let home = fixture.home();
    let NativeProjectionPlan::Fork {
        basis,
        native_turn_count,
        ..
    } = fixture
        .storage
        .prepare_native_projection(
            &home,
            &NativeProjectionRequest::new(
                child,
                fixture.selected_path(child),
                crate::syndic::execution_binding(),
                beryl_app::conversation_tools::ConversationToolRegistry::canonical().profile(),
            ),
            point(),
        )
        .unwrap()
    else {
        panic!("nested context requires an exact native fork")
    };
    assert!(native_turn_count.get() > 0);
    let native_lineage =
        CasLineageProof::native(NativeCasLineage::Fork, basis.represented_prefix()).unwrap();
    let server = WireServer::spawn(vec![
        fork(source_thread(&fixture, child)),
        seed(vec![assistant(&frame(&fixture, child))]),
        Step::Unsubscribe(TARGET),
    ]);
    let mut session = admit(&fixture, &server, process);
    let observer = session.recovery_replay_diagnostics_observer();
    let error = obtain(&fixture, &mut session, child, 1_000_000).unwrap_err();
    assert!(
        matches!(
            error,
            ProjectionExecutionError::SyndicValue(SyndicValueError::InvalidLineageProof {
                reason: "context completion does not match prepared native disposition"
            })
        ),
        "successful injection before the source floor must retain the exact proof refusal: {error:?}"
    );
    assert_target_retired(&fixture, child);
    let binding = fixture
        .storage
        .current_binding(&home, child, point())
        .unwrap()
        .unwrap();
    let BindingState::Stale(stale) = binding.binding().state() else {
        panic!("ordering-floor refusal published usable context authority")
    };
    assert_eq!(stale.observed_lineage(), Some(native_lineage));
    assert_eq!(stale.observed_prefix(), Some(basis.represented_prefix()));
    assert_eq!(stale.observed_native_turn_count(), Some(native_turn_count));
    assert!(stale.loaded_generation().is_some());
    let source_binding = fixture
        .storage
        .current_binding(&home, source, point())
        .unwrap()
        .unwrap();
    let BindingState::Valid(unchanged_source) = source_binding.binding().state() else {
        panic!("disposing the nested target changed its completed source")
    };
    assert_eq!(
        unchanged_source.lineage(),
        CasLineageProof::ContextAware(source_proof)
    );
    let snapshot = observer.snapshot().unwrap();
    assert!(snapshot.released());
    assert_eq!(snapshot.logical_items(), 1);
    assert_eq!(snapshot.final_capacity().unwrap().pages().leased, 0);
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|request| request["method"] == "thread/inject_items")
            .count(),
        1
    );
    drop(home);
    close(session, server);
}
