use super::*;
use crate::{
    main_window::*,
    running_owner::RunningProcessOwner,
    runtime_admission::OnboardingFacts,
    startup_owner::{StartedProcess, StartupCommands},
    theme_runtime::GpuiAppearanceWindowSet,
    thread_root_picker::*,
};
use beryl_home_store::HomeCommand;
use beryl_model::{
    AdmittedHostPath, Availability, ExecutionBinding, PathFlavor, RootId, RuntimeId,
    RuntimeLaunchForm, RuntimeMode, RuntimeNativePath, SyndicDraftId, SyndicThreadId, WindowId,
};
use beryl_state::{
    AvailabilitySnapshot, CatalogSourceRevisions, CreateRuntimeWithHomeRoot, PublishCatalogClaim,
    RecordRevision, RememberedTarget, RootRegistration, RuntimeRegistration, UnixMillis,
    WindowClaimReplacementPreparation,
};
use gpui::{AppContext, TestAppContext};
use std::{cell::RefCell, num::NonZeroUsize, rc::Rc};
use syndic_storage::{CreateThread, DraftEditHistoryPolicyV1};

#[path = "committed_first_conversation/support.rs"]
mod support;
#[path = "../../pending_composer_activation/support.rs"]
mod widget_support;
use support::*;

#[path = "runtime_setup_shell/catalog.rs"]
mod catalog;
#[path = "runtime_setup_shell/confirmation.rs"]
mod confirmation;
#[path = "runtime_setup_shell/confirmation_failures.rs"]
mod confirmation_failures;
#[path = "runtime_setup_shell/confirmation_recovery.rs"]
mod confirmation_recovery;
#[path = "runtime_setup_shell/lifecycle.rs"]
mod lifecycle;
#[path = "runtime_setup_shell/native.rs"]
mod native;
#[path = "runtime_setup_shell/primary.rs"]
mod primary;
#[path = "runtime_setup_shell/thread_switcher.rs"]
mod thread_switcher;

fn mounted(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    Rc<RefCell<RunningProcessOwner>>,
    gpui::WindowHandle<MainWindowShellRoot>,
    FaultController,
) {
    let (directory, mut process, prepared, appearance, faults) = std::thread::spawn(|| {
        let (directory, mut process, prepared, appearance, faults) = prepared_process();
        eprintln!("runtime setup fixture home: {}", directory.path().display());
        let mut inputs = window_services::inputs();
        inputs.configurator_source = Arc::new(|| Box::new(configure));
        inputs.restored_activation_source = Arc::new(|record| {
            Ok((
                widget_support::activation(
                    record.selected_thread().unwrap().thread_id(),
                    201,
                    202,
                    1,
                    0,
                ),
                widget_support::fixture::operation_id(203),
            ))
        });
        drop(process.window_services(inputs).unwrap());
        (directory, process, prepared, appearance, faults)
    })
    .join()
    .unwrap();
    let services = process.runtime_setup_services().unwrap();
    let reader = process.running_threads_reader().unwrap();
    let (owner, window) = cx.update(|app| {
        let appearance =
            GpuiAppearanceWindowSet::new(appearance, NonZeroUsize::new(4).unwrap(), app);
        let windows = PublishedMainWindowRestoreSet::from_virtual_prepared_test(
            prepared,
            appearance.clone(),
            app,
        )
        .unwrap();
        let window = windows.shells()[0].window();
        let id = window.read(app).unwrap().controller().unwrap().window_id();
        let owner = RunningProcessOwner::test_start_unmounted(
            StartedProcess {
                configuration: configuration(),
                services: process,
                windows,
                appearance,
                startup_surface: None,
                commands: StartupCommands::test_running(),
            },
            app,
        );
        window
            .update(app, |root, window, cx| {
                root.test_runtime_setup_fixture(services, vec![id], Some(reader), window, cx);
            })
            .unwrap();
        (owner, window)
    });
    (directory, owner, window, faults)
}

