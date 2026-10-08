use super::*;

pub(super) fn populate(owner: &Rc<RefCell<RunningProcessOwner>>, count: u8) {
    let process = owner.borrow_mut().test_take_services();
    let process = std::thread::spawn(move || {
        let graph = process.graph().unwrap();
        let home = graph.home();
        let state = graph.state();
        for index in 1..=count {
            let host =
                |path: &str| AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap();
            let native = |path: &str| {
                RuntimeNativePath::from_admitted(RuntimeMode::host(), PathFlavor::Windows, path)
                    .unwrap()
            };
            let available =
                AvailabilitySnapshot::observed(Availability::Available, UnixMillis::new(1))
                    .unwrap();
            let executable = format!(r"C:\Codex\runtime-{index:03}.exe");
            let root = format!(r"C:\Work\root-{index:03}");
            let creation = CreateRuntimeWithHomeRoot::new(
                RuntimeRegistration::new(
                    RuntimeId::from_bytes([index; 16]),
                    host(&executable),
                    RuntimeMode::host(),
                    RuntimeLaunchForm::CodexCli,
                    native(&executable),
                    UnixMillis::new(1),
                    available,
                )
                .unwrap(),
                RootRegistration::new(
                    RootId::from_bytes([index; 16]),
                    native(&root),
                    host(&root),
                    UnixMillis::new(1),
                    available,
                ),
            )
            .unwrap();
            let mut command = HomeCommand::new(home.home_revision().unwrap());
            command
                .add(state.runtime_roots().create_runtime_with_home_root(
                    state.runtime_roots().revision(home).unwrap(),
                    creation,
                ))
                .unwrap();
            assert!(matches!(
                home.execute(command),
                beryl_home_store::CommandOutcome::Committed { .. }
            ));
        }
        process
    })
    .join()
    .unwrap();
    owner.borrow_mut().test_restore_services(process);
}

pub(super) fn settled(
    picker: &gpui::Entity<ThreadRootPicker>,
    total: usize,
    cx: &mut TestAppContext,
) {
    wait(
        cx,
        |cx| {
            cx.update(|app| {
                let picker = picker.read(app);
                let roots = picker.diagnostics();
                roots.total_count == total
                    && roots.pending_page_count == 0
                    && !roots.collection_failed
                    && picker.runtime_diagnostics().is_some_and(|runtime| {
                        runtime.pending_page_count == 0 && !runtime.collection_failed
                    })
            })
        },
        "paired catalog pages did not settle",
    );
}

#[gpui::test]
fn large_setup_catalog_is_coherent_and_bounded_in_both_sections(cx: &mut TestAppContext) {
    let (directory, owner, window, _) = mounted(cx);
    populate(&owner, 72);
    let picker = open(window, cx);
    settled(&picker, 72, cx);
    cx.update(|app| {
        let picker = picker.read(app);
        let roots = picker.diagnostics();
        let runtimes = picker.runtime_diagnostics().unwrap();
        assert_eq!(runtimes.total_count, 72);
        assert!(roots.resident_row_count <= 24 * 32);
        assert!(runtimes.resident_row_count <= 24 * 32);
        assert!(roots.realized_row_count < 72 && runtimes.realized_row_count < 72);
    });
    assert!(
        window
            .read_with(cx, |root, _| {
                let state = root.test_runtime_setup_state();
                state.3 <= 24 * 32 && state.4 <= 24 * 32
            })
            .unwrap()
    );
    close_empty(owner, window, cx);
    drop(directory);
}

