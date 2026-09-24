#![cfg(feature = "test-faults")]

#[path = "generated_parent_execution/corruption.rs"]
mod corruption;
mod support;

use beryl_home_store::*;
use beryl_model::*;
use syndic_storage::*;

const TEXT: &str = "the exact resolution";

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(400_000).unwrap()
}

fn seeded(
    home: &support::TestHome,
) -> (HomeStore, SyndicStorage, DiscussionParentExecutionRequest) {
    let mut candidate = support::open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let (input, receipt, gate) = support::generated_input::seed(&store, storage.clone(), TEXT);
    let request = DiscussionParentExecutionRequest {
        child_gate: gate,
        parent_thread_id: input.thread_id(),
        input_id: input.id(),
        turn_id: receipt.parent_turn_id,
        context_owner: receipt.context_owner,
        context_digest: receipt.context_digest,
        resolution: TEXT.to_owned(),
    };
    (store, storage, request)
}

fn proven(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: &DiscussionParentExecutionRequest,
) -> PreparedDiscussionParentExecution {
    let DiscussionParentExecution::Proven(proof) = storage
        .prepare_discussion_parent_execution(store, request.clone())
        .unwrap()
    else {
        panic!("execution proof expected")
    };
    proof
}

fn activate(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: &DiscussionParentExecutionRequest,
) -> CasTurnSource {
    support::exact_cas::establish_turn(
        store,
        storage.clone(),
        request.parent_thread_id,
        request.turn_id,
        support::timestamp(101),
    )
}

fn finish(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: &DiscussionParentExecutionRequest,
    source: &CasTurnSource,
    status: TurnEndStatus,
) {
    support::exact_cas::admit_event(
        store,
        storage.clone(),
        request.parent_thread_id,
        request.turn_id,
        source,
        SourceEventPayload::TurnActivated,
        support::timestamp(102),
    );
    let input = storage
        .accepted_input(store, request.input_id, limit())
        .unwrap()
        .unwrap();
    let AcceptedInputSource::DiscussionHandoff(receipt) = input.source() else {
        panic!()
    };
    support::exact_cas::correlate_user_item(
        store,
        storage.clone(),
        request.parent_thread_id,
        request.turn_id,
        receipt.canonical_item_id,
        source,
        support::timestamp(103),
    );
    support::exact_cas::admit_event(
        store,
        storage.clone(),
        request.parent_thread_id,
        request.turn_id,
        source,
        SourceEventPayload::TurnEnded(status),
        support::timestamp(104),
    );
}

