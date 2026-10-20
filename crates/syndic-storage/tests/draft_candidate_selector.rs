#![cfg(feature = "test-faults")]

#[path = "draft_edit_history_retention/common.rs"]
mod common;
#[path = "support/mod.rs"]
mod domain_support;
#[path = "draft_edit_history/support.rs"]
mod support;

use support::*;
use syndic_storage::{
    DiscussionParentEligibility, DiscussionParentRequest,
    DraftEditorCandidatePublicationEvidenceV1,
    DraftEditorCandidatePublicationSourceCaptureRequestV1,
    DraftEditorCandidateSessionDisposeOutcomeV1, DraftEditorCandidateSessionDisposeRequestV1,
    DraftRootHistoryPairV1, GeneratedDiscussionInput,
};

fn parent_fixture() -> (TestHome, HomeStore, SyndicStorage, DiscussionParentRequest) {
    parent_fixture_with_faults(FaultController::new())
}

fn parent_fixture_with_faults(
    faults: FaultController,
) -> (TestHome, HomeStore, SyndicStorage, DiscussionParentRequest) {
    let directory = TestHome::new("candidate-selector");
    let mut candidate = beryl_home_store::HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(&directory.0, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let parent = domain_support::generated_input::seed_pending(&store, &storage);
    (directory, store, storage, parent)
}

fn admit(store: &HomeStore, storage: &SyndicStorage, parent: DiscussionParentRequest) {
    let DiscussionParentEligibility::Proven(proven) =
        storage.prepare_discussion_parent(store, parent).unwrap()
    else {
        panic!("parent not ready");
    };
    let prepared = storage
        .prepare_generated_discussion_input(
            store,
            proven,
            GeneratedDiscussionInput {
                parent_turn_id: beryl_model::SyndicTurnId::from_bytes([234; 16]),
                canonical_item_id: beryl_model::SyndicItemId::from_bytes([232; 16]),
                resolution: "exact generated resolution".into(),
                admitted_at: SyndicTimestamp::from_unix_millis(100),
            },
        )
        .unwrap();
    domain_support::discussion_input::committed(store, prepared.into_contribution());
}

#[test]
fn generated_admission_requalifies_clean_and_dirty_candidates_without_saving_or_writing() {
    for dirty in [false, true] {
        let (_directory, store, storage, parent) = parent_fixture();
        let before = current(&storage, &store, parent.parent_thread_id);
        let prior = selector(&before);
        let mut head = open_session(&storage, &store, &before, 180, 181);
        if dirty {
            head = common::commit_edit(&storage, &store, &head, 182, "preserved café 🌸 draft")
                .adopted_session()
                .clone();
        }
        let candidate = DraftEditorCandidateActivationBindingV1::from_head(&head);
        admit(&store, &storage, parent);
        let revision = storage.revision(&store).unwrap();
        let renewed = storage
            .qualify_draft_editor_candidate_selector(&store, candidate, prior)
            .unwrap();
        assert!(renewed.thread_revision() > prior.thread_revision());
        assert_eq!(
            renewed,
            selector(&current(&storage, &store, parent.parent_thread_id))
        );
        assert_eq!(
            current(&storage, &store, parent.parent_thread_id).draft(),
            before.draft()
        );
        assert_eq!(storage.revision(&store).unwrap(), revision);
        assert_eq!(
            storage
                .draft_editor_candidate_session(&store, head.draft_id(), head.session_id())
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::Active(head)
        );
        assert_eq!(
            storage
                .draft_editor_candidate_is_saved(&store, candidate, renewed)
                .unwrap(),
            !dirty
        );
        assert!(
            storage
                .draft_editor_candidate_is_saved(&store, candidate, prior)
                .is_err()
        );
    }
}

#[test]
fn candidate_selector_qualification_refuses_staging_and_a_changed_candidate() {
    let (_directory, store, storage, parent) = parent_fixture();
    let before = current(&storage, &store, parent.parent_thread_id);
    let prior = selector(&before);
    let head = open_session(&storage, &store, &before, 180, 181);
    let candidate = DraftEditorCandidateActivationBindingV1::from_head(&head);
    admit(&store, &storage, parent);
    let renewed = storage
        .qualify_draft_editor_candidate_selector(&store, candidate, prior)
        .unwrap();
    let edit = transaction(&storage, &store, &head, 182, "later edit", point(10));
    build(&storage, &store, &edit);
    assert!(
        storage
            .qualify_draft_editor_candidate_selector(&store, candidate, prior)
            .is_err()
    );
    committed(execute(
        &store,
        storage.settle_draft_piece_edit(storage.revision(&store).unwrap(), edit.prepared.clone()),
    ));
    assert!(
        storage
            .qualify_draft_editor_candidate_selector(&store, candidate, prior)
            .is_err()
    );
    assert!(
        storage
            .capture_draft_editor_candidate_publication_source(
                &store,
                DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                    renewed,
                    candidate,
                    DraftPieceOperationIdV1::from_bytes([183; 16]),
                    SyndicTimestamp::from_unix_millis(101)
                )
            )
            .is_err()
    );
}

