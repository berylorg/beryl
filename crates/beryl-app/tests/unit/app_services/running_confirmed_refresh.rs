pub(super) async fn exercise(
    owner: Rc<RefCell<RunningProcessOwner>>,
    context: crate::running_owner::ShutdownConfirmationContext,
    cancel: bool,
    cx: &mut AsyncApp,
) -> Rc<RefCell<RunningProcessOwner>> {
    let invoking = context.invoking();
    let intent = context.intent();
    owner
        .borrow_mut()
        .begin_confirmed_shutdown(context)
        .unwrap();
    let work = crate::cas_projection::test_faults::retain_projection_work(
        owner.borrow().test_services().graph().unwrap().cas(),
        beryl_model::SyndicThreadId::from_bytes([234; 16]),
    );
    let token = ProjectionCancellationToken::new();
    let result = Rc::new(RefCell::new(None));
    let delivered = result.clone();
    let gui_thread = std::thread::current().id();
    let (release, hold) = std::sync::mpsc::sync_channel(1);
    cx.update(|app| {
        RunningProcessOwner::test_observe_confirmed_shutdown_with(
            &owner,
            token.clone(),
            app,
            move |owner, result, _| {
                assert_eq!(std::thread::current().id(), gui_thread);
                assert!(delivered.borrow().is_none());
                *delivered.borrow_mut() = Some((owner.clone(), result));
            },
            true,
            || {},
            move |completion| {
                let _ = completion.test_duplicate_success();
                drop(work);
            },
            move || {
                hold.recv_timeout(Duration::from_secs(10)).unwrap();
            },
        )
    })
    .unwrap()
    .unwrap();
    let weak = Rc::downgrade(&owner);
    drop(owner);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let owner = weak.upgrade().expect("refresh retains the complete owner");
        if owner.borrow().test_confirmed_refreshes() > 0 {
            assert!(result.borrow().is_none());
            assert_eq!(
                owner.borrow().shutdown_status(),
                Some((invoking, intent, RunningShutdownStatus::Observing))
            );
            assert!(owner.borrow_mut().end_unadmitted_shutdown().is_err());
            assert!(
                cx.update(|app| RunningProcessOwner::observe_confirmed_shutdown(
                    &owner,
                    ProjectionCancellationToken::new(),
                    app,
                    |_, _, _| panic!("duplicate observation cannot complete"),
                ))
                .unwrap()
                .is_err()
            );
            owner
                .borrow()
                .test_services()
                .process
                .execution_permit()
                .commit(|| ())
                .unwrap();
            if cancel {
                token.cancel();
            }
            release.send(()).unwrap();
            break;
        }
        assert!(Instant::now() < deadline, "refresh was not reserved");
        cx.background_executor()
            .timer(Duration::from_millis(1))
            .await;
    }
    let (owner, result) = loop {
        if let Some(result) = result.borrow_mut().take() {
            break result;
        }
        assert!(Instant::now() < deadline, "refresh did not settle");
        cx.background_executor()
            .timer(Duration::from_millis(1))
            .await;
    };
    if cancel {
        assert!(matches!(result, Ok(ConfirmedShutdownAdmission::Cancelled)));
        assert_eq!(
            owner.borrow().shutdown_status(),
            Some((invoking, intent, RunningShutdownStatus::AwaitingObservation))
        );
        owner.borrow_mut().end_unadmitted_shutdown().unwrap();
    } else {
        assert!(
            matches!(result, Ok(ConfirmedShutdownAdmission::Admitted)),
            "{result:?}"
        );
        assert_eq!(
            owner.borrow().shutdown_status(),
            Some((invoking, intent, RunningShutdownStatus::Admitted))
        );
    }
    owner
}
