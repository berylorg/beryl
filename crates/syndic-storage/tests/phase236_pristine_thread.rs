#![cfg(feature = "test-faults")]

mod support;

use beryl_home_store::{CommandOutcome, HomeCommand, MutationContribution};
use beryl_model::{
    BindingRevision, ExecutionBinding, InputGateRevision, PathFlavor, RootId, RuntimeId,
    RuntimeMode, RuntimeNativePath, SyndicPathDigest, SyndicTurnId,
};
use syndic_storage::test_faults::{
    DraftPieceCandidateRootCollision, FixtureBatch, FixtureDelete, FixtureRecord,
    draft_piece_root_with_fixture_payload, inject_draft_piece_candidate_root_collision,
    open_branch_thread_attributes, publish_draft_edit_history_pair,
    rekey_draft_piece_root_for_collision, replace_draft_edit_history_frontier,
};
use syndic_storage::{
    BindingHeadRecord, BindingLifecycle, CreateThread, DraftEditHistoryPolicyV1,
    DraftPieceOperationIdV1, DraftPieceRootKeyV1, HistorySummaryRecord, InputGateRecord,
    InputGateState, PristineThreadAudit, PristineThreadRemovalAudit, SelectedPathProof,
    SyndicPointReadLimit, SyndicStorage, ThreadCatalogSummaryRecord, ThreadRecord,
    canonical_empty_draft_edit_history_v1, empty_selected_path_digest,
};

use support::{TestHome, commit, draft_id, id, open, timestamp};

fn history_policy() -> DraftEditHistoryPolicyV1 {
    DraftEditHistoryPolicyV1::new(65_536, 1).unwrap()
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn execution(seed: u8) -> ExecutionBinding {
    ExecutionBinding::new(
        RuntimeId::from_bytes([seed; 16]),
        RootId::from_bytes([seed.wrapping_add(1); 16]),
        RuntimeNativePath::from_admitted(
            RuntimeMode::host(),
            PathFlavor::Windows,
            format!("C:\\phase236-{seed}"),
        )
        .unwrap(),
    )
}

fn execute(store: &beryl_home_store::HomeStore, contribution: MutationContribution) {
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command.add(contribution).unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::Committed {
            later_failure: None,
            ..
        }
    ));
}

fn create(
    store: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
    seed: u8,
    binding: ExecutionBinding,
) -> CreateThread {
    let creation = CreateThread::ordinary(
        id(seed),
        draft_id(seed.wrapping_add(1)),
        binding,
        timestamp(u64::from(seed)),
        history_policy(),
    );
    execute(
        store,
        storage.create_thread(storage.revision(store).unwrap(), creation.clone()),
    );
    creation
}

fn replace_current_payload(
    store: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
    thread: beryl_model::SyndicThreadId,
    logical_utf8_bytes: u64,
    marker_count: u64,
    rekey: bool,
) {
    let current = storage
        .current_draft(store, thread, point_limit())
        .unwrap()
        .unwrap();
    let mut replacement = current.root().reference();
    if rekey {
        replacement = rekey_draft_piece_root_for_collision(
            replacement,
            DraftPieceRootKeyV1::direct_canonical_empty(
                current.draft().id(),
                DraftPieceOperationIdV1::from_bytes([0xEC; 16]),
            ),
        );
    }
    replacement =
        draft_piece_root_with_fixture_payload(replacement, logical_utf8_bytes, marker_count);
    execute(
        store,
        inject_draft_piece_candidate_root_collision(
            store,
            storage,
            replacement,
            DraftPieceCandidateRootCollision::Exact,
        ),
    );
    let history = canonical_empty_draft_edit_history_v1(replacement, history_policy());
    execute(
        store,
        replace_draft_edit_history_frontier(
            store,
            storage.clone(),
            current.draft().history().key(),
            history.clone(),
        ),
    );
    execute(
        store,
        publish_draft_edit_history_pair(
            store,
            storage.clone(),
            current.draft().clone(),
            replacement,
            history.reference(),
        ),
    );
}

#[test]
fn eligible_noncanonical_empty_current_draft_and_exact_identity_are_exposed() {
    let home = TestHome::new("phase236-pristine-type-delete-shape");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let binding = execution(20);
    let creation = create(&store, &storage, 21, binding.clone());

    replace_current_payload(&store, &storage, creation.thread_id(), 0, 0, true);

    let candidate = storage
        .inspect_pristine_thread(&store, creation.thread_id(), &binding)
        .unwrap()
        .expect("a noncanonical empty current root remains reusable");
    assert_eq!(candidate.thread_id(), creation.thread_id());
    assert_eq!(candidate.draft_id(), creation.draft_id());
    assert_eq!(candidate.created_at(), creation.created_at());
    assert_eq!(candidate.execution(), &binding);
    assert_eq!(
        creation.initial_catalog_summary().thread_id(),
        creation.thread_id()
    );

    assert!(
        storage
            .inspect_pristine_thread(&store, creation.thread_id(), &execution(40))
            .unwrap()
            .is_none()
    );
}

