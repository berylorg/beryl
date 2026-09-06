use super::*;
use gpui::{AppContext, EntityInputHandler};

#[gpui::test]
fn prepared_transfer_uses_real_hidden_host_and_preserves_post_native_failure_custody(
    cx: &mut gpui::TestAppContext,
) {
    use beryl_app::composer_marker_seal::{DraftMarkerSealService, DraftMarkerSealServiceLimits};
    use beryl_app::theme_runtime::{
        AppearanceCoordinator, AppearanceCoordinatorConfig, GpuiAppearanceWindowSet,
    };
    use std::num::NonZeroUsize;
    cx.update(gpui_text_input::ensure_text_input_bindings);
    for mount_failure in [false, true] {
        let (fixture, prepared, appearance) = home_support::join(
            home_support::worker(move || {
                let fixture = Fixture::new(if mount_failure { 151 } else { 141 });
                let mut custody = fixture.begin(if mount_failure { 152 } else { 142 });
                assert_eq!(
                    custody.advance(&CommandCancellation::new()).unwrap(),
                    MainWindowInitialComposerProgress::Activated
                );
                let prepared = custody
                    .prepare(&mut config)
                    .unwrap_or_else(|failure| panic!("{}", failure.error));
                let coordinator = AppearanceCoordinator::new(
                    AppearanceCoordinatorConfig::new(NonZeroUsize::new(4).unwrap()),
                    beryl_state::PreparedThemeAppearance::fallback(
                        fixture
                            .state
                            .themes()
                            .settings_identity(beryl_model::DomainRevision::new(1).unwrap(), None),
                    ),
                );
                let appearance = coordinator.current();
                let seals = DraftMarkerSealService::new(
                    &fixture.store,
                    fixture.store.health().generation().unwrap(),
                    fixture.storage.clone(),
                    fixture.state.assets(),
                    DraftMarkerSealServiceLimits::new(
                        NonZeroUsize::new(1).unwrap(),
                        NonZeroUsize::new(1).unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap();
                let submission = MainWindowComposerSubmissionRequestSource::new(
                    beryl_app::cas_projection::ProjectionServiceConfig::try_new(
                        1,
                        4,
                        beryl_home_store::MinimumTurnCaptureReserve::try_new(1).unwrap(),
                    )
                    .unwrap()
                    .turn_start_admission_requirement(),
                );
                let prepared = prepared
                    .into_shell(Box::new(config), seals, submission, appearance.clone())
                    .unwrap_or_else(|failure| panic!("{}", failure.error));
                assert_eq!(fixture.process.main_window_occupancy(), 1);
                (fixture, prepared, appearance)
            }),
            cx,
        );
        let owner = cx.update(|app| {
            GpuiAppearanceWindowSet::new(appearance, NonZeroUsize::new(4).unwrap(), app)
        });
        let result = cx.update(|app| {
            let mut host = GpuiMainWindowShellHost::new(app, owner.clone());
            if mount_failure {
                host.test_reject_mount_after_native();
            }
            host.construct_hidden(prepared)
        });
        let unpublished = if mount_failure {
            result
                .err()
                .expect("post-native failure")
                .into_unpublished()
        } else {
            let shell = result.unwrap_or_else(|_| panic!("ordinary production hidden host"));
            for _ in 0..32 {
                cx.run_until_parked();
                cx.update(|app| {
                    app.update_window(shell.window().into(), |_, window, app| {
                        window.draw(app).clear()
                    })
                    .unwrap()
                });
            }
            assert!(!cx.window_visibility(shell.window().into()).is_visible);
            shell
                .window()
                .read_with(cx, |root, app| {
                    assert!(
                        root.controller()
                            .unwrap()
                            .composer_mount()
                            .unwrap()
                            .read(app)
                            .selected_first_presentable(app)
                    )
                })
                .unwrap();
            cx.update(|app| shell.close_before_publication(app))
                .unwrap_or_else(|_| panic!("unpublished shell cleanup"))
        };
        assert_eq!(fixture.process.main_window_occupancy(), 1);
        if mount_failure {
            let (foreign, foreign_custody) = home_support::join(
                home_support::worker(|| {
                    let foreign = Fixture::new(151);
                    let mut custody = foreign.begin(152);
                    assert_eq!(
                        custody.advance(&CommandCancellation::new()).unwrap(),
                        MainWindowInitialComposerProgress::Activated
                    );
                    (foreign, custody)
                }),
                cx,
            );
            let draft = foreign_custody.acquisition().draft_id();
            let foreign_service = foreign.service.clone();
            let refusal = home_support::join(
                home_support::worker(move || {
                    unpublished.prepare_abandonment(&foreign_service, CommandCancellation::new())
                }),
                cx,
            );
            let MainWindowShellAbandonmentPreparationOutcome::NotCommitted { unpublished, .. } =
                refusal
            else {
                panic!("foreign window cleanup refuses after original candidate settlement")
            };
            assert!(matches!(
                fixture.session(draft, 152),
                DraftEditorCandidateSessionReadOutcomeV1::Disposed(_)
            ));
            assert!(matches!(
                foreign.session(draft, 152),
                DraftEditorCandidateSessionReadOutcomeV1::Active(_)
            ));
            assert_eq!(fixture.process.main_window_occupancy(), 1);
            assert_eq!(foreign.process.main_window_occupancy(), 1);
            let service = fixture.service.clone();
            home_support::join(
                home_support::worker(move || {
                    let MainWindowShellAbandonmentPreparationOutcome::ExactAcquired { abandonment } =
                        unpublished.prepare_abandonment(&service, CommandCancellation::new())
                    else {
                        panic!("original window cleanup retains exact custody")
                    };
                    assert!(matches!(
                        abandonment.abandon(&service, CommandCancellation::new()),
                        MainWindowShellAbandonmentOutcome::Committed { .. }
                    ));
                    foreign.retire_and_release(foreign_custody);
                }),
                cx,
            );
            assert_eq!(fixture.process.main_window_occupancy(), 0);
            continue;
        }
        let service = fixture.service.clone();
        let cancellation = CommandCancellation::new();
        cancellation.cancel();
        let pending = home_support::join(
            home_support::worker(move || unpublished.prepare_abandonment(&service, cancellation)),
            cx,
        );
        let MainWindowShellAbandonmentPreparationOutcome::InitialComposerPending {
            unpublished,
            ..
        } = pending
        else {
            panic!("candidate retirement refusal preserves shell custody")
        };
        assert_eq!(fixture.process.main_window_occupancy(), 1);
        let service = fixture.service.clone();
        home_support::join(
            home_support::worker(move || {
                let MainWindowShellAbandonmentPreparationOutcome::ExactAcquired { abandonment } =
                    unpublished.prepare_abandonment(&service, CommandCancellation::new())
                else {
                    panic!("candidate then window cleanup")
                };
                assert!(matches!(
                    abandonment.abandon(&service, CommandCancellation::new()),
                    MainWindowShellAbandonmentOutcome::Committed { .. }
                ));
            }),
            cx,
        );
        assert_eq!(fixture.process.main_window_occupancy(), 0);
    }
}

#[gpui::test]
fn production_prepared_editor_realizes_and_mutation_blocks_fresh_retirement(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(gpui_text_input::ensure_text_input_bindings);
    let (fixture, prepared) = home_support::join(
        home_support::worker(|| {
            let fixture = Fixture::new(121);
            let mut custody = fixture.begin(122);
            assert_eq!(
                custody.advance(&CommandCancellation::new()).unwrap(),
                MainWindowInitialComposerProgress::Activated
            );
            let prepared = custody
                .prepare(&mut config)
                .unwrap_or_else(|failure| panic!("{}", failure.error));
            (fixture, prepared)
        }),
        cx,
    );
    let (prepared, custody) = prepared.into_parts();
    let selection = prepared.selection_identity();
    let (owner, view) = cx.add_window_view(|window, cx| {
        MainWindowConversationComposer::from_prepared(
            prepared,
            Box::new(|_, _| gpui_text_input::ClipboardWriteOutcome::Failed),
            window,
            cx,
        )
        .unwrap()
    });
    composer_support::drive(view, 32);
    assert!(owner.read_with(view, |owner, app| owner.selected_first_presentable(app)));
    assert_eq!(
        owner.read_with(view, |owner, _| owner.selection_identity()),
        selection
    );
    let input = owner.read_with(view, |owner, _| owner.gpui_input());
    view.update(|window, app| {
        input.update(app, |input, cx| {
            input.replace_text_in_range(None, "retained edit", window, cx)
        })
    });
    composer_support::drive(view, 32);
    let worker = home_support::worker(move || custody.retire(CommandCancellation::new()));
    let outcome = worker.join().unwrap();
    let MainWindowInitialComposerRetirement::Pending(failure) = outcome else {
        panic!("mutated candidate must retain operation custody")
    };
    assert!(failure.error.contains("fresh"), "{}", failure.error);
    assert_eq!(fixture.process.main_window_occupancy(), 1);
    assert!(matches!(
        fixture.session(selection.binding().candidate().draft_id(), 122),
        DraftEditorCandidateSessionReadOutcomeV1::Active(_)
    ));
}