fn settle(store: &HomeStore, proof: PreparedDiscussionParentExecution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(
            proof
                .into_terminal_settlement(support::timestamp(200))
                .unwrap()
                .contribution(),
        )
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

#[test]
fn acceptance_precedes_activity_and_pending_dispatch_never_proves_completion() {
    let home = support::TestHome::new("parent-execution-acceptance");
    let (store, storage, request) = seeded(&home);
    assert!(matches!(
        storage
            .prepare_discussion_parent_execution(&store, request.clone())
            .unwrap(),
        DiscussionParentExecution::Waiting
    ));
    let source = activate(&store, &storage, &request);
    let proof = proven(&store, &storage, &request);
    assert_eq!(
        proof.disposition(),
        &DiscussionParentExecutionDisposition::Accepted(source.clone())
    );
    assert!(
        proof
            .clone()
            .into_terminal_settlement(support::timestamp(110))
            .is_err()
    );
    finish(
        &store,
        &storage,
        &request,
        &source,
        TurnEndStatus::complete(),
    );
    let mut stale = HomeCommand::new(store.home_revision().unwrap());
    stale.add_validation(proof.into_validation()).unwrap();
    assert!(matches!(
        store.execute(stale),
        CommandOutcome::NotCommitted { .. }
    ));
    let mut wrong = request.clone();
    wrong.resolution.push('!');
    assert!(
        storage
            .prepare_discussion_parent_execution(&store, wrong)
            .is_err()
    );
    let mut wrong = request.clone();
    wrong.turn_id = SyndicTurnId::from_bytes([249; 16]);
    assert!(
        storage
            .prepare_discussion_parent_execution(&store, wrong)
            .is_err()
    );
}

#[test]
fn only_complete_execution_archives_and_all_terminal_outcomes_release() {
    for status in [
        TurnEndStatus::complete(),
        TurnEndStatus::new(
            TurnTerminalOutcome::Complete,
            Some(TurnIncompleteReason::CompletionMismatch),
        )
        .unwrap(),
        TurnEndStatus::new(TurnTerminalOutcome::Interrupted, None).unwrap(),
        TurnEndStatus::new(TurnTerminalOutcome::Failed, None).unwrap(),
        TurnEndStatus::incomplete(TurnIncompleteReason::StreamLost),
    ] {
        let home = support::TestHome::new("parent-execution-terminal");
        let (store, storage, request) = seeded(&home);
        let source = activate(&store, &storage, &request);
        finish(&store, &storage, &request, &source, status);
        let proof = proven(&store, &storage, &request);
        assert_eq!(
            proof.disposition(),
            &DiscussionParentExecutionDisposition::Terminal {
                status,
                cas: Some(source)
            }
        );
        let input = proof.input().clone();
        settle(&store, proof);
        assert_eq!(
            storage
                .discussion_handoff_gate(&store, request.child_gate.thread_id(), limit())
                .unwrap()
                .unwrap()
                .state(),
            DiscussionHandoffGateState::Open
        );
        let attributes = storage
            .thread_attributes(&store, request.child_gate.thread_id(), limit())
            .unwrap()
            .unwrap();
        assert_eq!(
            attributes.archive() == ThreadArchiveState::BranchDiscussionOpen,
            status.outcome() != TurnTerminalOutcome::Complete
        );
        assert_eq!(
            storage
                .accepted_input(&store, request.input_id, limit())
                .unwrap(),
            Some(input)
        );
    }
}

#[test]
fn historical_parent_completion_survives_a_new_parent_turn_and_reopen() {
    let home = support::TestHome::new("parent-execution-historical");
    let (store, storage, request) = seeded(&home);
    let source = activate(&store, &storage, &request);
    finish(
        &store,
        &storage,
        &request,
        &source,
        TurnEndStatus::complete(),
    );
    support::converge_and_release_terminal_history(
        &store,
        storage.clone(),
        request.parent_thread_id,
        request.turn_id,
    );
    let next = support::exact_cas::submit_current_draft(
        &store,
        storage.clone(),
        request.parent_thread_id,
        SyndicDraftId::from_bytes([250; 16]),
        SyndicItemId::from_bytes([251; 16]),
        "later work",
        support::timestamp(120),
    );
    assert_ne!(next, request.turn_id);
    let old = proven(&store, &storage, &request);
    drop(storage);
    drop(store);
    let mut candidate = support::open(home.path());
    let fresh = SyndicStorage::register(&mut candidate).unwrap();
    let mut publication = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap();
    let access = publication.recovery_access().unwrap();
    let DiscussionParentExecution::Proven(proof) = fresh
        .prepare_discussion_parent_execution_candidate(&access, request.clone())
        .unwrap()
    else {
        panic!("historical candidate proof")
    };
    assert_eq!(old.disposition(), proof.disposition());
    let store = publication.publish().unwrap();
    let mut stale = HomeCommand::new(store.home_revision().unwrap());
    stale.add_validation(old.into_validation()).unwrap();
    assert!(matches!(
        store.execute(stale),
        CommandOutcome::NotCommitted { .. }
    ));
    settle(&store, proof);
    assert_eq!(
        fresh
            .thread(&store, request.parent_thread_id, limit())
            .unwrap()
            .unwrap()
            .committed_tail(),
        Some(next)
    );
}

#[test]
fn lost_dispatch_authority_settles_incomplete_without_inventing_cas_acceptance() {
    for accepted in [false, true] {
        let home = support::TestHome::new("parent-execution-lost-authority");
        let (store, storage, request) = seeded(&home);
        let cas = if accepted {
            Some(activate(&store, &storage, &request))
        } else {
            support::exact_cas::activate_turn(
                &store,
                storage.clone(),
                request.parent_thread_id,
                request.turn_id,
                support::timestamp(101),
            );
            assert!(matches!(
                storage
                    .prepare_discussion_parent_execution(&store, request.clone())
                    .unwrap(),
                DiscussionParentExecution::Waiting
            ));
            None
        };
        let page = storage
            .delivery_recovery_startup_page(
                &store,
                None,
                CursorReadLimits::new(
                    DELIVERY_RECOVERY_GATE_PAGE_MAX_RECORDS,
                    DELIVERY_RECOVERY_GATE_PAGE_MAX_BYTES,
                )
                .unwrap(),
            )
            .unwrap();
        let source = page
            .records()
            .iter()
            .find(|source| source.thread_id() == request.parent_thread_id)
            .unwrap();
        let DeliveryRecoveryCase::Active(active) = storage
            .classify_delivery_recovery(&store, source, limit())
            .unwrap()
        else {
            panic!("active dispatch")
        };
        support::discussion_input::committed(
            &store,
            storage.abandon_active_binding(
                storage.revision(&store).unwrap(),
                active
                    .generic_abandonment(" authority lost", support::timestamp(110))
                    .unwrap(),
            ),
        );
        let state = storage
            .turn_state(&store, request.turn_id, limit())
            .unwrap()
            .unwrap();
        let gate = storage
            .input_gate(&store, request.parent_thread_id, limit())
            .unwrap()
            .unwrap();
        let status = TurnEndStatus::incomplete(TurnIncompleteReason::AuthorityLost);
        let event = LiveSourceEvent::new(
            request.parent_thread_id,
            request.turn_id,
            state.revision(),
            gate.revision(),
            SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
            None,
            SourceEventPayload::TurnEnded(status),
            support::timestamp(110),
        )
        .unwrap();
        support::discussion_input::committed(
            &store,
            storage.admit_live_source_event(
                storage.revision(&store).unwrap(),
                event.clone(),
                crate::support::fixture_activity(
                    &store,
                    &storage,
                    event.thread_id(),
                    event.turn_id(),
                ),
            ),
        );
        let proof = proven(&store, &storage, &request);
        assert_eq!(
            proof.disposition(),
            &DiscussionParentExecutionDisposition::Terminal { status, cas }
        );
        settle(&store, proof);
        assert_eq!(
            storage
                .thread_attributes(&store, request.child_gate.thread_id(), limit())
                .unwrap()
                .unwrap()
                .archive(),
            ThreadArchiveState::BranchDiscussionOpen
        );
    }
}
