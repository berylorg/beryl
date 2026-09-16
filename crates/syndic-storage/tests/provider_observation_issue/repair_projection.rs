use super::*;

#[test]
fn incomplete_repair_supersedes_in_progress_transcript_publication() {
    for advance in [false, true] {
        let fixture = setup("repair-projection-supersession");
        let target = super::repair_retained::terminal_target(&fixture, false);
        let gate = fixture
            .storage
            .input_gate(&fixture.store, fixture.thread, limit())
            .unwrap()
            .unwrap();
        committed_command(
            fixture
                .store
                .execute_current(fixture.storage.current_require_terminal_repair(
                    RequireTerminalRepair::new(fixture.thread, gate.revision(), target.clone()),
                )),
        );
        let thread = fixture
            .storage
            .thread(&fixture.store, fixture.thread, limit())
            .unwrap()
            .unwrap();
        let head = fixture
            .storage
            .transcript_view_head(&fixture.store, fixture.thread, limit())
            .unwrap()
            .unwrap();
        committed_command(
            fixture
                .store
                .execute_current(fixture.storage.current_start_transcript_build(
                    StartTranscriptBuild::new(fixture.thread, thread.revision(), head.revision()),
                )),
        );
        if advance {
            let build = fixture
                .storage
                .transcript_build(&fixture.store, fixture.thread, head.generation(), limit())
                .unwrap()
                .unwrap();
            committed_command(
                fixture
                    .store
                    .execute_current(fixture.storage.current_advance_transcript_build(
                        AdvanceTranscriptBuild::new(
                            fixture.thread,
                            head.generation(),
                            build.revision(),
                        ),
                    )),
            );
        }
        let build = fixture
            .storage
            .transcript_build(&fixture.store, fixture.thread, head.generation(), limit())
            .unwrap()
            .unwrap();
        assert!(matches!(
            build.phase(),
            TranscriptBuildPhase::Collecting { .. } | TranscriptBuildPhase::Publishing { .. }
        ));
        let gate = fixture
            .storage
            .input_gate(&fixture.store, fixture.thread, limit())
            .unwrap()
            .unwrap();
        let state = fixture
            .storage
            .turn_state(&fixture.store, fixture.turn, limit())
            .unwrap()
            .unwrap();
        committed_command(
            fixture
                .store
                .execute_current(fixture.storage.current_converge_repair_incomplete(
                    ConvergeRepairIncomplete::new(
                        fixture.thread,
                        gate.revision(),
                        state.revision(),
                        target,
                        TurnIncompleteReason::ItemAuditFailed,
                        timestamp(20),
                    ),
                )),
        );
        let stale = fixture
            .storage
            .transcript_view_head(&fixture.store, fixture.thread, limit())
            .unwrap()
            .unwrap();
        assert_eq!(stale.lifecycle(), ProjectionLifecycle::Stale);
        assert_eq!(
            stale.generation(),
            head.generation().checked_next().unwrap()
        );
        let superseded = fixture
            .storage
            .transcript_build(&fixture.store, fixture.thread, head.generation(), limit())
            .unwrap()
            .unwrap();
        assert_eq!(superseded.phase(), TranscriptBuildPhase::Superseded);
        let before = fixture.storage.revision(&fixture.store).unwrap();
        not_committed_command(
            fixture
                .store
                .execute_current(fixture.storage.current_advance_transcript_build(
                    AdvanceTranscriptBuild::new(
                        fixture.thread,
                        head.generation(),
                        build.revision(),
                    ),
                )),
        );
        assert_eq!(fixture.storage.revision(&fixture.store).unwrap(), before);
        exact_cas::converge_and_release_terminal_history(
            &fixture.store,
            fixture.storage.clone(),
            fixture.thread,
            fixture.turn,
        );
        fixture
            .store
            .scrub_whole_home(beryl_home_store::WholeHomeScrubTrigger::Explicit)
            .unwrap();
    }
}
