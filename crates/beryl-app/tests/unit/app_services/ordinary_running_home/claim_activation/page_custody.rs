use super::*;
use crate::main_window::{
    MainWindowClaimRetirementKind, MainWindowComposerSelectionIdentity,
    MainWindowComposerWidgetRelease,
};

type ReleaseEvidence = (
    Vec<(MainWindowComposerSelectionIdentity, u64, u64, usize)>,
    Option<(
        MainWindowClaimRetirementKind,
        MainWindowComposerWidgetRelease,
    )>,
);

pub(super) async fn accepted_original_release(
    window: WindowHandle<MainWindowShellRoot>,
    original: MainWindowComposerSelectionIdentity,
    cx: &mut gpui::AsyncApp,
) -> ReleaseEvidence {
    let evidence = current_release(window, cx).await;
    assert!(
        evidence
            .0
            .iter()
            .any(|(selection, environment, generation, accepted)| {
                *selection == original && *environment != 0 && *generation != 0 && *accepted > 0
            }),
        "no original Page custody received a positive Accepted release: {:?}",
        evidence.0
    );
    let (kind, widget) = evidence.1.expect("original widget release is missing");
    assert_eq!(kind, MainWindowClaimRetirementKind::OrdinarySelection);
    assert_eq!(widget.selection(), original);
    evidence
}

pub(super) async fn current_release(
    window: WindowHandle<MainWindowShellRoot>,
    cx: &mut gpui::AsyncApp,
) -> ReleaseEvidence {
    cx.update(|app| {
        let root = window.read(app).unwrap();
        (
            root.test_original_claim_page_release_evidence(),
            root.test_original_claim_widget_release(),
        )
    })
    .unwrap()
}

#[test]
fn native_original_ordinary_saved_prior_releases_adopted_pages_once_after_completed_disposal() {
    let final_selection = Rc::new(RefCell::new(None));
    let qualified_selection = final_selection.clone();
    run_mounted_with_prepared_home(
        2,
        0,
        fixture::prepare_home,
        final_selection,
        move |owner, faults, cx| {
            Box::pin(async move {
                let (window, window_id, prior) = scenario::recover(
                    owner.clone(),
                    faults.clone(),
                    Cut::SaveNoncommit,
                    control::Control::Normal,
                    SaveExpectation::DirtyPageSetup,
                    cx,
                )
                .await;
                assert!(
                    cx.update(|app| {
                        window
                            .read(app)
                            .unwrap()
                            .test_thread_creation_composer_adopted_custody_items(app)
                    })
                    .unwrap()
                        > 0,
                    "first authentic recovery did not adopt original Page custody"
                );
                let (same_window, same_window_id, selected) = scenario::recover(
                    owner,
                    faults,
                    Cut::DisposalCommitted,
                    control::Control::Normal,
                    SaveExpectation::SavedNoop,
                    cx,
                )
                .await;
                assert_eq!(same_window, window);
                assert_eq!(same_window_id, window_id);
                assert_ne!(selected.thread_id(), prior.thread_id());
                *qualified_selection.borrow_mut() = Some((same_window_id, selected.thread_id()));
                activate_exit(window, cx);
            })
        },
        true,
        2,
    );
}

