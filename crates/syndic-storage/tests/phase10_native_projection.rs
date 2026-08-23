#![cfg(feature = "test-faults")]

#[path = "phase10_native_projection/divergent_fork.rs"]
mod divergent_fork;
#[path = "phase10_native_projection/fixtures.rs"]
mod fixtures;
#[path = "support/mod.rs"]
mod support;

use beryl_model::{
    CasConversationToolProfile, CasNativeTurnCount, CasTurnId, SyndicDraftId, SyndicPathDigest,
    SyndicThreadId, SyndicTurnId,
};
use syndic_storage::{
    BindingState, NativeProjectionError, NativeProjectionPlan, NativeProjectionRequest,
    NativeProjectionUnavailable, SelectedPathProof, SyndicPointReadLimit, SyndicStorage,
    empty_selected_path_digest,
};

use support::{TestHome, id, open, seed_populated};

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn selected_path(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread_id: SyndicThreadId,
) -> SelectedPathProof {
    let thread = storage
        .thread(store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    SelectedPathProof::new(
        thread.committed_tail(),
        thread.revision(),
        thread.selected_path_digest(),
    )
}

#[test]
fn exact_current_projection_reuses_the_matching_native_binding() {
    let home = TestHome::new("phase10-native-current");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = fixtures::seed_root_pending(&store, storage, 70, true);
    let before_revision = storage.revision(&store).unwrap();

    let NativeProjectionPlan::Current { basis, source } = storage
        .prepare_native_projection(
            &store,
            &NativeProjectionRequest::new(
                fixture.thread,
                fixture.selected,
                fixture.execution.clone(),
                fixture.tool_profile,
            ),
            point_limit(),
        )
        .unwrap()
    else {
        panic!("exact current projection must reuse its native binding")
    };
    assert_eq!(basis.thread_id(), fixture.thread);
    assert_eq!(basis.selected_path().tail(), Some(fixture.pending));
    assert_eq!(basis.expected_binding_revision(), fixture.binding_revision);
    assert_eq!(basis.selected_path(), fixture.selected);
    assert_eq!(basis.represented_prefix().tail(), None);
    assert_eq!(source.thread_id(), fixture.thread);
    assert_eq!(source.binding_revision(), fixture.binding_revision);
    assert_eq!(source.selected_path(), fixture.selected);
    assert_eq!(
        source.binding().native_turn_count(),
        CasNativeTurnCount::ZERO
    );
    assert_eq!(source.binding().execution(), &fixture.execution);
    assert_eq!(source.binding().tool_profile(), fixture.tool_profile);
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    store.close().unwrap();
}

#[test]
fn root_pending_turn_selects_fresh_native_lineage() {
    let home = TestHome::new("phase10-native-fresh");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = fixtures::seed_root_pending(&store, storage, 74, false);
    let before_revision = storage.revision(&store).unwrap();

    let NativeProjectionPlan::Fresh { basis } = storage
        .prepare_native_projection(
            &store,
            &NativeProjectionRequest::new(
                fixture.thread,
                fixture.selected,
                fixture.execution,
                fixture.tool_profile,
            ),
            point_limit(),
        )
        .unwrap()
    else {
        panic!("root pending turn must select fresh native lineage")
    };
    assert_eq!(basis.thread_id(), fixture.thread);
    assert_eq!(basis.expected_binding_revision(), fixture.binding_revision);
    assert_eq!(basis.selected_path(), fixture.selected);
    assert_eq!(basis.represented_prefix().tail(), None);
    assert_eq!(
        basis.represented_prefix().digest(),
        empty_selected_path_digest()
    );
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    store.close().unwrap();
}

#[test]
fn exact_terminal_parent_selects_native_resume() {
    let home = TestHome::new("phase10-native-resume");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    let fixture = fixtures::append_pending(
        &store,
        storage,
        id(30),
        SyndicTurnId::from_bytes([90; 16]),
        support::populated::source_turn(),
    );
    let before_revision = storage.revision(&store).unwrap();

    let NativeProjectionPlan::Resume { basis, source } = storage
        .prepare_native_projection(
            &store,
            &NativeProjectionRequest::new(
                fixture.thread,
                fixture.selected,
                fixture.execution.clone(),
                fixture.tool_profile,
            ),
            point_limit(),
        )
        .unwrap()
    else {
        panic!("exact terminal parent must select native resume")
    };
    assert_eq!(
        basis.represented_prefix().tail(),
        Some(support::populated::source_turn())
    );
    assert_eq!(source.thread_id(), fixture.thread);
    assert_eq!(
        source.binding().native_turn_count(),
        CasNativeTurnCount::new(1)
    );
    assert_eq!(source.binding().execution(), &fixture.execution);
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    store.close().unwrap();
}

#[test]
fn compatible_thread_revision_descendant_preserves_exact_native_resume() {
    let home = TestHome::new("phase10-native-compatible-descendant");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    let fixture = fixtures::append_pending(
        &store,
        storage,
        id(30),
        SyndicTurnId::from_bytes([92; 16]),
        support::populated::source_turn(),
    );
    let requested = fixture.selected;
    let admission = fixtures::seed_accepted_input_admission_descendant(&store, storage, &fixture);
    let current = admission.selected;
    assert!(current.is_compatible_descendant_of(requested));
    assert_ne!(current.thread_revision(), requested.thread_revision());
    let admitted = storage
        .accepted_input(&store, admission.input, point_limit())
        .unwrap()
        .expect("admission descendant retains its immutable accepted-input receipt");
    assert_eq!(
        admitted.admission().expected_thread_revision(),
        requested.thread_revision()
    );
    let gate = storage
        .input_gate(&store, fixture.thread, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(gate.revision(), admission.gate_revision);
    assert!(matches!(
        gate.state(),
        syndic_storage::InputGateState::PendingTurn(turn) if *turn == fixture.pending
    ));
    let before_revision = storage.revision(&store).unwrap();

    let NativeProjectionPlan::Resume { basis, source } = storage
        .prepare_native_projection(
            &store,
            &NativeProjectionRequest::new(
                fixture.thread,
                requested,
                fixture.execution,
                fixture.tool_profile,
            ),
            point_limit(),
        )
        .unwrap()
    else {
        panic!("compatible path-neutral revision descendant must preserve native resume")
    };
    assert_eq!(basis.selected_path(), current);
    assert_eq!(
        basis.represented_prefix().tail(),
        Some(support::populated::source_turn())
    );
    assert_eq!(source.thread_id(), fixture.thread);
    assert_eq!(
        source.binding().native_turn_count(),
        CasNativeTurnCount::new(1)
    );
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    store.close().unwrap();
}

#[test]
fn inclusive_fork_and_cross_execution_mismatch_use_the_exact_ancestor() {
    let home = TestHome::new("phase10-native-inclusive-fork");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    let child = id(94);
    let child_draft = SyndicDraftId::from_bytes([95; 16]);
    let parent = support::populated::source_turn();
    fixtures::seed_child_at_tail(&store, storage, id(30), child, child_draft);
    let fixture = fixtures::append_pending(
        &store,
        storage,
        child,
        SyndicTurnId::from_bytes([96; 16]),
        parent,
    );
    let before_revision = storage.revision(&store).unwrap();

    let NativeProjectionPlan::Fork {
        basis,
        source,
        through_turn,
        native_turn_count,
    } = storage
        .prepare_native_projection(
            &store,
            &NativeProjectionRequest::new(
                fixture.thread,
                fixture.selected,
                fixture.execution.clone(),
                fixture.tool_profile,
            ),
            point_limit(),
        )
        .unwrap()
    else {
        panic!("cross-thread terminal parent must select inclusive native fork")
    };
    assert_eq!(basis.represented_prefix().tail(), Some(parent));
    assert_eq!(source.thread_id(), id(30));
    assert_eq!(
        source.binding().native_turn_count(),
        CasNativeTurnCount::new(1)
    );
    assert_eq!(
        through_turn,
        Some(CasTurnId::new("source-history-turn").unwrap())
    );
    assert_eq!(native_turn_count, CasNativeTurnCount::new(1));

    let NativeProjectionPlan::Unavailable {
        source: None,
        reason,
        ..
    } = storage
        .prepare_native_projection(
            &store,
            &NativeProjectionRequest::new(
                fixture.thread,
                fixture.selected,
                fixtures::alternate_execution(),
                fixture.tool_profile,
            ),
            point_limit(),
        )
        .unwrap()
    else {
        panic!("cross-execution ancestor must fail without target retirement authority")
    };
    assert_eq!(reason, NativeProjectionUnavailable::SourceExecutionMismatch);
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    store.close().unwrap();
}

#[test]
fn exact_current_projection_with_another_tool_profile_is_unavailable() {
    let home = TestHome::new("phase10-native-profile-mismatch");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = fixtures::seed_root_pending(&store, storage, 98, true);
    let different_profile = CasConversationToolProfile::v1([0x7b; 32]);
    let before_revision = storage.revision(&store).unwrap();

    let NativeProjectionPlan::Unavailable {
        basis,
        source: Some(source),
        reason,
    } = storage
        .prepare_native_projection(
            &store,
            &NativeProjectionRequest::new(
                fixture.thread,
                fixture.selected,
                fixture.execution,
                different_profile,
            ),
            point_limit(),
        )
        .unwrap()
    else {
        panic!("mismatched native tool profile must be unavailable")
    };
    assert_eq!(basis.tool_profile(), different_profile);
    assert_eq!(source.thread_id(), fixture.thread);
    assert_eq!(source.binding_revision(), fixture.binding_revision);
    assert_eq!(source.selected_path(), fixture.selected);
    assert_eq!(
        reason,
        NativeProjectionUnavailable::SourceToolProfileMismatch
    );
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    store.close().unwrap();
}

#[test]
fn canonical_native_binding_preserves_exact_identity_and_count_across_reopen() {
    let home = TestHome::new("phase10-native-binding-reopen");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    let thread_id = id(30);
    let selected = selected_path(&store, storage, thread_id);
    let binding = storage
        .current_binding(&store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    assert_eq!(binding.binding().selected_path(), selected);
    let BindingState::Valid(usable) = binding.binding().state() else {
        panic!("canonical terminal native binding must remain valid")
    };
    assert_eq!(usable.cas_thread_id().as_str(), "source-history-thread");
    assert_eq!(usable.native_turn_count(), CasNativeTurnCount::new(1));
    assert_eq!(
        usable.represented_prefix().tail(),
        Some(support::populated::source_turn())
    );
    assert_eq!(usable.represented_prefix().digest(), selected.digest());
    assert_eq!(
        usable.represented_prefix().source_thread_revision(),
        selected.thread_revision()
    );
    assert_eq!(usable.tool_profile(), support::test_tool_profile());
    let expected = binding.clone();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert_eq!(
        storage
            .current_binding(&reopened, thread_id, point_limit())
            .unwrap()
            .unwrap(),
        expected
    );
    reopened.close().unwrap();
}

#[test]
fn terminal_selected_tail_rejects_native_planning_without_mutation_after_reopen() {
    let home = TestHome::new("phase10-native-terminal-tail");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    let thread_id = id(30);
    let selected = selected_path(&store, storage, thread_id);
    let binding = storage
        .current_binding(&store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    let BindingState::Valid(usable) = binding.binding().state() else {
        panic!("canonical terminal native binding must remain valid")
    };
    let request = NativeProjectionRequest::new(
        thread_id,
        selected,
        usable.execution().clone(),
        usable.tool_profile(),
    );
    let before_revision = storage.revision(&store).unwrap();
    assert!(matches!(
        storage.prepare_native_projection(&store, &request, point_limit()),
        Err(NativeProjectionError::CurrentTailNotPendingOrdinaryUser)
    ));
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert!(matches!(
        storage.prepare_native_projection(&reopened, &request, point_limit()),
        Err(NativeProjectionError::CurrentTailNotPendingOrdinaryUser)
    ));
    reopened.close().unwrap();
}

#[test]
fn stale_selected_path_fails_closed_before_native_binding_selection() {
    let home = TestHome::new("phase10-native-stale-path");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    let thread_id = id(30);
    let selected = selected_path(&store, storage, thread_id);
    let stale = SelectedPathProof::new(
        selected.tail(),
        selected.thread_revision(),
        SyndicPathDigest::from_bytes([0x5a; 32]),
    );
    let before_revision = storage.revision(&store).unwrap();
    let before_binding = storage
        .current_binding(&store, thread_id, point_limit())
        .unwrap()
        .unwrap();
    assert!(matches!(
        storage.prepare_native_projection(
            &store,
            &NativeProjectionRequest::new(
                thread_id,
                stale,
                support::exact_cas::execution_binding(),
                support::test_tool_profile(),
            ),
            point_limit(),
        ),
        Err(NativeProjectionError::StaleSelectedPath)
    ));
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    assert_eq!(
        storage
            .current_binding(&store, thread_id, point_limit())
            .unwrap()
            .unwrap(),
        before_binding
    );
    store.close().unwrap();
}

#[test]
fn context_bearing_thread_requires_its_exact_context_projection() {
    let home = TestHome::new("phase10-native-discussion-context");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    seed_populated(&store, storage);
    let thread_id = id(36);
    let before_revision = storage.revision(&store).unwrap();
    assert!(matches!(
        storage.prepare_native_projection(
            &store,
            &NativeProjectionRequest::new(
                thread_id,
                selected_path(&store, storage, thread_id),
                support::exact_cas::execution_binding(),
                support::test_tool_profile(),
            ),
            point_limit(),
        ),
        Err(NativeProjectionError::DiscussionContextProjectionRequired)
    ));
    assert_eq!(storage.revision(&store).unwrap(), before_revision);
    store.close().unwrap();
}
