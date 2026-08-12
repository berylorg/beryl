use beryl_model::{CasItemId, SyndicDraftId, SyndicItemId, SyndicTurnId};

use super::*;
use crate::activity_handoff::{child_handoff_candidate, owner_item};
use crate::support::exact_cas::{
    admit_event, admit_started_then_completed_item, correlate_user_item, establish_turn,
    submit_current_draft,
};

#[test]
fn promotion_reconciliation_accepts_draft_save_and_pending_admission_descendants() {
    let (home, store, storage, fixture) = seeded("phase61-promotion-descendants");
    let promotion = promotion(&store, storage);
    execute(&store, storage.promote_accepted_input(promotion.clone()));

    let current = storage
        .current_draft(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let DraftPayloadUpdateDecision::Update(update) =
        DraftPayloadUpdate::prepare_reference(&current, fixture.accepted_content, timestamp(21))
            .unwrap()
    else {
        panic!("fixture content changes the current draft");
    };
    execute(
        &store,
        storage.update_draft_payload(storage.revision(&store).unwrap(), update),
    );
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &promotion, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );

    let transcript_before_admission = storage
        .transcript_view_head(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    admit_pending_input(&store, storage, &fixture, 123, timestamp(22));
    assert_eq!(
        storage
            .transcript_view_head(&store, fixture.thread, limit())
            .unwrap()
            .unwrap(),
        transcript_before_admission,
    );
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &promotion, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );

    let current = storage
        .current_draft(&store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    let DraftPayloadUpdateDecision::Update(update) =
        DraftPayloadUpdate::prepare_reference(&current, fixture.accepted_content, timestamp(23))
            .unwrap()
    else {
        panic!("rotated draft must accept another payload");
    };
    execute(
        &store,
        storage.update_draft_payload(storage.revision(&store).unwrap(), update),
    );
    admit_pending_input(&store, storage, &fixture, 124, timestamp(24));
    assert_eq!(
        storage
            .accepted_input_promotion_status(&store, &promotion, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    drop(store);
    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        reopened_storage
            .accepted_input_promotion_status(&reopened, &promotion, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

fn complete_child_answer(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    child: beryl_model::SyndicThreadId,
    next_draft: SyndicDraftId,
    submitted_item: SyndicItemId,
    final_item: SyndicItemId,
    cas_item: &str,
    base_time: u64,
) -> (SyndicTurnId, ProjectionSourceRange) {
    let turn = submit_current_draft(
        store,
        storage,
        child,
        next_draft,
        submitted_item,
        "second child question",
        timestamp(base_time),
    );
    let source = establish_turn(store, storage, child, turn, timestamp(base_time + 1));
    admit_event(
        store,
        storage,
        child,
        turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(base_time + 1),
    );
    correlate_user_item(
        store,
        storage,
        child,
        turn,
        submitted_item,
        &source,
        timestamp(base_time + 2),
    );
    let answer = ProviderItemV1::AgentMessage(ProviderAgentMessageV1 {
        text: ProviderTextV1::inline("final child"),
        phase: Some(ProviderMessagePhaseV1::FinalAnswer),
        memory_citation: None,
    });
    admit_started_then_completed_item(
        store,
        storage,
        child,
        turn,
        final_item,
        &source,
        CasItemId::new(cas_item).unwrap(),
        answer.clone(),
        answer,
        timestamp(base_time + 3),
        timestamp(base_time + 4),
    );
    admit_event(
        store,
        storage,
        child,
        turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(
                TurnTerminalOutcome::Interrupted,
                Some(TurnIncompleteReason::ItemAuditFailed),
            )
            .unwrap(),
        ),
        timestamp(base_time + 5),
    );
    (turn, ProjectionSourceRange::new(0, 11).unwrap())
}

#[test]
fn promotion_reconciliation_accepts_multiple_child_activity_handoffs() {
    let fixture = child_handoff_candidate("phase61-promotion-child-activity", false);
    let store = fixture.store;
    let storage = fixture.storage;
    let gate = storage
        .input_gate(&store, fixture.owner, limit())
        .unwrap()
        .unwrap();
    let owner_turn = *match gate.state() {
        InputGateState::PendingTurn(turn) => turn,
        other => panic!("owner fixture must be pending, got {other:?}"),
    };
    let source = establish_turn(&store, storage, fixture.owner, owner_turn, timestamp(20));
    admit_event(
        &store,
        storage,
        fixture.owner,
        owner_turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(20),
    );
    correlate_user_item(
        &store,
        storage,
        fixture.owner,
        owner_turn,
        owner_item(),
        &source,
        timestamp(21),
    );

    let current = storage
        .current_draft(&store, fixture.owner, limit())
        .unwrap()
        .unwrap();
    let queued_content = storage
        .canonical_item(&store, owner_item(), limit())
        .unwrap()
        .unwrap()
        .presentation_content()
        .unwrap();
    let DraftPayloadUpdateDecision::Update(update) =
        DraftPayloadUpdate::prepare_reference(&current, queued_content, timestamp(22)).unwrap()
    else {
        panic!("child fixture replacement draft must become nonempty");
    };
    execute(
        &store,
        storage.update_draft_payload(storage.revision(&store).unwrap(), update),
    );
    let current = storage
        .current_draft(&store, fixture.owner, limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, fixture.owner, limit())
        .unwrap()
        .unwrap();
    let admission = AcceptedInputAdmission::new(
        fixture.owner,
        current.thread().revision(),
        current.draft().id(),
        current.draft().revision(),
        current.draft().content(),
        gate.revision(),
        SyndicDraftId::from_bytes([230; 16]),
        None,
        timestamp(23),
    );
    let accepted_input = admission.accepted_input_id();
    execute(
        &store,
        storage.admit_accepted_input(storage.revision(&store).unwrap(), admission),
    );
    let ready = storage
        .ready_steering_input(&store, accepted_input, limit())
        .unwrap()
        .expect("active admission must be ready for steering");
    execute(
        &store,
        storage.begin_accepted_input_delivery(
            storage.revision(&store).unwrap(),
            BeginAcceptedInputDelivery::new(
                fixture.owner,
                accepted_input,
                ready.accepted_input_revision(),
                ready.target().clone(),
            ),
        ),
    );
    let delivering = storage
        .delivering_steering_input(&store, accepted_input, limit())
        .unwrap()
        .expect("begun steering delivery must be observable");
    execute(
        &store,
        storage.record_steering_rejection(
            storage.revision(&store).unwrap(),
            SteeringRejection::new(
                fixture.owner,
                accepted_input,
                delivering.accepted_input_revision(),
                delivering.target().clone(),
            ),
        ),
    );
    admit_event(
        &store,
        storage,
        fixture.owner,
        owner_turn,
        &source,
        SourceEventPayload::TurnEnded(
            TurnEndStatus::new(TurnTerminalOutcome::Failed, None).unwrap(),
        ),
        timestamp(24),
    );
    crate::support::converge_and_release_terminal_history(
        &store,
        storage,
        fixture.owner,
        owner_turn,
    );

    let limits = CursorReadLimits::new(256, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap();
    let sources = storage
        .accepted_next_source_page(&store, storage.revision(&store).unwrap(), None, limits)
        .unwrap();
    let owner_source = sources
        .records()
        .iter()
        .find(|source| source.thread_id() == fixture.owner)
        .copied()
        .expect("owner must expose its rejected input as next-turn work");
    let owner_candidate = storage
        .accepted_next_candidate_page(&store, owner_source, None, limits)
        .unwrap()
        .into_candidate()
        .expect("idle owner must expose its earliest next-turn candidate");
    assert_eq!(owner_candidate.input_id(), accepted_input);
    let promotion = PromoteAcceptedInput::new(
        owner_candidate,
        SyndicTurnId::from_bytes([231; 16]),
        SyndicItemId::from_bytes([232; 16]),
        timestamp(25),
    );
    execute(&store, storage.promote_accepted_input(promotion.clone()));
    let descendants = [
        (
            fixture.child_turn,
            fixture.final_answer,
            ProjectionSourceRange::new(0, 11).unwrap(),
        ),
        {
            let (turn, range) = complete_child_answer(
                &store,
                storage,
                fixture.child,
                SyndicDraftId::from_bytes([233; 16]),
                SyndicItemId::from_bytes([234; 16]),
                SyndicItemId::from_bytes([235; 16]),
                "phase61-child-answer-two",
                26,
            );
            (turn, SyndicItemId::from_bytes([235; 16]), range)
        },
    ];
    for (child_turn, final_item, final_range) in descendants {
        let activity = storage
            .activity_query_head(&store, fixture.owner, limit())
            .unwrap()
            .unwrap();
        execute(
            &store,
            storage.publish_activity_child_handoff(
                storage.revision(&store).unwrap(),
                PublishActivityChildHandoff::new(
                    fixture.owner,
                    activity.revision(),
                    fixture.child,
                    child_turn,
                    final_item,
                    final_range,
                ),
            ),
        );
        assert_eq!(
            storage
                .accepted_input_promotion_status(&store, &promotion, limit())
                .unwrap(),
            AcceptedInputPromotionStatus::Exact
        );
    }
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();

    drop(store);
    let mut reopened = open(fixture.home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        reopened_storage
            .accepted_input_promotion_status(&reopened, &promotion, limit())
            .unwrap(),
        AcceptedInputPromotionStatus::Exact
    );
    reopened
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}
