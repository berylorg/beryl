use super::*;

pub(super) fn release(
    store: &HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
) {
    let thread = storage
        .thread(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    if head.lifecycle() != ProjectionLifecycle::Current {
        let generation = head.generation();
        execute(
            store,
            storage.start_transcript_build(
                storage.revision(store).unwrap(),
                StartTranscriptBuild::new(thread_id, thread.revision(), head.revision()),
            ),
        );
        for _ in 0..1_024 {
            let build = storage
                .transcript_build(store, thread_id, generation, point_limit())
                .unwrap()
                .unwrap();
            if build.phase() == TranscriptBuildPhase::Complete {
                break;
            }
            execute(
                store,
                storage.advance_transcript_build(
                    storage.revision(store).unwrap(),
                    AdvanceTranscriptBuild::new(thread_id, generation, build.revision()),
                ),
            );
        }
    }
    let gate = storage
        .input_gate(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let state = storage
        .turn_state(store, turn_id, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Current);
    execute(
        store,
        storage.complete_terminal_history(
            storage.revision(store).unwrap(),
            CompleteTerminalHistory::new(
                thread_id,
                turn_id,
                gate,
                state.revision(),
                head.generation(),
                head.revision(),
            ),
        ),
    );
}
