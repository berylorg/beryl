use crate::app_services::recovery_threadless::ThreadlessRecoveryWindow;
use crate::running_owner::InterruptedExitCandidate;

mod revalidation {
    use super::*;
    include!("interrupted_exit_revalidation_support.rs");
}

mod process_work {
    use super::*;
    include!("interrupted_exit_process_work_support.rs");
}

mod construction {
    use super::*;
    include!("interrupted_exit_construction_support.rs");
}

mod service_preparation {
    use super::*;
    include!("interrupted_exit_services_support.rs");
}

mod native_appearance {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/native_shell_appearance.rs"
    ));
}

pub(super) async fn verify(
    owner: &Rc<RefCell<RunningProcessOwner>>,
    request: &crate::startup_owner::RunningExitRequest,
    foreign_candidate: &InterruptedExitCandidate,
    generation: beryl_home_store::HomeGeneration,
    faults: &FaultController,
    publication_delivery: RecoveryPublicationDelivery,
    cx: &mut AsyncApp,
) -> gpui::Entity<crate::theme_runtime::GpuiAppearanceWindowSet> {
    let window = owner.borrow().test_process().windows.shells()[0].window();
    let window_id = window
        .read_with(cx, |root, _| root.controller().unwrap().window_id())
        .unwrap();
    let candidate = construction::verify(
        owner,
        request,
        generation,
        foreign_candidate.candidate.generation(),
        faults,
        cx,
    )
    .await;
    let (candidate, mut source, appearance) = cx
        .background_executor()
        .spawn(async move {
            let home_id = candidate.home_id();
            let mut candidate = candidate;
            let state = BerylState::reacquire_candidate(&candidate).unwrap();
            let access = candidate.recovery_access().unwrap();
            let before = access.home_revision().unwrap();
            let source =
                ThreadlessRecoveryWindow::prepare(&access, &state, home_id, generation, window_id)
                    .unwrap();
            source.revalidate(&access, &state).unwrap();
            assert_eq!(before, access.home_revision().unwrap());
            let appearance = crate::theme_runtime::AppearanceCoordinator::new(
                crate::theme_runtime::AppearanceCoordinatorConfig::new(
                    NonZeroUsize::new(4).unwrap(),
                ),
                native_appearance::system_font_appearance(&state),
            )
            .current();
            (
                InterruptedExitCandidate {
                    candidate,
                    session: state.session(),
                },
                Some(source),
                appearance,
            )
        })
        .await;
    let original = format!("{:?}", owner.borrow().interrupted_exit_session().unwrap());
    window
        .update(cx, |root, window, app| {
            let drafts = owner
                .borrow_mut()
                .test_replace_recovery_drafts(None)
                .unwrap();
            drafts
                .borrow_mut()
                .adopt_recovered_threadless_shell(root, &mut source, window, app)
                .unwrap();
            owner
                .borrow_mut()
                .test_replace_recovery_drafts(Some(drafts));
            assert!(root.controller().unwrap().is_threadless());
            assert_eq!(root.controller().unwrap().window_id(), window_id);
            assert!(root.controller().unwrap().composer_mount().is_none());
            assert!(
                root.set_shutdown_interaction_gated(false, app)
                    .unwrap_err()
                    .contains("fresh appearance")
            );
        })
        .unwrap();
    assert_eq!(
        original,
        format!("{:?}", owner.borrow().interrupted_exit_session().unwrap())
    );
    assert!(!RunningProcessOwner::finish_exit(owner, request));
    let appearance = cx
        .update(|app| {
            use crate::theme_runtime::{AppearancePublicationTarget, GpuiAppearanceWindowSet};
            let fresh =
                GpuiAppearanceWindowSet::new(appearance, NonZeroUsize::new(4).unwrap(), app);
            let mut running = owner.borrow_mut();
            let previous = running.test_process_appearance();
            previous.update(app, |owner, _| owner.retire());
            running
                .test_process_mut()
                .windows
                .bind_interrupted_exit_appearance(window, &fresh, app)
                .unwrap();
            assert_eq!(fresh.read(app).target().snapshot().count, 1);
            fresh
        })
        .unwrap();
    assert!(!RunningProcessOwner::finish_exit(owner, request));
    let candidate = revalidation::verify(owner, request, candidate, &appearance, cx).await;
    let candidate = process_work::verify(owner, request, candidate, cx).await;
    service_preparation::verify(
        owner,
        request,
        candidate,
        generation,
        window_id,
        faults,
        &appearance,
        publication_delivery,
        cx,
    )
    .await
}
