use super::projection::{end_and_freeze, generated_item, publish_projection};
use super::*;

#[test]
fn detached_generated_resolution_preserves_selected_transcript_and_history_summary() {
    let f = MediaHome::new("generated-projection-detached");
    f.frame(
        20,
        true,
        ProviderImageGenerationStatusV1::Completed,
        None,
        Some(ProviderTextV1::inline("C:\\retained.png")),
        6,
    );
    end_and_freeze(&f);
    let (set, projection) = publish_projection(&f);
    converge_and_release_terminal_history(&f.store, f.storage.clone(), f.thread, f.turn);
    let thread = f
        .storage
        .thread(&f.store, f.thread, limit())
        .unwrap()
        .unwrap();
    let draft = f
        .storage
        .current_draft(&f.store, f.thread, limit())
        .unwrap()
        .unwrap();
    let summary = f
        .storage
        .history_summary(&f.store, f.thread, limit())
        .unwrap()
        .unwrap();
    let head = f
        .storage
        .transcript_view_head(&f.store, f.thread, limit())
        .unwrap()
        .unwrap();
    let entry = f
        .storage
        .transcript_entries(
            &f.store,
            f.thread,
            head.generation(),
            None,
            CursorReadLimits::new(256, 65_536).unwrap(),
        )
        .unwrap();
    let user = entry
        .records()
        .iter()
        .find(|entry| entry.item_id() != generated_item())
        .unwrap();
    let draft_revision = draft.draft().revision().checked_next().unwrap();
    crate::support::commit(
        &f.store,
        f.storage.clone(),
        crate::support::batch([
            syndic_storage::test_faults::FixtureRecord::Draft(DraftRecord::new(
                draft.draft().id(),
                f.thread,
                draft_revision,
                DraftSubmissionIntent::Replacement(ReplacementEditIntent::new(
                    f.turn,
                    thread.selected_path(),
                    CurrentTranscriptEntryProof::new(head.generation(), user.position()),
                )),
                draft.draft().root_history(),
                draft.draft().created_at(),
                summary.last_activity_at(),
            )),
            syndic_storage::test_faults::FixtureRecord::DraftByThread(DraftByThreadRecord::new(
                f.thread,
                draft.draft().id(),
                draft_revision,
                thread.revision(),
            )),
        ]),
    );
    let replacement = crate::support::exact_cas::submit_current_draft(
        &f.store,
        f.storage.clone(),
        f.thread,
        draft_id(73),
        SyndicItemId::from_bytes([74; 16]),
        "replace selected history",
        timestamp(10),
    );
    assert_ne!(replacement, f.turn);
    let selected_head = f
        .storage
        .transcript_view_head(&f.store, f.thread, limit())
        .unwrap()
        .unwrap();
    let selected_summary = f
        .storage
        .history_summary(&f.store, f.thread, limit())
        .unwrap()
        .unwrap();
    assert_eq!(selected_head.committed_tail(), Some(replacement));
    let asset = f.prepare(20);
    assert_committed(f.store.execute(f.publication(20, asset, 11)));
    assert_eq!(
        f.storage
            .transcript_view_head(&f.store, f.thread, limit())
            .unwrap(),
        Some(selected_head)
    );
    assert_eq!(
        f.storage
            .history_summary(&f.store, f.thread, limit())
            .unwrap(),
        Some(selected_summary)
    );
    assert_eq!(publish_projection(&f), (set, projection));
    assert_eq!(
        f.storage
            .turn_state(&f.store, f.turn, limit())
            .unwrap()
            .unwrap()
            .finalized_item_count(),
        2
    );
    f.store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
}
