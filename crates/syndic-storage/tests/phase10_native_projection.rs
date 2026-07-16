#![cfg(feature = "test-faults")]
// The independent builder and valid populated-state fixtures both include the
// same test-only exact-CAS constants under their own module namespaces.
#![allow(clippy::duplicate_mod)]

#[path = "phase9_recovery_projection/support.rs"]
mod builder_support;
#[path = "support/mod.rs"]
mod support;

use beryl_home_store::{HomeCommand, HomeStore};
use beryl_model::{
    CasConversationToolProfile, CasNativeTurnCount, CasThreadId, CasTurnId, ExecutionBinding,
    PathFlavor, RootId, RuntimeId, RuntimeMode, RuntimeNativePath, SyndicDraftId, SyndicItemId,
    SyndicThreadId,
};
use syndic_storage::*;

use builder_support::{Builder, TestHome, exact_cas, open, point_limit, stage_prepared_content};

fn execute(store: &HomeStore, contribution: beryl_home_store::MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    store.execute(command).unwrap();
}

fn plan(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    selected: SelectedPathProof,
    execution: ExecutionBinding,
) -> NativeProjectionPlan {
    plan_with_profile(
        store,
        storage,
        thread,
        selected,
        execution,
        exact_cas::tool_profile(),
    )
}

fn plan_with_profile(
    store: &HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    selected: SelectedPathProof,
    execution: ExecutionBinding,
    tool_profile: CasConversationToolProfile,
) -> NativeProjectionPlan {
    storage
        .prepare_native_projection(
            store,
            &NativeProjectionRequest::new(thread, selected, execution, tool_profile),
            point_limit(),
        )
        .unwrap()
}

fn alternate_execution() -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([210; 16]),
        RootId::from_bytes([211; 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            "C:\\phase10-alternate-root",
        )
        .unwrap(),
    )
}

#[test]
fn root_pending_turn_selects_fresh_native_lineage() {
    let home = TestHome::new("phase10-native-fresh");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut builder = Builder::new(&store, storage, 30);
    builder.submit_text("root pending");
    let selected = builder.selected_path();

    let NativeProjectionPlan::Fresh { basis } = plan(
        &store,
        storage,
        builder.thread(),
        selected,
        exact_cas::execution_binding(),
    ) else {
        panic!("an empty represented prefix must select a fresh CAS thread")
    };
    assert_eq!(basis.thread_id(), builder.thread());
    assert_eq!(basis.selected_path(), selected);
    assert_eq!(basis.represented_prefix().tail(), None);
    assert_eq!(basis.tool_profile(), exact_cas::tool_profile());
}

#[test]
fn exact_current_projection_wins_without_remote_lineage_work() {
    let home = TestHome::new("phase10-native-current");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut builder = Builder::new(&store, storage, 31);
    builder.submit_text("root pending");
    let selected = builder.selected_path();
    let current = storage
        .current_binding(&store, builder.thread(), point_limit())
        .unwrap()
        .unwrap();
    let represented = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    let cas_thread = CasThreadId::new("phase10-current-cas").unwrap();
    execute(
        &store,
        storage.publish_valid_binding(
            storage.revision(&store).unwrap(),
            PublishValidBinding::new(
                builder.thread(),
                current.binding().revision(),
                selected,
                exact_cas::execution_binding(),
                cas_thread.clone(),
                represented,
                CasNativeTurnCount::ZERO,
                exact_cas::tool_profile(),
                CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
            ),
        ),
    );

    let NativeProjectionPlan::Current { source, .. } = plan(
        &store,
        storage,
        builder.thread(),
        selected,
        exact_cas::execution_binding(),
    ) else {
        panic!("the exact current usable projection must win")
    };
    assert_eq!(source.thread_id(), builder.thread());
    assert_eq!(source.binding().cas_thread_id(), &cas_thread);
}

