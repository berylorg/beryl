#![cfg(feature = "test-faults")]

#[path = "phase9_recovery_projection/support.rs"]
mod support;

use std::{sync::Arc, thread, time::Duration};

use beryl_home_store::{
    CommandOutcome, HomeCommand, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    test_faults::{FaultController, FaultPoint},
};
use syndic_storage::{
    ComposerAtom, ComposerPayload, DraftPayloadUpdate, DraftPayloadUpdateDecision, PreparedContent,
    RecoveryProjectionError, RecoveryProjectionRequest, SyndicStorage, SyndicTimestamp,
    TurnTerminalOutcome,
};

use support::{Builder, TestHome, point_limit, stage_prepared_content};

fn open_with_faults(path: &std::path::Path, faults: FaultController) -> HomeStore {
    HomeStore::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap()
}

#[test]
fn revision_change_during_recovery_assembly_rejects_the_whole_result() {
    let home = TestHome::new("phase9-recovery-revision-race");
    let faults = FaultController::new();
    let mut store = open_with_faults(home.path(), faults.clone());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut builder = Builder::new(&store, storage, 9);
    let completed = builder.submit_text("stable recovery history");
    builder.complete_without_assistant(completed, TurnTerminalOutcome::Complete);
    builder.submit_text("pending input");
    let thread_id = builder.thread();
    let selected_path = builder.selected_path();

    let current = storage
        .current_draft(&store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let payload =
        ComposerPayload::new(vec![ComposerAtom::text("concurrent draft").unwrap()]).unwrap();
    let content = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(&store, storage, &content);
    let update = match DraftPayloadUpdate::prepare(
        &current,
        &content,
        SyndicTimestamp::from_unix_millis(100),
    )
    .unwrap()
    {
        DraftPayloadUpdateDecision::Update(update) => update,
        DraftPayloadUpdateDecision::NoChange => unreachable!(),
    };

    let block = faults.block_next(FaultPoint::BeforeReadConfirmation);
    let store = Arc::new(store);
    let reader_store = Arc::clone(&store);
    let reader = thread::spawn(move || {
        storage.prepare_recovery_projection(
            &reader_store,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                thread_id,
                selected_path,
                Some(100_000),
            ),
        )
    });
    assert!(block.wait_until_reached(Duration::from_secs(10)));

    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(storage.update_draft_payload(storage.revision(&store).unwrap(), update))
        .unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed recovery-race update, got {outcome:?}"),
    }
    block.release();

    assert!(matches!(
        reader.join().unwrap(),
        Err(RecoveryProjectionError::ConcurrentChange)
    ));
    store
        .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let store = Arc::try_unwrap(store).unwrap_or_else(|_| panic!("reader retained the home"));
    store.close().unwrap();
}
