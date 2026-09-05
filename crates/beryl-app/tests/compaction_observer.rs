#[path = "../src/compaction_diagnostics.rs"]
mod compaction_diagnostics;
#[path = "../src/shell/compaction_observer.rs"]
mod observer;

use beryl_backend::{
    ApprovalRequest, CompactionInterruptedReason, CompactionReceiptState as State,
    CompactionUnknownReason, JsonRpcError, ManagedBackendError, ThreadItem, ThreadStatus,
    TurnError, TurnInfo, TurnStatus, TurnStreamEvent,
};
use beryl_model::workspace::WorkspaceId;
use observer::*;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    time::Duration,
};

const THREAD: &str = "11111111-1111-4111-8111-111111111111";
const TURN: &str = "22222222-2222-4222-8222-222222222222";
const SESSION: &str = "33333333-3333-4333-8333-333333333333";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

fn secs(value: u64) -> Duration {
    Duration::from_secs(value)
}
fn target() -> ObserverTarget {
    ObserverTarget {
        workspace_id: "workspace-one".into(),
        execution_target: WorkspaceId::host_windows("C:\\work\\beryl"),
        generation: 7,
        thread_id: THREAD.into(),
    }
}
fn operation() -> OperationIdentity {
    OperationIdentity {
        thread_id: THREAD.into(),
        operation_id: "44444444-4444-4444-8444-444444444444".into(),
        observation_session_id: SESSION.into(),
    }
}
fn receipt(state: State) -> Receipt {
    let absent = matches!(
        state,
        State::Unknown {
            reason: CompactionUnknownReason::AbsentOrExpired
                | CompactionUnknownReason::RuntimeChange
        }
    );
    Receipt {
        operation: operation(),
        turn_id: (!absent).then(|| TURN.into()),
        state,
    }
}
fn item() -> ThreadItem {
    serde_json::from_value(serde_json::json!({"type":"contextCompaction", "id":"compact-item"}))
        .unwrap()
}
fn item_done() -> TurnStreamEvent {
    TurnStreamEvent::ItemCompleted {
        thread_id: THREAD.into(),
        turn_id: TURN.into(),
        item: item(),
    }
}
fn terminal(status: TurnStatus) -> TurnStreamEvent {
    TurnStreamEvent::TurnCompleted {
        thread_id: THREAD.into(),
        turn: TurnInfo {
            id: TURN.into(),
            status,
            items: vec![item()],
            error: None,
        },
    }
}
fn idle() -> TurnStreamEvent {
    TurnStreamEvent::ThreadStatusChanged {
        thread_id: THREAD.into(),
        status: ThreadStatus::Idle,
    }
}

struct TestClock {
    now: Rc<Cell<Duration>>,
    stop_at: Duration,
}
impl Clock for TestClock {
    fn now(&self) -> Duration {
        self.now.get()
    }
    fn wait(&mut self, duration: Duration, cancel: &Cancellation) {
        self.now.set(self.now.get() + duration);
        if self.now.get() >= self.stop_at {
            cancel.cancel();
        }
    }
}

#[derive(Default)]
struct Sink {
    updates: Vec<ObserverUpdate>,
    cancel_after: Option<usize>,
}
impl UpdateSink for Sink {
    fn publish(&mut self, update: ObserverUpdate, cancel: &Cancellation) -> bool {
        self.updates.push(update);
        if self.cancel_after == Some(self.updates.len()) {
            cancel.cancel();
        }
        !cancel.is_cancelled()
    }
}