#[test]
fn candidate_selector_qualification_refuses_a_changed_durable_checkpoint() {
    let (_directory, store, storage, parent) = parent_fixture();
    let before = current(&storage, &store, parent.parent_thread_id);
    let prior = selector(&before);
    let head = open_session(&storage, &store, &before, 180, 181);
    let adopted = common::commit_edit(&storage, &store, &head, 182, "new durable checkpoint");
    let candidate = DraftEditorCandidateActivationBindingV1::from_head(adopted.adopted_session());
    let source = storage
        .capture_draft_editor_candidate_publication_source(
            &store,
            DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                prior,
                candidate,
                DraftPieceOperationIdV1::from_bytes([183; 16]),
                SyndicTimestamp::from_unix_millis(100),
            ),
        )
        .unwrap();
    let prepared = storage
        .prepare_draft_editor_candidate_publication(
            &store,
            source,
            DraftEditorCandidatePublicationEvidenceV1::UnchangedEmpty,
        )
        .unwrap();
    let outcome = execute(
        &store,
        storage.publish_draft_editor_candidate(storage.revision(&store).unwrap(), prepared.clone()),
    );
    storage
        .reconcile_draft_editor_candidate_publication(&store, &prepared, outcome)
        .unwrap();
    let DraftEditorCandidateSessionReadOutcomeV1::Active(now) = storage
        .draft_editor_candidate_session(&store, head.draft_id(), head.session_id())
        .unwrap()
    else {
        panic!("missing exact session");
    };
    assert!(
        storage
            .qualify_draft_editor_candidate_selector(
                &store,
                DraftEditorCandidateActivationBindingV1::from_head(&now),
                prior
            )
            .is_err()
    );
}

#[test]
fn candidate_selector_qualification_refuses_foreign_disposed_and_future_authority() {
    let (_directory, store, storage, parent) = parent_fixture();
    let before = current(&storage, &store, parent.parent_thread_id);
    let prior = selector(&before);
    let head = open_session(&storage, &store, &before, 180, 181);
    let candidate = DraftEditorCandidateActivationBindingV1::from_head(&head);
    let (_other_directory, other, other_storage, _) = parent_fixture();
    assert!(
        storage
            .qualify_draft_editor_candidate_selector(&other, candidate, prior)
            .is_err()
    );
    assert!(
        other_storage
            .qualify_draft_editor_candidate_selector(&store, candidate, prior)
            .is_err()
    );
    let future = DraftEditorCurrentSelectorV1::new(
        prior.thread_id(),
        prior.thread_revision().checked_next().unwrap(),
        prior.draft_id(),
        prior.selector_revision(),
        prior.root(),
        prior.history(),
    );
    assert!(
        storage
            .qualify_draft_editor_candidate_selector(&store, candidate, future)
            .is_err()
    );
    let prepared = storage
        .prepare_dispose_draft_editor_candidate_session(
            &store,
            DraftEditorCandidateSessionDisposeRequestV1::new(
                head.draft_id(),
                head.session_id(),
                DraftPieceOperationIdV1::from_bytes([184; 16]),
                head.session_generation(),
                DraftRootHistoryPairV1::new(head.published_root(), head.published_history()),
            ),
        )
        .unwrap();
    let outcome = execute(
        &store,
        storage.dispose_draft_editor_candidate_session(
            storage.revision(&store).unwrap(),
            prepared.clone(),
        ),
    );
    assert!(matches!(
        &outcome,
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
    let disposed = storage
        .reconcile_draft_editor_candidate_session_disposal(&store, &prepared, outcome)
        .unwrap();
    assert!(matches!(
        disposed,
        DraftEditorCandidateSessionDisposeOutcomeV1::Disposed(_)
    ));
    assert!(matches!(
        storage
            .draft_editor_candidate_session(&store, head.draft_id(), head.session_id())
            .unwrap(),
        DraftEditorCandidateSessionReadOutcomeV1::Disposed(_)
    ));
    assert!(
        storage
            .qualify_draft_editor_candidate_selector(&store, candidate, prior)
            .is_err()
    );
}

#[test]
fn candidate_selector_qualification_refuses_a_mutation_inside_its_read_interval() {
    let faults = FaultController::new();
    let (_directory, store, storage, parent) = parent_fixture_with_faults(faults.clone());
    let before = current(&storage, &store, parent.parent_thread_id);
    let prior = selector(&before);
    let head = open_session(&storage, &store, &before, 180, 181);
    let candidate = DraftEditorCandidateActivationBindingV1::from_head(&head);
    let blocked = faults.block_next(FaultPoint::BeforeReadConfirmation);
    let result = std::thread::scope(|scope| {
        let reader = scope
            .spawn(|| storage.qualify_draft_editor_candidate_selector(&store, candidate, prior));
        assert!(blocked.wait_until_reached(std::time::Duration::from_secs(10)));
        admit(&store, &storage, parent);
        blocked.release();
        reader.join().unwrap()
    });
    assert!(matches!(
        result,
        Err(
            syndic_storage::DraftEditorCandidatePublicationCommandErrorV1::Read(
                syndic_storage::SyndicReadError::ConcurrentChange { .. }
            )
        )
    ));
    assert!(
        storage
            .qualify_draft_editor_candidate_selector(&store, candidate, prior)
            .is_ok()
    );
}
