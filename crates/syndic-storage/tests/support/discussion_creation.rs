use super::{draft_id, exact_cas, id, timestamp};
use beryl_home_store::{CommandOutcome, CursorReadLimits, HomeCommand, HomeStore};
use beryl_model::SyndicThreadId;
use syndic_storage::*;
fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(400_000).unwrap()
}

pub fn create_child(
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
        .find(|entry| entry.projection_id() == super::populated::source_projection())
        .unwrap();
    let source = storage
        .prepare_discussion_source(
            store,
            DiscussionContextSource::new(
                parent,
                super::populated::source_turn(),
                super::populated::source_item(),
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