struct Script {
    now: Rc<Cell<Duration>>,
    log: Rc<RefCell<Vec<(&'static str, Duration)>>>,
    connect_results: VecDeque<Result<String, PortError>>,
    start_result: Result<Receipt, PortError>,
    prepare_error: Option<PortError>,
    start_duration: Duration,
    receipts: VecDeque<Result<Receipt, PortError>>,
    statuses: VecDeque<Result<(String, ThreadStatus), PortError>>,
    status_durations: VecDeque<Duration>,
    events: VecDeque<(Duration, Result<TurnStreamEvent, PortError>)>,
    cancel_on: Option<(&'static str, Cancellation)>,
    poll_stop: Duration,
    cancellation: Cancellation,
}
impl Script {
    fn new(now: Rc<Cell<Duration>>, cancellation: Cancellation) -> Self {
        Self {
            now,
            log: Default::default(),
            connect_results: Default::default(),
            start_result: Ok(receipt(State::Accepted)),
            prepare_error: None,
            start_duration: Duration::ZERO,
            receipts: Default::default(),
            statuses: Default::default(),
            status_durations: Default::default(),
            events: Default::default(),
            cancel_on: None,
            poll_stop: secs(125),
            cancellation,
        }
    }
    fn record(&self, name: &'static str) {
        self.log.borrow_mut().push((name, self.now.get()));
        if let Some((expected, cancel)) = &self.cancel_on
            && *expected == name
        {
            cancel.cancel();
        }
    }
    fn times(&self, name: &str) -> Vec<Duration> {
        self.log
            .borrow()
            .iter()
            .filter(|(method, _)| *method == name)
            .map(|(_, at)| *at)
            .collect()
    }
    fn event(&mut self, at: u64, event: TurnStreamEvent) {
        self.events.push_back((secs(at), Ok(event)));
    }
}
impl BackendPort for Script {
    fn connect(&mut self, timeout: Duration) -> Result<String, PortError> {
        assert_eq!(timeout, REQUEST_TIMEOUT);
        self.record("connect");
        self.connect_results
            .pop_front()
            .unwrap_or_else(|| Ok(SESSION.into()))
    }
    fn disconnect(&mut self) {
        self.record("disconnect");
    }
    fn prepare(&mut self, thread_id: &str) -> Result<OperationIdentity, PortError> {
        if let Some(error) = &self.prepare_error {
            return Err(error.clone());
        }
        assert_eq!(thread_id, THREAD);
        self.record("prepare");
        Ok(operation())
    }
    fn subscribe(&mut self, thread_id: &str, timeout: Duration) -> Result<(), PortError> {
        assert_eq!(thread_id, THREAD);
        assert_eq!(timeout, REQUEST_TIMEOUT);
        self.record("subscribe");
        Ok(())
    }
    fn start(&mut self, timeout: Duration) -> Result<Receipt, PortError> {
        assert_eq!(timeout, REQUEST_TIMEOUT);
        self.record("start");
        self.now.set(self.now.get() + self.start_duration);
        self.start_result.clone()
    }
    fn read(&mut self, turn_id: Option<&str>, timeout: Duration) -> Result<Receipt, PortError> {
        if let Some(turn) = turn_id {
            assert_eq!(turn, TURN);
        }
        assert_eq!(timeout, REQUEST_TIMEOUT);
        self.record("read");
        self.receipts
            .pop_front()
            .unwrap_or_else(|| Ok(receipt(State::Running)))
    }
    fn status(
        &mut self,
        thread_id: &str,
        timeout: Duration,
    ) -> Result<(String, ThreadStatus), PortError> {
        assert_eq!(thread_id, THREAD);
        assert_eq!(timeout, REQUEST_TIMEOUT);
        self.record("status");
        self.now
            .set(self.now.get() + self.status_durations.pop_front().unwrap_or_default());
        self.statuses
            .pop_front()
            .unwrap_or_else(|| Ok((THREAD.into(), ThreadStatus::Idle)))
    }
    fn poll(&mut self, timeout: Duration) -> Result<Option<TurnStreamEvent>, PortError> {
        assert!(timeout <= Duration::from_millis(250));
        self.record("poll");
        let until = self.now.get() + timeout;
        if self.events.front().is_some_and(|(at, _)| *at <= until) {
            let (at, event) = self.events.pop_front().unwrap();
            self.now.set(self.now.get().max(at));
            return event.map(Some);
        }
        self.now.set(until);
        if until >= self.poll_stop {
            self.cancellation.cancel();
        }
        Ok(None)
    }
    fn deny_approval(
        &mut self,
        _: &ApprovalRequest,
        _: &str,
        _: Option<&str>,
        timeout: Duration,
    ) -> Result<(), PortError> {
        assert_eq!(timeout, REQUEST_TIMEOUT);
        self.record("deny");
        Ok(())
    }
}

struct Harness {
    backend: Script,
    clock: TestClock,
    sink: Sink,
    cancel: Cancellation,
    diagnostics: Option<compaction_diagnostics::CompactionDiagnosticHandle>,
}
impl Harness {
    fn capture(&mut self, warning: u64) -> compaction_diagnostics::CompactionDiagnostics {
        let ring = compaction_diagnostics::CompactionDiagnostics::default();
        self.diagnostics = Some(
            ring.begin(compaction_diagnostics::CompactionDiagnosticStart {
                local_generation: target().generation,
                workspace_identity: Some("workspace-one"),
                runtime_alias: Some("compaction-runtime-7"),
                thread_identity: None,
                warning_threshold: secs(warning),
            }),
        );
        ring
    }
    fn new() -> Self {
        let now = Rc::new(Cell::new(Duration::ZERO));
        let cancel = Cancellation::default();
        Self {
            backend: Script::new(now.clone(), cancel.clone()),
            clock: TestClock {
                now,
                stop_at: secs(125),
            },
            sink: Sink::default(),
            cancel,
            diagnostics: None,
        }
    }
    fn run(&mut self, warning: u64) {
        run_observer(
            &mut self.backend,
            &mut self.clock,
            &mut self.sink,
            target(),
            REQUEST_TIMEOUT,
            secs(warning),
            &self.cancel,
            self.diagnostics.clone(),
        );
    }
    fn outcomes(&self) -> Vec<&Outcome> {
        self.sink
            .updates
            .iter()
            .filter_map(|update| match &update.kind {
                UpdateKind::Finished(outcome) => Some(outcome),
                _ => None,
            })
            .collect()
    }
    fn warnings(&self) -> usize {
        self.sink
            .updates
            .iter()
            .filter(|update| update.kind == UpdateKind::Warning)
            .count()
    }
    fn uncertainty(&self, reason: UnconfirmedReason) -> bool {
        self.sink
            .updates
            .iter()
            .any(|update| update.kind == UpdateKind::Unconfirmed(reason))
    }
}

#[test]
fn diagnostics_explain_slow_success_with_exact_identity_warning_and_fresh_idle() {
    use compaction_diagnostics::{
        CompactionDiagnosticCategory as C, CompactionDiagnosticStage as S,
    };
    let mut h = Harness::new();
    let ring = h.capture(30);
    h.backend.event(12, item_done());
    h.backend.receipts = [Ok(receipt(State::Running)), Ok(receipt(State::Completed))].into();
    h.backend.statuses.push_back(Ok((
        THREAD.into(),
        ThreadStatus::Active {
            active_flags: vec![],
        },
    )));
    h.run(30);
    assert_eq!(h.outcomes(), vec![&Outcome::Succeeded]);
    assert_eq!(h.backend.times("read"), vec![secs(30), secs(60)]);
    let snapshot = ring.snapshot();
    let stages = snapshot
        .events
        .iter()
        .map(|event| (event.stage, event.category))
        .collect::<Vec<_>>();
    for expected in [
        (S::Subscription, C::Requested),
        (S::Subscription, C::Succeeded),
        (S::Start, C::Requested),
        (S::Start, C::Accepted),
        (S::Lifecycle, C::ItemCompleted),
        (S::Warning, C::ThresholdReached),
        (S::Receipt, C::Completed),
        (S::Status, C::Idle),
        (S::Lifecycle, C::Succeeded),
        (S::Cleanup, C::Finished),
    ] {
        assert!(stages.contains(&expected), "missing {expected:?}");
    }
    let warning = snapshot
        .events
        .iter()
        .find(|event| event.stage == S::Warning)
        .unwrap();
    assert_eq!(warning.elapsed_micros, 30_000_000);
    assert_eq!(warning.last_event_age_micros, Some(18_000_000));
    let summary = snapshot.summary.as_ref().unwrap();
    assert_eq!(
        summary.operation.value.as_deref(),
        Some(operation().operation_id.as_str())
    );
    assert_eq!(
        summary.backend_observation_session.value.as_deref(),
        Some(SESSION)
    );
    assert_eq!(summary.turn.value.as_deref(), Some(TURN));
    assert_eq!(
        summary.outcome,
        compaction_diagnostics::CompactionDiagnosticOutcome::Completed
    );
    assert!(!serde_json::to_string(&snapshot).unwrap().contains("C:"));
}

#[test]
fn diagnostics_retain_lost_ack_then_correlated_receipt_acceptance_without_resubmit() {
    use compaction_diagnostics::{
        CompactionDiagnosticCategory as C, CompactionDiagnosticStage as S,
    };
    let mut h = Harness::new();
    let ring = h.capture(5);
    h.backend.start_result = Err(PortError::Unavailable);
    h.backend.receipts.push_back(Ok(receipt(State::Completed)));
    h.run(5);
    let snapshot = ring.snapshot();
    let categories = snapshot
        .events
        .iter()
        .map(|event| (event.stage, event.category))
        .collect::<Vec<_>>();
    assert!(categories.contains(&(S::Start, C::Indeterminate)));
    assert!(categories.contains(&(S::Transport, C::ReconnectRequested)));
    assert!(categories.contains(&(S::Transport, C::Reconnected)));
    assert!(categories.contains(&(S::Receipt, C::Completed)));
    assert!(!categories.contains(&(S::Start, C::Accepted)));
    assert_eq!(h.backend.times("start").len(), 1);
    let summary = snapshot.summary.unwrap();
    assert_eq!(
        summary.acceptance,
        compaction_diagnostics::CompactionDiagnosticAcceptance::Accepted
    );
    assert_eq!(
        summary.operation.value.as_deref(),
        Some(operation().operation_id.as_str())
    );
    assert_eq!(
        summary.backend_observation_session.value.as_deref(),
        Some(SESSION)
    );
    assert_eq!(summary.turn.value.as_deref(), Some(TURN));
}

#[test]
fn diagnostic_cleanup_survives_receiver_drop_at_warning_without_backend_terminal_claim() {
    use compaction_diagnostics::{
        CompactionDiagnosticCategory as C, CompactionDiagnosticStage as S,
    };
    struct DropAtWarning<S> {
        inner: S,
        task: Option<ObserverTask>,
    }
    impl<S: UpdateSink> UpdateSink for DropAtWarning<S> {
        fn publish(&mut self, update: ObserverUpdate, cancellation: &Cancellation) -> bool {
            if update.kind == UpdateKind::Warning {
                self.task.take();
            }
            self.inner.publish(update, cancellation)
        }
    }
    let mut h = Harness::new();
    let ring = h.capture(10);
    let (task, sink, cancellation) = observer_channel();
    let mut sink = DropAtWarning {
        inner: sink,
        task: Some(task),
    };
    run_observer(
        &mut h.backend,
        &mut h.clock,
        &mut sink,
        target(),
        REQUEST_TIMEOUT,
        secs(10),
        &cancellation,
        h.diagnostics.take(),
    );
    let snapshot = ring.snapshot();
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| event.stage == S::Warning)
    );
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| (event.stage, event.category) == (S::Cleanup, C::Cancelled))
    );
    assert!(
        !snapshot
            .events
            .iter()
            .any(|event| (event.stage, event.category) == (S::Lifecycle, C::Succeeded))
    );
    assert_eq!(
        snapshot.summary.unwrap().outcome,
        compaction_diagnostics::CompactionDiagnosticOutcome::Cancelled
    );
    assert_eq!(h.backend.times("start").len(), 1);
    assert!(h.backend.times("read").is_empty());
}