#[test]
fn native_nonfinal_close_drains_recovered_pages_before_preserving_survivor_and_final_exit() {
    let final_selection = Rc::new(RefCell::new(None));
    let qualified_selection = final_selection.clone();
    run_mounted_with_prepared_home(
        2,
        0,
        fixture::prepare_home,
        final_selection,
        move |owner, faults, cx| {
            Box::pin(async move {
                let (survivor, _, _) = scenario::recover(
                    owner.clone(),
                    faults.clone(),
                    Cut::SaveNoncommit,
                    control::Control::Normal,
                    SaveExpectation::DirtyPageSetup,
                    cx,
                )
                .await;
                let (same_survivor, survivor_id, selected) = scenario::recover(
                    owner.clone(),
                    faults,
                    Cut::DisposalCommitted,
                    control::Control::Normal,
                    SaveExpectation::SavedNoop,
                    cx,
                )
                .await;
                assert_eq!(same_survivor, survivor);
                *qualified_selection.borrow_mut() = Some((survivor_id, selected.thread_id()));
                let before = snapshot(&owner);
                let surviving_record = before
                    .windows()
                    .iter()
                    .find(|record| record.window_id() == survivor_id)
                    .unwrap()
                    .clone();
                let closing = windows(&owner)[1];
                let closing_id = before.windows()[1].window_id();
                assert_ne!(closing_id, survivor_id);
                wait_for_unviewed_catalog_source(
                    &owner,
                    before.windows()[1].selected_thread().unwrap().thread_id(),
                    cx,
                )
                .await;
                assert!(
                    cx.update(|app| {
                        closing
                            .read(app)
                            .unwrap()
                            .test_thread_creation_composer_adopted_custody_items(app)
                    })
                    .unwrap()
                        > 0,
                    "nonfinal close must start with actual adopted Page custody"
                );
                let survivor_native = native(survivor, cx).await;
                let survivor_placement = capture(survivor, cx).await;
                let closing_native = native(closing, cx).await;
                let closing_identity = cx
                    .update(|app| {
                        let root = closing.read(app).unwrap();
                        let mount = root.controller().unwrap().composer_mount().unwrap();
                        let composer = mount.read(app).contribution().unwrap();
                        composer.read(app).selection_identity()
                    })
                    .unwrap();
                let page_release = Rc::new(RefCell::new(Vec::new()));
                let page_observation =
                    crate::main_window::MainWindowShell::test_observe_nonfinal_page_release(
                        closing_id,
                        page_release.clone(),
                    )
                    .unwrap();
                let resident = composer(survivor, cx).await;
                let (identity, input, selection, pages) = cx
                    .update(|app| {
                        let resident = resident.read(app);
                        let input = resident.gpui_input();
                        let surface = input.read(app).surface().unwrap();
                        (
                            resident.selection_identity(),
                            input.clone(),
                            surface.selection(),
                            surface
                                .pages()
                                .iter()
                                .map(|page| (page.range(), page.text().to_owned()))
                                .collect::<Vec<_>>(),
                        )
                    })
                    .unwrap();
                assert!(
                    owner
                        .borrow()
                        .test_running_home_recovery_identity()
                        .is_none()
                );
                post_close(closing_native);
                wait_until(
                    cx,
                    || {
                        owner.borrow().test_ordinary_command_status().0 == 1
                            && !owner.borrow().exit_requested()
                    },
                    "Page-bearing recovered nonfinal native close",
                )
                .await;
                assert!(!unsafe { IsWindow(Some(closing_native)).as_bool() });
                let released = page_release.borrow();
                assert!(
                    released.iter().any(|(_, _, _, accepted)| *accepted > 0),
                    "new Healthy nonfinal close group received no Accepted Page release"
                );
                for (selection, environment, generation, _) in released.iter() {
                    assert_eq!(selection.window_id(), closing_id);
                    assert_eq!(selection.claim(), closing_identity.claim());
                    let original = selection.binding();
                    let current = closing_identity.binding();
                    assert_eq!(original.home_id(), current.home_id());
                    assert_eq!(original.home_generation(), current.home_generation());
                    assert_eq!(original.host_generation(), current.host_generation());
                    assert_eq!(
                        original.candidate().draft_id(),
                        current.candidate().draft_id()
                    );
                    assert_eq!(
                        original.candidate().session_id(),
                        current.candidate().session_id()
                    );
                    assert_eq!(
                        original.candidate().session_generation(),
                        current.candidate().session_generation()
                    );
                    assert_eq!(
                        original.presentation_generation(),
                        current.presentation_generation()
                    );
                    assert_ne!(*environment, 0);
                    assert_ne!(*generation, 0);
                }
                drop(released);
                drop(page_observation);
                assert!(unsafe { IsWindow(Some(survivor_native)).as_bool() });
                assert_eq!(windows(&owner), vec![survivor]);
                assert_eq!(snapshot(&owner).windows(), &[surviving_record]);
                assert_eq!(native(survivor, cx).await, survivor_native);
                assert_eq!(capture(survivor, cx).await, survivor_placement);
                assert_eq!(composer(survivor, cx).await, resident);
                cx.update(|app| {
                    let resident = resident.read(app);
                    assert_eq!(resident.selection_identity(), identity);
                    assert_eq!(resident.gpui_input(), input);
                    let input = input.read(app);
                    assert!(input.is_enabled());
                    assert!(input.is_quiescent());
                    assert!(input.is_surface_current_and_interactive());
                    let surface = input.surface().unwrap();
                    assert_eq!(surface.selection(), selection);
                    for page in surface.pages() {
                        assert_eq!(
                            page.key().binding(),
                            identity.binding().range_binding().binding()
                        );
                        assert_eq!(
                            page.key().revision(),
                            identity.binding().range_binding().revision()
                        );
                    }
                    assert_eq!(
                        surface
                            .pages()
                            .iter()
                            .map(|page| (page.range(), page.text().to_owned()))
                            .collect::<Vec<_>>(),
                        pages
                    );
                })
                .unwrap();
                assert!(
                    owner
                        .borrow()
                        .test_running_home_recovery_identity()
                        .is_none()
                );
                assert!(!owner.borrow().test_services().running_selection_pending());
                assert!(
                    owner
                        .borrow()
                        .test_last_ordinary_command_failure()
                        .is_none()
                );
                assert!(owner.borrow().shutdown_status().is_none());
                activate_exit(survivor, cx);
            })
        },
        true,
        1,
    );
}
