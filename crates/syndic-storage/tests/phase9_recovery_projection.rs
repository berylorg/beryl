#![cfg(feature = "test-faults")]

#[path = "phase9_recovery_projection/support.rs"]
mod support;

use beryl_model::{RecoveryItemSequenceDigest, SyndicPathDigest};
use sha2::{Digest, Sha256};
use syndic_storage::*;

use support::{Builder, TestHome, open};

#[test]
fn exact_root_to_tail_items_exclude_pending_input_and_reopen_deterministically() {
    let home = TestHome::new("phase9-recovery-exact-order");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut builder = Builder::new(&store, storage, 1);

    let first = builder.submit_text("first user");
    builder.complete_with_assistant(first, AssistantMessagePhase::Commentary, "first assistant");
    let spanning = format!(
        "{}β-tail",
        "x".repeat(CONTENT_CHUNK_MAX_BYTES.saturating_sub(1))
    );
    let second = builder.submit_text(&spanning);
    builder.complete_with_assistant(
        second,
        AssistantMessagePhase::FinalAnswer,
        "second assistant",
    );
    let pending = builder.submit_text("excluded pending input");
    let selected = builder.selected_path();
    let request = RecoveryProjectionRequest::for_pending_selected_turn_parent(
        builder.thread(),
        selected,
        Some(400_000),
    );

    let RecoveryAssembly::Ready(projection) = storage
        .prepare_recovery_projection(&store, request)
        .unwrap()
    else {
        panic!("nonempty parent prefix must require recovery")
    };
    assert_eq!(projection.thread_id(), builder.thread());
    assert_eq!(projection.selected_path(), selected);
    assert_eq!(projection.represented_prefix().tail(), Some(second.turn));
    assert_ne!(projection.represented_prefix().tail(), Some(pending.turn));
    assert_eq!(
        projection
            .items()
            .iter()
            .map(|item| (item.role(), item.text_kind(), item.text()))
            .collect::<Vec<_>>(),
        vec![
            (
                RecoveryItemRole::User,
                RecoveryItemTextKind::InputText,
                "first user",
            ),
            (
                RecoveryItemRole::Assistant,
                RecoveryItemTextKind::OutputText,
                "first assistant",
            ),
            (
                RecoveryItemRole::User,
                RecoveryItemTextKind::InputText,
                spanning.as_str(),
            ),
            (
                RecoveryItemRole::Assistant,
                RecoveryItemTextKind::OutputText,
                "second assistant",
            ),
        ]
    );
    let manifest = storage
        .canonical_item(&store, second.user_item, support::point_limit())
        .unwrap()
        .unwrap();
    assert!(
        manifest
            .record()
            .payload()
            .content()
            .expect("projected assistant message has content")
            .summary()
            .chunk_count()
            > 1
    );
    assert_eq!(
        projection.sequence_digest(),
        expected_digest(projection.items())
    );
    let expected_items = projection.items().to_vec();
    let expected_digest = projection.sequence_digest();
    store.close().unwrap();

    let mut reopened = open(home.path());
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let RecoveryAssembly::Ready(reopened_projection) = storage
        .prepare_recovery_projection(&reopened, request)
        .unwrap()
    else {
        panic!("reopened nonempty prefix must remain recoverable")
    };
    assert_eq!(reopened_projection.items(), expected_items);
    assert_eq!(reopened_projection.sequence_digest(), expected_digest);
    reopened.close().unwrap();
}

#[test]
fn root_pending_is_native_fresh_even_without_model_metadata() {
    let home = TestHome::new("phase9-recovery-native-empty");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut builder = Builder::new(&store, storage, 2);
    let pending = builder.submit_text("root pending");
    let selected = builder.selected_path();
    let request = RecoveryProjectionRequest::for_pending_selected_turn_parent(
        builder.thread(),
        selected,
        None,
    );
    let RecoveryAssembly::NativeEmptyPrefix {
        thread_id,
        selected_path,
        source_revision,
    } = storage
        .prepare_recovery_projection(&store, request)
        .unwrap()
    else {
        panic!("root pending turn must select native Fresh")
    };
    assert_eq!(thread_id, builder.thread());
    assert_eq!(selected_path.tail(), Some(pending.turn));
    assert_eq!(selected_path, selected);
    assert_eq!(source_revision, storage.revision(&store).unwrap());
}

