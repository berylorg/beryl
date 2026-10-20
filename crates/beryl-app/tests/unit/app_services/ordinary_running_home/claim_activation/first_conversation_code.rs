use super::*;
use crate::runtime_admission::OnboardingFacts;
use beryl_model::{
    AdmittedHostPath, Availability, ExecutionBinding, PathFlavor, RootId, RuntimeId,
    RuntimeLaunchForm, RuntimeMode, RuntimeNativePath,
};
use beryl_state::{
    AvailabilitySnapshot, CatalogSourceRevisions, CreateRuntimeWithHomeRoot, PublishCatalogClaim,
    RecordRevision, RememberedTarget, RootRegistration, RuntimeRegistration, UnixMillis,
    WindowClaimReplacementPreparation,
};

#[path = "../../../../support/committed_first_conversation.rs"]
mod onboarding;

#[test]
fn native_recovered_first_conversation_submits_code_and_routes_fresh_inner_ranges() {
    let final_selection = Rc::new(RefCell::new(None));
    let qualified = final_selection.clone();
    run_mounted_with_prepared_home(
        0,
        0,
        |_| {},
        final_selection,
        move |owner, faults, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                native_code::draw(window, cx);
                wait_for_services(&owner, cx).await;
                let prior_panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                assert!(
                    window
                        .read_with(cx, |root, _| {
                            root.controller().unwrap().is_threadless()
                                && root.controller().unwrap().composer_mount().is_none()
                        })
                        .unwrap()
                );
                let generation = owner
                    .borrow()
                    .test_services()
                    .graph()
                    .unwrap()
                    .home()
                    .health()
                    .generation()
                    .unwrap();
                owner.borrow_mut().test_enable_resident_frame_capture();
                let process = owner.borrow_mut().test_take_services();
                let (process, thread, exact_window) = cx
                    .background_executor()
                    .spawn(async move {
                        let (thread, _, exact_window) =
                            onboarding::commit_onboarding(&process, None);
                        let home = process.graph().unwrap().home();
                        faults.fail_next(
                            beryl_home_store::test_faults::FaultPoint::BeforeReadConfirmation,
                        );
                        assert!(home.home_revision().is_err());
                        (process, thread, exact_window)
                    })
                    .await;
                owner.borrow_mut().test_restore_services(process);
                native_code::wait(
                    window,
                    cx,
                    |cx| {
                        cx.update(|app| {
                            RunningProcessOwner::test_dispatch_scheduled_resident_frame(
                                &owner, app,
                            );
                            RunningProcessOwner::test_observe_running_home_failure(&owner, app);
                        })
                        .unwrap();
                        if let Some(InterruptedExitRecoveryOutcome::Unavailable(error)) =
                            owner.borrow().automatic_recovery_outcome().as_deref()
                        {
                            panic!("native first conversation recovery: {error}");
                        }
                        matches!(
                            owner.borrow().automatic_recovery_outcome().as_deref(),
                            Some(InterruptedExitRecoveryOutcome::Completed)
                        ) && window
                            .read_with(cx, |root, app| {
                                root.controller()
                                    .unwrap()
                                    .composer_mount()
                                    .is_some_and(|mount| {
                                        mount.read(app).contribution().is_some_and(|composer| {
                                            let current = composer.read(app).selection_identity();
                                            Some(current.claim()) == exact_window.selected_thread()
                                                && current.window_id() == exact_window.window_id()
                                                && current.binding().home_generation() != generation
                                        })
                                    })
                            })
                            .unwrap()
                    },
                    "native committed first conversation recovery",
                )
                .await;
                assert_eq!(windows(&owner), vec![window]);
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                assert_ne!(panel.entity_id(), prior_panel.entity_id());
                assert!(
                    panel
                        .read_with(cx, |panel, _| panel.snapshot().records.is_empty())
                        .unwrap()
                );
                native_code::submit_code(&owner, window, cx).await;
                native_code::qualify(window, &panel, cx).await;
                assert_eq!(
                    window
                        .read_with(cx, |root, _| root.test_transcript_panel().entity_id())
                        .unwrap(),
                    panel.entity_id()
                );
                *qualified.borrow_mut() = Some((exact_window.window_id(), thread));
                activate_exit(window, cx);
            })
        },
        true,
        1,
    );
}
