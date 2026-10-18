use super::*;
use crate::main_window::{
    MainWindowNoticeRouteRejection, MainWindowNoticeWidgetEvent, NoticeAdmission,
    NoticeConditionId, NoticeContent, NoticeDismissal, NoticeKind, NoticeRecord, NoticeRejection,
    NoticeVariant,
};
use crate::theme_runtime::AppearancePublicationTarget;

pub(in crate::main_window::shell::host) fn prepare(
    candidate: &HomeRecoveryCandidate,
) -> Arc<AppearanceGeneration> {
    let state = BerylState::reacquire_candidate(candidate).unwrap();
    AppearanceCoordinator::new(
        AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
        PreparedThemeAppearance::fallback(
            state
                .themes()
                .settings_identity(DomainRevision::new(1).unwrap(), None),
        ),
    )
    .current()
}

fn record() -> NoticeRecord {
    NoticeRecord {
        window_id: window_id(),
        condition: NoticeConditionId::new(),
        revision: 1,
        kind: NoticeKind::Warning,
        content: NoticeContent::new(
            NoticeVariant::Warning,
            NoticeDismissal::Dismissible,
            "recovery notice",
            "recovery notice",
        ),
    }
}

fn pending_registration(
    shell: &MainWindowShell,
    owner: &Entity<GpuiAppearanceWindowSet>,
    guard: Rc<dyn Fn(&mut App) -> Result<(), String>>,
    cx: &mut TestAppContext,
) -> (
    Result<(), crate::theme_runtime::AdapterRegistrationError>,
    usize,
) {
    let window = shell.window();
    let minimum = window
        .read_with(cx, |root, _| root.controller().unwrap().minimum_size())
        .unwrap();
    cx.simulate_window_resize(
        window.into(),
        gpui::size(minimum.width * 0.5, minimum.height * 0.5),
    );
    let commits = Rc::new(std::cell::Cell::new(0));
    let commit_count = commits.clone();
    let registration = cx
        .update(|app| {
            shell.begin_bind_interrupted_exit_appearance(
                owner,
                guard,
                Rc::new(move |_| commit_count.set(commit_count.get() + 1)),
                app,
            )
        })
        .unwrap();
    let result = Rc::new(RefCell::new(None));
    let completion = result.clone();
    cx.update(|app| {
        app.spawn(async move |_| *completion.borrow_mut() = Some(registration.await))
            .detach();
    });
    cx.run_until_parked();
    window
        .update(cx, |_, window, _| {
            assert!(sizing::contains(window.viewport_size(), minimum));
            assert!(sizing::contains(
                window.minimum_size_for_test().unwrap(),
                minimum
            ));
        })
        .unwrap();
    let result = result
        .borrow_mut()
        .take()
        .expect("original native registration settled");
    (result, commits.get())
}

