use super::*;

pub(super) fn finish_current_transcript(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) {
    let thread_record = storage
        .thread(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, thread, point_limit())
        .unwrap()
        .unwrap();
    if head.lifecycle() == ProjectionLifecycle::Current {
        return;
    }
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Stale);
    let generation = head.generation();
    execute(
        store,
        storage.start_transcript_build(
            storage.revision(store).unwrap(),
            StartTranscriptBuild::new(thread, thread_record.revision(), head.revision()),
        ),
    );
    for _ in 0..1_024 {
        let build = storage
            .transcript_build(store, thread, generation, point_limit())
            .unwrap()
            .unwrap();
        if build.phase() == TranscriptBuildPhase::Complete {
            return;
        }
        execute(
            store,
            storage.advance_transcript_build(
                storage.revision(store).unwrap(),
                AdvanceTranscriptBuild::new(thread, generation, build.revision()),
            ),
        );
    }
    panic!("bounded transcript fixture did not finish");
}

#[test]
fn divergent_nonempty_prefix_selects_inclusive_fork_not_source_mutation() {
    let home = TestHome::new("phase10-native-inclusive-fork");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut source_builder = Builder::new(&store, storage, 34);
    let first = source_builder.submit_text("shared first");
    source_builder.complete_without_assistant(first, TurnTerminalOutcome::Complete);
    finish_current_transcript(&store, storage, source_builder.thread());
    let (child, child_selected) =
        create_child_pending_at_tail(&store, storage, source_builder.thread());

    let second = source_builder.submit_text("source advances");
    source_builder.complete_without_assistant(second, TurnTerminalOutcome::Complete);

    let NativeProjectionPlan::Fork {
        basis,
        source,
        through_turn,
        native_turn_count,
    } = plan(
        &store,
        storage,
        child,
        child_selected,
        exact_cas::execution_binding(),
    )
    else {
        panic!("a nonempty earlier prefix must select an inclusive native fork")
    };
    assert_eq!(basis.represented_prefix().tail(), Some(first.turn));
    assert_eq!(source.thread_id(), source_builder.thread());
    assert_eq!(
        source.binding().native_turn_count(),
        CasNativeTurnCount::new(2)
    );
    assert_eq!(native_turn_count, CasNativeTurnCount::new(1));
    assert_eq!(
        through_turn,
        Some(CasTurnId::new(format!("test-turn-{}", first.turn)).unwrap())
    );
}
