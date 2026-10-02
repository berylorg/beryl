use super::*;
use std::{
    future::Future,
    sync::Arc,
    task::{Context, Wake},
};

mod recovery {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/unit/startup_exit_recovery.rs"
    ));
}

fn producer() -> StartupCommands {
    StartupCommands(Rc::new(RefCell::new(Commands {
        stage: Stage::Waiting,
        exit: false,
        process_exit: false,
        exit_window: None,
        ordinary_close: false,
        retry: None,
        wake: None,
        active_exit: None,
        exit_gates: Default::default(),
    })))
}

fn handoff(commands: &StartupCommands) -> RunningExitCommands {
    commands.0.borrow_mut().stage = Stage::Running;
    RunningExitCommands::new(commands.clone())
}

#[test]
fn process_exit_waits_for_gate_then_delivers_once() {
    let producer = producer();
    let mut running = handoff(&producer);
    assert!(!producer.diagnostic_exit_pending());
    running.set_gate(RunningExitGate::SettingsReconciliation, true);
    producer.request_process_exit();
    producer.request_process_exit();
    assert!(producer.0.borrow().process_exit);
    assert!(!running.exit_requested());
    assert!(producer.diagnostic_exit_pending());
    running.set_gate(RunningExitGate::SettingsReconciliation, false);
    let request = take(&mut running);
    assert!(!request.is_ordinary_close());
    assert!(!producer.0.borrow().process_exit);
    assert!(!producer.exit_requested());
    assert!(producer.diagnostic_exit_pending());
    assert!(running.finish_exit(&request));
    assert!(!running.exit_requested());
    assert!(!producer.diagnostic_exit_pending());
}

#[test]
fn process_exit_defers_to_exact_active_close_without_replacing_it() {
    let producer = producer();
    let mut running = handoff(&producer);
    running
        .window_command(WindowId::from_bytes([31; 16]))
        .request_close();
    let original = take(&mut running);
    producer.request_process_exit();
    assert!(running.is_active(&original));
    assert!(original.is_ordinary_close());
    assert!(running.finish_exit(&original));
    let exit = take(&mut running);
    assert!(!exit.is_ordinary_close());
    assert!(!Rc::ptr_eq(&exit.identity(), &original.identity()));
    producer.request_process_exit();
    assert!(running.finish_exit(&exit));
    assert!(!running.exit_requested());
}

fn take(commands: &mut RunningExitCommands) -> RunningExitRequest {
    let mut next = std::pin::pin!(commands.next_exit());
    match next.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(request) => request,
        Poll::Pending => panic!("expected pending Exit intent"),
    }
}

#[test]
fn pending_exit_transfers_once_and_duplicates_do_not_replay_after_cancellation() {
    let producer = producer();
    producer.request_exit();
    let mut running = handoff(&producer);
    let request = take(&mut running);
    producer.request_exit();
    producer.clone().request_exit();
    assert!(running.finish_exit(&request));
    assert!(!running.exit_requested());
    assert!(!running.finish_exit(&request));
    producer.request_exit();
    let successor = take(&mut running);
    assert!(!running.finish_exit(&request));
    assert!(running.exit_requested());
    assert!(running.finish_exit(&successor));
}

#[test]
fn foreign_completion_cannot_end_an_active_request() {
    let first = producer();
    let second = producer();
    let mut running = handoff(&first);
    let mut other = handoff(&second);
    first.request_exit();
    second.request_exit();
    let request = take(&mut running);
    let foreign = take(&mut other);
    assert!(!running.finish_exit(&foreign));
    assert!(running.exit_requested());
    assert!(running.finish_exit(&request));
    assert!(other.finish_exit(&foreign));
}