pub(super) fn verify_retained(
    shell: &mut MainWindowShell,
    fresh: Arc<AppearanceGeneration>,
    cx: &mut TestAppContext,
) -> Entity<GpuiAppearanceWindowSet> {
    let window = shell.window();
    let old_owner = shell.appearance_owner.clone();
    let owner = cx.update(|app| {
        GpuiAppearanceWindowSet::new(fresh.clone(), NonZeroUsize::new(1).unwrap(), app)
    });
    assert!(
        cx.update(|app| shell.bind_interrupted_exit_appearance(&owner, app))
            .is_err()
    );
    assert_eq!(shell.appearance_owner.entity_id(), old_owner.entity_id());
    cx.update(|app| old_owner.update(app, |owner, _| owner.retire()));
    shell.published = false;
    assert!(
        cx.update(|app| shell.bind_interrupted_exit_appearance(&owner, app))
            .is_err()
    );
    shell.published = true;
    shell.appearance_registered.set(false);
    assert!(
        cx.update(|app| shell.bind_interrupted_exit_appearance(&owner, app))
            .is_err()
    );
    shell.appearance_registered.set(true);
    assert_eq!(shell.appearance_owner.entity_id(), old_owner.entity_id());
    assert!(
        cx.update(|app| MainWindowShellRoot::bind_interrupted_exit_appearance(window, &owner, app))
            .is_err()
    );
    window
        .update(cx, |root, _, cx| {
            root.appearance_release = Some(cx.on_release(|_, _| {}));
        })
        .unwrap();
    let prior = window
        .read_with(cx, |root, _| {
            (
                root.controller().unwrap().appearance.generation.clone(),
                root.controller().unwrap().window_id(),
            )
        })
        .unwrap();
    let cancellation = CommandCancellation::new();
    let cancel = cancellation.clone();
    let admitted = Rc::new(std::cell::Cell::new(false));
    let validations = Rc::new(std::cell::Cell::new(0));
    let validated = validations.clone();
    let cancelled_owner = cx.update(|app| {
        GpuiAppearanceWindowSet::new(fresh.clone(), NonZeroUsize::new(1).unwrap(), app)
    });
    let (result, commits) = pending_registration(
        shell,
        &cancelled_owner,
        Rc::new(move |app| {
            validated.set(validated.get() + 1);
            if !admitted.replace(true) {
                let cancel = cancel.clone();
                app.defer(move |_| cancel.cancel());
            }
            if cancellation.is_cancelled() {
                Err("original registration cancelled".into())
            } else {
                Ok(())
            }
        }),
        cx,
    );
    assert!(matches!(
        result,
        Err(
            crate::theme_runtime::AdapterRegistrationError::Preparation {
                class: crate::theme_runtime::AdapterFailureClass::Unavailable,
                ..
            }
        )
    ));
    assert_eq!(commits, 0);
    assert_eq!(validations.get(), 2);
    assert_eq!(
        cx.update(|app| cancelled_owner.read(app).target().snapshot().count),
        0
    );
    assert_eq!(shell.appearance_owner.entity_id(), old_owner.entity_id());
    let retired_owner = cx.update(|app| {
        GpuiAppearanceWindowSet::new(fresh.clone(), NonZeroUsize::new(1).unwrap(), app)
    });
    let retire = retired_owner.clone();
    let admitted = Rc::new(std::cell::Cell::new(false));
    let preparations = Rc::new(std::cell::Cell::new(0));
    let prepared = preparations.clone();
    let (result, commits) = pending_registration(
        shell,
        &retired_owner,
        Rc::new(move |app| {
            prepared.set(prepared.get() + 1);
            if !admitted.replace(true) {
                let retire = retire.clone();
                app.defer(move |app| retire.update(app, |owner, _| owner.retire()));
            }
            Ok(())
        }),
        cx,
    );
    assert!(matches!(
        result,
        Err(
            crate::theme_runtime::AdapterRegistrationError::Preparation {
                class: crate::theme_runtime::AdapterFailureClass::Unavailable,
                ..
            }
        )
    ));
    assert_eq!(commits, 0);
    assert_eq!(preparations.get(), 1);
    assert_eq!(
        cx.update(|app| retired_owner.read(app).target().snapshot().count),
        0
    );
    window
        .read_with(cx, |root, _| {
            assert!(Arc::ptr_eq(
                &root.controller().unwrap().appearance.generation,
                &prior.0
            ));
            assert_eq!(root.controller().unwrap().window_id(), prior.1);
        })
        .unwrap();
    assert_eq!(shell.appearance_owner.entity_id(), old_owner.entity_id());
    let validations = Rc::new(std::cell::Cell::new(0));
    let validated = validations.clone();
    let (result, commits) = pending_registration(
        shell,
        &owner,
        Rc::new(move |_| {
            validated.set(validated.get() + 1);
            Ok(())
        }),
        cx,
    );
    result.unwrap();
    assert_eq!(commits, 1);
    assert_eq!(validations.get(), 2);
    shell.complete_interrupted_exit_appearance_binding(owner.clone());
    assert_eq!(shell.appearance_owner.entity_id(), owner.entity_id());
    assert!(
        cx.update(|app| shell.bind_interrupted_exit_appearance(&owner, app))
            .is_err()
    );
    window
        .read_with(cx, |root, _| {
            assert!(root.shutdown_interaction_gated);
            assert!(root.appearance_release.is_some());
            assert!(Arc::ptr_eq(
                &root.controller.as_ref().unwrap().appearance.generation,
                &fresh
            ));
        })
        .unwrap();
    assert_eq!(
        cx.update(|app| owner.read(app).target().snapshot().count),
        1
    );
    owner
}