#[test]
fn current_projection_with_a_different_tool_profile_is_typed_unavailable() {
    let home = TestHome::new("phase10-native-profile-mismatch");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut builder = Builder::new(&store, storage, 41);
    builder.submit_text("root pending");
    let selected = builder.selected_path();
    let current = storage
        .current_binding(&store, builder.thread(), point_limit())
        .unwrap()
        .unwrap();
    let represented = CasRepresentedPrefixProof::new(
        None,
        selected.thread_revision(),
        empty_selected_path_digest(),
    );
    execute(
        &store,
        storage.publish_valid_binding(
            storage.revision(&store).unwrap(),
            PublishValidBinding::new(
                builder.thread(),
                current.binding().revision(),
                selected,
                exact_cas::execution_binding(),
                CasThreadId::new("phase10-profile-cas").unwrap(),
                represented,
                CasNativeTurnCount::ZERO,
                exact_cas::tool_profile(),
                CasLineageProof::native(NativeCasLineage::Fresh, represented).unwrap(),
            ),
        ),
    );
    let published = storage
        .current_binding(&store, builder.thread(), point_limit())
        .unwrap()
        .unwrap();
    let different_profile = CasConversationToolProfile::v1([0x7b; 32]);

    let NativeProjectionPlan::Unavailable {
        basis,
        source: Some(source),
        reason,
    } = plan_with_profile(
        &store,
        storage,
        builder.thread(),
        selected,
        exact_cas::execution_binding(),
        different_profile,
    )
    else {
        panic!("a mismatched native tool profile must not be reused")
    };
    assert_eq!(basis.tool_profile(), different_profile);
    assert_eq!(source.thread_id(), builder.thread());
    assert_eq!(source.binding_revision(), published.binding().revision());
    assert_eq!(source.selected_path(), selected);
    assert_eq!(
        source.binding().cas_thread_id().as_str(),
        "phase10-profile-cas"
    );
    assert_eq!(
        reason,
        NativeProjectionUnavailable::SourceToolProfileMismatch
    );
}

#[test]
fn exact_terminal_parent_selects_native_resume() {
    let home = TestHome::new("phase10-native-resume");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut builder = Builder::new(&store, storage, 32);
    let first = builder.submit_text("first");
    builder.complete_with_assistant(first, AssistantMessagePhase::FinalAnswer, "answer");
    builder.submit_text("second pending");
    let selected = builder.selected_path();

    let NativeProjectionPlan::Resume { basis, source } = plan(
        &store,
        storage,
        builder.thread(),
        selected,
        exact_cas::execution_binding(),
    ) else {
        panic!("the exact same-thread terminal parent must select native resume")
    };
    assert_eq!(basis.represented_prefix().tail(), Some(first.turn));
    assert_eq!(source.thread_id(), builder.thread());
    assert_eq!(
        source.binding().native_turn_count(),
        CasNativeTurnCount::new(1)
    );
}

#[test]
fn exact_native_source_with_another_execution_is_explicitly_unavailable() {
    let home = TestHome::new("phase10-native-execution-mismatch");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut builder = Builder::new(&store, storage, 33);
    let first = builder.submit_text("first");
    builder.complete_without_assistant(first, TurnTerminalOutcome::Complete);
    builder.submit_text("second pending");

    let NativeProjectionPlan::Unavailable {
        source: Some(source),
        reason,
        ..
    } = plan(
        &store,
        storage,
        builder.thread(),
        builder.selected_path(),
        alternate_execution(),
    )
    else {
        panic!("cross-execution native authority must not be reused")
    };
    assert_eq!(source.thread_id(), builder.thread());
    assert_eq!(
        source.binding().execution(),
        &exact_cas::execution_binding()
    );
    assert_eq!(reason, NativeProjectionUnavailable::SourceExecutionMismatch);
}

