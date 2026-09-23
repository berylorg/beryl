use super::*;

#[test]
fn pending_gate_cannot_select_an_older_turn_owned_by_the_same_child() {
    let home = TestHome::new("parent-old-child-turn");
    let (store, storage, first) = seeded(&home, FaultController::new());
    let old_turn = first.resolving_target.pending().active_turn_id();
    support::discussion_handoff::complete_resolving_turn(&store, &storage);
    let request = support::discussion_handoff::active_request_for(
        &store,
        &storage,
        id(36),
        draft_id(242),
        SyndicItemId::from_bytes([243; 16]),
        ResolutionIntentId::from_bytes([244; 16]),
        JobId::from_bytes([245; 16]),
    );
    let mut request = admit_request(&store, &storage, request);
    let DiscussionHandoffGateState::Pending {
        intent_id, job_id, ..
    } = request.child_gate.state()
    else {
        unreachable!()
    };
    request.child_gate = DiscussionHandoffGateRecord::new(
        id(36),
        request.child_gate.revision(),
        DiscussionHandoffGateState::Pending {
            intent_id,
            job_id,
            resolving_turn_id: old_turn,
        },
    );
    support::commit(
        &store,
        storage.clone(),
        support::batch([FixtureRecord::DiscussionHandoffGate(request.child_gate)]),
    );
    assert!(storage.prepare_discussion_parent(&store, request).is_err());
    store.close().unwrap();
}

#[test]
fn parent_eligibility_preserves_draft_and_rejects_wrong_binding_or_stale_proof() {
    let home = TestHome::new("parent-eligibility");
    let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
        home.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = beryl_state::BerylState::register(&mut candidate).unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(
            beryl_state::BerylState::required_domains()
                .unwrap()
                .merge(SyndicStorage::required_domains().unwrap())
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    seed_populated(&store, storage.clone());
    let request = support::discussion_handoff::active_request(
        &store,
        &storage,
        ResolutionIntentId::from_bytes([210; 16]),
        JobId::from_bytes([211; 16]),
    );
    let admission = beryl_state::BranchHandoffJobAdmission::new(
        request.intent_id,
        beryl_state::ResolutionAttemptOrdinal::new(1).unwrap(),
        request.thread_id,
        request.parent.thread_id,
        request.context_owner,
        request.context_digest,
        request.resolving_target.pending().active_turn_id(),
        beryl_state::ResolutionRequestIdentity::new(
            request.resolving_target.pending().cas_thread_id().clone(),
            request.resolving_target.cas_turn_id().clone(),
            beryl_model::DynamicToolCallId::new("resolve").unwrap(),
        ),
        beryl_state::ParentQueueOrdinal::new(request.parent.accepted_high_water),
        beryl_state::ResolutionText::new("resolution").unwrap(),
    );
    let job_id = admission.job_id();
    let request = admit_request(&store, &storage, request);
    support::converge_and_release_terminal_history(
        &store,
        storage.clone(),
        id(30),
        support::populated::source_turn(),
    );
    let draft = storage
        .current_draft(&store, id(30), limit())
        .unwrap()
        .unwrap()
        .draft()
        .clone();
    let proof = proven(&store, &storage, request);
    assert_eq!(proof.disposition(), DiscussionParentDisposition::Ready);
    assert!(proof.clone().into_archived_release().is_err());
    let mut validation = HomeCommand::new(store.home_revision().unwrap());
    validation
        .add(state.durable_jobs().admit_branch_handoff(
            state.durable_jobs().revision(&store).unwrap(),
            beryl_state::AdmitBranchHandoffJob::new(admission),
        ))
        .unwrap();
    validation
        .add_validation(proof.clone().into_ready_validation().unwrap())
        .unwrap();
    assert!(matches!(
        store.execute(validation),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    assert_eq!(
        storage
            .current_draft(&store, id(30), limit())
            .unwrap()
            .unwrap()
            .draft(),
        &draft
    );
    let mut wrong = request;
    wrong.parent_thread_id = id(40);
    assert!(storage.prepare_discussion_parent(&store, wrong).is_err());
    wrong = request;
    wrong.context_digest = beryl_model::DiscussionContextDigest::from_bytes([99; 32]);
    assert!(storage.prepare_discussion_parent(&store, wrong).is_err());
    let turn = exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        id(30),
        draft_id(221),
        SyndicItemId::from_bytes([222; 16]),
        "parent work",
        timestamp(20),
    );
    assert!(matches!(
        storage.prepare_discussion_parent(&store, request).unwrap(),
        DiscussionParentEligibility::Waiting
    ));
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(state.durable_jobs().complete_resolving_turn(
            state.durable_jobs().revision(&store).unwrap(),
            beryl_state::CompleteResolvingTurn::new(
                job_id,
                beryl_model::JobRevision::new(1).unwrap(),
            ),
        ))
        .unwrap();
    command
        .add_validation(proof.into_ready_validation().unwrap())
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        storage
            .discussion_handoff_gate(&store, id(36), limit())
            .unwrap(),
        Some(request.child_gate)
    );
    assert_eq!(
        storage
            .turn(&store, turn, limit())
            .unwrap()
            .unwrap()
            .origin_thread_id(),
        id(30)
    );
    assert_eq!(draft.submission_intent(), DraftSubmissionIntent::Ordinary);
    store.close().unwrap();
}