#[test]
fn first_window_origin_survives_pending_and_active_duplicates() {
    let producer = producer();
    let mut running = handoff(&producer);
    let first = WindowId::from_bytes([31; 16]);
    let second = WindowId::from_bytes([32; 16]);
    let first_command = running.window_command(first);
    let second_command = running.window_command(second);
    first_command.request_exit();
    second_command.request_exit();
    producer.request_exit();
    let mut request = take(&mut running);
    assert_eq!(
        running.bind_invoking_window(&mut request, Some(second)),
        Some(first)
    );
    second_command.clone().request_exit();
    assert!(running.finish_exit(&request));
    assert!(!running.exit_requested());
    assert_eq!(
        running.bind_invoking_window(&mut request, Some(second)),
        None
    );
    second_command.request_exit();
    let mut successor = take(&mut running);
    assert_eq!(
        running.bind_invoking_window(&mut successor, Some(first)),
        Some(second)
    );
    assert_eq!(
        running.bind_invoking_window(&mut request, Some(first)),
        None
    );
    assert!(running.finish_exit(&successor));
}

#[test]
fn deferred_startup_origin_binds_once_and_foreign_requests_cannot_bind() {
    let producer = producer();
    producer.request_exit();
    let mut running = handoff(&producer);
    let first = WindowId::from_bytes([33; 16]);
    let second = WindowId::from_bytes([34; 16]);
    running.window_command(second).request_exit();
    let mut request = take(&mut running);
    let foreign_producer = self::producer();
    let foreign = handoff(&foreign_producer);
    assert_eq!(
        foreign.bind_invoking_window(&mut request, Some(second)),
        None
    );
    assert_eq!(request.invoking, None);
    assert_eq!(running.bind_invoking_window(&mut request, None), None);
    assert_eq!(
        running.bind_invoking_window(&mut request, Some(first)),
        Some(first)
    );
    assert_eq!(
        running.bind_invoking_window(&mut request, Some(second)),
        Some(first)
    );
    assert!(running.finish_exit(&request));
}

thread_local! {
    static REENTRANT: RefCell<Option<StartupCommands>> = const { RefCell::new(None) };
}

struct ObserveWake(std::sync::atomic::AtomicUsize);

impl Wake for ObserveWake {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        REENTRANT.with(|slot| {
            assert!(slot.borrow().as_ref().unwrap().exit_requested());
        });
    }
}

