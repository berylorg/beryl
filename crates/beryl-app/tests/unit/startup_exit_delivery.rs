use super::*;
use std::{
    future::Future,
    sync::Arc,
    task::{Context, Wake},
};

fn producer() -> StartupCommands {
    StartupCommands(Rc::new(RefCell::new(Commands {
        stage: Stage::Waiting,
        exit: false,
        retry: None,
        wake: None,
        active_exit: None,
    })))
}

fn handoff(commands: &StartupCommands) -> RunningExitCommands {
    commands.0.borrow_mut().stage = Stage::Running;
    RunningExitCommands::new(commands.clone())
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
