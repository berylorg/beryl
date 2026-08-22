#![cfg(feature = "test-faults")]

mod support;

use std::num::NonZeroUsize;

use beryl_model::{
    BindingRevision, ImageLabelOrdinal, InputGateRevision, ProjectionRevision,
    RecoveryItemSequenceDigest, RecoveryItemSequenceRole, SyndicDraftId, SyndicDraftMarkerId,
    SyndicItemId, SyndicPathDigest, SyndicThreadId, SyndicTurnId, ThreadRevision,
};
use beryl_stream::PagePool;
use syndic_storage::test_faults::{FixtureDelete, FixtureRecord};
use syndic_storage::*;

use support::{
    batch, commit, composer_content_records, draft_id, fixture_turn_state, id, open,
    seed_canonical_empty_thread, timestamp, TestHome,
};

struct RecoveryFixture {
    thread: SyndicThreadId,
    selected: SelectedPathProof,
    represented_tail: SyndicTurnId,
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}

fn recovery_page_pool(page_capacity: usize) -> PagePool {
    PagePool::new(
        NonZeroUsize::new(page_capacity).unwrap(),
        NonZeroUsize::new(1).unwrap(),
    )
    .unwrap()
}

fn turn_id(thread_byte: u8, index: usize) -> SyndicTurnId {
    SyndicTurnId::from_bytes([thread_byte.wrapping_add(10 + u8::try_from(index).unwrap()); 16])
}

fn same_home_path_records(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    draft: SyndicDraftId,
    tail: SyndicTurnId,
    digest: SyndicPathDigest,
    history_complete: bool,
    last_activity_at: SyndicTimestamp,
) -> Vec<FixtureRecord> {
    seed_canonical_empty_thread(store, storage, thread, draft);
    let thread_revision = ThreadRevision::new(1).unwrap();
    let projection_revision = ProjectionRevision::new(1).unwrap();
    let binding_revision = BindingRevision::new(1).unwrap();
    let selected = SelectedPathProof::new(Some(tail), thread_revision, digest);
    let thread_record = ThreadRecord::new(
        thread,
        selected,
        draft,
        ThreadLineageProof::new(
            None,
            None,
            ThreadLineageDepth::FIRST,
            root_thread_lineage_digest(thread),
        ),
        ThreadImageLabelFrontiers::empty(),
        None,
    );
    let execution = ThreadExecutionRecord::new(
        thread,
        storage
            .thread_execution(store, thread, point_limit())
            .unwrap()
            .unwrap()
            .execution()
            .clone(),
    );
    let attributes = ThreadAttributesRecord::ordinary(thread);
    let history = HistorySummaryRecord::new(
        thread,
        projection_revision,
        thread_revision,
        Some(tail),
        digest,
        history_complete,
        last_activity_at,
    );
    let catalog =
        ThreadCatalogSummaryRecord::initial(&thread_record, &execution, &attributes, &history);
    vec![
        FixtureRecord::Thread(thread_record),
        FixtureRecord::ThreadCatalogSummary(catalog),
        FixtureRecord::TranscriptViewHead(TranscriptViewHeadRecord::new(
            thread,
            TranscriptGeneration::FIRST,
            projection_revision,
            0,
            Some(tail),
            digest,
            ProjectionLifecycle::Current,
        )),
        FixtureRecord::HistorySummary(history),
        FixtureRecord::Binding(BindingRecord::new(
            thread,
            binding_revision,
            selected,
            BindingState::unbound("fixture").unwrap(),
        )),
        FixtureRecord::BindingHead(BindingHeadRecord::new(
            thread,
            binding_revision,
            BindingLifecycle::Unbound,
            digest,
        )),
    ]
}

