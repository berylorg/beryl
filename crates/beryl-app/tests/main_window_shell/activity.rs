use super::*;
use beryl_app::{
    ActivityWindowHarness,
    cas_projection::{
        RuntimeInterestConfig, RuntimeInterestKind, RuntimeInterestStatus,
        RuntimeInterestTestHarness, RuntimeInterestTestProbe,
    },
    runtime_activity_enrollment::RuntimeActivityEnrollmentOperations,
};
use beryl_model::{AdmittedHostPath, CasProcessGeneration, PathFlavor};
use std::time::Duration;

#[path = "../../../syndic-storage/tests/support/mod.rs"]
pub(super) mod syndic_support;

#[gpui::test]
fn activity_slot_uses_real_pending_turn_and_preserves_composer_at_window_minimum(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(ensure_text_input_bindings);
    let (mut fixture, prepared) = support::join(
        support::worker(|| {
            let mut fixture = ShellFixture::new(161);
            let prepared = fixture.prepare();
            (fixture, prepared)
        }),
        cx,
    );
    let owner = fixture.owner(cx);
    let mut shell = cx.update(|app| {
        GpuiMainWindowShellHost::new(app, owner)
            .construct_hidden(prepared)
            .unwrap_or_else(|_| panic!("Activity shell"))
    });
    draw(shell.window(), cx);
    cx.update(|app| shell.publish(app)).unwrap();
    draw(shell.window(), cx);
    let home = fixture.store.clone();
    let storage = fixture.storage.clone();
    let state = fixture.state.clone();
    let (harness, runtime, interest) = support::join(
        support::worker(move || {
            let window = beryl_model::WindowId::from_bytes([161; 16]);
            let thread = SyndicThreadId::from_bytes([201; 16]);
            let turn = syndic_support::exact_cas::submit_current_draft(
                &home,
                storage.clone(),
                thread,
                SyndicDraftId::from_bytes([231; 16]),
                beryl_model::SyndicItemId::from_bytes([232; 16]),
                "Activity mounting",
                SyndicTimestamp::from_unix_millis(1_000),
            );
            let binding = storage
                .thread_execution(
                    &home,
                    thread,
                    syndic_storage::SyndicPointReadLimit::new(16_384).unwrap(),
                )
                .unwrap()
                .unwrap()
                .execution()
                .clone();
            let timeout = Duration::from_secs(10);
            let runtime = RuntimeInterestTestHarness::with_enrollments(
                RuntimeInterestConfig::new(
                    NonZeroUsize::new(1).unwrap(),
                    NonZeroUsize::new(4).unwrap(),
                    timeout,
                )
                .unwrap(),
                RuntimeActivityEnrollmentOperations::new(
                    home.home_id(),
                    NonZeroUsize::new(1).unwrap(),
                ),
            );
            let path = |path| AdmittedHostPath::from_admitted(PathFlavor::Windows, path).unwrap();
            let spec = beryl_backend::ManagedBackendLaunchSpec::new(
                binding.runtime_id(),
                path(r"C:\activity-test\codex.exe"),
                RuntimeMode::Host,
                beryl_model::RuntimeLaunchForm::CodexCli,
                native_path(RuntimeMode::Host, r"C:\activity-test\codex.exe"),
                binding.root_path().clone(),
                path(r"C:\activity-test\tokens"),
                native_path(RuntimeMode::Host, r"C:\activity-test\tokens"),
            )
            .unwrap();
            let interest = runtime
                .acquire(
                    spec,
                    binding,
                    RuntimeInterestKind::RequiredWork,
                    RuntimeInterestTestProbe::new(CasProcessGeneration::new(91).unwrap()),
                )
                .unwrap();
            assert!(matches!(
                interest.wait_for_change(RuntimeInterestStatus::Starting, timeout),
                RuntimeInterestStatus::Ready(_)
            ));
            interest
                .enroll_activity_for_test(&home, &storage, thread, turn)
                .unwrap();
            let bootstrap = state.session().minimal_bootstrap(&home).unwrap().unwrap();
            let claim = bootstrap
                .windows()
                .iter()
                .find(|record| record.window_id() == window)
                .unwrap()
                .selected_thread()
                .unwrap();
            let update = state
                .session()
                .prepare_activity_panel_height(
                    &home,
                    window,
                    beryl_state::ActivityPanelHeight::new(202.25).unwrap(),
                )
                .unwrap();
            let mut command = beryl_home_store::HomeCommand::new(home.home_revision().unwrap());
            update.add_to(&mut command).unwrap();
            assert!(matches!(
                home.execute(command),
                beryl_home_store::CommandOutcome::Committed {
                    later_failure: None,
                    ..
                }
            ));
            let harness =
                ActivityWindowHarness::new(&home, &storage, &state, &runtime, window, claim, 64, 4);
            (harness, runtime, interest)
        }),
        cx,
    );
    shell
        .window()
        .update(cx, |root, _, cx| root.test_mount_activity(&harness, cx))
        .unwrap();
    for _ in 0..200 {
        draw(shell.window(), cx);
        if shell
            .window()
            .read_with(cx, |root, app| {
                root.test_activity_diagnostics(app).is_some()
            })
            .unwrap()
        {
            break;
        }
        cx.background_executor
            .advance_clock(Duration::from_millis(100));
    }
    let facts = shell
        .window()
        .read_with(cx, |root, app| root.test_activity_diagnostics(app))
        .unwrap()
        .expect("Auto-visible pending turn");
    assert_eq!(facts.total, 0);
    assert!(!facts.inert);
    assert_eq!(facts.height, 202.25);
    shell
        .window()
        .update(cx, |root, window, app| {
            root.test_focus_activity_resize(window, app)
        })
        .unwrap();
    cx.simulate_keystrokes(shell.window().into(), "up");
    for _ in 0..8 {
        cx.background_executor
            .advance_clock(Duration::from_millis(200));
        draw(shell.window(), cx);
    }
    let home = fixture.store.clone();
    let state = fixture.state.clone();
    let saved_height = support::join(
        support::worker(move || {
            state
                .session()
                .window_presentation(&home, beryl_model::WindowId::from_bytes([161; 16]))
                .unwrap()
                .unwrap()
                .activity_panel_height()
                .get()
        }),
        cx,
    );
    assert_eq!(saved_height, 210.25);
    let minimum = shell
        .window()
        .read_with(cx, |root, _| root.controller().unwrap().minimum_size())
        .unwrap();
    cx.simulate_window_resize(shell.window().into(), minimum);
    draw(shell.window(), cx);
    let (oversized_composer, protected_activity, body) = shell
        .window()
        .update(cx, |root, window, _| {
            root.test_activity_allocation_for_content(gpui::px(10_000.), window)
        })
        .unwrap();
    assert!(oversized_composer > gpui::px(0.));
    assert!(protected_activity >= gpui::px(56.));
    assert!(oversized_composer + protected_activity <= body);
    let mut visual = gpui::VisualTestContext::from_window(shell.window().into(), cx);
    let activity = visual.debug_bounds("activity-panel").unwrap();
    let transcript = visual
        .debug_bounds("main-window-transcript-region")
        .unwrap();
    let composer = visual.debug_bounds("main-window-user-input-panel").unwrap();
    let status = visual.debug_bounds("main-window-status-line").unwrap();
    assert!(activity.size.height >= gpui::px(56.));
    assert_eq!(transcript.bottom(), activity.top());
    assert_eq!(activity.bottom(), composer.top());
    assert_eq!(composer.bottom(), status.top());
    drop((interest, runtime));
}