#[test]
fn diagnostic_rejection_omits_backend_message_and_runtime_path() {
    let mut h = Harness::new();
    let ring = h.capture(30);
    h.backend.start_result = Err(PortError::Rejected(
        "secret prompt and C:\\private\\file".into(),
    ));
    h.run(30);
    let snapshot = ring.snapshot();
    let encoded = serde_json::to_string(&snapshot).unwrap();
    assert!(!encoded.contains("secret"));
    assert!(!encoded.contains("private"));
    assert!(!encoded.contains("C:"));
    assert_eq!(
        snapshot.summary.unwrap().outcome,
        compaction_diagnostics::CompactionDiagnosticOutcome::Rejected
    );
}

#[test]
fn diagnostics_do_not_bind_unrelated_receipt_turn_identity() {
    let mut h = Harness::new();
    let ring = h.capture(10);
    let mut unrelated = receipt(State::Completed);
    unrelated.turn_id = Some("unrelated-turn-identity".into());
    h.backend.receipts.push_back(Ok(unrelated));
    h.run(10);
    let snapshot = ring.snapshot();
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("unrelated-turn-identity")
    );
    assert_eq!(snapshot.summary.unwrap().turn.value.as_deref(), Some(TURN));
    assert!(h.outcomes().is_empty());
}