fn open(
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut TestAppContext,
) -> gpui::Entity<ThreadRootPicker> {
    window
        .update(cx, |root, window, cx| {
            root.test_open_runtime_setup(window, cx)
        })
        .unwrap();
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| root.test_runtime_setup_picker().is_some())
                .unwrap()
        },
        "setup entry did not open",
    );
    window
        .read_with(cx, |root, _| root.test_runtime_setup_picker().unwrap())
        .unwrap()
}

fn close_empty(
    owner: Rc<RefCell<RunningProcessOwner>>,
    window: gpui::WindowHandle<MainWindowShellRoot>,
    cx: &mut TestAppContext,
) {
    stop_observers(&owner, cx);
    wait(
        cx,
        |cx| {
            window
                .update(cx, |root, window, cx| {
                    root.retire_setup_first_mount(window, cx).unwrap()
                })
                .unwrap()
        },
        "setup resources did not drain",
    );
    window
        .update(cx, |root, window, cx| {
            root.retire_notices(window, cx);
            window.remove_window();
        })
        .unwrap();
    retire_virtual_bindings(&owner, cx);
    let mut process = owner.borrow_mut().test_take_services();
    std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let mut cleanups = Vec::new();
            match process
                .graph()
                .unwrap()
                .runtime_setup()
                .retire(&mut cleanups)
            {
                Ok(()) => {
                    assert!(
                        cleanups.is_empty(),
                        "healthy fixture retirement requires no failed cleanup custody"
                    );
                    break;
                }
                Err(error) => {
                    assert!(
                        Instant::now() < deadline,
                        "original healthy setup fixture retirement did not settle: {error}"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
        close(&mut process);
    })
    .join()
    .unwrap();
}

#[gpui::test]
fn setup_secondary_opens_shared_confirmed_picker_and_cancel_restores_command(
    cx: &mut TestAppContext,
) {
    let (directory, owner, window, _) = mounted(cx);
    let picker = open(window, cx);
    wait(
        cx,
        |cx| {
            cx.update(|app| {
                let picker = picker.read(app);
                picker
                    .runtime_diagnostics()
                    .is_some_and(|d| d.pending_page_count == 0)
                    && picker.diagnostics().pending_page_count == 0
            })
        },
        "empty coherent pages did not settle",
    );
    assert!(cx.update(|app| picker.read(app).selected_key().is_none()));
    assert!(!cx.update(|app| {
        picker
            .read(app)
            .confirmation_state()
            .unwrap()
            .can_dispatch()
    }));
    picker.update(cx, |picker, cx| {
        picker.dispatch_command(PickerCommand::AddRuntime, cx)
    });
    wait(
        cx,
        |cx| cx.has_pending_prompt(),
        "runtime form choice did not open",
    );
    picker.update(cx, |picker, cx| {
        picker.dispatch_command(PickerCommand::AddRuntime, cx)
    });
    cx.simulate_prompt_answer("Cancel");
    wait(
        cx,
        |cx| {
            !window
                .read_with(cx, |root, _| root.test_runtime_setup_state().0)
                .unwrap()
        },
        "choice cancel stayed pending",
    );
    assert!(cx.update(|app| {
        picker
            .read(app)
            .command_state(&PickerCommand::AddRuntime)
            .unwrap()
            .can_dispatch()
    }));
    assert!(
        window
            .read_with(cx, |root, _| root.controller().unwrap().is_threadless())
            .unwrap()
    );
    close_empty(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn staged_first_conversation_close_waits_original_widget_and_mount_worker(cx: &mut TestAppContext) {
    let (directory, owner, window, _) = mounted(cx);
    let _picker = open(window, cx);
    let flight = {
        let process = owner.borrow_mut().test_take_services();
        let (process, flight) = std::thread::spawn(move || {
            commit_onboarding(&process, None);
            let flight = process.graph().unwrap().runtime_setup().test_first_flight();
            (process, flight)
        })
        .join()
        .unwrap();
        owner.borrow_mut().test_restore_services(process);
        flight
    };
    window
        .update(cx, |root, window, cx| {
            root.test_attach_runtime_setup_flight(flight.clone(), window, cx)
        })
        .unwrap();
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| root.test_runtime_setup_state().2)
                .unwrap()
        },
        "original first mount was not staged",
    );
    let retained = window
        .read_with(cx, |root, app| {
            root.test_hold_runtime_setup_mount_worker(app)
        })
        .unwrap();
    let draft = window
        .update(cx, |root, window, cx| {
            root.set_shutdown_interaction_gated(true, cx).unwrap();
            root.begin_shutdown_draft(window, cx).unwrap()
        })
        .unwrap();
    let pending = window
        .update(cx, |root, window, cx| {
            root.advance_shutdown_draft(&draft, window, cx).unwrap()
        })
        .unwrap();
    assert_eq!(
        pending,
        MainWindowShutdownDraftAdvance::Resident(
            MainWindowConversationComposerCloseAdvance::Preparing
        )
    );
    cx.run_until_parked();
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_state().2)
            .unwrap()
    );
    retained();
    wait(
        cx,
        |cx| {
            window
                .update(cx, |root, window, cx| {
                    root.advance_shutdown_draft(&draft, window, cx).unwrap()
                        == MainWindowShutdownDraftAdvance::Threadless
                })
                .unwrap()
        },
        "original staged mount did not release after its worker joined",
    );
    assert!(
        !window
            .read_with(cx, |root, _| root.test_runtime_setup_state().2)
            .unwrap()
    );
    assert_eq!(
        window
            .update(cx, |root, window, cx| root
                .release_shutdown_draft(&draft, window, cx)
                .unwrap())
            .unwrap(),
        MainWindowShutdownDraftRelease::Released
    );
    assert!(flight.cancellation().is_cancelled());
    drop(draft);
    close_empty(owner, window, cx);
    drop(directory);
}