#[test]
fn finalized_interrupted_and_failed_turns_remain_exact_recovery_history() {
    for (name, thread_byte, outcome) in [
        (
            "phase9-recovery-interrupted",
            6,
            TurnTerminalOutcome::Interrupted,
        ),
        ("phase9-recovery-failed", 7, TurnTerminalOutcome::Failed),
    ] {
        let home = TestHome::new(name);
        let mut store = open(home.path());
        let storage = SyndicStorage::register(&mut store).unwrap();
        let mut builder = Builder::new(&store, storage, thread_byte);
        let completed = builder.submit_text("retained user history");
        builder.complete_without_assistant(completed, outcome);
        builder.submit_text("pending input");

        let RecoveryAssembly::Ready(projection) = storage
            .prepare_recovery_projection(
                &store,
                RecoveryProjectionRequest::for_pending_selected_turn_parent(
                    builder.thread(),
                    builder.selected_path(),
                    Some(100_000),
                ),
            )
            .unwrap()
        else {
            panic!("finalized interrupted and failed history must be recoverable")
        };
        assert_eq!(
            projection.items(),
            &[RecoveryItem::UserInputText("retained user history".into())]
        );
    }
}

#[test]
fn recovery_sequence_digest_matches_the_fixed_v1_vector() {
    let home = TestHome::new("phase9-recovery-digest-vector");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut builder = Builder::new(&store, storage, 8);
    let completed = builder.submit_text("u");
    builder.complete_with_assistant(completed, AssistantMessagePhase::FinalAnswer, "a");
    builder.submit_text("pending input");

    let RecoveryAssembly::Ready(projection) = storage
        .prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                builder.thread(),
                builder.selected_path(),
                Some(100_000),
            ),
        )
        .unwrap()
    else {
        panic!("nonempty history must produce a recovery sequence")
    };
    assert_eq!(
        projection.sequence_digest(),
        RecoveryItemSequenceDigest::from_bytes([
            0x9a, 0x25, 0xb0, 0xeb, 0xf7, 0x06, 0x26, 0xc5, 0xf0, 0x41, 0x84, 0x6d, 0xd5, 0x87,
            0x82, 0xf4, 0x92, 0x43, 0x9e, 0xf7, 0xa9, 0xe8, 0x9b, 0x7d, 0x6e, 0x14, 0x90, 0x53,
            0xa6, 0xc1, 0x79, 0xb2,
        ])
    );
}

#[test]
fn absolute_utf8_ceiling_accepts_exactly_and_rejects_plus_one() {
    with_single_user_prefix(
        "phase9-recovery-absolute-exact",
        &"a".repeat(RecoveryUtf8ByteCount::MAX as usize),
        |store, storage, thread, selected| {
            let request = RecoveryProjectionRequest::for_pending_selected_turn_parent(
                thread,
                selected,
                Some(u64::MAX),
            );
            let RecoveryAssembly::Ready(projection) =
                storage.prepare_recovery_projection(store, request).unwrap()
            else {
                panic!("nonempty exact-limit prefix must be ready")
            };
            assert_eq!(projection.utf8_bytes().get(), RecoveryUtf8ByteCount::MAX);
        },
    );
    with_single_user_prefix(
        "phase9-recovery-absolute-plus-one",
        &"b".repeat(RecoveryUtf8ByteCount::MAX as usize + 1),
        |store, storage, thread, selected| {
            let error = storage
                .prepare_recovery_projection(
                    store,
                    RecoveryProjectionRequest::for_pending_selected_turn_parent(
                        thread,
                        selected,
                        Some(u64::MAX),
                    ),
                )
                .unwrap_err();
            assert!(matches!(
                error,
                RecoveryProjectionError::BudgetOverflow {
                    kind: RecoveryBudgetKind::Utf8Bytes,
                    maximum: RecoveryUtf8ByteCount::MAX,
                    actual,
                } if actual == RecoveryUtf8ByteCount::MAX + 1
            ));
        },
    );
}

#[test]
fn half_window_budget_and_missing_or_zero_metadata_are_exact() {
    with_single_user_prefix(
        "phase9-recovery-model-window",
        "elevenbytes",
        |store, storage, thread, selected| {
            let exact = RecoveryProjectionRequest::for_pending_selected_turn_parent(
                thread,
                selected,
                Some(22),
            );
            let RecoveryAssembly::Ready(projection) =
                storage.prepare_recovery_projection(store, exact).unwrap()
            else {
                panic!("exact half-window prefix must be ready")
            };
            assert_eq!(projection.utf8_bytes().get(), 11);
            assert!(matches!(
                storage
                    .prepare_recovery_projection(
                        store,
                        RecoveryProjectionRequest::for_pending_selected_turn_parent(
                            thread,
                            selected,
                            Some(21),
                        ),
                    )
                    .unwrap_err(),
                RecoveryProjectionError::BudgetOverflow {
                    kind: RecoveryBudgetKind::Utf8Bytes,
                    maximum: 10,
                    actual: 11,
                }
            ));
            assert!(matches!(
                storage
                    .prepare_recovery_projection(
                        store,
                        RecoveryProjectionRequest::for_pending_selected_turn_parent(
                            thread, selected, None,
                        ),
                    )
                    .unwrap_err(),
                RecoveryProjectionError::MissingModelContextWindow
            ));
            assert!(matches!(
                storage
                    .prepare_recovery_projection(
                        store,
                        RecoveryProjectionRequest::for_pending_selected_turn_parent(
                            thread,
                            selected,
                            Some(0),
                        ),
                    )
                    .unwrap_err(),
                RecoveryProjectionError::ZeroModelContextWindow
            ));
        },
    );
}

