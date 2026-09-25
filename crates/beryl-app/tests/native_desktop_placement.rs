#![cfg(target_os = "windows")]

#[path = "support/desktop_placement_native.rs"]
mod native;

use beryl_app::main_window::{
    WindowsDesktopPlacementFailure, WindowsDesktopPlacementOutcome, WindowsDesktopPlacementStage,
    prepare_windows_desktop_placement, windows_desktop_id_from_guid, windows_desktop_id_to_guid,
};
use beryl_model::VirtualDesktopId;
use gpui::{
    Application, AsyncApp, WindowsHiddenWindowLease,
    with_windows_window_destruction_observer_for_test,
};
use native::{Apartment, alive, desktops, dispose, facts, open, pump, visible, with_manager};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        System::Com::{
            APTTYPE, APTTYPEQUALIFIER, COINIT, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED,
            CoGetApartmentType, CoInitializeEx,
        },
        UI::WindowsAndMessaging::GetForegroundWindow,
    },
    core::GUID,
};

const CHANGED_MODE: i32 = 0x80010106u32 as i32;

#[test]
fn desktop_identity_uses_canonical_guid_numeric_bytes() {
    let bytes = [
        0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc,
        0xfe,
    ];
    let desktop = VirtualDesktopId::from_bytes(bytes);
    let native = windows_desktop_id_to_guid(desktop);
    assert_eq!(native.data1, 0x01234567);
    assert_eq!(native.data2, 0x89ab);
    assert_eq!(native.data3, 0xcdef);
    assert_eq!(
        native.data4,
        [0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0xfe]
    );
    assert_eq!(windows_desktop_id_from_guid(native), desktop);
    for bytes in [[0; 16], [0xff; 16]] {
        let desktop = VirtualDesktopId::from_bytes(bytes);
        assert_eq!(
            windows_desktop_id_from_guid(windows_desktop_id_to_guid(desktop)),
            desktop
        );
    }
}

fn apartment() -> Result<(i32, i32), i32> {
    let mut kind = APTTYPE::default();
    let mut qualifier = APTTYPEQUALIFIER::default();
    unsafe { CoGetApartmentType(&mut kind, &mut qualifier) }
        .map(|()| (kind.0, qualifier.0))
        .map_err(|error| error.code().0)
}

#[derive(Debug)]
struct WorkerReport {
    outcome: WindowsDesktopPlacementOutcome,
    initial: Result<(i32, i32), i32>,
    before: Result<(i32, i32), i32>,
    after: Result<(i32, i32), i32>,
    after_owner_drop: Result<(i32, i32), i32>,
}

fn run_worker(
    lease: WindowsHiddenWindowLease,
    saved: Option<VirtualDesktopId>,
    prior_apartment: Option<COINIT>,
) -> Result<WorkerReport, i32> {
    let initial = apartment();
    let owner = if let Some(mode) = prior_apartment {
        unsafe { CoInitializeEx(None, mode) }
            .ok()
            .map_err(|error| error.code().0)?;
        Some(Apartment)
    } else {
        None
    };
    let before = apartment();
    let outcome = prepare_windows_desktop_placement(lease, saved);
    let after = apartment();
    drop(owner);
    Ok(WorkerReport {
        outcome,
        initial,
        before,
        after,
        after_owner_drop: apartment(),
    })
}

#[derive(Debug)]
enum ExpectedOutcome {
    Exact(WindowsDesktopPlacementOutcome),
    RejectedMove,
}

impl ExpectedOutcome {
    fn matches(&self, outcome: WindowsDesktopPlacementOutcome) -> bool {
        match self {
            Self::Exact(expected) => *expected == outcome,
            Self::RejectedMove => matches!(outcome,
                WindowsDesktopPlacementOutcome::CurrentDesktopDefault {
                    failure: Some(WindowsDesktopPlacementFailure {
                        stage: WindowsDesktopPlacementStage::MoveWindow,
                        hresult,
                    }),
                } if hresult < 0
            ),
        }
    }
}