#[test]
fn unknown_start_receipt_has_indeterminate_acceptance() {
    let mut h = Harness::new();
    let ring = h.capture(200);
    h.backend.start_result = Ok(receipt(State::Unknown {
        reason: CompactionUnknownReason::AbsentOrExpired,
    }));
    h.run(200);
    assert_eq!(
        ring.snapshot().summary.unwrap().acceptance,
        compaction_diagnostics::CompactionDiagnosticAcceptance::Indeterminate
    );
    assert_eq!(h.backend.times("start").len(), 1);
}

#[test]
fn prepare_rejection_never_captures_unvalidated_selected_thread_content() {
    let mut h = Harness::new();
    let ring = h.capture(30);
    h.backend.prepare_error = Some(PortError::Rejected("invalid thread identity".into()));
    let mut invalid_target = target();
    invalid_target.thread_id = "C:\\private\\selected-thread-content".into();
    run_observer(
        &mut h.backend,
        &mut h.clock,
        &mut h.sink,
        invalid_target,
        REQUEST_TIMEOUT,
        secs(30),
        &h.cancel,
        h.diagnostics.take(),
    );
    let snapshot = ring.snapshot();
    assert_eq!(
        snapshot.summary.as_ref().unwrap().thread.validity,
        compaction_diagnostics::CompactionDiagnosticIdentityValidity::Missing
    );
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("private")
    );
    assert!(h.backend.times("start").is_empty());
}

#[test]
fn slow_work_warns_once_and_late_receipt_releases_only_one_success() {
    let mut h = Harness::new();
    h.backend.receipts = [Ok(receipt(State::Running)), Ok(receipt(State::Completed))].into();
    h.run(10);
    assert_eq!(h.warnings(), 1);
    assert_eq!(h.outcomes(), vec![&Outcome::Succeeded]);
    assert_eq!(h.backend.times("read"), vec![secs(10), secs(40)]);
    assert_eq!(h.backend.times("status"), vec![secs(10), secs(40)]);
    assert_eq!(h.backend.times("start"), vec![secs(0)]);
    assert_eq!(h.backend.times("subscribe"), vec![secs(0)]);
}