#[test]
fn context_bearing_pending_turn_requires_the_later_context_projection() {
    let home = TestHome::new("phase10-native-discussion-context");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    support::commit(
        &store,
        storage,
        support::batch(support::populated::populated_records()),
    );
    let thread = support::id(36);
    let draft = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let payload =
        ComposerPayload::new(vec![ComposerAtom::text("discuss context").unwrap()]).unwrap();
    let content = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(&store, storage, &content);
    let DraftPayloadUpdateDecision::Update(update) =
        DraftPayloadUpdate::prepare(&draft, &content, SyndicTimestamp::from_unix_millis(6))
            .unwrap()
    else {
        panic!("discussion draft must become nonempty")
    };
    execute(
        &store,
        storage.update_draft_payload(storage.revision(&store).unwrap(), update),
    );
    let draft = storage
        .current_draft(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    execute(
        &store,
        storage.submit_idle_draft(
            storage.revision(&store).unwrap(),
            IdleSubmission::new(
                thread,
                draft.thread().revision(),
                draft.draft().id(),
                draft.draft().revision(),
                draft.draft().content(),
                gate.record().revision(),
                SyndicDraftId::from_bytes([250; 16]),
                SyndicItemId::from_bytes([251; 16]),
                AdmissionMarkers::default(),
                SyndicTimestamp::from_unix_millis(7),
            ),
        ),
    );
    let selected = storage
        .thread(&store, thread, point_limit())
        .unwrap()
        .unwrap();
    let result = storage.prepare_native_projection(
        &store,
        &NativeProjectionRequest::new(
            thread,
            SelectedPathProof::new(
                selected.record().committed_tail(),
                selected.record().revision(),
                selected.record().selected_path_digest(),
            ),
            exact_cas::execution_binding(),
            exact_cas::tool_profile(),
        ),
        point_limit(),
    );

    assert!(matches!(
        result,
        Err(NativeProjectionError::DiscussionContextProjectionRequired)
    ));
}

fn create_child_pending_at_tail(
    store: &HomeStore,
    storage: SyndicStorage,
    source_thread: SyndicThreadId,
) -> (SyndicThreadId, SelectedPathProof) {
    let tail = storage
        .thread_tail(store, source_thread, point_limit())
        .unwrap()
        .unwrap();
    let child = SyndicThreadId::from_bytes([220; 16]);
    let created_at = tail.last_activity_at();
    execute(
        store,
        storage.create_thread(
            storage.revision(store).unwrap(),
            CreateThread::from_tail(
                child,
                SyndicDraftId::from_bytes([221; 16]),
                created_at,
                tail,
            )
            .unwrap(),
        ),
    );
    let payload = ComposerPayload::new(vec![ComposerAtom::text("child pending").unwrap()]).unwrap();
    let content = PreparedContent::composer(&payload).unwrap();
    stage_prepared_content(store, storage, &content);
    let draft = storage
        .current_draft(store, child, point_limit())
        .unwrap()
        .unwrap();
    let DraftPayloadUpdateDecision::Update(update) = DraftPayloadUpdate::prepare(
        &draft,
        &content,
        SyndicTimestamp::from_unix_millis(created_at.unix_millis() + 1),
    )
    .unwrap() else {
        panic!("child branch draft must become nonempty")
    };
    execute(
        store,
        storage.update_draft_payload(storage.revision(store).unwrap(), update),
    );
    let draft = storage
        .current_draft(store, child, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(store, child, point_limit())
        .unwrap()
        .unwrap();
    execute(
        store,
        storage.submit_idle_draft(
            storage.revision(store).unwrap(),
            IdleSubmission::new(
                child,
                draft.thread().revision(),
                draft.draft().id(),
                draft.draft().revision(),
                draft.draft().content(),
                gate.record().revision(),
                SyndicDraftId::from_bytes([222; 16]),
                SyndicItemId::from_bytes([223; 16]),
                AdmissionMarkers::default(),
                SyndicTimestamp::from_unix_millis(created_at.unix_millis() + 2),
            ),
        ),
    );
    let thread = storage
        .thread(store, child, point_limit())
        .unwrap()
        .unwrap();
    (
        child,
        SelectedPathProof::new(
            thread.record().committed_tail(),
            thread.record().revision(),
            thread.record().selected_path_digest(),
        ),
    )
}

#[test]
fn cross_execution_ancestor_is_unavailable_without_target_retirement_authority() {
    let home = TestHome::new("phase10-native-ancestor-execution-mismatch");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut source_builder = Builder::new(&store, storage, 37);
    let first = source_builder.submit_text("shared first");
    source_builder.complete_without_assistant(first, TurnTerminalOutcome::Complete);
    finish_current_transcript(&store, storage, source_builder.thread());
    let (child, child_selected) =
        create_child_pending_at_tail(&store, storage, source_builder.thread());

    let NativeProjectionPlan::Unavailable {
        source: None,
        reason,
        ..
    } = plan(
        &store,
        storage,
        child,
        child_selected,
        alternate_execution(),
    )
    else {
        panic!("a mismatched ancestor must not grant target retirement authority")
    };
    assert_eq!(reason, NativeProjectionUnavailable::SourceExecutionMismatch);
}

fn finish_current_transcript(store: &HomeStore, storage: SyndicStorage, thread: SyndicThreadId) {
    let thread_record = storage
        .thread(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let head = storage
        .transcript_view_head(store, thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(head.record().lifecycle(), ProjectionLifecycle::Stale);
    let generation = head.record().generation();
    execute(
        store,
        storage.start_transcript_build(
            storage.revision(store).unwrap(),
            StartTranscriptBuild::new(
                thread,
                thread_record.record().revision(),
                head.record().revision(),
            ),
        ),
    );
    for _ in 0..1_024 {
        let build = storage
            .transcript_build(store, thread, generation, point_limit())
            .unwrap()
            .unwrap();
        if build.record().phase() == TranscriptBuildPhase::Complete {
            return;
        }
        execute(
            store,
            storage.advance_transcript_build(
                storage.revision(store).unwrap(),
                AdvanceTranscriptBuild::new(thread, generation, build.record().revision()),
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