fn require(failures: &mut Vec<String>, condition: bool, message: impl Into<String>) {
    if !condition {
        failures.push(message.into());
    }
}

async fn scenario(
    cx: &mut AsyncApp,
    name: &'static str,
    saved: Option<VirtualDesktopId>,
    prior_apartment: Option<COINIT>,
    expected: ExpectedOutcome,
    expected_desktop: GUID,
    failures: &mut Vec<String>,
) -> usize {
    let (window, raw) = cx.update(|cx| open(cx, name)).unwrap();
    let foreground = unsafe { GetForegroundWindow() };
    let (lease, released) = window
        .update(cx, |_, window, _| window.lease_hidden_windows_window())
        .unwrap()
        .unwrap();
    let worker = std::thread::spawn(move || run_worker(lease, saved, prior_apartment));
    while !worker.is_finished() {
        pump(cx).await;
    }
    let result = worker.join();
    let settled = released.await.unwrap();
    require(
        failures,
        alive(raw) && !visible(raw) && !settled.native_destroyed && !settled.close_requested,
        format!("{name}: hidden native ownership changed"),
    );
    require(
        failures,
        unsafe { GetForegroundWindow() } == foreground,
        format!("{name}: worker activated a window"),
    );
    match result {
        Ok(Ok(report)) => {
            require(
                failures,
                expected.matches(report.outcome),
                format!(
                    "{name}: outcome {:?}, expected {expected:?}",
                    report.outcome
                ),
            );
            require(
                failures,
                report.before == report.after,
                format!("{name}: worker changed caller apartment: {report:?}"),
            );
            require(
                failures,
                report.after_owner_drop == report.initial,
                format!("{name}: unbalanced COM initialization: {report:?}"),
            );
            println!("desktop_placement case={name} report={report:?}");
        }
        other => failures.push(format!("{name}: controlled worker failed: {other:?}")),
    }
    let published = window
        .update(cx, |_, window, cx| window.publish(cx))
        .unwrap();
    require(
        failures,
        published.is_ok(),
        format!("{name}: publication failed: {published:?}"),
    );
    pump(cx).await;
    // Root retention covers this test-only read after the hidden lease has settled.
    let observed = cx
        .background_executor()
        .spawn(async move { with_manager(|manager| facts(manager, raw)) })
        .await;
    match observed {
        Ok(observed) => {
            require(
                failures,
                observed.desktop == Ok(expected_desktop),
                format!("{name}: unexpected first-show desktop {observed:?}"),
            );
            require(
                failures,
                observed.current.is_ok(),
                format!("{name}: current-desktop query failed: {observed:?}"),
            );
            println!("desktop_placement case={name} first_show={observed:?}");
        }
        Err(error) => failures.push(format!("{name}: observation COM failed {error:#010x}")),
    }
    require(
        failures,
        alive(raw) && visible(raw) && unsafe { GetForegroundWindow() } == foreground,
        format!("{name}: publication changed activation or native visibility"),
    );
    dispose(cx, window, raw).await;
    raw
}

