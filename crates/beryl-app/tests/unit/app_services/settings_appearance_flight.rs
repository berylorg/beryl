use super::*;
use crate::main_window::{
    MainWindowConversationComposer, MainWindowCreationOwner, MainWindowShellRoot,
};
use beryl_model::WindowId;
use beryl_state::MinimalSessionBootstrap;
use gpui::AppContext;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        UI::{
            Controls::TDM_CLICK_BUTTON,
            WindowsAndMessaging::{FindWindowW, IsWindow, IsWindowVisible, PostMessageW, WM_CLOSE},
        },
    },
    core::PCWSTR,
};

#[allow(dead_code)]
#[path = "ordinary_commands_support.rs"]
mod mounted_support;
#[allow(dead_code)]
#[path = "running_resident_recovery_support.rs"]
mod resident_fixture;
use mounted_support::*;

#[test]
fn native_ready_new_window_preserves_original_identity_until_appearance_returns() {
    ready_new_window_during_appearance(false);
}

#[test]
fn native_ready_new_window_cancelled_during_appearance_retires_original_without_publication() {
    ready_new_window_during_appearance(true);
}

fn ready_new_window_during_appearance(cancel: bool) {
    let ledger = std::env::var_os("BERYL_NATIVE_RETAINED_FIXTURE_LEDGER")
        .expect("native appearance fixtures require their retained resource ledger");
    assert!(std::path::Path::new(&ledger).is_absolute());
    run_mounted_with_faults(
        1,
        0,
        move |owner, faults, cx| {
            Box::pin(async move {
                wait_for_services(&owner, cx).await;
                let source = windows(&owner)[0];
                let original = snapshot(&owner);
                let (registry, home_id, generation, path) = {
                    let retained = owner.borrow();
                    let services = retained.test_services();
                    let home = services.graph().unwrap().home();
                    (
                        services.windows.clone(),
                        home.home_id(),
                        home.health().generation().unwrap(),
                        home.canonical_path()
                            .join("themes/installed/native-test.toml"),
                    )
                };
                assert_eq!(registry.main_window_occupancy(), 1);
                let (creation, id) = cx
                    .update(|app| {
                        let creation = MainWindowCreationOwner::test_process_owner(app);
                        let root = source.entity(app).unwrap();
                        let id = creation.update(app, |creation, cx| {
                            creation.test_hold_next_publication();
                            creation.activate(&root, cx).unwrap()
                        });
                        (creation, id)
                    })
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(10);
                let hidden = loop {
                    let ready = cx
                        .update(|app| {
                            let hidden = creation.read(app).test_hidden_window(id)?;
                            app.update_window(hidden.into(), |_, native, app| {
                                native.draw_and_present_for_test(app)
                            })
                            .unwrap();
                            creation
                                .update(app, |creation, cx| {
                                    creation.test_hidden_ready_to_publish(id, cx)
                                })
                                .then_some(hidden)
                        })
                        .unwrap();
                    if let Some(hidden) = ready {
                        break hidden;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "original New Window was not ready"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                };
                let hidden_native = native(hidden, cx).await;
                assert!(!unsafe { IsWindowVisible(hidden_native).as_bool() });
                assert_eq!(registry.main_window_occupancy(), 2);
                let original_entity = hidden.entity(cx).unwrap().entity_id();
                cx.update(|app| {
                    let root = source.entity(app).unwrap();
                    assert!(
                        creation
                            .update(app, |creation, cx| creation.activate(&root, cx))
                            .is_err()
                    );
                    assert_eq!(creation.read(app).pending_count(), 1);
                })
                .unwrap();
                let blocked = hold_appearance_read(&owner, path, faults, cx).await;
                assert!(!owner.borrow().test_services_ready());
                cx.update(|app| {
                    creation.update(app, |creation, cx| {
                        assert!(creation.test_hidden_ready_to_publish(id, cx));
                        creation.test_release_publication(id, cx);
                    });
                })
                .unwrap();
                for _ in 0..3 {
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    assert!(!owner.borrow().test_services_ready());
                    assert_eq!(windows(&owner), vec![source]);
                    assert_eq!(registry.main_window_occupancy(), 2);
                    cx.update(|app| {
                        let live = app.windows();
                        let main = live
                            .iter()
                            .filter_map(|window| window.downcast::<MainWindowShellRoot>())
                            .collect::<Vec<_>>();
                        assert_eq!(main.len(), 2);
                        assert!(main.contains(&source) && main.contains(&hidden));
                        assert_eq!(
                            live.iter()
                                .filter(|window| window
                                    .downcast::<gpui_settings_window::SettingsWindowView>()
                                    .is_some())
                                .count(),
                            1
                        );
                        assert_eq!(live.len(), 3);
                        assert_eq!(creation.read(app).test_hidden_window(id), Some(hidden));
                        assert_eq!(creation.read(app).pending_count(), 1);
                        assert_eq!(hidden.entity(app).unwrap().entity_id(), original_entity);
                        assert_eq!(
                            hidden.read(app).unwrap().controller().unwrap().window_id(),
                            id
                        );
                        assert!(unsafe { IsWindow(Some(hidden_native)).as_bool() });
                        assert!(!unsafe { IsWindowVisible(hidden_native).as_bool() });
                    })
                    .unwrap();
                }
                if cancel {
                    cx.update(|app| creation.update(app, |creation, cx| creation.cancel(id, cx)))
                        .unwrap();
                    let deadline = Instant::now() + Duration::from_secs(5);
                    loop {
                        let destroyed = cx
                            .update(|app| {
                                creation.read(app).test_hidden_window(id).is_none()
                                    && !app.windows().contains(&hidden.into())
                                    && !unsafe { IsWindow(Some(hidden_native)).as_bool() }
                            })
                            .unwrap();
                        if destroyed {
                            break;
                        }
                        assert!(
                            Instant::now() < deadline,
                            "cancelled original native window did not retire"
                        );
                        cx.background_executor()
                            .timer(Duration::from_millis(10))
                            .await;
                    }
                    assert!(!owner.borrow().test_services_ready());
                    assert_eq!(windows(&owner), vec![source]);
                }
                blocked.release();
                wait_for_services(&owner, cx).await;
                let deadline = Instant::now() + Duration::from_secs(10);
                loop {
                    let settled = cx
                        .update(|app| creation.read(app).pending_count() == 0)
                        .unwrap();
                    if settled {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "original deferred creation did not settle after appearance delivery"
                    );
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                }
                wait_for_services(&owner, cx).await;
                {
                    let retained = owner.borrow();
                    let home = retained.test_services().graph().unwrap().home();
                    assert_eq!(home.home_id(), home_id);
                    assert_eq!(home.health().generation(), Some(generation));
                }
                if cancel {
                    assert_eq!(windows(&owner), vec![source]);
                    let abandoned = snapshot(&owner);
                    assert_eq!(abandoned.windows(), original.windows());
                    assert_eq!(abandoned.header().windows(), original.header().windows());
                    assert_eq!(abandoned.header().fallback(), original.header().fallback());
                    assert_eq!(
                        abandoned.header().exit_intent(),
                        original.header().exit_intent()
                    );
                    assert!(abandoned.header().revision() > original.header().revision());
                    assert_eq!(registry.main_window_occupancy(), 1);
                    assert!(!unsafe { IsWindow(Some(hidden_native)).as_bool() });
                } else {
                    let current = windows(&owner);
                    assert_eq!(current.len(), 2);
                    assert!(current.contains(&source) && current.contains(&hidden));
                    assert_eq!(registry.main_window_occupancy(), 2);
                    assert_eq!(native(hidden, cx).await, hidden_native);
                    assert!(unsafe { IsWindowVisible(hidden_native).as_bool() });
                    cx.update(|app| {
                        assert_eq!(hidden.entity(app).unwrap().entity_id(), original_entity);
                        assert_eq!(
                            hidden.read(app).unwrap().controller().unwrap().window_id(),
                            id
                        );
                    })
                    .unwrap();
                    assert_eq!(
                        snapshot(&owner)
                            .windows()
                            .iter()
                            .filter(|window| window.window_id() == id)
                            .count(),
                        1
                    );
                }
                wait_for_services(&owner, cx).await;
                let settled = snapshot(&owner);
                for _ in 0..3 {
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    wait_for_services(&owner, cx).await;
                    assert_eq!(snapshot(&owner), settled);
                    assert_eq!(
                        cx.update(|app| creation.read(app).pending_count()).unwrap(),
                        0
                    );
                }
                activate_exit(source, cx);
            })
        },
        true,
        if cancel { 1 } else { 2 },
    );
}

#[test]
fn native_settings_appearance_flight_preserves_readers_until_real_graph_retirement() {
    let published = Rc::new(RefCell::new(None));
    let retained_publication = published.clone();
    run_mounted_with_faults(
        0,
        0,
        move |owner, faults, cx| {
            Box::pin(async move {
                wait_for_services(&owner, cx).await;
                let (ordinary_mounted, admission) = appearance_admission(&owner, cx);
                assert!(
                    ordinary_mounted,
                    "ordinary appearance admission: {admission}"
                );
                let (running, model, stop, activity, theme_path) = {
                    let owner = owner.borrow();
                    let home = owner.test_services().graph().unwrap().home();
                    assert!(owner.catalog_query_reader().is_some());
                    (
                        owner.running_threads_reader().unwrap(),
                        owner.model_selection_reader().unwrap(),
                        owner.status_stop_worker().unwrap(),
                        owner.activity_reader().unwrap(),
                        home.canonical_path()
                            .join("themes/installed/native-test.toml"),
                    )
                };
                let blocked = faults.block_next(FaultPoint::BeforeThemeRead);
                let (blocked, reached, attempts, io_error) = cx
                    .background_executor()
                    .spawn(async move {
                        let deadline = Instant::now() + Duration::from_secs(10);
                        let mut reached = false;
                        let mut attempts = 0;
                        let mut io_error = None;
                        match std::fs::read(&theme_path) {
                            Ok(original) => {
                                for attempt in 1..=100 {
                                    if blocked.wait_until_reached(Duration::ZERO) {
                                        reached = true;
                                        break;
                                    }
                                    if Instant::now() >= deadline {
                                        break;
                                    }
                                    let mut bytes = original.clone();
                                    bytes.extend_from_slice(
                                        format!(
                                            "\n# appearance flight reader continuity {attempt:03}\n"
                                        )
                                        .as_bytes(),
                                    );
                                    attempts = attempt;
                                    if let Err(error) = std::fs::write(&theme_path, bytes) {
                                        io_error = Some(error.kind());
                                        break;
                                    }
                                    reached = blocked.wait_until_reached(
                                        deadline
                                            .saturating_duration_since(Instant::now())
                                            .min(Duration::from_millis(100)),
                                    );
                                    if reached {
                                        break;
                                    }
                                }
                            }
                            Err(error) => io_error = Some(error.kind()),
                        }
                        reached |= blocked.wait_until_reached(Duration::ZERO);
                        (blocked, reached, attempts, io_error)
                    })
                    .await;
                if !reached || io_error.is_some() {
                    blocked.release();
                }
                assert!(
                    reached && io_error.is_none(),
                    "real Settings appearance read cut was not reached: reached={reached}, writes={attempts}, io_error={io_error:?}, services_ready={}, original_running_reader_current={}, admission={}",
                    owner.borrow().test_services_ready(),
                    running.current(),
                    appearance_admission(&owner, cx).1,
                );
                {
                    let owner = owner.borrow();
                    assert!(!owner.test_services_ready());
                    let current = owner.running_threads_reader().unwrap();
                    assert!(running.current());
                    assert!(running.same_publication(&current));
                    assert!(running.same_publication(&owner.thread_creation_reader().unwrap()));
                    assert!(
                        model
                            .lifetime
                            .ptr_eq(&owner.model_selection_reader().unwrap().lifetime)
                    );
                    assert_eq!(
                        stop.worker_identity(),
                        owner.status_stop_worker().unwrap().worker_identity()
                    );
                    assert!(owner.status_stop_worker().unwrap().publication_current());
                    assert!(activity.same_publication(&owner.activity_reader().unwrap()));
                    assert!(owner.catalog_query_reader().is_some());
                }
                blocked.release();
                wait_for_services(&owner, cx).await;
                assert!(
                    running.same_publication(&owner.borrow().running_threads_reader().unwrap())
                );

                let mut services = owner.borrow_mut().test_take_services();
                let graph = services.graph.take().unwrap();
                owner.borrow_mut().test_restore_services(services);
                {
                    let owner = owner.borrow();
                    assert!(owner.test_services_ready());
                    assert!(owner.test_services().graph().is_none());
                    assert!(
                        running.current(),
                        "the separately retained graph is still published"
                    );
                    assert!(owner.running_threads_reader().is_none());
                    assert!(owner.thread_creation_reader().is_none());
                    assert!(owner.model_selection_reader().is_none());
                    assert!(owner.status_stop_worker().is_none());
                    assert!(owner.activity_reader().is_none());
                    assert!(owner.catalog_query_reader().is_none());
                }
                let mut services = owner.borrow_mut().test_take_services();
                services.graph = Some(graph);
                owner.borrow_mut().test_restore_services(services);
                assert!(
                    running.same_publication(&owner.borrow().running_threads_reader().unwrap())
                );
                *retained_publication.borrow_mut() = Some(running);
                drop(model);
                drop(stop);
                drop(activity);
                activate_exit(windows(&owner)[0], cx);
            })
        },
        true,
        1,
    );
    assert!(
        !published.borrow().as_ref().unwrap().current(),
        "ordinary final teardown invalidates the original publication"
    );
}

fn appearance_admission(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    cx: &mut AsyncApp,
) -> (bool, String) {
    cx.update(|app| {
        let ordinary_mounted = RunningProcessOwner::mounted_owner(app)
            .and_then(|owner| owner.upgrade())
            .is_some_and(|mounted| Rc::ptr_eq(&mounted, owner));
        let settings_count = app
            .windows()
            .iter()
            .filter(|window| {
                window
                    .downcast::<gpui_settings_window::SettingsWindowView>()
                    .is_some()
            })
            .count();
        let owner = owner.borrow();
        let graph = owner.test_services().graph();
        let source = graph
            .and_then(|graph| graph.current_appearance())
            .map(|appearance| format!("{:?}", appearance.prepared().source()));
        (
            ordinary_mounted,
            format!(
                "ordinary_mounted={ordinary_mounted}, command_failure={:?}, settings_windows={settings_count}, settings_disabled={:?}, theme_failure={:?}, health={:?}, appearance_source={source:?}",
                owner.test_last_ordinary_command_failure(),
                crate::settings_command::SettingsCommandRoute::disabled_reason(app),
                graph.and_then(|graph| graph.settings_theme_failure()),
                graph.map(|graph| graph.home().health().state()),
            ),
        )
    })
    .unwrap()
}

struct HeldAppearanceRead(beryl_home_store::test_faults::FaultBlock);

impl HeldAppearanceRead {
    fn release(&self) {
        self.0.release();
    }
}

impl Drop for HeldAppearanceRead {
    fn drop(&mut self) {
        self.0.release();
    }
}

async fn hold_appearance_read(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    path: std::path::PathBuf,
    faults: FaultController,
    cx: &mut AsyncApp,
) -> HeldAppearanceRead {
    let blocked = faults.block_next(FaultPoint::BeforeThemeRead);
    let (blocked, reached, attempts, io_error) = cx
        .background_executor()
        .spawn(async move {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut reached = false;
            let mut attempts = 0;
            let mut io_error = None;
            match std::fs::read(&path) {
                Ok(original) => {
                    for attempt in 1..=100 {
                        if blocked.wait_until_reached(Duration::ZERO) {
                            reached = true;
                            break;
                        }
                        if Instant::now() >= deadline {
                            break;
                        }
                        let mut bytes = original.clone();
                        bytes.extend_from_slice(
                            format!("\n# deferred New Window appearance {attempt:03}\n").as_bytes(),
                        );
                        attempts = attempt;
                        if let Err(error) = std::fs::write(&path, bytes) {
                            io_error = Some(error.kind());
                            break;
                        }
                        reached = blocked.wait_until_reached(
                            deadline
                                .saturating_duration_since(Instant::now())
                                .min(Duration::from_millis(100)),
                        );
                        if reached {
                            break;
                        }
                    }
                }
                Err(error) => io_error = Some(error.kind()),
            }
            reached |= blocked.wait_until_reached(Duration::ZERO);
            (blocked, reached, attempts, io_error)
        })
        .await;
    let held = HeldAppearanceRead(blocked);
    if !reached || io_error.is_some() {
        held.release();
    }
    assert!(
        reached && io_error.is_none(),
        "deferred-window appearance read not reached: reached={reached}, writes={attempts}, io_error={io_error:?}, admission={}",
        appearance_admission(owner, cx).1,
    );
    held
}
