use crate::exit_session::{ExitSessionExecution, ExitSessionReconciled};
use crate::running_owner::RunningShutdownSession;

#[test]
fn native_exit_session_reconciliation_retains_failure_and_exact_resolution() {
    run_with_reconciliation(CaptureCase::Success, None, Some(false));
}

#[test]
fn native_exit_session_reconciliation_unwind_keeps_custody_fenced() {
    run_with_reconciliation(CaptureCase::Success, None, Some(true));
}

pub(super) async fn exercise(
    owner: Rc<RefCell<RunningProcessOwner>>,
    faults: FaultController,
    unwind: bool,
    cx: &mut AsyncApp,
) -> Rc<RefCell<RunningProcessOwner>> {
    cx.update(|app| {
        assert!(owner.borrow().require_shutdown_session_ready().is_err());
        assert!(
            RunningProcessOwner::reconcile_shutdown_session(&owner, app, |_, _| panic!(
                "no session outcome"
            ))
            .is_err()
        );
    })
    .unwrap();
    faults.fail_next(FaultPoint::AfterCommitBeforePersist);
    let (sender, receiver) = futures_channel::oneshot::channel();
    cx.update(|app| {
        RunningProcessOwner::publish_shutdown_session(&owner, app, move |owner, _| {
            assert!(owner.borrow().require_shutdown_session_ready().is_err());
            assert!(matches!(
                owner.borrow().shutdown_session(),
                Some(RunningShutdownSession::Settled(Ok(
                    ExitSessionExecution::Indeterminate(_)
                )))
            ));
            sender.send(()).unwrap();
        })
    })
    .unwrap()
    .unwrap();
    receiver.await.unwrap();
    let (home, handle) = {
        let owner = owner.borrow();
        let home = owner.test_services().graph().unwrap().home();
        let mut handles = home.pending_reconciliations();
        assert_eq!(handles.len(), 1);
        (home.service_reference(), handles.pop().unwrap())
    };
    faults.fail_next(FaultPoint::BeforeReconciliationSnapshot);
    let owner = pass(owner, unwind, cx).await;
    assert_eq!(home.pending_reconciliations().len(), 1);
    if unwind {
        assert!(matches!(
            owner.borrow().shutdown_session(),
            Some(RunningShutdownSession::Unwound)
        ));
    } else {
        let first_failure = pending_failure(&owner);
        let owner = pass(owner, false, cx).await;
        assert_eq!(pending_failure(&owner), first_failure);
        cx.background_executor()
            .spawn(async move {
                assert!(matches!(
                    home.retry_reconciliation(&handle).unwrap(),
                    beryl_home_store::ReconciliationResolution::ExactNew { .. }
                ));
            })
            .await;
        let owner = pass(owner, false, cx).await;
        {
            let borrowed = owner.borrow();
            let graph = borrowed.test_services().graph().unwrap();
            let Some(RunningShutdownSession::Reconciled(ExitSessionReconciled::ExactNew {
                receipt,
                original_failure,
            })) = borrowed.shutdown_session()
            else {
                panic!("expected exact new")
            };
            assert!(!original_failure.to_string().is_empty());
            assert!(
                graph
                    .state()
                    .session()
                    .committed_revision(graph.home(), receipt)
                    .unwrap()
                    .is_some()
            );
            assert!(graph.home().pending_reconciliations().is_empty());
            assert!(borrowed.require_shutdown_session_ready().is_ok());
        }
        assert_terminal(&owner, cx);
        return owner;
    }
    assert_terminal(&owner, cx);
    // Release the test fixture through the store's exact operation after proving the app stays fenced.
    cx.background_executor()
        .spawn(async move {
            assert!(home.retry_reconciliation(&handle).is_err());
            assert!(matches!(
                home.retry_reconciliation(&handle).unwrap(),
                beryl_home_store::ReconciliationResolution::ExactNew { .. }
            ));
        })
        .await;
    owner
}

fn pending_failure(owner: &Rc<RefCell<RunningProcessOwner>>) -> String {
    let borrowed = owner.borrow();
    assert!(borrowed.require_shutdown_session_ready().is_err());
    let Some(RunningShutdownSession::Reconciled(ExitSessionReconciled::Pending {
        failure, ..
    })) = borrowed.shutdown_session()
    else {
        panic!("expected retained pass failure")
    };
    failure.to_string()
}

fn assert_terminal(owner: &Rc<RefCell<RunningProcessOwner>>, cx: &mut AsyncApp) {
    cx.update(|app| {
        assert!(
            RunningProcessOwner::reconcile_shutdown_session(owner, app, |_, _| panic!(
                "terminal result cannot repeat"
            ))
            .is_err()
        );
        assert_session_fenced(owner, app);
    })
    .unwrap();
}

async fn pass(
    owner: Rc<RefCell<RunningProcessOwner>>,
    unwind: bool,
    cx: &mut AsyncApp,
) -> Rc<RefCell<RunningProcessOwner>> {
    let original_attempt = owner.borrow().test_services().graph().unwrap().shutdown;
    let placements = owner.borrow().shutdown_placements().unwrap();
    let weak = Rc::downgrade(&owner);
    let (release, parked) = std::sync::mpsc::sync_channel(1);
    let (sender, receiver) = futures_channel::oneshot::channel();
    let gui_thread = std::thread::current().id();
    cx.update(|app| {
        RunningProcessOwner::test_reconcile_shutdown_session(
            &owner,
            app,
            move |owner, app| {
                assert_eq!(std::thread::current().id(), gui_thread);
                assert!(owner.try_borrow_mut().is_ok());
                assert!(!owner.borrow().test_services_on_worker());
                assert_eq!(
                    owner.borrow().test_services().graph().unwrap().shutdown,
                    original_attempt
                );
                assert_eq!(owner.borrow().shutdown_placements().unwrap(), placements);
                assert_session_fenced(owner, app);
                sender.send(owner.clone()).ok().unwrap();
            },
            move || {
                assert_ne!(std::thread::current().id(), gui_thread);
                parked.recv_timeout(Duration::from_secs(10)).unwrap();
                assert!(!unwind, "injected reconciliation worker unwind");
            },
        )
        .unwrap();
        assert!(owner.borrow().test_services_on_worker());
        assert!(owner.borrow().require_shutdown_session_ready().is_err());
        assert!(matches!(
            owner.borrow().shutdown_session(),
            Some(RunningShutdownSession::Reconciling)
        ));
        assert!(
            RunningProcessOwner::reconcile_shutdown_session(&owner, app, |_, _| panic!(
                "duplicate pass"
            ))
            .is_err()
        );
        assert_session_fenced(&owner, app);
    })
    .unwrap();
    drop(owner);
    assert!(weak.upgrade().is_some());
    release.send(()).unwrap();
    receiver.await.unwrap()
}
