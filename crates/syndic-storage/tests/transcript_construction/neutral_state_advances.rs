use super::*;
use beryl_home_store::{CommandOutcome, WholeHomeScrubTrigger};
use syndic_storage::{CancelBindingActivation, LiveSourceEvent, SourceEventSequence};

struct Fixture {
    store: HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    pending: SubmittedTurn,
    home: TestHome,
}

fn fixture(parent: bool) -> Fixture {
    let home = TestHome::new("transcript-neutral-state-advance");
    let mut candidate = open(home.path());
    let storage = SyndicStorage::register(&mut candidate).unwrap();
    let store = candidate
        .prepare_publication(SyndicStorage::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let thread = create_thread(&store, storage.clone());
    if parent {
        let root = submit_text(
            &store,
            storage.clone(),
            thread,
            "prior complete root",
            draft_id(3),
            SyndicItemId::from_bytes([30; 16]),
            timestamp(3),
        );
        complete_turn(
            &store,
            storage.clone(),
            thread,
            root,
            timestamp(4),
            timestamp(5),
            timestamp(6),
        );
        converge_and_release_terminal_history(&store, storage.clone(), thread, root.turn);
    }
    let authored = (0..70)
        .map(|index| format!("paragraph {index}\n\n"))
        .collect::<String>();
    let pending = submit_text(
        &store,
        storage.clone(),
        thread,
        &authored,
        draft_id(4),
        SyndicItemId::from_bytes([31; 16]),
        timestamp(8),
    );
    let generation = project_item(&store, storage.clone(), pending.item);
    assert!(
        storage
            .item_projection_set(&store, pending.item, generation, point_limit())
            .unwrap()
            .unwrap()
            .projection_count()
            > 64
    );
    Fixture {
        store,
        storage,
        thread,
        pending,
        home,
    }
}

#[derive(Clone, Copy)]
enum BuildStage {
    Collecting,
    Publishing,
}

fn stage(f: &Fixture, stage: BuildStage) -> TranscriptBuildRecord {
    let (generation, _) = start_transcript_build(&f.store, f.storage.clone(), f.thread);
    let mut build = advance_transcript_build(&f.store, f.storage.clone(), f.thread, generation);
    match stage {
        BuildStage::Collecting => assert!(matches!(
            build.phase(),
            TranscriptBuildPhase::Collecting { .. }
        )),
        BuildStage::Publishing => {
            while matches!(build.phase(), TranscriptBuildPhase::Collecting { .. }) {
                build = advance_transcript_build(&f.store, f.storage.clone(), f.thread, generation);
            }
            for _ in 0..4 {
                build = advance_transcript_build(&f.store, f.storage.clone(), f.thread, generation);
                if build.entry_count() > 1 {
                    break;
                }
            }
            assert!(matches!(
                build.phase(),
                TranscriptBuildPhase::Publishing { .. }
            ));
            assert!(build.entry_count() >= 64);
        }
    }
    f.store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    build
}

fn assert_superseded_and_resume(f: &Fixture, before: TranscriptBuildRecord) {
    let old_paths = path_turns(&f.store, f.storage.clone(), f.thread, before.generation());
    let old_entries =
        transcript_entries(&f.store, f.storage.clone(), f.thread, before.generation());
    let old = f
        .storage
        .transcript_build(&f.store, f.thread, before.generation(), point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(old.phase(), TranscriptBuildPhase::Superseded);
    assert_eq!(old.revision(), before.revision().checked_next().unwrap());
    assert_eq!(old.entry_count(), before.entry_count());
    assert_eq!(old.entry_digest(), before.entry_digest());
    let head = f
        .storage
        .transcript_view_head(&f.store, f.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(
        head.generation(),
        before.generation().checked_next().unwrap()
    );
    assert_eq!(head.revision(), old.revision());
    assert_eq!(head.lifecycle(), ProjectionLifecycle::Stale);
    assert_eq!(head.entry_count(), 0);
    assert!(
        f.storage
            .transcript_build(&f.store, f.thread, head.generation(), point_limit())
            .unwrap()
            .is_none()
    );
    f.store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    let (generation, _) = start_transcript_build(&f.store, f.storage.clone(), f.thread);
    let complete = finish_transcript_build(&f.store, f.storage.clone(), f.thread, generation);
    assert_eq!(complete.phase(), TranscriptBuildPhase::Complete);
    let state = f
        .storage
        .turn_state(&f.store, f.pending.turn, point_limit())
        .unwrap()
        .unwrap();
    let current_paths = path_turns(&f.store, f.storage.clone(), f.thread, generation);
    let current = current_paths
        .iter()
        .find(|path| path.turn_id() == f.pending.turn)
        .unwrap();
    assert_eq!(current.state_revision(), state.revision());
    assert_eq!(current.lifecycle(), state.lifecycle());
    assert_eq!(current.source_event_count(), state.source_event_count());
    assert_eq!(
        path_turns(&f.store, f.storage.clone(), f.thread, before.generation()),
        old_paths
    );
    assert_eq!(
        transcript_entries(&f.store, f.storage.clone(), f.thread, before.generation()),
        old_entries
    );
    f.store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
}

fn activation_race(stage_kind: BuildStage) {
    let f = fixture(true);
    let before = stage(&f, stage_kind);
    let item = f
        .storage
        .canonical_item(&f.store, f.pending.item, point_limit())
        .unwrap()
        .unwrap();
    let turn = f
        .storage
        .turn(&f.store, f.pending.turn, point_limit())
        .unwrap()
        .unwrap();
    support::exact_cas::activate_turn(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.pending.turn,
        timestamp(9),
    );
    assert_eq!(
        f.storage
            .canonical_item(&f.store, f.pending.item, point_limit())
            .unwrap()
            .unwrap(),
        item
    );
    assert_eq!(
        f.storage
            .turn(&f.store, f.pending.turn, point_limit())
            .unwrap()
            .unwrap(),
        turn
    );
    assert_superseded_and_resume(&f, before);
    f.store.close().unwrap();
    drop(f.home);
}

fn live_activation_race(stage_kind: BuildStage) {
    let f = fixture(true);
    let source = establish_turn(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.pending.turn,
        timestamp(9),
    );
    let before = stage(&f, stage_kind);
    let item = f
        .storage
        .canonical_item(&f.store, f.pending.item, point_limit())
        .unwrap()
        .unwrap();
    let turn = f
        .storage
        .turn(&f.store, f.pending.turn, point_limit())
        .unwrap()
        .unwrap();
    admit(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.pending.turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(10),
    );
    assert_eq!(
        f.storage
            .canonical_item(&f.store, f.pending.item, point_limit())
            .unwrap()
            .unwrap(),
        item
    );
    assert_eq!(
        f.storage
            .turn(&f.store, f.pending.turn, point_limit())
            .unwrap()
            .unwrap(),
        turn
    );
    assert_superseded_and_resume(&f, before);
    f.store.close().unwrap();
    drop(f.home);
}

#[test]
fn binding_activation_supersedes_a_collected_turn_in_collecting_build() {
    activation_race(BuildStage::Collecting);
}

#[test]
fn binding_activation_supersedes_a_partially_published_build() {
    activation_race(BuildStage::Publishing);
}

#[test]
fn live_activation_supersedes_a_collected_turn_in_collecting_build() {
    live_activation_race(BuildStage::Collecting);
}

#[test]
fn live_activation_supersedes_a_partially_published_build() {
    live_activation_race(BuildStage::Publishing);
}

#[test]
fn neutral_activation_refreshes_current_path_without_replacing_head_or_entries() {
    let f = fixture(false);
    let (generation, _) = start_transcript_build(&f.store, f.storage.clone(), f.thread);
    finish_transcript_build(&f.store, f.storage.clone(), f.thread, generation);
    let head = f
        .storage
        .transcript_view_head(&f.store, f.thread, point_limit())
        .unwrap()
        .unwrap();
    let build = f
        .storage
        .transcript_build(&f.store, f.thread, generation, point_limit())
        .unwrap()
        .unwrap();
    let entries = transcript_entries(&f.store, f.storage.clone(), f.thread, generation);
    let source = establish_turn(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.pending.turn,
        timestamp(9),
    );
    admit(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.pending.turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(10),
    );
    assert_eq!(
        f.storage
            .transcript_view_head(&f.store, f.thread, point_limit())
            .unwrap()
            .unwrap(),
        head
    );
    assert_eq!(
        f.storage
            .transcript_build(&f.store, f.thread, generation, point_limit())
            .unwrap()
            .unwrap(),
        build
    );
    assert_eq!(
        transcript_entries(&f.store, f.storage.clone(), f.thread, generation),
        entries
    );
    let path = path_turns(&f.store, f.storage.clone(), f.thread, generation)[0];
    let state = f
        .storage
        .turn_state(&f.store, f.pending.turn, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(path.state_revision(), state.revision());
    assert_eq!(path.lifecycle(), syndic_storage::TurnLifecycle::Active);
    f.store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    f.store.close().unwrap();
}

#[test]
fn neutral_advances_preserve_stale_head_without_a_build() {
    let f = fixture(false);
    let head = f
        .storage
        .transcript_view_head(&f.store, f.thread, point_limit())
        .unwrap()
        .unwrap();
    let source = establish_turn(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.pending.turn,
        timestamp(9),
    );
    admit(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.pending.turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(10),
    );
    assert_eq!(
        f.storage
            .transcript_view_head(&f.store, f.thread, point_limit())
            .unwrap()
            .unwrap(),
        head
    );
    assert!(
        f.storage
            .transcript_build(&f.store, f.thread, head.generation(), point_limit())
            .unwrap()
            .is_none()
    );
    f.store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    f.store.close().unwrap();
}

#[test]
fn uncollected_turn_state_advances_keep_the_active_build_resumable() {
    let f = fixture(false);
    let (generation, before) = start_transcript_build(&f.store, f.storage.clone(), f.thread);
    let source = establish_turn(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.pending.turn,
        timestamp(9),
    );
    admit(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.pending.turn,
        &source,
        SourceEventPayload::TurnActivated,
        timestamp(10),
    );
    assert_eq!(
        f.storage
            .transcript_build(&f.store, f.thread, generation, point_limit())
            .unwrap()
            .unwrap(),
        before
    );
    finish_transcript_build(&f.store, f.storage.clone(), f.thread, generation);
    f.store
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    f.store.close().unwrap();
}

#[test]
fn activation_cancellation_supersedes_the_captured_pending_state() {
    let f = fixture(true);
    let (_, snapshot) = support::exact_cas::activate_turn(
        &f.store,
        f.storage.clone(),
        f.thread,
        f.pending.turn,
        timestamp(9),
    );
    let before = stage(&f, BuildStage::Collecting);
    let binding = f
        .storage
        .current_binding(&f.store, f.thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = f
        .storage
        .input_gate(&f.store, f.thread, point_limit())
        .unwrap()
        .unwrap();
    let state = f
        .storage
        .turn_state(&f.store, f.pending.turn, point_limit())
        .unwrap()
        .unwrap();
    let request = CancelBindingActivation::new(
        f.thread,
        binding.binding().revision(),
        gate.revision(),
        state.revision(),
        binding.binding().selected_path(),
        snapshot,
        f.pending.turn,
    );
    execute(
        &f.store,
        f.storage
            .cancel_binding_activation(f.storage.revision(&f.store).unwrap(), request),
    );
    assert_superseded_and_resume(&f, before);
    f.store.close().unwrap();
}

#[path = "neutral_state_advances/guards.rs"]
mod guards;