struct DeliveryRelease(Option<std::sync::mpsc::Sender<()>>);
impl DeliveryRelease {
    fn release(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}
impl Drop for DeliveryRelease {
    fn drop(&mut self) {
        self.release();
    }
}

#[gpui::test]
fn obsolete_query_delivery_cannot_discard_new_query_staged_root_page(cx: &mut TestAppContext) {
    let (directory, owner, window, _) = mounted(cx);
    populate(&owner, 2);
    let picker = open(window, cx);
    settled(&picker, 2, cx);
    let old_revision = window
        .read_with(cx, |root, _| root.test_runtime_setup_bootstrap().0 + 1)
        .unwrap();
    let new_revision = old_revision + 1;
    let (old_sender, old_receiver) = std::sync::mpsc::channel();
    let (new_sender, new_receiver) = std::sync::mpsc::channel();
    let (reached, reached_receiver) = std::sync::mpsc::channel();
    let mut old_release = DeliveryRelease(Some(old_sender));
    let mut new_release = DeliveryRelease(Some(new_sender));
    let old_receiver = std::sync::Mutex::new(old_receiver);
    let new_receiver = std::sync::Mutex::new(new_receiver);
    let delivery = Arc::new(move |runtime: bool, revision: u64| {
        if !runtime && revision == old_revision {
            reached.send(false).unwrap();
            old_receiver
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .expect("old page delivery release");
        } else if runtime && revision == new_revision {
            reached.send(true).unwrap();
            new_receiver
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .expect("new page delivery release");
        }
    });
    window
        .update(cx, |root, _, _| {
            root.test_runtime_setup_page_delivery(Some(delivery))
        })
        .unwrap();
    let input = cx.update(|app| picker.read(app).search_input());
    input.update(cx, |input, cx| input.replace_selected_text("root-001", cx));
    wait(
        cx,
        |_| matches!(reached_receiver.try_recv(), Ok(false)),
        "old query root delivery was not held",
    );
    input.update(cx, |input, cx| {
        input.select_all_text(cx);
        input.replace_selected_text("root-002", cx);
    });
    wait(
        cx,
        |_| matches!(reached_receiver.try_recv(), Ok(true)),
        "new query runtime delivery was not held",
    );
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| {
                    root.test_runtime_setup_bootstrap() == (new_revision, true, false)
                })
                .unwrap()
        },
        "new query root page was not staged",
    );
    old_release.release();
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| root.test_runtime_setup_page_jobs() == 1)
                .unwrap()
        },
        "obsolete delivery did not leave the current runtime request",
    );
    assert_eq!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_bootstrap())
            .unwrap(),
        (new_revision, true, false)
    );
    new_release.release();
    settled(&picker, 1, cx);
    assert_eq!(
        cx.update(|app| picker.read(app).query_text().to_owned()),
        "root-002"
    );
    close_empty(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn every_scope_transition_and_return_clear_search_in_owner_and_page_source(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, _) = mounted(cx);
    populate(&owner, 2);
    let picker = open(window, cx);
    settled(&picker, 2, cx);
    let input = cx.update(|app| picker.read(app).search_input());
    input.update(cx, |input, cx| input.replace_selected_text("root-002", cx));
    settled(&picker, 1, cx);
    assert_eq!(
        cx.update(|app| picker.read(app).query_text().to_owned()),
        "root-002"
    );
    picker.update(cx, |picker, cx| {
        picker.dispatch_command(
            PickerCommand::BrowseRoots(PickerRowKey(format!(
                "runtime:{}",
                RuntimeId::from_bytes([1; 16])
            ))),
            cx,
        )
    });
    settled(&picker, 1, cx);
    assert!(cx.update(|app| picker.read(app).query_text().is_empty()));
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_query().is_empty())
            .unwrap()
    );
    input.update(cx, |input, cx| input.replace_selected_text("absent", cx));
    settled(&picker, 0, cx);
    picker.update(cx, |picker, cx| {
        picker.dispatch_command(PickerCommand::Return, cx)
    });
    settled(&picker, 2, cx);
    assert!(cx.update(|app| picker.read(app).query_text().is_empty()));
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_query().is_empty())
            .unwrap()
    );
    picker.update(cx, |picker, cx| {
        picker.dispatch_command(
            PickerCommand::BrowseRoots(PickerRowKey(format!(
                "runtime:{}",
                RuntimeId::from_bytes([1; 16])
            ))),
            cx,
        )
    });
    settled(&picker, 1, cx);
    assert!(cx.update(|app| picker.read(app).query_text().is_empty()));
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_query().is_empty())
            .unwrap()
    );
    close_empty(owner, window, cx);
    drop(directory);
}