fn seed_recovery_fixture(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread_byte: u8,
    entries: &[(&str, TurnLifecycle)],
    pending_successor: bool,
) -> RecoveryFixture {
    assert!(!entries.is_empty());
    let thread = id(thread_byte);
    let draft = draft_id(thread_byte.wrapping_add(1));
    let mut records = Vec::new();
    let mut parent = None;
    let mut parent_digest = None;

    for (index, (text, lifecycle)) in entries.iter().enumerate() {
        let turn = turn_id(thread_byte, index);
        let item = SyndicItemId::from_bytes(
            [thread_byte.wrapping_add(40 + u8::try_from(index).unwrap()); 16],
        );
        let depth = TurnDepth::new(u64::try_from(index).unwrap() + 1).unwrap();
        let digest = match (parent, parent_digest) {
            (None, None) => root_turn_chain_digest(turn),
            (Some(parent), Some(parent_digest)) => {
                child_turn_chain_digest(turn, parent, parent_digest)
            }
            _ => unreachable!(),
        };
        let skip_depth = if depth.get() == 1 {
            None
        } else {
            Some((depth.get() & (depth.get() - 1)).max(1))
        };
        records.push(FixtureRecord::Turn(TurnRecord::new(
            turn,
            thread,
            TurnKind::OrdinaryUser,
            parent.map_or(ConversationParent::Root, ConversationParent::Turn),
            skip_depth.map(|depth| turn_id(thread_byte, usize::try_from(depth - 1).unwrap())),
            depth,
            digest,
            timestamp(u64::try_from(index).unwrap() + 2),
        )));
        records.push(FixtureRecord::TurnState(fixture_turn_state(
            turn,
            TurnStateRevision::FIRST,
            *lifecycle,
            1,
            1,
            timestamp(u64::try_from(index).unwrap() + 2),
        )));
        let outcome = match lifecycle {
            TurnLifecycle::Complete => TurnTerminalOutcome::Complete,
            TurnLifecycle::Interrupted => TurnTerminalOutcome::Interrupted,
            TurnLifecycle::Failed => TurnTerminalOutcome::Failed,
            _ => panic!("recovery fixture entry must be terminal"),
        };
        records.push(FixtureRecord::SourceEvent(
            SourceEventRecord::new(
                turn,
                SourceEventSequence::FIRST,
                None,
                SourceEventPayload::TurnEnded(TurnEndStatus::new(outcome, None).unwrap()),
            )
            .unwrap(),
        ));
        if let Some(parent) = parent {
            records.push(FixtureRecord::TurnChild(TurnChildIndexRecord::new(
                parent, turn, depth, digest,
            )));
        }
        let payload = ComposerPayload::new(vec![ComposerAtom::text(*text).unwrap()]).unwrap();
        let (content, content_records) = composer_content_records(&payload);
        records.extend(content_records);
        let revision = ProjectionRevision::new(u64::try_from(index).unwrap() + 1).unwrap();
        records.push(FixtureRecord::CanonicalItem(
            CanonicalItemRecord::local_user_input(
                item,
                turn,
                TurnItemOrdinal::FIRST,
                revision,
                content,
                None,
            ),
        ));
        records.push(FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn,
            TurnItemOrdinal::FIRST,
            item,
            revision,
        )));
        parent = Some(turn);
        parent_digest = Some(digest);
    }

    let represented_tail = parent.unwrap();
    let represented_digest = parent_digest.unwrap();
    let (selected_tail, selected_digest) = if pending_successor {
        let pending_index = entries.len();
        let pending = turn_id(thread_byte, pending_index);
        let depth = TurnDepth::new(u64::try_from(pending_index).unwrap() + 1).unwrap();
        let digest = child_turn_chain_digest(pending, represented_tail, represented_digest);
        let skip_depth = (depth.get() & (depth.get() - 1)).max(1);
        records.extend([
            FixtureRecord::Turn(TurnRecord::new(
                pending,
                thread,
                TurnKind::OrdinaryUser,
                ConversationParent::Turn(represented_tail),
                Some(turn_id(
                    thread_byte,
                    usize::try_from(skip_depth - 1).unwrap(),
                )),
                depth,
                digest,
                timestamp(u64::try_from(pending_index).unwrap() + 2),
            )),
            FixtureRecord::TurnState(fixture_turn_state(
                pending,
                TurnStateRevision::FIRST,
                TurnLifecycle::Pending,
                0,
                0,
                timestamp(u64::try_from(pending_index).unwrap() + 2),
            )),
            FixtureRecord::TurnChild(TurnChildIndexRecord::new(
                represented_tail,
                pending,
                depth,
                digest,
            )),
            FixtureRecord::InputGate(
                InputGateRecord::new(
                    thread,
                    InputGateRevision::new(1).unwrap(),
                    InputGateState::PendingTurn(pending),
                    0,
                    None,
                    None,
                    0,
                    0,
                    0,
                )
                .unwrap(),
            ),
        ]);
        (pending, digest)
    } else {
        (represented_tail, represented_digest)
    };
    records.extend(same_home_path_records(
        store,
        storage,
        thread,
        draft,
        selected_tail,
        selected_digest,
        !pending_successor,
        timestamp(u64::try_from(entries.len()).unwrap() + 2),
    ));
    commit(store, storage, batch(records));
    RecoveryFixture {
        thread,
        selected: SelectedPathProof::new(
            Some(selected_tail),
            ThreadRevision::new(1).unwrap(),
            selected_digest,
        ),
        represented_tail,
    }
}