#[test]
fn saved_desktop_worker_preserves_native_and_com_lifetimes() {
    let completed = Arc::new(Mutex::new(None));
    let captured = completed.clone();
    let destroyed = Arc::new(Mutex::new(Vec::new()));
    let observed = destroyed.clone();
    let gui_thread = std::thread::current().id();
    with_windows_window_destruction_observer_for_test(
        move |raw| {
            observed
                .lock()
                .unwrap()
                .push((raw, std::thread::current().id()))
        },
        || {
            Application::new().run(move |cx| {
                let (control_window, control) = open(cx, "control");
                let foreground = unsafe { GetForegroundWindow() };
                control_window
                    .update(cx, |_, window, cx| window.publish(cx).unwrap())
                    .unwrap();
                cx.spawn(async move |cx| {
                    let started = Instant::now();
                    let mut failures = Vec::new();
                    let mut expected_handles = Vec::new();
                    pump(cx).await;
                    require(
                        &mut failures,
                        unsafe { GetForegroundWindow() } == foreground,
                        "control publication activated a window",
                    );
                    let discovered = cx
                        .background_executor()
                        .spawn(async move { desktops(control) })
                        .await;
                    match discovered {
                        Ok((current, alternate)) => {
                            println!(
                                "desktop_placement current={current:?} alternate={alternate:?}"
                            );
                            let known = windows_desktop_id_from_guid(alternate.unwrap_or(current));
                            let accepted = WindowsDesktopPlacementOutcome::SavedDesktopAccepted {
                                desktop: known,
                            };
                            let default = WindowsDesktopPlacementOutcome::CurrentDesktopDefault {
                                failure: None,
                            };
                            expected_handles.push(
                                scenario(
                                    cx,
                                    "absent",
                                    None,
                                    None,
                                    ExpectedOutcome::Exact(default),
                                    current,
                                    &mut failures,
                                )
                                .await,
                            );
                            expected_handles.push(
                                scenario(
                                    cx,
                                    "absent_sta",
                                    None,
                                    Some(COINIT_APARTMENTTHREADED),
                                    ExpectedOutcome::Exact(default),
                                    current,
                                    &mut failures,
                                )
                                .await,
                            );
                            expected_handles.push(
                                scenario(
                                    cx,
                                    "accepted",
                                    Some(known),
                                    None,
                                    ExpectedOutcome::Exact(accepted),
                                    windows_desktop_id_to_guid(known),
                                    &mut failures,
                                )
                                .await,
                            );
                            expected_handles.push(
                                scenario(
                                    cx,
                                    "accepted_existing_mta",
                                    Some(known),
                                    Some(COINIT_MULTITHREADED),
                                    ExpectedOutcome::Exact(accepted),
                                    windows_desktop_id_to_guid(known),
                                    &mut failures,
                                )
                                .await,
                            );
                            let init_failure =
                                WindowsDesktopPlacementOutcome::CurrentDesktopDefault {
                                    failure: Some(WindowsDesktopPlacementFailure {
                                        stage: WindowsDesktopPlacementStage::InitializeCom,
                                        hresult: CHANGED_MODE,
                                    }),
                                };
                            expected_handles.push(
                                scenario(
                                    cx,
                                    "incompatible_sta",
                                    Some(known),
                                    Some(COINIT_APARTMENTTHREADED),
                                    ExpectedOutcome::Exact(init_failure),
                                    current,
                                    &mut failures,
                                )
                                .await,
                            );
                            let mut missing = [0; 16];
                            getrandom::fill(&mut missing).unwrap();
                            expected_handles.push(
                                scenario(
                                    cx,
                                    "nonexistent",
                                    Some(VirtualDesktopId::from_bytes(missing)),
                                    None,
                                    ExpectedOutcome::RejectedMove,
                                    current,
                                    &mut failures,
                                )
                                .await,
                            );
                        }
                        Err(error) => failures.push(format!(
                            "could not discover owned control desktop: {error:#010x}"
                        )),
                    }
                    require(
                        &mut failures,
                        started.elapsed() <= Duration::from_secs(30),
                        "native desktop scenarios exceeded 30 second budget",
                    );
                    expected_handles.push(control);
                    *captured.lock().unwrap() = Some((expected_handles, failures));
                    control_window
                        .update(cx, |_, window, _| window.remove_window())
                        .unwrap();
                })
                .detach();
            })
        },
    );
    let (expected, failures) = completed
        .lock()
        .unwrap()
        .take()
        .expect("native desktop worker test completed");
    assert_eq!(
        *destroyed.lock().unwrap(),
        expected
            .iter()
            .map(|raw| (*raw, gui_thread))
            .collect::<Vec<_>>()
    );
    assert!(expected.into_iter().all(|raw| !alive(raw)));
    assert!(failures.is_empty(), "{}", failures.join("; "));
}