#[test]
fn lost_ack_and_all_events_recover_original_operation_without_resubmit() {
    let mut h = Harness::new();
    h.backend.start_result = Err(PortError::Unavailable);
    h.backend.receipts.push_back(Ok(receipt(State::Completed)));
    h.run(5);
    assert_eq!(h.outcomes(), vec![&Outcome::Succeeded]);
    assert_eq!(h.backend.times("connect"), vec![secs(0), secs(30)]);
    assert_eq!(h.backend.times("start").len(), 1);
    assert_eq!(h.backend.times("prepare").len(), 1);
    assert!(
        h.sink
            .updates
            .iter()
            .all(|update| update.operation.as_ref() == Some(&operation()))
    );
    assert!(h.sink.updates.iter().any(
        |update| matches!(&update.kind, UpdateKind::TurnKnown { turn_id } if turn_id == TURN)
    ));
}

#[test]
fn idle_before_terminal_requires_one_fresh_read_and_no_early_receipt_poll() {
    let mut h = Harness::new();
    h.backend.event(1, item_done());
    h.backend.event(2, idle());
    h.backend.event(3, terminal(TurnStatus::Completed));
    h.run(100);
    assert_eq!(h.outcomes(), vec![&Outcome::Succeeded]);
    assert_eq!(h.backend.times("status"), vec![secs(3)]);
    assert!(h.backend.times("read").is_empty());
    assert_eq!(h.warnings(), 0);
}

#[test]
fn terminal_before_item_needs_both_and_does_not_reuse_idle() {
    let mut h = Harness::new();
    h.backend.event(1, terminal(TurnStatus::Completed));
    h.backend.event(2, idle());
    h.backend.event(3, item_done());
    h.backend.statuses.push_back(Ok((
        THREAD.into(),
        ThreadStatus::Active {
            active_flags: vec![],
        },
    )));
    h.backend.event(4, idle());
    h.run(100);
    assert_eq!(h.backend.times("status"), vec![secs(3), secs(100)]);
    assert_eq!(h.outcomes(), vec![&Outcome::Succeeded]);
    assert_eq!(h.clock.now(), secs(100));
}

#[test]
fn completed_turn_without_completed_item_stays_unconfirmed() {
    let mut h = Harness::new();
    h.backend.event(1, terminal(TurnStatus::Completed));
    h.backend.event(2, idle());
    h.run(200);
    assert!(h.outcomes().is_empty());
    assert!(h.backend.times("status").is_empty());
}

#[test]
fn unrelated_thread_turn_and_compaction_items_cannot_establish_stop_identity() {
    let mut h = Harness::new();
    h.backend.start_result = Err(PortError::Unavailable);
    h.backend.receipts = (0..4)
        .map(|_| {
            Ok(receipt(State::Unknown {
                reason: CompactionUnknownReason::AbsentOrExpired,
            }))
        })
        .collect();
    h.backend.event(31, item_done());
    h.backend.event(32, terminal(TurnStatus::Completed));
    h.backend.event(33, idle());
    h.run(10);
    assert!(h.outcomes().is_empty());
    assert!(
        !h.sink
            .updates
            .iter()
            .any(|update| matches!(update.kind, UpdateKind::TurnKnown { .. }))
    );
}

#[test]
fn mismatched_receipt_operation_or_known_turn_cannot_finish() {
    let mut h = Harness::new();
    let mut wrong_op = receipt(State::Completed);
    wrong_op.operation.operation_id = "other".into();
    let mut wrong_turn = receipt(State::Completed);
    wrong_turn.turn_id = Some("other".into());
    h.backend.receipts = [Ok(wrong_op), Ok(wrong_turn)].into();
    h.run(10);
    assert!(h.outcomes().is_empty());
    assert!(h.uncertainty(UnconfirmedReason::InvalidEvidence));
}

#[test]
fn wrong_thread_status_cannot_supply_idle_confirmation() {
    let mut h = Harness::new();
    h.backend.event(1, item_done());
    h.backend.event(2, terminal(TurnStatus::Completed));
    h.backend
        .statuses
        .push_back(Ok(("other-thread".into(), ThreadStatus::Idle)));
    h.run(200);
    assert!(h.outcomes().is_empty());
    assert_eq!(h.backend.times("status"), vec![secs(2)]);
}

#[test]
fn retryable_errors_do_not_terminate_but_exact_failed_terminal_does() {
    let mut h = Harness::new();
    h.backend.event(
        1,
        TurnStreamEvent::TurnError {
            thread_id: THREAD.into(),
            turn_id: TURN.into(),
            error: TurnError {
                message: "retry".into(),
                additional_details: None,
                codex_error_info: None,
            },
            will_retry: true,
        },
    );
    h.backend.event(2, terminal(TurnStatus::Failed));
    h.run(100);
    assert_eq!(
        h.outcomes(),
        vec![&Outcome::Failed {
            message: "Backend compaction failed.".into()
        }]
    );
    assert!(
        h.sink
            .updates
            .iter()
            .any(|update| update.kind == UpdateKind::Activity(Activity::Retrying))
    );
    assert!(h.backend.times("status").is_empty());
}

