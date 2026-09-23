use super::*;
use beryl_model::{SyndicDraftId, SyndicItemId};
#[path = "edit_gates.rs"]
mod edit_gates;
#[path = "../draft_edit_history_retention/common.rs"]
mod edit_support;
#[path = "mutation_gates.rs"]
mod mutation_gates;
#[path = "../draft_edit_history/support.rs"]
#[allow(dead_code, unused_imports)]
mod support;

fn committed(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

fn prepare_acceptance(store: &HomeStore, storage: &SyndicStorage) -> FirstAcceptance {
    let selected = storage
        .current_draft(store, id(36), limit())
        .unwrap()
        .unwrap();
    let session = support::open_session(storage, store, &selected, 220, 221);
    let edited = edit_support::commit_edit(storage, store, &session, 222, "next discussion input");
    let session = edited.adopted_session();
    let source = storage
        .capture_draft_editor_candidate_publication_source(
            store,
            DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                support::selector(&selected),
                DraftEditorCandidateActivationBindingV1::from_head(session),
                DraftPieceOperationIdV1::from_bytes([223; 16]),
                timestamp(200),
            ),
        )
        .unwrap();
    let prepared = storage
        .prepare_draft_editor_candidate_publication(
            store,
            source,
            DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        )
        .unwrap();
    committed(
        store,
        storage.publish_draft_editor_candidate(storage.revision(store).unwrap(), prepared),
    );
    let DraftEditorCandidateSessionReadOutcomeV1::Active(session) = storage
        .draft_editor_candidate_session(store, session.draft_id(), session.session_id())
        .unwrap()
    else {
        panic!("active editor")
    };
    let key = DraftComposerBuildKeyV1::new(
        session.newest_root(),
        DraftComposerFormatV1::ComposerV1,
        DraftComposerMaterializationOperationIdV1::from_bytes([224; 16]),
    );
    committed(
        store,
        storage.begin_draft_composer_materialization(storage.revision(store).unwrap(), key),
    );
    let mut mapping = None;
    for _ in 0..128 {
        if let DraftComposerMaterializationStatusV1::Sealed(value) = storage
            .draft_composer_materialization_status(store, key)
            .unwrap()
        {
            mapping = Some(value);
            break;
        }
        let step = storage
            .prepare_draft_composer_materialization_step(store, key)
            .unwrap()
            .unwrap();
        committed(
            store,
            storage.advance_draft_composer_materialization(storage.revision(store).unwrap(), step),
        );
    }
    let selected = storage
        .current_draft(store, id(36), limit())
        .unwrap()
        .unwrap();
    let gate = storage.input_gate(store, id(36), limit()).unwrap().unwrap();
    FirstAcceptance::new(
        id(36),
        selected.thread().revision(),
        storage
            .image_label_authority_head(store, id(36), limit())
            .unwrap()
            .unwrap(),
        selected.draft().id(),
        selected.draft().revision(),
        DraftEditorCandidateActivationBindingV1::from_head(&session),
        mapping.unwrap(),
        gate.revision(),
        gate.state().clone(),
        SyndicDraftId::from_bytes([225; 16]),
        SyndicItemId::from_bytes([226; 16]),
        None,
        DraftPieceOperationIdV1::from_bytes([227; 16]),
        timestamp(201),
    )
}

pub(super) fn accept_next(store: &HomeStore, storage: &SyndicStorage) {
    let acceptance = prepare_acceptance(store, storage);
    committed(
        store,
        storage.first_acceptance(storage.revision(store).unwrap(), acceptance),
    );
}

fn blocked(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    let outcome = store.execute(command);
    assert!(
        matches!(outcome, CommandOutcome::NotCommitted { .. }),
        "{outcome:?}"
    );
    assert!(
        format!("{outcome:?}").contains("DiscussionMutationBlocked"),
        "{outcome:?}"
    );
}

#[test]
fn queued_future_input_fences_prepared_and_fresh_handoff_admission() {
    let home = TestHome::new("handoff-queue-race");
    let (store, storage, mut request) = seeded(&home, FaultController::new());
    let prepared = storage
        .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request.clone()))
        .unwrap();
    let gate = storage
        .input_gate(&store, id(36), limit())
        .unwrap()
        .unwrap();
    let waiting = InputGateRecord::new(
        id(36),
        gate.revision().checked_next().unwrap(),
        InputGateState::AwaitingTerminal(request.resolving_target.pending().active_turn_id()),
        gate.accepted_high_water(),
        gate.route_generation_high_water(),
        gate.selected_route(),
        0,
        0,
        0,
    )
    .unwrap();
    crate::support::commit(
        &store,
        storage.clone(),
        crate::support::batch([syndic_storage::test_faults::FixtureRecord::InputGate(
            waiting,
        )]),
    );
    accept_next(&store, &storage);
    let queued = storage
        .input_gate(&store, id(36), limit())
        .unwrap()
        .unwrap();
    assert_eq!(queued.live_next_turn_count(), 1);
    assert!(matches!(
        store.execute(command(&store, prepared)),
        CommandOutcome::NotCommitted { .. }
    ));
    let restored = InputGateRecord::new(
        id(36),
        queued.revision().checked_next().unwrap(),
        gate.state().clone(),
        queued.accepted_high_water(),
        queued.route_generation_high_water(),
        queued.selected_route(),
        queued.live_steering_count(),
        queued.live_next_turn_count(),
        queued.live_logical_utf8_bytes(),
    )
    .unwrap();
    request.input_gate_revision = restored.revision();
    request.thread_revision = storage
        .thread(&store, id(36), limit())
        .unwrap()
        .unwrap()
        .revision();
    crate::support::commit(
        &store,
        storage.clone(),
        crate::support::batch([syndic_storage::test_faults::FixtureRecord::InputGate(
            restored,
        )]),
    );
    let prepared = storage
        .prepare_discussion_handoff(&store, DiscussionHandoffMutation::Admit(request))
        .unwrap();
    assert!(matches!(
        store.execute(command(&store, prepared)),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        storage
            .discussion_handoff_gate(&store, id(36), limit())
            .unwrap()
            .unwrap()
            .state(),
        DiscussionHandoffGateState::Open
    );
    store.close().unwrap();
}