fn seed_user_assistant_fixture(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    user_text: &str,
) -> RecoveryFixture {
    support::seed_populated(store, storage);
    let thread = id(30);
    let root = SyndicTurnId::from_bytes([29; 16]);
    let item = SyndicItemId::from_bytes([210; 16]);
    let payload = ComposerPayload::new(vec![ComposerAtom::text(user_text).unwrap()]).unwrap();
    let (content, content_records) = composer_content_records(&payload);
    let revision = ProjectionRevision::new(1).unwrap();
    let mut records = content_records;
    records.extend([
        FixtureRecord::TurnState(fixture_turn_state(
            root,
            TurnStateRevision::FIRST,
            TurnLifecycle::Complete,
            0,
            1,
            timestamp(2),
        )),
        FixtureRecord::CanonicalItem(CanonicalItemRecord::local_user_input(
            item,
            root,
            TurnItemOrdinal::FIRST,
            revision,
            content,
            None,
        )),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            root,
            TurnItemOrdinal::FIRST,
            item,
            revision,
        )),
    ]);
    commit(store, storage, batch(records));
    let selected = selected_path(store, storage, thread);
    RecoveryFixture {
        thread,
        selected,
        represented_tail: support::populated::source_turn(),
    }
}

fn prepare_ready(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    fixture: &RecoveryFixture,
    model_tokens: Option<u64>,
) -> RecoveryProjection {
    let request = if fixture.selected.tail() == Some(fixture.represented_tail) {
        RecoveryProjectionRequest::for_current_selected_path(
            fixture.thread,
            fixture.selected,
            model_tokens,
        )
    } else {
        RecoveryProjectionRequest::for_pending_selected_turn_parent(
            fixture.thread,
            fixture.selected,
            model_tokens,
        )
    };
    let RecoveryAssembly::Ready(projection) =
        storage.prepare_recovery_projection(store, request).unwrap()
    else {
        panic!("nonempty canonical recovery history must be ready")
    };
    projection
}

fn replay(
    storage: SyndicStorage,
    store: &beryl_home_store::HomeStore,
    projection: RecoveryProjection,
) -> Vec<(RecoveryItemSequenceRole, String)> {
    let mut cursor = storage.open_recovery_cursor(store, projection).unwrap();
    let pool = recovery_page_pool(RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES);
    let mut lease = pool.try_lease().unwrap();
    let mut items: Vec<(RecoveryItemSequenceRole, String)> = Vec::new();
    let mut saw_terminal = false;
    loop {
        let Some(page) = storage
            .read_recovery_cursor_page(
                store,
                &mut cursor,
                lease,
                RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES,
            )
            .unwrap()
        else {
            break;
        };
        if page.item_offset() == 0 {
            assert_eq!(page.sequence_ordinal(), items.len() as u64 + 1);
            items.push((page.role(), String::new()));
        }
        let text = &mut items.last_mut().unwrap().1;
        assert_eq!(page.item_offset(), text.len() as u64);
        text.push_str(page.text());
        if page.item_terminal() {
            assert_eq!(text.len() as u64, page.declared_item_utf8_bytes());
        }
        saw_terminal = page.sequence_terminal();
        lease = page.into_page_lease();
    }
    assert!(saw_terminal);
    assert!(matches!(
        storage.read_recovery_cursor_page(
            store,
            &mut cursor,
            pool.try_lease().unwrap(),
            RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES,
        ),
        Err(RecoveryProjectionError::CursorTerminal)
    ));
    assert_eq!(pool.diagnostics().available, 1);
    items
}

