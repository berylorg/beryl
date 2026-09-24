use super::*;

#[test]
fn concurrent_duplicate_preparations_publish_only_one_attempt() {
    let fixture = Fixture::new();
    let prepare = |intent| {
        let DiscussionResolutionAdmission::Prepared(prepared) = fixture
            .settlement
            .prepare_resolution_for_test(
                &fixture.context("same-call"),
                ResolutionIntentId::from_bytes([intent; 16]),
                ResolutionText::new("same text").unwrap(),
                CommandCancellation::new(),
            )
            .unwrap()
        else {
            panic!("expected preparation");
        };
        prepared
    };
    let first = prepare(210);
    let second = prepare(211);
    assert!(matches!(
        first.execute(),
        DiscussionSettlementOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert!(matches!(
        second.execute(),
        DiscussionSettlementOutcome::NotCommitted { .. }
    ));
    assert!(
        matches!(fixture.admit("same-call", 212, "replacement").unwrap(),
        DiscussionResolutionOutcome::Existing(job) if job == JobId::from_bytes([210; 16]))
    );
    let command = fixture.service.live_home_command().unwrap();
    assert!(
        fixture
            .state
            .durable_jobs()
            .job(command.home(), JobId::from_bytes([211; 16]))
            .unwrap()
            .is_none()
    );
    drop(command);
    fixture.close();
}

#[test]
fn contradictory_latest_pointer_cannot_acknowledge_a_live_attempt() {
    let fixture = Fixture::new();
    let job = admitted(fixture.admit("resolve", 210, "original").unwrap());
    let command = fixture.service.live_home_command().unwrap();
    let store = command.home();
    let jobs = fixture.state.durable_jobs();
    let prior = jobs.job(store, job).unwrap().unwrap();
    support::discussion_input::committed(
        store,
        jobs.corrupt_handoff_job_index_for_test(
            jobs.revision(store).unwrap(),
            prior,
            beryl_state::HandoffJobIndexFault::LatestJobAtKey(JobId::from_bytes([216; 16])),
        ),
    );
    drop(command);
    assert!(matches!(
        fixture.admit("different", 211, "new"),
        Err(DiscussionSettlementError::IdentityMismatch)
    ));
    fixture.close();
}

#[test]
fn archived_parent_rejects_fresh_nested_resolution_without_a_job() {
    let fixture = Fixture::new();
    admitted(fixture.admit("parent", 210, "parent resolution").unwrap());
    let command = fixture.service.live_home_command().unwrap();
    let store = command.home();
    support::discussion_creation::create_child(store, &fixture.syndic, id(36), 230, 231);
    let source = support::discussion_handoff::active_request_for(
        store,
        &fixture.syndic,
        id(230),
        support::draft_id(232),
        beryl_model::SyndicItemId::from_bytes([233; 16]),
        ResolutionIntentId::from_bytes([234; 16]),
        JobId::from_bytes([234; 16]),
    );
    let limit = SyndicPointReadLimit::new(400_000).unwrap();
    let gate = fixture
        .syndic
        .discussion_handoff_gate(store, id(36), limit)
        .unwrap()
        .unwrap();
    let attributes = fixture
        .syndic
        .thread_attributes(store, id(36), limit)
        .unwrap()
        .unwrap();
    let archive = fixture
        .syndic
        .prepare_discussion_handoff(
            store,
            DiscussionHandoffMutation::ReleaseAndArchive {
                expected: gate,
                attributes_revision: attributes.revision(),
                archived_at: support::timestamp(400),
            },
        )
        .unwrap();
    support::discussion_input::committed(store, archive.contribution());
    let context = BranchDiscussionResolutionContext::for_test(
        &fixture.service,
        id(230),
        source.resolving_target.pending().active_turn_id(),
        ResolutionRequestIdentity::new(
            source.resolving_target.pending().cas_thread_id().clone(),
            source.resolving_target.cas_turn_id().clone(),
            DynamicToolCallId::new("nested").unwrap(),
        ),
    );
    drop(command);
    let outcome = fixture
        .service
        .admit_discussion_resolution(
            &fixture.settlement,
            &context,
            ResolutionIntentId::from_bytes([234; 16]),
            ResolutionText::new("nested resolution").unwrap(),
            CommandCancellation::new(),
        )
        .unwrap()
        .publish(|outcome| outcome);
    assert!(matches!(
        outcome,
        DiscussionResolutionOutcome::ParentArchived
    ));
    assert!(matches!(
        fixture.admit("later", 235, "archived").unwrap(),
        DiscussionResolutionOutcome::DiscussionArchived
    ));
    assert!(matches!(
        fixture.admit("parent", 236, "changed").unwrap(),
        DiscussionResolutionOutcome::Existing(_)
    ));
    let command = fixture.service.live_home_command().unwrap();
    assert!(
        fixture
            .state
            .durable_jobs()
            .latest_attempt(command.home(), id(230))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture
            .syndic
            .discussion_handoff_gate(command.home(), id(230), limit)
            .unwrap()
            .unwrap()
            .state(),
        DiscussionHandoffGateState::Open
    );
    drop(command);
    fixture.close();
}

#[test]
fn reopened_service_rejects_old_context_even_for_an_existing_request() {
    let fixture = Fixture::new();
    admitted(fixture.admit("resolve", 210, "original").unwrap());
    let context = fixture.context("resolve");
    let Fixture {
        directory,
        service,
        settlement,
        operations,
        state,
        syndic,
        faults,
        source,
    } = fixture;
    drop((settlement, operations, state, syndic, faults, source));
    service.close().unwrap();
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = BerylState::register(&mut candidate).unwrap();
    let syndic = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(
            BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    let process = ProcessAdmissionGate::new();
    let settlement = DiscussionSettlementService::new(
        DiscussionSettlementOperations::new(process.clone(), NonZeroUsize::new(1).unwrap()),
        store.service_reference(),
        state.clone(),
        syndic.clone(),
    );
    let service = ProjectionConnectionService::new(
        process,
        store,
        syndic,
        ProjectionServiceConfig::try_new(8, 4, MinimumTurnCaptureReserve::try_new(1).unwrap())
            .unwrap(),
        Box::new(IdleProvider),
    )
    .unwrap();
    assert!(matches!(
        service.admit_discussion_resolution(
            &settlement,
            &context,
            ResolutionIntentId::from_bytes([211; 16]),
            ResolutionText::new("repeat").unwrap(),
            CommandCancellation::new()
        ),
        Err(DiscussionSettlementError::IdentityMismatch)
    ));
    drop(settlement);
    service.close().unwrap();
}