#[test]
fn native_cancellation_wakes_outside_command_borrow() {
    let producer = producer();
    let observer = Arc::new(ObserveWake(std::sync::atomic::AtomicUsize::new(0)));
    let wake = Waker::from(observer.clone());
    let cancellation = MainWindowNativeRestoreSetCancellation::test_with_waiter(&wake);
    producer.0.borrow_mut().stage = Stage::Native(cancellation.clone());
    REENTRANT.with(|slot| *slot.borrow_mut() = Some(producer.clone()));
    producer.request_exit();
    assert!(cancellation.is_cancelled());
    assert_eq!(observer.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    REENTRANT.with(|slot| *slot.borrow_mut() = None);
}

#[test]
fn deferred_exit_wakes_outside_borrow_and_survives_cancelled_wait() {
    let producer = producer();
    let mut running = handoff(&producer);
    let observer = Arc::new(ObserveWake(std::sync::atomic::AtomicUsize::new(0)));
    let wake = Waker::from(observer.clone());
    REENTRANT.with(|slot| *slot.borrow_mut() = Some(producer.clone()));
    {
        let mut next = std::pin::pin!(running.next_exit());
        assert!(
            next.as_mut()
                .poll(&mut Context::from_waker(&wake))
                .is_pending()
        );
    }
    producer.request_exit();
    assert_eq!(observer.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    let request = take(&mut running);
    assert!(running.finish_exit(&request));
    assert!(!running.exit_requested());
    REENTRANT.with(|slot| *slot.borrow_mut() = None);
}

#[test]
fn running_gates_share_precedence_and_clear_independently() {
    let producer = producer();
    let mut running = handoff(&producer);
    let command = running.window_command(WindowId::from_bytes([35; 16]));
    let retained = command.clone();
    for mask in 0..8 {
        running.set_gate(RunningExitGate::Unavailable, mask & 1 != 0);
        running.set_gate(RunningExitGate::SettingsReconciliation, mask & 2 != 0);
        running.set_gate(RunningExitGate::HomeUnavailable, mask & 4 != 0);
        let expected = if mask & 4 != 0 {
            Some(
                "The Beryl home store is unavailable. See the Beryl-home failure notice for automatic recovery.",
            )
        } else if mask & 2 != 0 {
            Some("Application Exit is waiting for Settings reconciliation.")
        } else if mask & 1 != 0 {
            Some("Application Exit is not available.")
        } else {
            None
        };
        assert_eq!(command.disabled_reason(), expected);
        assert_eq!(retained.disabled_reason(), expected);
        retained.request_exit();
        assert_eq!(running.exit_requested(), mask == 0);
        if mask == 0 {
            let request = take(&mut running);
            assert!(running.finish_exit(&request));
        }
    }
    running.set_gate(RunningExitGate::HomeUnavailable, false);
    running.set_gate(RunningExitGate::HomeUnavailable, false);
    assert_eq!(
        retained.disabled_reason(),
        Some("Application Exit is waiting for Settings reconciliation.")
    );
    assert!(!running.exit_requested());
}

#[test]
fn disabled_activations_and_reopening_neither_queue_nor_wake() {
    let producer = producer();
    let mut running = handoff(&producer);
    let command = running.window_command(WindowId::from_bytes([36; 16]));
    let observer = Arc::new(ObserveWake(std::sync::atomic::AtomicUsize::new(0)));
    let wake = Waker::from(observer.clone());
    REENTRANT.with(|slot| *slot.borrow_mut() = Some(producer.clone()));
    assert!(
        running
            .poll_exit(&mut Context::from_waker(&wake))
            .is_pending()
    );
    running.set_gate(RunningExitGate::SettingsReconciliation, true);
    command.request_exit();
    producer.request_exit();
    producer.clone().request_exit();
    assert!(!running.exit_requested());
    assert!(producer.0.borrow().exit_window.is_none());
    assert_eq!(observer.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    running.set_gate(RunningExitGate::SettingsReconciliation, false);
    assert!(!running.exit_requested());
    assert_eq!(observer.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(
        running
            .poll_exit(&mut Context::from_waker(&wake))
            .is_pending()
    );
    command.request_exit();
    assert_eq!(observer.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    let request = take(&mut running);
    assert_eq!(request.invoking_window(), Some(command.invoking));
    assert!(running.finish_exit(&request));
    REENTRANT.with(|slot| *slot.borrow_mut() = None);
}

#[test]
fn gates_preserve_pending_origin_and_active_request_until_exact_completion() {
    let producer = producer();
    let mut running = handoff(&producer);
    let first = running.window_command(WindowId::from_bytes([37; 16]));
    let second = running.window_command(WindowId::from_bytes([38; 16]));
    first.request_exit();
    running.set_gate(RunningExitGate::HomeUnavailable, true);
    second.request_exit();
    producer.request_exit();
    let request = take(&mut running);
    assert_eq!(request.invoking_window(), Some(first.invoking));
    running.set_gate(RunningExitGate::HomeUnavailable, false);
    second.request_exit();
    running.set_gate(RunningExitGate::Unavailable, true);
    assert!(running.finish_exit(&request));
    second.request_exit();
    assert!(!running.exit_requested());
    running.set_gate(RunningExitGate::Unavailable, false);
    assert!(!running.exit_requested());
    second.request_exit();
    let successor = take(&mut running);
    assert_eq!(successor.invoking_window(), Some(second.invoking));
    assert!(!running.finish_exit(&request));
    assert!(running.finish_exit(&successor));
}

#[test]
fn running_gates_do_not_suppress_startup_cancellation_or_accepted_handoff() {
    let producer = producer();
    producer
        .0
        .borrow_mut()
        .exit_gates
        .set(RunningExitGate::Unavailable, true);
    let cancellation = MainWindowNativeRestoreSetCancellation::test_with_waiter(Waker::noop());
    producer.0.borrow_mut().stage = Stage::Native(cancellation.clone());
    producer.request_exit();
    assert!(cancellation.is_cancelled());
    let mut running = handoff(&producer);
    let command = running.window_command(WindowId::from_bytes([39; 16]));
    command.request_exit();
    let request = take(&mut running);
    assert_eq!(request.invoking_window(), None);
    assert!(running.finish_exit(&request));
    producer.request_exit();
    assert!(!running.exit_requested());
    running.set_gate(RunningExitGate::Unavailable, false);
    assert!(!running.exit_requested());
    producer.request_exit();
    let request = take(&mut running);
    assert_eq!(request.invoking_window(), None);
    assert!(running.finish_exit(&request));
}

#[test]
fn bound_home_failure_and_recovery_cannot_revive_retained_exit_producers() {
    use beryl_home_store::{
        HomeHealthState, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
        test_faults::{FaultController, FaultPoint},
    };
    let directory = tempfile::tempdir().unwrap();
    let faults = FaultController::new();
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(directory.path(), HomeSchemaVersion::CURRENT),
        faults.clone(),
    )
    .unwrap();
    let _state = beryl_state::BerylState::register(&mut candidate).unwrap();
    let home = candidate
        .prepare_publication(beryl_state::BerylState::required_domains().unwrap())
        .unwrap()
        .publish()
        .unwrap();
    let producer = producer();
    let mut running = handoff(&producer);
    running.bind_home(home.service_reference());
    let command = running.window_command(WindowId::from_bytes([40; 16]));
    let retained = command.clone();
    assert_eq!(retained.disabled_reason(), None);
    command.request_exit();
    faults.fail_next(FaultPoint::BeforeReadConfirmation);
    assert!(home.home_revision().is_err());
    assert_eq!(home.health().state(), HomeHealthState::Failed);
    running.set_gate(RunningExitGate::Unavailable, true);
    running.set_gate(RunningExitGate::SettingsReconciliation, true);
    assert!(
        retained
            .disabled_reason()
            .unwrap()
            .contains("Beryl-home failure notice")
    );
    running.set_gate(RunningExitGate::HomeUnavailable, false);
    running.set_gate(RunningExitGate::SettingsReconciliation, false);
    running.set_gate(RunningExitGate::Unavailable, false);
    producer.request_exit();
    let request = take(&mut running);
    assert_eq!(request.invoking_window(), Some(command.invoking));
    assert!(running.finish_exit(&request));
    let observer = Arc::new(ObserveWake(std::sync::atomic::AtomicUsize::new(0)));
    let wake = Waker::from(observer.clone());
    assert!(
        running
            .poll_exit(&mut Context::from_waker(&wake))
            .is_pending()
    );
    retained.request_exit();
    producer.request_exit();
    assert!(!running.exit_requested());
    assert!(producer.0.borrow().exit_window.is_none());
    assert_eq!(observer.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    let candidate = home.recover_same_home().unwrap();
    assert!(retained.disabled_reason().is_some());
    retained.request_exit();
    assert!(!running.exit_requested());
    let home = candidate.publish().unwrap();
    assert_eq!(home.health().state(), HomeHealthState::Healthy);
    assert!(retained.disabled_reason().is_some());
    retained.request_exit();
    producer.request_exit();
    assert!(!running.exit_requested());
    assert_eq!(observer.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    let fresh_producer = self::producer();
    let mut fresh = handoff(&fresh_producer);
    fresh.bind_home(home.service_reference());
    let fresh_command = fresh.window_command(command.invoking);
    assert_eq!(fresh_command.disabled_reason(), None);
    fresh_command.request_exit();
    let request = take(&mut fresh);
    assert!(fresh.finish_exit(&request));
    home.close().unwrap();
    assert!(fresh_command.disabled_reason().is_some());
    fresh_command.request_exit();
    assert!(!fresh.exit_requested());
}