#[gpui::test]
fn committed_first_runtime_mounts_editor_and_transcript_in_original_virtual_window(
    cx: &mut TestAppContext,
) {
    eprintln!("runtime setup healthy first: before mounted");
    let (directory, owner, window, _) = mounted(cx);
    eprintln!(
        "runtime setup healthy first: mounted fixture {}",
        directory.path().display()
    );
    let picker = open(window, cx);
    eprintln!("runtime setup healthy first: picker open, before original commit");
    let (thread, draft, record, flight) = {
        let process = owner.borrow_mut().test_take_services();
        let (process, result) = std::thread::spawn(move || {
            let (thread, draft, record) = commit_onboarding(&process, None);
            let flight = process.graph().unwrap().runtime_setup().test_first_flight();
            (process, (thread, draft, record, flight))
        })
        .join()
        .unwrap();
        owner.borrow_mut().test_restore_services(process);
        result
    };
    eprintln!("runtime setup healthy first: original commit complete");
    let original_id = window.window_id();
    window
        .update(cx, |root, window, cx| {
            root.test_attach_runtime_setup_flight(flight, window, cx)
        })
        .unwrap();
    eprintln!("runtime setup healthy first: original flight attached, before mounting wait");
    wait(
        cx,
        |cx| {
            window
                .read_with(cx, |root, app| {
                    root.controller().unwrap().composer_mount().is_some()
                        && root.test_first_conversation_transcript_claim(
                            record.selected_thread().unwrap(),
                            app,
                        )
                })
                .unwrap()
        },
        "healthy first shell did not publish",
    );
    eprintln!("runtime setup healthy first: original shell published");
    assert_eq!(window.window_id(), original_id);
    assert_eq!(cx.windows().len(), 1);
    assert!(
        window
            .read_with(cx, |root, _| root.test_runtime_setup_picker().is_none())
            .unwrap()
    );
    assert!(!cx.update(|app| picker.read(app).diagnostics().collection_failed));
    let selected = window
        .read_with(cx, |root, app| {
            root.controller()
                .unwrap()
                .composer_mount()
                .unwrap()
                .read(app)
                .contribution()
                .unwrap()
                .read(app)
                .selection_identity()
        })
        .unwrap();
    assert_eq!(selected.claim().thread_id(), thread);
    assert_eq!(selected.binding().candidate().draft_id(), draft);
    eprintln!("runtime setup healthy first: before disposal");
    dispose_recovered(owner, window, cx);
    eprintln!("runtime setup healthy first: disposal complete");
    drop(directory);
}