#[test]
fn restart_audit_distinguishes_missing_exact_conflict_and_reopened_exact() {
    let home = TestHome::new("phase236-pristine-restart-audit");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let binding = execution(30);
    let thread_id = id(31);
    assert!(matches!(
        storage
            .audit_pristine_thread(&store, thread_id, &binding)
            .unwrap(),
        PristineThreadAudit::Missing
    ));

    let creation = create(&store, &storage, 31, binding.clone());
    let audit = storage
        .audit_pristine_thread(&store, thread_id, &binding)
        .unwrap();
    let PristineThreadAudit::Exact(candidate) = audit else {
        panic!("created ordinary thread must audit exact")
    };
    assert_eq!(candidate.thread_id(), creation.thread_id());
    assert_eq!(candidate.draft_id(), creation.draft_id());
    assert_eq!(candidate.execution(), &binding);
    assert!(matches!(
        storage
            .audit_pristine_thread(&store, thread_id, &execution(32))
            .unwrap(),
        PristineThreadAudit::Conflict
    ));

    drop(storage);
    store.close().unwrap();
    let mut reopened = open(home.path());
    let reopened_storage = SyndicStorage::register(&mut reopened).unwrap();
    let reopened_audit = reopened_storage
        .audit_pristine_thread(&reopened, thread_id, &binding)
        .unwrap();
    let PristineThreadAudit::Exact(reopened_candidate) = reopened_audit else {
        panic!("reopened ordinary thread must audit exact")
    };
    assert_eq!(reopened_candidate.thread_id(), creation.thread_id());
    assert_eq!(reopened_candidate.draft_id(), creation.draft_id());
}

#[test]
fn restart_audit_reports_present_partial_closure_as_conflict() {
    let home = TestHome::new("phase236-pristine-partial-audit");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let binding = execution(35);
    let creation = create(&store, &storage, 36, binding.clone());
    let mut batch = FixtureBatch::new();
    batch
        .delete(FixtureDelete::DraftByThread(creation.thread_id()))
        .unwrap();
    commit(&store, storage.clone(), batch);

    assert!(matches!(
        storage
            .audit_pristine_thread(&store, creation.thread_id(), &binding)
            .unwrap(),
        PristineThreadAudit::Conflict
    ));
    assert!(
        storage
            .inspect_pristine_thread(&store, creation.thread_id(), &binding)
            .is_err()
    );
}

