use super::*;
use syndic_storage::{
    ClaimCompactionDispatch, CompactionAdmissionRead, CompactionAttemptNonce,
    CompactionOperationId, CompactionOperationNonce, CompactionProviderEvent,
    CompactionProviderSequence, CompactionSettlement, CompactionThreadStatus,
    PublishCompactionProviderEvent, SettleCompactionOperation, SyndicPointReadLimit,
};

#[gpui::test]
fn mounted_compaction_keeps_its_workflow_label_through_terminal_until_exact_gate_settlement(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let (fixture, prepared, acquisition) = support::join(
        support::worker(|| {
            let mut fixture = syndic::Fixture::new(187);
            eprintln!(
                "mounted compaction fixture: {}",
                fixture.home_path().display()
            );
            let (prepared, acquisition) = support::prepare_shell(&mut fixture);
            (fixture, prepared, acquisition)
        }),
        cx,
    );
    let worker = fixture.store.exact_stop_worker();
    let publication = Arc::new(());
    let appearance = cx.update(|app| {
        beryl_app::theme_runtime::GpuiAppearanceWindowSet::new(
            prepared.appearance().clone(),
            std::num::NonZeroUsize::new(4).unwrap(),
            app,
        )
    });
    let shell = cx.update(|app| {
        GpuiMainWindowShellHost::new(app, appearance)
            .construct_hidden(prepared)
            .unwrap_or_else(|_| panic!("compaction shell"))
    });
    let window = shell.window();
    window
        .update(cx, |root, window, cx| {
            root.test_mount_exact_status_worker(
                worker.clone(),
                Arc::downgrade(&publication),
                fixture.state.session(),
                window,
                cx,
            )
        })
        .unwrap();
    support::draw(window, cx);
    let (fixture, operation) = support::join(
        support::worker(move || {
            let mut fixture = fixture;
            let submitted = fixture.submit_text(" compacted user turn");
            fixture.complete_with_assistant(submitted, " completed ordinary answer");
            let home = fixture.home();
            let CompactionAdmissionRead::Admissible(candidate) = fixture
                .storage
                .compaction_admission_read(&home, fixture.thread, limit())
                .unwrap()
            else {
                panic!("compaction candidate");
            };
            let attempt = CompactionAttemptNonce::from_bytes([183; 16]);
            let admission = candidate.admission(
                CompactionOperationNonce::from_bytes([182; 16]),
                attempt,
                CasLoadedSessionGeneration::new(
                    CasProcessGeneration::new(87).unwrap(),
                    CasLoadedThreadGeneration::new(88).unwrap(),
                ),
                SyndicTimestamp::from_unix_millis(72_000),
            );
            let operation = admission.operation_id();
            clean(
                home.execute_current(
                    fixture
                        .storage
                        .current_admit_compaction_operation(admission),
                ),
            );
            let record = fixture
                .storage
                .compaction_operation(&home, operation, limit())
                .unwrap()
                .unwrap();
            clean(
                home.execute_current(fixture.storage.current_claim_compaction_dispatch(
                    ClaimCompactionDispatch::new(operation, record.revision(), attempt),
                )),
            );
            drop(home);
            (fixture, operation)
        }),
        cx,
    );
    support::wait(window, cx, |d| d.0 == "compacting" && !d.1 && !d.2);
    let fixture = support::join(
        support::worker(move || {
            for (index, event) in [
                CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Active),
                CompactionProviderEvent::TurnStarted(CasTurnId::new("compaction").unwrap()),
                CompactionProviderEvent::Marker {
                    item_id: SyndicItemId::from_bytes([184; 16]),
                    lifecycle: syndic_storage::CompactionMarkerLifecycle::Started,
                },
                CompactionProviderEvent::Marker {
                    item_id: SyndicItemId::from_bytes([184; 16]),
                    lifecycle: syndic_storage::CompactionMarkerLifecycle::Completed,
                },
                CompactionProviderEvent::ThreadStatus(CompactionThreadStatus::Idle),
                CompactionProviderEvent::Terminal(syndic_storage::TurnEndStatus::complete()),
            ]
            .into_iter()
            .enumerate()
            {
                publish(&fixture, operation, event, 72_010 + index as u64);
            }
            fixture
        }),
        cx,
    );
    support::wait(window, cx, |d| d.0 == "compacting" && !d.1 && !d.2);
    let exact = support::join(
        support::worker({
            let worker = worker.clone();
            let thread = fixture.thread;
            move || worker.selected_operation_snapshot(thread)
        }),
        cx,
    );
    assert_eq!(exact.state, ExactParentState::Compacting);
    assert!(!exact.operation_active);
    let fixture = support::join(
        support::worker(move || {
            let home = fixture.home();
            let record = fixture
                .storage
                .compaction_operation(&home, operation, limit())
                .unwrap()
                .unwrap();
            clean(
                home.execute_current(fixture.storage.current_settle_compaction_operation(
                    SettleCompactionOperation::new(
                        operation,
                        record.revision(),
                        CompactionSettlement::ManualSuccess,
                    ),
                )),
            );
            drop(home);
            fixture
        }),
        cx,
    );
    support::wait(window, cx, |d| d.0 == "ok" && !d.1 && !d.2);
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    drop(shell);
    drop(acquisition);
    let (directory, service) = fixture.into_service();
    assert!(matches!(
        service.close().unwrap(),
        ProjectionConnectionServiceCloseOutcome::Closed
    ));
    drop(directory);
}

fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(1_000_000).unwrap()
}
fn clean(outcome: CommandOutcome) {
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "{outcome:?}"
    );
}
fn publish(
    fixture: &syndic::Fixture,
    operation: CompactionOperationId,
    event: CompactionProviderEvent,
    at: u64,
) {
    let home = fixture.home();
    let record = fixture
        .storage
        .compaction_operation(&home, operation, limit())
        .unwrap()
        .unwrap();
    let sequence = record
        .provider_frontier()
        .map_or(CompactionProviderSequence::FIRST, |frontier| {
            frontier.checked_next().unwrap()
        });
    clean(
        home.execute_current(fixture.storage.current_publish_compaction_provider_event(
            PublishCompactionProviderEvent::new(
                operation,
                record.revision(),
                sequence,
                event,
                SyndicTimestamp::from_unix_millis(at),
            ),
        )),
    );
}