#[test]
fn interrupted_receipt_is_distinct_and_does_not_claim_idle() {
    let mut h = Harness::new();
    h.backend.receipts.push_back(Ok(receipt(State::Interrupted {
        reason: CompactionInterruptedReason::Interrupted,
    })));
    h.run(10);
    assert_eq!(h.outcomes(), vec![&Outcome::Interrupted]);
    assert!(h.backend.times("status").is_empty());
}

#[test]
fn disconnect_resubscribes_at_thirty_seconds_and_preserves_read_budget() {
    let mut h = Harness::new();
    h.backend
        .events
        .push_back((secs(1), Err(PortError::Unavailable)));
    h.run(10);
    assert_eq!(h.backend.times("connect"), vec![secs(0), secs(31)]);
    assert_eq!(h.backend.times("subscribe"), vec![secs(0), secs(31)]);
    assert_eq!(
        h.backend.times("read"),
        vec![secs(31), secs(61), secs(91), secs(121)]
    );
    assert_eq!(h.backend.times("start").len(), 1);
    assert_eq!(h.warnings(), 1);
}

#[test]
fn read_failure_never_confirms_success_and_reconnects_without_duplicate_mutation() {
    let mut h = Harness::new();
    h.backend.receipts.push_back(Err(PortError::Unavailable));
    h.run(10);
    assert!(h.outcomes().is_empty());
    assert_eq!(h.backend.times("connect"), vec![secs(0), secs(40)]);
    assert_eq!(
        h.backend.times("status"),
        vec![secs(40), secs(70), secs(100)]
    );
    assert_eq!(h.backend.times("start").len(), 1);
}

#[test]
fn reconnect_to_new_process_never_resumes_or_adopts_it() {
    let mut h = Harness::new();
    h.backend.connect_results = [Ok(SESSION.into()), Ok("new-runtime".into())].into();
    h.backend
        .events
        .push_back((secs(1), Err(PortError::Unavailable)));
    h.run(10);
    assert!(h.outcomes().is_empty());
    assert_eq!(h.backend.times("connect").len(), 2);
    assert_eq!(h.backend.times("subscribe").len(), 1);
    assert!(h.backend.times("read").is_empty());
    assert!(h.uncertainty(UnconfirmedReason::Receipt(
        CompactionUnknownReason::RuntimeChange
    )));
}

#[test]
fn unknown_receipts_never_become_success_and_retention_does_not_accumulate_history() {
    let mut h = Harness::new();
    h.backend.receipts = (0..4)
        .map(|_| {
            Ok(receipt(State::Unknown {
                reason: CompactionUnknownReason::ObservationGap,
            }))
        })
        .collect();
    h.run(10);
    assert!(h.outcomes().is_empty());
    assert_eq!(h.warnings(), 1);
    assert!(h.sink.updates.len() <= 4);
    assert_eq!(
        h.backend.times("read"),
        vec![secs(10), secs(40), secs(70), secs(100)]
    );
}

#[test]
fn target_generation_workspace_and_execution_identity_gate_updates() {
    let mut h = Harness::new();
    h.sink.cancel_after = Some(1);
    h.run(10);
    let update = &h.sink.updates[0];
    assert!(update.belongs_to(&target()));
    let mut stale = target();
    stale.generation += 1;
    assert!(!update.belongs_to(&stale));
    stale = target();
    stale.workspace_id = "other".into();
    assert!(!update.belongs_to(&stale));
    stale = target();
    stale.execution_target = WorkspaceId::host_windows("C:\\other");
    assert!(!update.belongs_to(&stale));
    assert!(h.backend.times("start").is_empty());
}

#[test]
fn cancellation_after_start_request_emits_no_backend_outcome_or_later_reads() {
    let mut h = Harness::new();
    h.backend.cancel_on = Some(("start", h.cancel.clone()));
    h.run(1);
    assert_eq!(h.backend.times("start").len(), 1);
    assert!(h.outcomes().is_empty());
    assert!(h.backend.times("read").is_empty());
    assert_eq!(h.backend.log.borrow().last().unwrap().0, "disconnect");
}

#[test]
fn cancellation_after_status_request_does_not_publish_success() {
    let mut h = Harness::new();
    h.backend.receipts.push_back(Ok(receipt(State::Completed)));
    h.backend.cancel_on = Some(("status", h.cancel.clone()));
    h.run(1);
    assert!(h.outcomes().is_empty());
    assert_eq!(h.backend.times("status").len(), 1);
}