pub(super) fn verify(
    window: WindowHandle<MainWindowShellRoot>,
    old_owner: Entity<GpuiAppearanceWindowSet>,
    fresh: Arc<AppearanceGeneration>,
    cx: &mut TestAppContext,
) -> Entity<GpuiAppearanceWindowSet> {
    cx.update(|app| crate::main_window::test_publish_home_open_notice_classification(true, app));
    let old_warning_timer = window
        .update(cx, |root, window, cx| {
            root.test_repeat_home_warning_trigger(window, cx);
            root.test_home_warning_timer()
                .expect("successful startup warning")
        })
        .unwrap();
    let old_ingress = window
        .update(cx, |root, window, cx| root.notice_ingress(window, cx))
        .unwrap();
    assert!(matches!(
        cx.update(|app| old_ingress.admit(record(), app)),
        NoticeAdmission::Admitted(_)
    ));
    let old_widget = window
        .read_with(cx, |root, _| root.notice_widget().downgrade())
        .unwrap();
    let old_generation = cx.update(|app| old_owner.read(app).target().snapshot().current);
    let fresh_owner = cx.update(|app| {
        GpuiAppearanceWindowSet::new(fresh.clone(), NonZeroUsize::new(1).unwrap(), app)
    });
    assert!(
        cx.update(|app| MainWindowShellRoot::bind_interrupted_exit_appearance(
            window,
            &fresh_owner,
            app
        ))
        .is_err()
    );
    assert!(
        window
            .read_with(cx, |root, _| root.notice_projection().is_some())
            .unwrap()
    );
    cx.update(|app| old_owner.update(app, |owner, _| owner.retire()));
    let stale_owner = cx.update(|app| {
        GpuiAppearanceWindowSet::new(old_generation.clone(), NonZeroUsize::new(1).unwrap(), app)
    });
    assert!(
        cx.update(|app| MainWindowShellRoot::bind_interrupted_exit_appearance(
            window,
            &stale_owner,
            app
        ))
        .is_err()
    );
    let foreign = std::thread::spawn(|| {
        let (candidate, _, directory) = Home::new().recover();
        let appearance = prepare(&candidate);
        drop(candidate.abort());
        directory.close().unwrap();
        appearance
    })
    .join()
    .unwrap();
    let foreign_owner =
        cx.update(|app| GpuiAppearanceWindowSet::new(foreign, NonZeroUsize::new(1).unwrap(), app));
    assert!(
        cx.update(|app| MainWindowShellRoot::bind_interrupted_exit_appearance(
            window,
            &foreign_owner,
            app
        ))
        .is_err()
    );
    window
        .read_with(cx, |root, _| {
            assert!(Arc::ptr_eq(
                &root.controller().unwrap().appearance.generation,
                &old_generation
            ));
            assert!(root.notice_projection().is_some());
        })
        .unwrap();
    cx.update(|app| {
        MainWindowShellRoot::bind_interrupted_exit_appearance(window, &fresh_owner, app)
    })
    .unwrap();
    assert!(
        cx.update(|app| MainWindowShellRoot::bind_interrupted_exit_appearance(
            window,
            &fresh_owner,
            app
        ))
        .is_err()
    );
    let fresh_ingress = window
        .update(cx, |root, window, cx| {
            assert!(root.shutdown_interaction_gated);
            assert!(Arc::ptr_eq(
                &root.controller().unwrap().appearance.generation,
                &fresh
            ));
            assert!(root.notice_projection().is_none());
            root.test_repeat_home_warning_trigger(window, cx);
            root.test_expire_home_warning(&old_warning_timer, window, cx);
            assert!(root.notice_projection().is_none());
            assert!(root.test_home_warning_timer().is_none());
            root.notice_ingress(window, cx)
        })
        .unwrap();
    assert!(old_widget.upgrade().is_none());
    assert!(matches!(
        cx.update(|app| old_ingress.admit(record(), app)),
        NoticeAdmission::Rejected(NoticeRejection::Disposed)
    ));
    assert!(matches!(
        cx.update(|app| fresh_ingress.admit(record(), app)),
        NoticeAdmission::Admitted(_)
    ));
    let visible = window
        .read_with(cx, |root, _| {
            root.notice_projection().unwrap().token.clone()
        })
        .unwrap();
    assert_eq!(
        cx.update(|app| fresh_ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(visible), app)),
        Err(MainWindowNoticeRouteRejection::Inert)
    );
    assert_eq!(
        cx.update(|app| fresh_owner.read(app).target().snapshot().count),
        1
    );
    assert_eq!(
        cx.update(|app| old_owner.read(app).target().snapshot().count),
        0
    );
    assert_eq!(
        cx.update(|app| stale_owner.read(app).target().snapshot().count),
        0
    );
    assert_eq!(
        cx.update(|app| foreign_owner.read(app).target().snapshot().count),
        0
    );
    fresh_owner
}
