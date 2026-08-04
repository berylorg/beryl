use beryl_model::{RecoveryItemSequenceRole, SyndicThreadId, SyndicTurnId};
use syndic_storage::test_faults::FixtureRecord;
use syndic_storage::{
    CasTurnSource, DeliveryRecoveryCase, LiveSourceEvent, ProviderObservationIssueReason,
    RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES, RecoveryAssembly, RecoveryProjection,
    RecoveryProjectionError, RecoveryProjectionRequest, SourceEventPayload, SourceEventRecord,
    SourceEventSequence, SyndicStorage, TurnEndStatus, TurnIncompleteReason, TurnLifecycle,
    TurnStateRecord, TurnTerminalOutcome, UnsupportedHistoryReason,
};

use crate::{
    projection_support::{Builder, TestHome, open, point_limit, recovery_page_pool},
    support::{batch, commit},
};

struct ContextFixture {
    home: TestHome,
    store: beryl_home_store::HomeStore,
    storage: SyndicStorage,
    thread: SyndicThreadId,
    predecessor: SyndicTurnId,
    selected: syndic_storage::SelectedPathProof,
    baseline: TurnStateRecord,
    terminal_source: CasTurnSource,
}

#[derive(Clone, Copy)]
struct CaptureFrontiers {
    item_count: u64,
    finalized: u64,
    open: u64,
    blocking: u64,
    issue: Option<ProviderObservationIssueReason>,
}

impl CaptureFrontiers {
    fn exact(state: &TurnStateRecord) -> Self {
        Self {
            item_count: state.item_count(),
            finalized: state.item_count(),
            open: 0,
            blocking: 0,
            issue: None,
        }
    }
}

fn context_fixture(name: &str, marker: bool) -> ContextFixture {
    let home = TestHome::new(name);
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut builder = Builder::new(&store, storage, 71);
    let predecessor = if marker {
        builder.submit_marker()
    } else {
        builder.submit_text("interrupted request")
    };
    let terminal_source = builder.activate_without_terminal(predecessor);
    let source = crate::recovery_support::startup_source(&store, storage);
    let DeliveryRecoveryCase::Active(active) = storage
        .classify_delivery_recovery(&store, &source, point_limit())
        .unwrap()
    else {
        panic!("authority-lost context fixture did not classify as active");
    };
    let observed_at = active.minimum_timestamp();
    crate::recovery_support::execute(
        &store,
        storage.abandon_active_binding(
            storage.revision(&store).unwrap(),
            active
                .generic_abandonment("phase63 authority-lost context", observed_at)
                .unwrap(),
        ),
    );
    let state = storage
        .turn_state(&store, predecessor.turn, point_limit())
        .unwrap()
        .unwrap();
    let gate = storage
        .input_gate(&store, builder.thread(), point_limit())
        .unwrap()
        .unwrap();
    let terminal = LiveSourceEvent::new(
        builder.thread(),
        predecessor.turn,
        state.revision(),
        gate.revision(),
        SourceEventSequence::new(state.source_event_count() + 1).unwrap(),
        None,
        SourceEventPayload::TurnEnded(TurnEndStatus::incomplete(
            TurnIncompleteReason::AuthorityLost,
        )),
        observed_at,
    )
    .unwrap();
    crate::recovery_support::execute(
        &store,
        storage.admit_live_source_event(storage.revision(&store).unwrap(), terminal),
    );
    builder.finalize_turn(predecessor.turn);
    let baseline = storage
        .turn_state(&store, predecessor.turn, point_limit())
        .unwrap()
        .unwrap();
    builder.submit_text("distinct pending follow-up");
    let selected = builder.selected_path();
    let thread = builder.thread();
    ContextFixture {
        home,
        store,
        storage,
        thread,
        predecessor: predecessor.turn,
        selected,
        baseline,
        terminal_source,
    }
}

fn install_incomplete(
    fixture: &ContextFixture,
    reason: TurnIncompleteReason,
    source: Option<CasTurnSource>,
    frontiers: CaptureFrontiers,
) {
    let status = TurnEndStatus::incomplete(reason);
    let sequence = SourceEventSequence::new(fixture.baseline.source_event_count()).unwrap();
    let state = TurnStateRecord::with_capture_frontiers_and_issue(
        fixture.predecessor,
        fixture.baseline.revision(),
        TurnLifecycle::Incomplete,
        sequence.get(),
        frontiers.item_count,
        frontiers.finalized,
        frontiers.open,
        frontiers.blocking,
        frontiers.issue,
        Some(status),
        fixture.baseline.updated_at(),
    )
    .unwrap();
    let event = SourceEventRecord::new(
        fixture.predecessor,
        sequence,
        source,
        SourceEventPayload::TurnEnded(status),
    )
    .unwrap();
    commit(
        &fixture.store,
        fixture.storage,
        batch([
            FixtureRecord::TurnState(state),
            FixtureRecord::SourceEvent(event),
        ]),
    );
}