#[test]
fn acknowledgement_warning_anchor_differs_from_indeterminate_dispatch_anchor() {
    let mut h = Harness::new();
    h.backend.start_duration = secs(2);
    h.run(10);
    assert_eq!(h.backend.times("read")[0], secs(12));
    let mut h = Harness::new();
    h.backend.start_duration = secs(2);
    h.backend.start_result = Err(PortError::Unavailable);
    h.run(10);
    assert_eq!(h.warnings(), 1);
    assert_eq!(h.backend.times("connect"), vec![secs(0), secs(32)]);
}

#[test]
fn production_start_error_classification_is_conservative() {
    assert_eq!(
        classify_start_error(ManagedBackendError::RequestFailed {
            method: "thread/compact/start".into(),
            error: JsonRpcError {
                code: -32602,
                message: "rejected".into(),
                data: None
            }
        }),
        PortError::Rejected("rejected".into())
    );
    assert_eq!(
        classify_start_error(ManagedBackendError::UnexpectedMessageShape),
        PortError::Unavailable
    );
    assert_eq!(
        classify_start_error(ManagedBackendError::Compaction(
            beryl_backend::CompactionError::IdentityMismatch { field: "turnId" }
        )),
        PortError::Unavailable
    );
}

#[test]
fn buffered_idle_after_completed_receipt_and_active_status_is_not_fresh_evidence() {
    let mut h = Harness::new();
    h.backend.receipts.push_back(Ok(receipt(State::Completed)));
    h.backend.statuses = (0..4)
        .map(|_| {
            Ok((
                THREAD.into(),
                ThreadStatus::Active {
                    active_flags: vec![],
                },
            ))
        })
        .collect();
    h.backend.event(11, idle());
    h.run(10);
    assert!(h.outcomes().is_empty());
    assert_eq!(
        h.backend.times("status"),
        vec![secs(10), secs(40), secs(70), secs(100)]
    );
}

#[test]
fn early_failed_idle_read_cannot_create_extra_reads_before_warning() {
    let mut h = Harness::new();
    h.backend.event(1, item_done());
    h.backend.event(2, terminal(TurnStatus::Completed));
    h.backend.statuses.push_back(Err(PortError::Unavailable));
    h.run(100);
    assert_eq!(h.backend.times("status"), vec![secs(2), secs(100)]);
    assert_eq!(h.backend.times("read"), vec![secs(100)]);
    assert_eq!(h.outcomes(), vec![&Outcome::Succeeded]);
}

#[test]
fn terminal_arriving_after_scheduled_read_shares_thirty_second_budget() {
    let mut h = Harness::new();
    h.backend.event(11, item_done());
    h.backend.event(12, terminal(TurnStatus::Completed));
    h.backend.event(13, idle());
    h.run(10);
    assert_eq!(h.backend.times("read"), vec![secs(10), secs(40)]);
    assert_eq!(h.backend.times("status"), vec![secs(10), secs(40)]);
    assert_eq!(h.clock.now(), secs(40));
}

#[test]
fn wrong_thread_and_turn_terminal_events_are_ignored() {
    let mut h = Harness::new();
    h.backend.event(
        1,
        TurnStreamEvent::ItemCompleted {
            thread_id: "wrong-thread".into(),
            turn_id: TURN.into(),
            item: item(),
        },
    );
    h.backend.event(
        2,
        TurnStreamEvent::TurnCompleted {
            thread_id: THREAD.into(),
            turn: TurnInfo {
                id: "wrong-turn".into(),
                status: TurnStatus::Failed,
                items: vec![item()],
                error: None,
            },
        },
    );
    h.backend.event(3, idle());
    h.run(200);
    assert!(h.outcomes().is_empty());
    assert!(
        !h.sink
            .updates
            .iter()
            .any(|update| matches!(update.kind, UpdateKind::Activity(_)))
    );
}

#[test]
fn stream_interruption_has_no_idle_claim() {
    let mut h = Harness::new();
    h.backend.event(1, terminal(TurnStatus::Interrupted));
    h.run(10);
    assert_eq!(h.outcomes(), vec![&Outcome::Interrupted]);
    assert!(h.backend.times("status").is_empty());
}

#[test]
fn rejection_before_dispatch_is_not_backend_failure() {
    let mut h = Harness::new();
    h.backend
        .connect_results
        .push_back(Err(PortError::Rejected("unsupported capability".into())));
    h.run(10);
    assert_eq!(
        h.outcomes(),
        vec![&Outcome::Rejected {
            message: "unsupported capability".into()
        }]
    );
    assert!(h.backend.times("start").is_empty());
    assert!(h.sink.updates[0].operation.is_none());
}

#[test]
fn definitively_rejected_start_never_reconnects_or_reads() {
    let mut h = Harness::new();
    h.backend.start_result = Err(PortError::Rejected("busy".into()));
    h.run(10);
    assert_eq!(
        h.outcomes(),
        vec![&Outcome::Rejected {
            message: "busy".into()
        }]
    );
    assert_eq!(h.backend.times("start").len(), 1);
    assert!(h.backend.times("read").is_empty());
}