#[test]
fn submitted_dirty_marker_and_nonordinary_threads_are_ineligible() {
    for (name, logical_utf8_bytes, marker_count) in
        [("dirty", 1_u64, 0_u64), ("marker", 0_u64, 1_u64)]
    {
        let home = TestHome::new(&format!("phase236-pristine-{name}"));
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let binding = execution(logical_utf8_bytes as u8 + marker_count as u8 + 50);
        let creation = create(&store, &storage, 52, binding.clone());
        replace_current_payload(
            &store,
            &storage,
            creation.thread_id(),
            logical_utf8_bytes,
            marker_count,
            false,
        );
        assert!(
            storage
                .inspect_pristine_thread(&store, creation.thread_id(), &binding)
                .unwrap()
                .is_none()
        );
    }

    let home = TestHome::new("phase236-pristine-submitted");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let binding = execution(60);
    let creation = create(&store, &storage, 61, binding.clone());
    let thread = storage
        .thread(&store, creation.thread_id(), point_limit())
        .unwrap()
        .unwrap();
    let execution_record = storage
        .thread_execution(&store, creation.thread_id(), point_limit())
        .unwrap()
        .unwrap();
    let attributes = storage
        .thread_attributes(&store, creation.thread_id(), point_limit())
        .unwrap()
        .unwrap();
    let tail = SyndicTurnId::from_bytes([0x3A; 16]);
    let digest = SyndicPathDigest::from_bytes([0x3B; 32]);
    let submitted = ThreadRecord::new(
        thread.id(),
        SelectedPathProof::new(Some(tail), thread.revision(), digest),
        thread.current_draft_id(),
        thread.lineage().clone(),
        thread.context_owner_id(),
    );
    let summary = HistorySummaryRecord::new(
        thread.id(),
        storage
            .thread_catalog_summary(&store, thread.id(), point_limit())
            .unwrap()
            .unwrap()
            .revision(),
        submitted.revision(),
        Some(tail),
        digest,
        true,
        timestamp(100),
    );
    let catalog =
        ThreadCatalogSummaryRecord::initial(&submitted, &execution_record, &attributes, &summary);
    let mut batch = FixtureBatch::new();
    batch.put(FixtureRecord::Thread(submitted)).unwrap();
    batch.put(FixtureRecord::HistorySummary(summary)).unwrap();
    batch
        .put(FixtureRecord::ThreadCatalogSummary(catalog))
        .unwrap();
    commit(&store, storage.clone(), batch);
    assert!(
        storage
            .inspect_pristine_thread(&store, creation.thread_id(), &binding)
            .is_err()
    );

    let home = TestHome::new("phase236-pristine-nonordinary");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let binding = execution(70);
    let creation = create(&store, &storage, 71, binding.clone());
    let thread = storage
        .thread(&store, creation.thread_id(), point_limit())
        .unwrap()
        .unwrap();
    let execution_record = storage
        .thread_execution(&store, thread.id(), point_limit())
        .unwrap()
        .unwrap();
    let summary = storage
        .history_summary(&store, thread.id(), point_limit())
        .unwrap()
        .unwrap();
    let attributes = open_branch_thread_attributes(thread.id());
    let catalog =
        ThreadCatalogSummaryRecord::initial(&thread, &execution_record, &attributes, &summary);
    let mut batch = FixtureBatch::new();
    batch
        .put(FixtureRecord::ThreadAttributes(attributes))
        .unwrap();
    batch
        .put(FixtureRecord::ThreadCatalogSummary(catalog))
        .unwrap();
    commit(&store, storage.clone(), batch);
    assert!(
        storage
            .inspect_pristine_thread(&store, creation.thread_id(), &binding)
            .unwrap()
            .is_none()
    );
}