#[test]
fn media_operational_empty_and_incomplete_history_reject_distinctly() {
    assert!(matches!(
        prefix_error("phase9-recovery-media", |builder| {
            let turn = builder.submit_marker();
            builder.complete_without_assistant(turn, TurnTerminalOutcome::Complete);
        }),
        RecoveryProjectionError::MediaHistory { .. }
    ));
    assert!(matches!(
        prefix_error("phase9-recovery-operational", |builder| {
            let turn = builder.submit_text("user");
            builder.complete_with_operational(turn, "provider operation");
        }),
        RecoveryProjectionError::UnsupportedHistory { .. }
    ));
    assert!(matches!(
        prefix_error("phase9-recovery-empty", |builder| {
            let turn = builder.submit_text("user");
            builder.complete_with_assistant(turn, AssistantMessagePhase::Unknown, "");
        }),
        RecoveryProjectionError::EmptyHistoryItem
    ));
    let incomplete = prefix_error("phase9-recovery-incomplete", |builder| {
        let turn = builder.submit_text("captured prefix");
        builder.complete_without_assistant(turn, TurnTerminalOutcome::Incomplete);
    });
    assert!(matches!(
        &incomplete,
        RecoveryProjectionError::IncompleteHistory { .. }
    ));
    assert!(incomplete.to_string().contains("not recovery-complete"));
    assert!(
        incomplete
            .to_string()
            .contains("exact outcome, history-incomplete reason, item audit")
    );
}

#[test]
fn stale_selected_path_is_rejected_before_history_assembly() {
    with_single_user_prefix(
        "phase9-recovery-stale-path",
        "history",
        |store, storage, thread, selected| {
            let stale =
                SelectedPathProof::new(selected.tail(), selected.thread_revision(), stale_digest());
            assert!(matches!(
                storage
                    .prepare_recovery_projection(
                        store,
                        RecoveryProjectionRequest::for_pending_selected_turn_parent(
                            thread,
                            stale,
                            Some(100),
                        ),
                    )
                    .unwrap_err(),
                RecoveryProjectionError::StaleSelectedPath
            ));
        },
    );
}

fn prefix_error(name: &str, complete: impl FnOnce(&mut Builder<'_>)) -> RecoveryProjectionError {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut builder = Builder::new(&store, storage, 4);
    complete(&mut builder);
    builder.submit_text("pending");
    let request = RecoveryProjectionRequest::for_pending_selected_turn_parent(
        builder.thread(),
        builder.selected_path(),
        Some(1_000_000),
    );
    storage
        .prepare_recovery_projection(&store, request)
        .unwrap_err()
}

fn with_single_user_prefix(
    name: &str,
    text: &str,
    check: impl FnOnce(
        &beryl_home_store::HomeStore,
        SyndicStorage,
        beryl_model::SyndicThreadId,
        SelectedPathProof,
    ),
) {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut builder = Builder::new(&store, storage, 3);
    let complete = builder.submit_text(text);
    builder.complete_without_assistant(complete, TurnTerminalOutcome::Complete);
    builder.submit_text("pending");
    let thread = builder.thread();
    let selected = builder.selected_path();
    check(&store, storage, thread, selected);
}

fn expected_digest(items: &[RecoveryItem]) -> RecoveryItemSequenceDigest {
    let total_bytes: u64 = items.iter().map(|item| item.text().len() as u64).sum();
    let mut hash = Sha256::new();
    hash.update(b"beryl.syndic.recovery-item-sequence.v1\0");
    hash.update((items.len() as u64).to_be_bytes());
    hash.update(total_bytes.to_be_bytes());
    for (index, item) in items.iter().enumerate() {
        hash.update((index as u64 + 1).to_be_bytes());
        hash.update([match item.role() {
            RecoveryItemRole::User => 0,
            RecoveryItemRole::Assistant => 1,
        }]);
        hash.update((item.text().len() as u64).to_be_bytes());
        hash.update(item.text().as_bytes());
    }
    RecoveryItemSequenceDigest::from_bytes(hash.finalize().into())
}

fn stale_digest() -> SyndicPathDigest {
    SyndicPathDigest::from_bytes([0x55; 32])
}
