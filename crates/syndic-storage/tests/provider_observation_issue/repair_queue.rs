use super::*;
use beryl_home_store::CursorReadLimits;

#[path = "../draft_edit_history_retention/common.rs"]
mod edit_support;
#[path = "../draft_edit_history/support.rs"]
#[allow(dead_code, unused_imports)]
mod support;

pub(super) fn acceptance_request(
    fixture: &Fixture,
    thread: SyndicThreadId,
    seed: u8,
) -> FirstAcceptance {
    let storage = &fixture.storage;
    let store = &fixture.store;
    let selected = storage
        .current_draft(store, thread, limit())
        .unwrap()
        .unwrap();
    let session = support::open_session(storage, store, &selected, seed, seed + 1);
    let edited =
        edit_support::commit_edit(storage, store, &session, seed + 2, "queued during repair");
    let session = edited.adopted_session();
    let source = storage
        .capture_draft_editor_candidate_publication_source(
            store,
            DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                support::selector(&selected),
                DraftEditorCandidateActivationBindingV1::from_head(session),
                DraftPieceOperationIdV1::from_bytes([seed + 7; 16]),
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
    let outcome = execute(
        store,
        storage.publish_draft_editor_candidate(storage.revision(store).unwrap(), prepared.clone()),
    );
    assert!(matches!(
        storage
            .reconcile_draft_editor_candidate_publication(store, &prepared, outcome)
            .unwrap(),
        DraftEditorCandidatePublicationOutcomeV1::Published(_, _)
    ));
    let DraftEditorCandidateSessionReadOutcomeV1::Active(session) = storage
        .draft_editor_candidate_session(store, session.draft_id(), session.session_id())
        .unwrap()
    else {
        panic!("active candidate");
    };
    let key = DraftComposerBuildKeyV1::new(
        session.newest_root(),
        DraftComposerFormatV1::ComposerV1,
        DraftComposerMaterializationOperationIdV1::from_bytes([seed + 3; 16]),
    );
    committed_command(execute(
        store,
        storage.begin_draft_composer_materialization(storage.revision(store).unwrap(), key),
    ));
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
        committed_command(execute(
            store,
            storage.advance_draft_composer_materialization(storage.revision(store).unwrap(), step),
        ));
    }
    let selected = storage
        .current_draft(store, thread, limit())
        .unwrap()
        .unwrap();
    let gate = storage.input_gate(store, thread, limit()).unwrap().unwrap();
    let request = FirstAcceptance::new(
        thread,
        selected.thread().revision(),
        storage
            .image_label_authority_head(store, thread, limit())
            .unwrap()
            .unwrap(),
        selected.draft().id(),
        selected.draft().revision(),
        DraftEditorCandidateActivationBindingV1::from_head(&session),
        mapping.expect("bounded materialization"),
        gate.revision(),
        gate.state().clone(),
        SyndicDraftId::from_bytes([seed + 4; 16]),
        SyndicItemId::from_bytes([seed + 5; 16]),
        None,
        DraftPieceOperationIdV1::from_bytes([seed + 6; 16]),
        timestamp(200),
    );
    request
}

fn accept(fixture: &Fixture, seed: u8) -> FirstAcceptance {
    let request = acceptance_request(fixture, fixture.thread, seed);
    committed_command(execute(
        &fixture.store,
        fixture.storage.first_acceptance(
            fixture.storage.revision(&fixture.store).unwrap(),
            request.clone(),
        ),
    ));
    request
}

#[test]
fn queued_input_before_and_during_repair_is_preserved_without_promotion() {
    let fixture = setup("repair-queued-input");
    let target = super::repair_retained::terminal_target(&fixture, false);
    let first = accept(&fixture, 100);
    let before = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(before.live_next_turn_count(), 1);
    committed_command(
        fixture
            .store
            .execute_current(fixture.storage.current_require_terminal_repair(
                RequireTerminalRepair::new(fixture.thread, before.revision(), target.clone()),
            )),
    );
    let admitted = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        admitted.live_next_turn_count(),
        before.live_next_turn_count()
    );
    assert_eq!(
        admitted.live_logical_utf8_bytes(),
        before.live_logical_utf8_bytes()
    );
    assert_eq!(admitted.selected_route(), before.selected_route());
    let second = accept(&fixture, 120);
    let gate = fixture
        .storage
        .input_gate(&fixture.store, fixture.thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.state(), &InputGateState::RepairRequired(target));
    assert_eq!(gate.live_next_turn_count(), 2);
    assert_eq!(gate.live_steering_count(), 0);
    assert_eq!(gate.accepted_high_water(), before.accepted_high_water() + 1);
    for request in [first, second] {
        assert_eq!(
            fixture
                .storage
                .first_acceptance_status(&fixture.store, &request, limit())
                .unwrap(),
            FirstAcceptanceStatus::ExactNew(FirstAcceptanceKind::Accepted)
        );
    }
    let limits = CursorReadLimits::new(64, ACCEPTED_NEXT_PAGE_MAX_BYTES).unwrap();
    let sources = fixture
        .storage
        .accepted_next_source_page(
            &fixture.store,
            fixture.storage.revision(&fixture.store).unwrap(),
            None,
            limits,
        )
        .unwrap();
    let source = sources
        .records()
        .iter()
        .find(|source| source.thread_id() == fixture.thread)
        .unwrap();
    assert!(
        fixture
            .storage
            .accepted_next_candidate_page(&fixture.store, *source, None, limits)
            .unwrap()
            .into_candidate()
            .is_none()
    );
    assert_eq!(
        fixture
            .storage
            .thread(&fixture.store, fixture.thread, limit())
            .unwrap()
            .unwrap()
            .committed_tail(),
        Some(fixture.turn)
    );
}