#[test]
fn non_idle_and_live_input_gate_facts_are_ineligible() {
    for (name, gate) in [
        (
            "non-idle",
            InputGateRecord::new(
                id(81),
                InputGateRevision::new(2).unwrap(),
                InputGateState::PendingTurn(SyndicTurnId::from_bytes([0x51; 16])),
                0,
                None,
                None,
                0,
                0,
                0,
            )
            .unwrap(),
        ),
        (
            "live-counts",
            InputGateRecord::new(
                id(81),
                InputGateRevision::new(2).unwrap(),
                InputGateState::Idle,
                1,
                None,
                None,
                1,
                1,
                2,
            )
            .unwrap(),
        ),
    ] {
        let home = TestHome::new(&format!("phase236-pristine-{name}"));
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let binding = execution(80);
        let creation = create(&store, &storage, 81, binding.clone());
        let mut batch = FixtureBatch::new();
        batch.put(FixtureRecord::InputGate(gate)).unwrap();
        commit(&store, storage.clone(), batch);
        assert!(
            storage
                .inspect_pristine_thread(&store, creation.thread_id(), &binding)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn writer_validation_rejects_a_stale_candidate() {
    let home = TestHome::new("phase236-pristine-stale");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let binding = execution(90);
    let creation = create(&store, &storage, 91, binding.clone());
    let candidate = storage
        .inspect_pristine_thread(&store, creation.thread_id(), &binding)
        .unwrap()
        .unwrap();

    create(&store, &storage, 93, execution(92));

    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add_validation(storage.validate_pristine_thread(candidate))
        .unwrap();
    assert!(matches!(
        store.execute(command),
        CommandOutcome::NotCommitted { .. }
    ));
}

#[test]
fn created_pristine_thread_deletes_the_exact_complete_closure_and_audits_removed() {
    let home = TestHome::new("phase237-pristine-created-delete");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let binding = execution(100);
    let creation = create(&store, &storage, 101, binding.clone());
    let candidate = storage
        .inspect_pristine_thread(&store, creation.thread_id(), &binding)
        .unwrap()
        .expect("created thread is pristine");

    assert_eq!(
        storage
            .audit_pristine_thread_removal(&store, &candidate)
            .unwrap(),
        PristineThreadRemovalAudit::Present
    );
    execute(&store, storage.delete_pristine_thread(candidate.clone()));
    assert_eq!(
        storage
            .audit_pristine_thread_removal(&store, &candidate)
            .unwrap(),
        PristineThreadRemovalAudit::Removed
    );
    let mut replay = HomeCommand::new(store.home_revision().unwrap());
    replay
        .add(storage.delete_pristine_thread(candidate.clone()))
        .unwrap();
    assert!(matches!(
        store.execute(replay),
        CommandOutcome::NotCommitted { .. }
    ));
    assert!(matches!(
        storage
            .audit_pristine_thread(&store, creation.thread_id(), &binding)
            .unwrap(),
        PristineThreadAudit::Missing
    ));
    assert!(
        storage
            .current_draft(&store, creation.thread_id(), point_limit())
            .unwrap()
            .is_none()
    );
}

#[test]
fn noncanonical_reused_candidate_is_validation_only_and_cannot_be_deleted() {
    let home = TestHome::new("phase237-pristine-reused-release");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let binding = execution(110);
    let creation = create(&store, &storage, 111, binding.clone());
    replace_current_payload(&store, &storage, creation.thread_id(), 0, 0, true);
    let candidate = storage
        .inspect_pristine_thread(&store, creation.thread_id(), &binding)
        .unwrap()
        .expect("noncanonical empty thread remains reusable");

    let mut delete = HomeCommand::new(store.home_revision().unwrap());
    delete
        .add(storage.delete_pristine_thread(candidate.clone()))
        .unwrap();
    assert!(matches!(
        store.execute(delete),
        CommandOutcome::NotCommitted { .. }
    ));
    assert_eq!(
        storage
            .audit_pristine_thread_removal(&store, &candidate)
            .unwrap(),
        PristineThreadRemovalAudit::Collision
    );
    assert!(
        storage
            .thread(&store, creation.thread_id(), point_limit())
            .unwrap()
            .is_some()
    );
}

#[test]
fn stale_or_partial_created_candidate_never_deletes_and_audits_collision() {
    let home = TestHome::new("phase237-pristine-stale-partial");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let binding = execution(120);
    let creation = create(&store, &storage, 121, binding.clone());
    let candidate = storage
        .inspect_pristine_thread(&store, creation.thread_id(), &binding)
        .unwrap()
        .expect("created thread is pristine");

    create(&store, &storage, 122, execution(123));
    let mut stale = HomeCommand::new(store.home_revision().unwrap());
    stale
        .add(storage.delete_pristine_thread(candidate.clone()))
        .unwrap();
    assert!(matches!(
        store.execute(stale),
        CommandOutcome::NotCommitted { .. }
    ));

    let mut partial = FixtureBatch::new();
    partial
        .delete(FixtureDelete::ThreadAttributes(creation.thread_id()))
        .unwrap();
    commit(&store, storage.clone(), partial);
    assert_eq!(
        storage
            .audit_pristine_thread_removal(&store, &candidate)
            .unwrap(),
        PristineThreadRemovalAudit::Collision
    );
}

#[test]
fn advanced_or_active_binding_head_rejects_pristine_authority() {
    for (name, revision, lifecycle) in [
        (
            "advanced",
            BindingRevision::new(2).unwrap(),
            BindingLifecycle::Unbound,
        ),
        (
            "active",
            BindingRevision::new(1).unwrap(),
            BindingLifecycle::Active,
        ),
    ] {
        let home = TestHome::new(&format!("phase238-pristine-binding-{name}"));
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let binding = execution(130);
        let creation = create(&store, &storage, 131, binding.clone());
        let candidate = storage
            .inspect_pristine_thread(&store, creation.thread_id(), &binding)
            .unwrap()
            .expect("created thread is initially pristine");
        let mut batch = FixtureBatch::new();
        batch
            .put(FixtureRecord::BindingHead(BindingHeadRecord::new(
                creation.thread_id(),
                revision,
                lifecycle,
                empty_selected_path_digest(),
            )))
            .unwrap();
        commit(&store, storage.clone(), batch);

        assert!(
            storage
                .inspect_pristine_thread(&store, creation.thread_id(), &binding)
                .is_err()
        );
        assert!(matches!(
            storage
                .audit_pristine_thread(&store, creation.thread_id(), &binding)
                .unwrap(),
            PristineThreadAudit::Conflict
        ));
        assert_eq!(
            storage
                .audit_pristine_thread_removal(&store, &candidate)
                .unwrap(),
            PristineThreadRemovalAudit::Collision
        );
        let mut delete = HomeCommand::new(store.home_revision().unwrap());
        delete
            .add(storage.delete_pristine_thread(candidate))
            .unwrap();
        assert!(matches!(
            store.execute(delete),
            CommandOutcome::NotCommitted { .. }
        ));
    }
}
