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
    shell.appearance_registered = false;
    assert!(
        cx.update(|app| shell.bind_interrupted_exit_appearance(&owner, app))
            .is_err()
    );
    shell.appearance_registered = true;
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
    cx.update(|app| shell.bind_interrupted_exit_appearance(&owner, app))
        .unwrap();
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