fn prepare(fixture: &ContextFixture) -> Result<RecoveryAssembly, RecoveryProjectionError> {
    fixture.storage.prepare_recovery_projection(
        &fixture.store,
        RecoveryProjectionRequest::for_pending_selected_turn_parent(
            fixture.thread,
            fixture.selected,
            Some(100_000),
        ),
    )
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
            items.push((page.role(), String::new()));
        }
        items.last_mut().unwrap().1.push_str(page.text());
        lease = page.into_page_lease();
    }
    items
}

#[test]
fn exact_source_less_authority_lost_parent_replays_after_reopen_as_context() {
    let fixture = context_fixture("phase63-authority-lost-context-reopen", false);
    install_incomplete(
        &fixture,
        TurnIncompleteReason::AuthorityLost,
        None,
        CaptureFrontiers::exact(&fixture.baseline),
    );
    let RecoveryAssembly::Ready(projection) = prepare(&fixture).unwrap() else {
        panic!("authority-lost predecessor must produce nonempty recovery context")
    };
    assert_eq!(
        projection.represented_prefix().tail(),
        Some(fixture.predecessor)
    );
    let digest = projection.sequence_digest();
    let path = fixture.home.path().to_path_buf();
    fixture.store.close().unwrap();

    let mut reopened = open(&path);
    let storage = SyndicStorage::register(&mut reopened).unwrap();
    let expected = vec![(
        RecoveryItemSequenceRole::UserInputText,
        "interrupted request".to_owned(),
    )];
    assert_eq!(replay(storage, &reopened, projection), expected);
    let RecoveryAssembly::Ready(reopened_projection) = storage
        .prepare_recovery_projection(
            &reopened,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                fixture.thread,
                fixture.selected,
                Some(100_000),
            ),
        )
        .unwrap()
    else {
        panic!("reopened authority-lost predecessor lost context eligibility")
    };
    assert_eq!(reopened_projection.sequence_digest(), digest);
    assert_eq!(replay(storage, &reopened, reopened_projection), expected);
}

#[test]
fn sourceful_or_differently_incomplete_parent_is_rejected() {
    let sourceful = context_fixture("phase63-authority-lost-sourceful", false);
    install_incomplete(
        &sourceful,
        TurnIncompleteReason::AuthorityLost,
        Some(sourceful.terminal_source.clone()),
        CaptureFrontiers::exact(&sourceful.baseline),
    );
    assert!(matches!(
        prepare(&sourceful),
        Err(RecoveryProjectionError::IncompleteHistory { .. })
    ));

    for (index, reason) in [
        TurnIncompleteReason::StreamLost,
        TurnIncompleteReason::WorkerStopped,
        TurnIncompleteReason::CompletionMismatch,
        TurnIncompleteReason::ItemAuditFailed,
        TurnIncompleteReason::UnsupportedHistory(UnsupportedHistoryReason::UnknownPublicItem),
        TurnIncompleteReason::UnsupportedHistory(UnsupportedHistoryReason::MalformedRequiredField),
        TurnIncompleteReason::UnsupportedHistory(
            UnsupportedHistoryReason::UnsupportedRequiredPayload,
        ),
        TurnIncompleteReason::UnsupportedHistory(UnsupportedHistoryReason::HostedImageGeneration),
        TurnIncompleteReason::UnsupportedHistory(UnsupportedHistoryReason::ImpossibleLifecycle),
    ]
    .into_iter()
    .enumerate()
    {
        let fixture = context_fixture(
            &format!("phase63-authority-lost-other-reason-{index}"),
            false,
        );
        install_incomplete(
            &fixture,
            reason,
            None,
            CaptureFrontiers::exact(&fixture.baseline),
        );
        assert!(matches!(
            prepare(&fixture),
            Err(RecoveryProjectionError::IncompleteHistory { .. })
        ));
    }
}