#[test]
fn exact_root_to_tail_items_exclude_pending_input_and_reopen_deterministically() {
    let home = TestHome::new("phase9-recovery-exact-order");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let entries = [
        ("complete root", TurnLifecycle::Complete),
        ("interrupted middle", TurnLifecycle::Interrupted),
        ("failed tail", TurnLifecycle::Failed),
    ];
    let fixture = seed_recovery_fixture(&store, storage, 100, &entries, true);
    let projection = prepare_ready(&store, storage, &fixture, Some(100_000));
    assert_eq!(projection.thread_id(), fixture.thread);
    assert_eq!(projection.selected_path(), fixture.selected);
    assert_eq!(
        projection.represented_prefix().tail(),
        Some(fixture.represented_tail)
    );
    assert_ne!(
        projection.represented_prefix().tail(),
        projection.selected_path().tail()
    );
    let expected = entries
        .iter()
        .map(|(text, _)| (RecoveryItemSequenceRole::UserInputText, (*text).to_owned()))
        .collect::<Vec<_>>();
    test_faults::reset_recovery_residency_metrics();
    assert_eq!(replay(storage, &store, projection), expected);
    let metrics = test_faults::recovery_residency_metrics();
    assert_eq!(metrics.max_resident_turns(), 1);
    assert_eq!(metrics.max_resident_items(), 1);
    assert!(metrics.max_cursor_page_bytes() <= RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES);
    let digest = projection.sequence_digest();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let reopened_projection = prepare_ready(&reopened, storage, &fixture, Some(100_000));
    assert_eq!(reopened_projection.sequence_digest(), digest);
    assert_eq!(replay(storage, &reopened, reopened_projection), expected);
    reopened.close().unwrap();
}

#[test]
fn caller_page_limit_preserves_utf8_progress_and_returns_the_exact_lease() {
    let home = TestHome::new("phase9-recovery-caller-page");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = seed_recovery_fixture(
        &store,
        storage,
        110,
        &[("\u{e9}\u{1f642}x", TurnLifecycle::Complete)],
        false,
    );
    let projection = prepare_ready(&store, storage, &fixture, Some(100_000));
    let mut cursor = storage.open_recovery_cursor(&store, projection).unwrap();
    let pool = recovery_page_pool(16);

    test_faults::reset_recovery_residency_metrics();
    assert!(matches!(
        storage.read_recovery_cursor_page(&store, &mut cursor, pool.try_lease().unwrap(), 0),
        Err(RecoveryProjectionError::InvalidCursorPageLimit { actual: 0 })
    ));
    assert_eq!(
        test_faults::recovery_residency_metrics().turn_item_read_attempts(),
        0
    );
    assert_eq!(pool.diagnostics().available, 1);
    assert!(matches!(
        storage.read_recovery_cursor_page(&store, &mut cursor, pool.try_lease().unwrap(), 1),
        Err(RecoveryProjectionError::CursorPageLimitTooSmall {
            offset: 0,
            actual: 1,
        })
    ));

    let lease = pool.try_lease().unwrap();
    let generation = lease.generation();
    let first = storage
        .read_recovery_cursor_page(&store, &mut cursor, lease, 3)
        .unwrap()
        .unwrap();
    assert_eq!(first.text(), "\u{e9}");
    assert!(!first.item_terminal());
    let lease = first.into_page_lease();
    assert_eq!(lease.generation(), generation);
    let second = storage
        .read_recovery_cursor_page(&store, &mut cursor, lease, 4)
        .unwrap()
        .unwrap();
    assert_eq!(second.text(), "\u{1f642}");
    assert_eq!(second.item_offset(), "\u{e9}".len() as u64);
    let terminal = storage
        .read_recovery_cursor_page(&store, &mut cursor, second.into_page_lease(), usize::MAX)
        .unwrap()
        .unwrap();
    assert_eq!(terminal.text(), "x");
    assert!(terminal.item_terminal());
    assert!(terminal.sequence_terminal());
    assert!(storage
        .read_recovery_cursor_page(&store, &mut cursor, terminal.into_page_lease(), 16,)
        .unwrap()
        .is_none());
    assert_eq!(pool.diagnostics().available, 1);
    store.close().unwrap();
}

