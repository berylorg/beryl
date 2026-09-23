use super::*;
use beryl_home_store::CursorReadLimits;
use beryl_model::{SyndicItemId, SyndicThreadId};
use support::{draft_id, exact_cas};
use syndic_storage::test_faults::FixtureRecord;

fn create_child(
    store: &HomeStore,
    storage: &SyndicStorage,
    parent: SyndicThreadId,
    child: u8,
    draft: u8,
) {
    exact_cas::converge_transcript(store, storage.clone(), parent);
    let thread = storage.thread(store, parent, limit()).unwrap().unwrap();
    let head = storage
        .transcript_view_head(store, parent, limit())
        .unwrap()
        .unwrap();
    let entries = storage
        .transcript_entries(
            store,
            parent,
            head.generation(),
            None,
            CursorReadLimits::new(64, 1_000_000).unwrap(),
        )
        .unwrap();
    let entry = entries
        .records()
        .iter()
        .find(|entry| entry.projection_id() == support::populated::source_projection())
        .unwrap();
    let source = storage
        .prepare_discussion_source(
            store,
            DiscussionContextSource::new(
                parent,
                support::populated::source_turn(),
                support::populated::source_item(),
                entry.projection_id(),
                entry.projection_revision(),
                DiscussionContextRange::new(0, 9).unwrap(),
            ),
            thread.selected_path(),
            CurrentTranscriptEntryProof::new(head.generation(), entry.position()),
            DiscussionContextText::new("assistant").unwrap(),
        )
        .unwrap();
    let prepared = storage
        .prepare_discussion_creation(
            store,
            source,
            CreateDiscussion::new(
                id(child),
                draft_id(draft),
                timestamp(5),
                DraftEditHistoryPolicyV1::new(65536, 1).unwrap(),
            ),
        )
        .unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(prepared.contribution()).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

fn admit_request(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: AdmitDiscussionHandoff,
) -> DiscussionParentRequest {
    let prepared = storage
        .prepare_discussion_handoff(store, DiscussionHandoffMutation::Admit(request.clone()))
        .unwrap();
    let result = DiscussionParentRequest {
        child_gate: prepared.intent().new_gate(),
        parent_thread_id: request.parent.thread_id,
        context_owner: request.context_owner,
        context_digest: request.context_digest,
    };
    execute(store, prepared);
    result
}

fn proven(
    store: &HomeStore,
    storage: &SyndicStorage,
    request: DiscussionParentRequest,
) -> PreparedDiscussionParent {
    let DiscussionParentEligibility::Proven(proof) =
        storage.prepare_discussion_parent(store, request).unwrap()
    else {
        panic!("expected parent proof");
    };
    proof
}

#[path = "parent_eligibility/draft.rs"]
mod draft;
#[path = "parent_eligibility/nested.rs"]
mod nested;
#[path = "parent_eligibility/ordinary.rs"]
mod ordinary;