#[test]
fn receipt_runtime_change_permanently_stops_polling_old_operation() {
    let mut h = Harness::new();
    h.backend.receipts.push_back(Ok(receipt(State::Unknown {
        reason: CompactionUnknownReason::RuntimeChange,
    })));
    h.run(10);
    assert!(h.outcomes().is_empty());
    assert_eq!(h.backend.times("read"), vec![secs(10)]);
    assert!(h.backend.times("status").is_empty());
    assert_eq!(h.backend.times("connect").len(), 1);
}

#[test]
fn bounded_channel_retains_only_capacity_and_receiver_drop_wakes_blocked_sender() {
    use std::sync::mpsc::{self, RecvTimeoutError};
    let (task, mut sink, cancel) = observer_channel();
    let update = ObserverUpdate {
        target: target(),
        operation: Some(operation()),
        kind: UpdateKind::Warning,
    };
    for _ in 0..UPDATE_CAPACITY {
        assert!(sink.publish(update.clone(), &cancel));
    }
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        done_tx.send(sink.publish(update, &cancel)).unwrap();
    });
    assert_eq!(
        done_rx.recv_timeout(Duration::from_millis(20)),
        Err(RecvTimeoutError::Timeout)
    );
    drop(task);
    assert!(!done_rx.recv_timeout(secs(2)).unwrap());
    worker.join().unwrap();
}

#[test]
fn channel_owner_cancellation_never_drops_or_fabricates_an_outcome() {
    let (task, mut sink, cancel) = observer_channel();
    let update = ObserverUpdate {
        target: target(),
        operation: Some(operation()),
        kind: UpdateKind::Warning,
    };
    assert!(sink.publish(update.clone(), &cancel));
    assert_eq!(task.try_recv().unwrap(), update);
    task.cancel();
    assert!(!sink.publish(
        ObserverUpdate {
            kind: UpdateKind::Finished(Outcome::Succeeded),
            ..update
        },
        &cancel
    ));
    assert!(task.try_recv().is_err());
}

#[test]
fn exact_turn_usage_is_preserved_without_arbitrary_stream_content() {
    let mut h = Harness::new();
    let usage = beryl_backend::ThreadTokenUsage {
        last: Default::default(),
        total: Default::default(),
        model_context_window: Some(128000),
    };
    h.backend.event(
        1,
        TurnStreamEvent::TokenUsageUpdated {
            thread_id: THREAD.into(),
            turn_id: TURN.into(),
            token_usage: usage.clone(),
        },
    );
    h.backend.event(
        2,
        TurnStreamEvent::TokenUsageUpdated {
            thread_id: THREAD.into(),
            turn_id: "wrong-turn".into(),
            token_usage: usage.clone(),
        },
    );
    h.run(200);
    assert_eq!(
        h.sink
            .updates
            .iter()
            .filter(|update| matches!(update.kind, UpdateKind::TokenUsage { .. }))
            .count(),
        1
    );
    assert!(h.sink.updates.iter().any(|update| update.kind
        == UpdateKind::TokenUsage {
            turn_id: TURN.into(),
            usage: usage.clone()
        }));
}

#[test]
fn early_idle_read_crossing_warning_is_sequential_with_first_scheduled_attempt() {
    let mut h = Harness::new();
    h.backend.event(8, item_done());
    h.backend.event(9, terminal(TurnStatus::Completed));
    h.backend.status_durations.push_back(secs(2));
    h.backend.statuses = (0..6)
        .map(|_| {
            Ok((
                THREAD.into(),
                ThreadStatus::Active {
                    active_flags: vec![],
                },
            ))
        })
        .collect();
    h.run(10);
    let reads = h.backend.times("read");
    assert!(reads[0] >= secs(11) && reads[0] <= secs(11) + Duration::from_millis(250));
    assert!(
        reads
            .windows(2)
            .all(|pair| pair[1] - pair[0] >= RECONCILE_INTERVAL)
    );
    assert_eq!(h.backend.times("status")[0], secs(9));
    assert_eq!(h.backend.times("status").len(), reads.len() + 1);
    assert_eq!(h.warnings(), 1);
    assert!(h.outcomes().is_empty());
}

#[test]
fn production_connector_must_match_the_frozen_execution_target() {
    let expected = WorkspaceId::host_windows("C:\\work\\beryl");
    let launch = beryl_backend::BackendLaunchSpec::managed_stdio(
        beryl_model::workspace::RuntimeMode::HostWindows,
        "C:\\work\\beryl",
    );
    assert!(connector_target_matches(&launch, &expected));
    assert!(!connector_target_matches(
        &launch,
        &WorkspaceId::host_windows("C:\\other")
    ));
}