#[test]
fn missing_or_mismatched_authority_lost_terminal_is_rejected() {
    let mismatched = context_fixture("phase63-authority-lost-mismatched-terminal", false);
    let authority_lost = TurnEndStatus::incomplete(TurnIncompleteReason::AuthorityLost);
    let mismatched_status = TurnEndStatus::incomplete(TurnIncompleteReason::WorkerStopped);
    let sequence = SourceEventSequence::new(mismatched.baseline.source_event_count()).unwrap();
    let mismatched_state = TurnStateRecord::with_capture_frontiers(
        mismatched.predecessor,
        mismatched.baseline.revision(),
        TurnLifecycle::Incomplete,
        sequence.get(),
        mismatched.baseline.item_count(),
        mismatched.baseline.item_count(),
        0,
        0,
        Some(authority_lost),
        mismatched.baseline.updated_at(),
    )
    .unwrap();
    let mismatched_event = SourceEventRecord::new(
        mismatched.predecessor,
        sequence,
        None,
        SourceEventPayload::TurnEnded(mismatched_status),
    )
    .unwrap();
    commit(
        &mismatched.store,
        mismatched.storage,
        batch([
            FixtureRecord::TurnState(mismatched_state),
            FixtureRecord::SourceEvent(mismatched_event),
        ]),
    );
    assert!(matches!(
        prepare(&mismatched),
        Err(RecoveryProjectionError::IncompleteHistory { .. })
    ));

    let missing = context_fixture("phase63-authority-lost-missing-terminal", false);
    let missing_sequence =
        SourceEventSequence::new(missing.baseline.source_event_count() + 1).unwrap();
    let missing_state = TurnStateRecord::with_capture_frontiers(
        missing.predecessor,
        missing.baseline.revision(),
        TurnLifecycle::Incomplete,
        missing_sequence.get(),
        missing.baseline.item_count(),
        missing.baseline.item_count(),
        0,
        0,
        Some(authority_lost),
        missing.baseline.updated_at(),
    )
    .unwrap();
    commit(
        &missing.store,
        missing.storage,
        batch([FixtureRecord::TurnState(missing_state)]),
    );
    assert!(matches!(
        prepare(&missing),
        Err(RecoveryProjectionError::MissingHistory {
            record: "authority-lost terminal source event"
        })
    ));
}

#[test]
fn incomplete_capture_frontiers_and_unsupported_items_remain_ineligible() {
    let variants = [
        CaptureFrontiers {
            item_count: 1,
            finalized: 0,
            open: 0,
            blocking: 0,
            issue: None,
        },
        CaptureFrontiers {
            item_count: 1,
            finalized: 1,
            open: 1,
            blocking: 0,
            issue: None,
        },
        CaptureFrontiers {
            item_count: 1,
            finalized: 1,
            open: 0,
            blocking: 1,
            issue: None,
        },
        CaptureFrontiers {
            item_count: 1,
            finalized: 1,
            open: 0,
            blocking: 0,
            issue: Some(ProviderObservationIssueReason::EventAfterCompletion),
        },
        CaptureFrontiers {
            item_count: 0,
            finalized: 0,
            open: 0,
            blocking: 0,
            issue: None,
        },
    ];
    for (index, frontiers) in variants.into_iter().enumerate() {
        let fixture = context_fixture(&format!("phase63-authority-lost-frontier-{index}"), false);
        install_incomplete(
            &fixture,
            TurnIncompleteReason::AuthorityLost,
            None,
            frontiers,
        );
        assert!(matches!(
            prepare(&fixture),
            Err(RecoveryProjectionError::IncompleteHistory { .. })
        ));
    }

    let marker = context_fixture("phase63-authority-lost-marker", true);
    install_incomplete(
        &marker,
        TurnIncompleteReason::AuthorityLost,
        None,
        CaptureFrontiers::exact(&marker.baseline),
    );
    assert!(matches!(
        prepare(&marker),
        Err(RecoveryProjectionError::MediaHistory { .. })
    ));
}

#[test]
fn authority_lost_context_is_allowed_only_at_the_immediate_predecessor() {
    let home = TestHome::new("phase63-authority-lost-not-immediate");
    let mut store = open(home.path());
    let storage = SyndicStorage::register(&mut store).unwrap();
    let mut builder = Builder::new(&store, storage, 72);
    let older = builder.submit_text("older interrupted request");
    builder.complete_without_assistant(older, TurnTerminalOutcome::Failed);
    let older_state = storage
        .turn_state(&store, older.turn, point_limit())
        .unwrap()
        .unwrap();
    let older_sequence = SourceEventSequence::new(older_state.source_event_count()).unwrap();
    let status = TurnEndStatus::incomplete(TurnIncompleteReason::AuthorityLost);
    commit(
        &store,
        storage,
        batch([
            FixtureRecord::TurnState(
                TurnStateRecord::with_capture_frontiers(
                    older.turn,
                    older_state.revision(),
                    TurnLifecycle::Incomplete,
                    older_sequence.get(),
                    older_state.item_count(),
                    older_state.item_count(),
                    0,
                    0,
                    Some(status),
                    older_state.updated_at(),
                )
                .unwrap(),
            ),
            FixtureRecord::SourceEvent(
                SourceEventRecord::new(
                    older.turn,
                    older_sequence,
                    None,
                    SourceEventPayload::TurnEnded(status),
                )
                .unwrap(),
            ),
        ]),
    );
    let immediate = builder.submit_text("complete immediate predecessor");
    builder.complete_without_assistant(immediate, TurnTerminalOutcome::Complete);
    builder.submit_text("pending follow-up");

    assert!(matches!(
        storage.prepare_recovery_projection(
            &store,
            RecoveryProjectionRequest::for_pending_selected_turn_parent(
                builder.thread(),
                builder.selected_path(),
                Some(100_000),
            ),
        ),
        Err(RecoveryProjectionError::IncompleteHistory { .. })
    ));
}