#[test]
fn recovery_sequence_digest_matches_the_fixed_v1_vector() {
    let home = TestHome::new("phase9-recovery-digest-vector");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = seed_user_assistant_fixture(&store, storage, "u");
    let projection = prepare_ready(&store, storage, &fixture, Some(100_000));
    assert_eq!(
        replay(storage, &store, projection),
        vec![
            (RecoveryItemSequenceRole::UserInputText, "u".to_owned()),
            (
                RecoveryItemSequenceRole::AssistantOutputText,
                "assistant".to_owned(),
            ),
        ]
    );
    assert_eq!(
        projection.sequence_digest(),
        RecoveryItemSequenceDigest::from_bytes([
            0xf5, 0xe6, 0xaa, 0xe4, 0xea, 0x77, 0x06, 0x77, 0x9d, 0x86, 0xd5, 0x8e, 0x76, 0xad,
            0xe7, 0xea, 0xb4, 0xfe, 0xa2, 0x53, 0x09, 0xd0, 0x64, 0x54, 0xfe, 0x7d, 0x19, 0x0f,
            0x78, 0x00, 0xfb, 0x88,
        ])
    );
    store.close().unwrap();
}

#[test]
fn recovery_crosses_content_chunks_and_emits_more_pages_than_items() {
    let home = TestHome::new("phase9-recovery-cross-chunk-pages");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let text = "z".repeat(CONTENT_CHUNK_MAX_BYTES + 37);
    let fixture = seed_recovery_fixture(
        &store,
        storage,
        120,
        &[(text.as_str(), TurnLifecycle::Interrupted)],
        false,
    );
    let projection = prepare_ready(&store, storage, &fixture, Some(u64::MAX));
    assert_eq!(projection.item_count().get(), 1);
    assert_eq!(projection.utf8_bytes().get(), text.len() as u64);
    let mut cursor = storage.open_recovery_cursor(&store, projection).unwrap();
    let pool = recovery_page_pool(4_096);
    let mut lease = pool.try_lease().unwrap();
    let mut recovered = String::new();
    let mut page_count = 0_u64;
    loop {
        let Some(page) = storage
            .read_recovery_cursor_page(&store, &mut cursor, lease, 4_096)
            .unwrap()
        else {
            break;
        };
        page_count += 1;
        recovered.push_str(page.text());
        lease = page.into_page_lease();
    }
    assert_eq!(recovered, text);
    assert!(page_count > u64::from(projection.item_count().get()));
    assert_eq!(pool.diagnostics().available, 1);
    store.close().unwrap();
}

#[test]
fn absolute_utf8_ceiling_accepts_exactly_and_rejects_plus_one() {
    for (name, thread_byte, length, accepted) in [
        (
            "phase9-recovery-absolute-exact",
            130,
            RecoveryUtf8ByteCount::MAX as usize,
            true,
        ),
        (
            "phase9-recovery-absolute-plus-one",
            140,
            RecoveryUtf8ByteCount::MAX as usize + 1,
            false,
        ),
    ] {
        let home = TestHome::new(name);
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let text = "a".repeat(length);
        let fixture = seed_recovery_fixture(
            &store,
            storage,
            thread_byte,
            &[(text.as_str(), TurnLifecycle::Complete)],
            false,
        );
        let request = RecoveryProjectionRequest::for_current_selected_path(
            fixture.thread,
            fixture.selected,
            Some(u64::MAX),
        );
        if accepted {
            let RecoveryAssembly::Ready(projection) = storage
                .prepare_recovery_projection(&store, request)
                .unwrap()
            else {
                panic!("the exact absolute byte limit must be ready")
            };
            assert_eq!(projection.utf8_bytes().get(), RecoveryUtf8ByteCount::MAX);
        } else {
            assert!(matches!(
                storage.prepare_recovery_projection(&store, request),
                Err(RecoveryProjectionError::BudgetOverflow {
                    kind: RecoveryBudgetKind::Utf8Bytes,
                    maximum: RecoveryUtf8ByteCount::MAX,
                    actual,
                }) if actual == RecoveryUtf8ByteCount::MAX + 1
            ));
        }
        store.close().unwrap();
    }
}

