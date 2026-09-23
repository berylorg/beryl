use super::{id, timestamp};
use beryl_home_store::{CommandOutcome, HomeCommand, HomeStore};
use beryl_model::{SyndicDraftId, SyndicItemId};
use syndic_storage::*;
#[path = "../draft_edit_history_retention/common.rs"]
mod edit_support;
#[path = "../draft_edit_history/support.rs"]
#[allow(dead_code, unused_imports)]
mod support;
fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(400_000).unwrap()
}

pub fn committed(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    let outcome = store.execute(command);
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "{outcome:?}"
    );
}

pub fn prepare_acceptance(store: &HomeStore, storage: &SyndicStorage) -> FirstAcceptance {
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

pub fn accept_next(store: &HomeStore, storage: &SyndicStorage) {
    let acceptance = prepare_acceptance(store, storage);
    committed(
        store,
        storage.first_acceptance(storage.revision(store).unwrap(), acceptance),
    );
}