#[test]
fn half_window_budget_and_missing_or_zero_metadata_are_exact() {
    let home = TestHome::new("phase9-recovery-model-window");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = seed_recovery_fixture(
        &store,
        storage,
        150,
        &[("elevenbytes", TurnLifecycle::Complete)],
        false,
    );
    let request = |tokens| {
        RecoveryProjectionRequest::for_current_selected_path(
            fixture.thread,
            fixture.selected,
            tokens,
        )
    };
    let RecoveryAssembly::Ready(projection) = storage
        .prepare_recovery_projection(&store, request(Some(22)))
        .unwrap()
    else {
        panic!("the exact half-window budget must be ready")
    };
    assert_eq!(projection.utf8_bytes().get(), 11);
    assert!(matches!(
        storage.prepare_recovery_projection(&store, request(Some(21))),
        Err(RecoveryProjectionError::BudgetOverflow {
            kind: RecoveryBudgetKind::Utf8Bytes,
            maximum: 10,
            actual: 11,
        })
    ));
    assert!(matches!(
        storage.prepare_recovery_projection(&store, request(None)),
        Err(RecoveryProjectionError::MissingModelContextWindow)
    ));
    assert!(matches!(
        storage.prepare_recovery_projection(&store, request(Some(0))),
        Err(RecoveryProjectionError::ZeroModelContextWindow)
    ));
    store.close().unwrap();
}

#[test]
fn canonical_itemless_terminal_history_fails_closed_and_reopens_deterministically() {
    let home = TestHome::new("phase9-recovery-incomplete-root");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    support::seed_populated(&store, storage);
    let thread = id(30);
    let selected = selected_path(&store, storage, thread);
    assert!(matches!(
        storage.prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_current_selected_path(thread, selected, Some(100_000)),
        ),
        Err(RecoveryProjectionError::IncompleteHistory { reason })
            if reason == "included turn has no canonical items"
    ));
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    assert!(matches!(
        storage.prepare_recovery_projection(
            &reopened,
            RecoveryProjectionRequest::for_current_selected_path(thread, selected, Some(100_000)),
        ),
        Err(RecoveryProjectionError::IncompleteHistory { reason })
            if reason == "included turn has no canonical items"
    ));
    reopened.close().unwrap();
}

#[test]
fn media_operational_empty_and_incomplete_history_reject_distinctly() {
    let error_for_payload = |name: &str, thread_byte: u8, payload: ComposerPayload| {
        let home = TestHome::new(name);
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let fixture = seed_recovery_fixture(
            &store,
            storage,
            thread_byte,
            &[("placeholder", TurnLifecycle::Complete)],
            false,
        );
        let item = SyndicItemId::from_bytes([thread_byte.wrapping_add(40); 16]);
        let (content, content_records) = composer_content_records(&payload);
        let mut records = content_records;
        records.push(FixtureRecord::CanonicalItem(
            CanonicalItemRecord::local_user_input(
                item,
                fixture.represented_tail,
                TurnItemOrdinal::FIRST,
                ProjectionRevision::new(1).unwrap(),
                content,
                None,
            ),
        ));
        commit(&store, storage, batch(records));
        let error = storage
            .prepare_recovery_projection(
                &store,
                RecoveryProjectionRequest::for_current_selected_path(
                    fixture.thread,
                    fixture.selected,
                    Some(100_000),
                ),
            )
            .unwrap_err();
        store.close().unwrap();
        error
    };

    let media = ComposerPayload::new(vec![ComposerAtom::image_marker(
        SyndicDraftMarkerId::from_bytes([211; 16]),
        ImageLabelOrdinal::FIRST,
    )])
    .unwrap();
    assert!(matches!(
        error_for_payload("phase9-recovery-media", 170, media),
        RecoveryProjectionError::MediaHistory {
            reason: "user input contains an image marker"
        }
    ));
    assert!(matches!(
        error_for_payload(
            "phase9-recovery-empty-item",
            180,
            ComposerPayload::default()
        ),
        RecoveryProjectionError::EmptyHistoryItem
    ));

    let home = TestHome::new("phase9-recovery-incomplete-finalization");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = seed_recovery_fixture(
        &store,
        storage,
        190,
        &[("incomplete", TurnLifecycle::Complete)],
        false,
    );
    commit(
        &store,
        storage,
        batch([FixtureRecord::TurnState(
            support::fixture_turn_state_with_finalization(
                fixture.represented_tail,
                TurnStateRevision::FIRST,
                TurnLifecycle::Complete,
                0,
                1,
                0,
                timestamp(3),
            ),
        )]),
    );
    assert!(matches!(
        storage.prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_current_selected_path(
                fixture.thread,
                fixture.selected,
                Some(100_000),
            ),
        ),
        Err(RecoveryProjectionError::IncompleteHistory { reason })
            if reason.starts_with("turn is not recovery-eligible")
    ));
    store.close().unwrap();

    let home = TestHome::new("phase9-recovery-operational");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    support::seed_populated(&store, storage);
    let thread = id(40);
    let turn = support::populated::active_turn();
    let item = support::populated::activity_item();
    let original = storage
        .canonical_item(&store, item, point_limit())
        .unwrap()
        .unwrap();
    let state = storage
        .turn_state(&store, turn, point_limit())
        .unwrap()
        .unwrap();
    let operational = CanonicalItemRecord::with_provider_state(
        item,
        turn,
        TurnItemOrdinal::FIRST,
        original.revision(),
        original.source_event().unwrap(),
        original.source_event_count(),
        original.cas_source().unwrap().clone(),
        None,
        original.provider().unwrap().clone(),
        None,
        CanonicalItemPresentation::Operational,
    )
    .unwrap();
    let mut replacement = batch([
        FixtureRecord::CanonicalItem(operational),
        FixtureRecord::TurnItem(TurnItemIndexRecord::new(
            turn,
            TurnItemOrdinal::FIRST,
            item,
            original.revision(),
        )),
        FixtureRecord::TurnState(fixture_turn_state(
            turn,
            state.revision(),
            TurnLifecycle::Complete,
            state.source_event_count(),
            1,
            timestamp(9),
        )),
    ]);
    for ordinal in 2..=4 {
        replacement
            .delete(FixtureDelete::TurnItem {
                turn,
                ordinal: TurnItemOrdinal::new(ordinal).unwrap(),
            })
            .unwrap();
    }
    commit(&store, storage, replacement);
    let selected = selected_path(&store, storage, thread);
    assert!(matches!(
        storage.prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_current_selected_path(thread, selected, Some(100_000),),
        ),
        Err(RecoveryProjectionError::UnsupportedHistory {
            reason: "operational canonical item"
        })
    ));
    store.close().unwrap();
}

#[test]
fn stale_selected_path_is_rejected_before_history_assembly() {
    let home = TestHome::new("phase9-recovery-stale-path");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let fixture = seed_recovery_fixture(
        &store,
        storage,
        160,
        &[("history", TurnLifecycle::Complete)],
        false,
    );
    let stale = SelectedPathProof::new(
        fixture.selected.tail(),
        fixture.selected.thread_revision(),
        SyndicPathDigest::from_bytes([0x55; 32]),
    );
    assert!(matches!(
        storage.prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_current_selected_path(fixture.thread, stale, Some(100),),
        ),
        Err(RecoveryProjectionError::StaleSelectedPath)
    ));
    store.close().unwrap();
}

fn selected_path(
    store: &beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
) -> SelectedPathProof {
    let thread = storage
        .thread(store, thread, point_limit())
        .unwrap()
        .unwrap();
    SelectedPathProof::new(
        thread.committed_tail(),
        thread.revision(),
        thread.selected_path_digest(),
    )
}
