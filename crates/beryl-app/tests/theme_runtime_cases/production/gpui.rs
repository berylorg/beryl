use super::*;
use beryl_app::theme_runtime::{
    AppearanceGeneration, AppearancePublicationFailure, AppearancePublicationTarget,
    GpuiAppearanceWindowSet, WindowAdapterId,
};
use gpui::{Entity, WindowHandle};
use std::sync::{Arc, atomic::Ordering};

#[path = "gpui_fixture.rs"]
mod fixture;
use fixture::{
    AdapterControls, ComposerFixture, TestRoot, color, draw, join_worker, on_worker, open_root,
};

struct Mounted {
    fixture: Arc<RuntimeFixture>,
    runtime: Option<ThemeRuntime>,
    _composers: [ComposerFixture; 2],
    windows: [WindowHandle<TestRoot>; 2],
    owner: Entity<GpuiAppearanceWindowSet>,
    target: Arc<beryl_app::theme_runtime::GpuiAppearancePublicationTarget>,
    controls: [AdapterControls; 2],
}

impl Mounted {
    fn new(cx: &mut gpui::TestAppContext) -> Self {
        let (fixture, runtime, composers, prepared) = on_worker(cx, || {
            let fixture = RuntimeFixture::new();
            install_active(&fixture);
            execute_settings(&fixture, SettingValue::active_theme_id("active").unwrap());
            let revision = fixture.state.settings().revision(&fixture.store).unwrap();
            let runtime = ThemeRuntime::start(
                &fixture.store,
                fixture.state.themes(),
                revision,
                Some(&fixture.active_record()),
                config(),
            )
            .unwrap();
            let composers = [ComposerFixture::new(31), ComposerFixture::new(91)];
            let prepared = [composers[0].prepared(), composers[1].prepared()];
            (Arc::new(fixture), runtime, composers, prepared)
        });
        let current = runtime.current().unwrap();
        let owner = cx.update(|app| {
            GpuiAppearanceWindowSet::new(current.clone(), NonZeroUsize::new(4).unwrap(), app)
        });
        let target = owner.read_with(cx, |owner, _| owner.target());
        let runtime = on_worker(cx, {
            let target = target.clone();
            move || {
                let mut runtime = runtime;
                runtime.attach_publication_target(target).unwrap();
                runtime
            }
        });
        let [first, second] = prepared;
        let windows = [
            open_root(&composers[0], first, current.clone(), cx),
            open_root(&composers[1], second, current, cx),
        ];
        let controls = [AdapterControls::default(), AdapterControls::default()];
        for i in 0..2 {
            draw(windows[i], cx);
            cx.update(|app| {
                owner.update(app, |owner, app| {
                    owner
                        .register(
                            Box::new(fixture::TestAdapter {
                                id: WindowAdapterId::new(NonZeroU64::new(i as u64 + 1).unwrap()),
                                window: windows[i],
                                target: target.clone(),
                                controls: controls[i].clone(),
                            }),
                            app,
                        )
                        .unwrap()
                })
            });
            draw(windows[i], cx);
        }
        Self {
            fixture,
            runtime: Some(runtime),
            _composers: composers,
            windows,
            owner,
            target,
            controls,
        }
    }

    fn run<T: Send + 'static>(
        &mut self,
        cx: &mut gpui::TestAppContext,
        work: impl FnOnce(&mut ThemeRuntime, &RuntimeFixture) -> T + Send + 'static,
    ) -> T {
        let mut runtime = self.runtime.take().unwrap();
        let fixture = self.fixture.clone();
        let (runtime, result) = on_worker(cx, move || {
            let result = work(&mut runtime, &fixture);
            (runtime, result)
        });
        self.runtime = Some(runtime);
        result
    }

    fn assert_generation(
        &self,
        expected: &Arc<AppearanceGeneration>,
        cx: &mut gpui::TestAppContext,
    ) {
        for window in self.windows {
            draw(window, cx);
            window
                .read_with(cx, |root, app| {
                    assert!(Arc::ptr_eq(&root.generation, expected));
                    assert_eq!(root.color, color(expected));
                    assert!(root.mount.read(app).selected_first_presentable(app));
                    let scene = root.snapshot.borrow();
                    let scene = scene.as_ref().unwrap();
                    assert!(
                        scene
                            .backgrounds
                            .iter()
                            .any(|(_, background)| *background == color(expected).into())
                    );
                    assert!(
                        scene
                            .glyphs
                            .iter()
                            .any(|(_, foreground)| *foreground == color(expected).into()),
                        "expected {:?}, glyph colors {:?}",
                        color(expected),
                        scene
                            .glyphs
                            .iter()
                            .map(|(_, color)| color)
                            .collect::<Vec<_>>()
                    );
                })
                .unwrap();
        }
    }
}

#[gpui::test]
fn phase294_actual_windows_adopt_every_runtime_source_and_preserve_editors(
    cx: &mut gpui::TestAppContext,
) {
    let mut mounted = Mounted::new(cx);
    let initial = mounted.runtime.as_ref().unwrap().current().unwrap();
    mounted.assert_generation(&initial, cx);
    let before = mounted.windows.map(|window| {
        window
            .read_with(cx, |root, app| {
                let owner = root.mount.read(app).contribution().unwrap();
                (
                    owner.entity_id(),
                    owner.read(app).surface_snapshot(app).unwrap(),
                )
            })
            .unwrap()
    });
    let updated = mounted.run(cx, |runtime, fixture| {
        replace_active_bytes(fixture, UPDATED_DOCUMENT);
        let result = runtime
            .consume_change_hints(
                &fixture.store,
                &[ThemeChangeHint::DocumentChanged(
                    InstalledThemeId::new("active").unwrap(),
                )],
            )
            .unwrap();
        assert_eq!(result.appearance(), RepositoryAppearanceResult::Published);
        runtime.current().unwrap()
    });
    assert_ne!(color(&initial), color(&updated));
    mounted.assert_generation(&updated, cx);
    let preview = mounted.run(cx, |runtime, _| {
        let request = runtime
            .begin_preview(
                PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(11).unwrap()),
                PreviewCandidateIdentity::Digest(ThemeDocumentDigest::from_bytes([11; 32])),
            )
            .unwrap();
        let candidate = request.candidate().clone();
        let prepared = beryl_state::PreparedThemeAppearance::fallback(
            runtime.current().unwrap().prepared().settings(),
        );
        runtime
            .publish_preview(request, PreparedPreviewAppearance::new(candidate, prepared))
            .unwrap();
        runtime.current().unwrap()
    });
    mounted.assert_generation(&preview, cx);
    let replacement = mounted.run(cx, |runtime, _| {
        let request = runtime
            .begin_preview(
                PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(12).unwrap()),
                PreviewCandidateIdentity::Digest(ThemeDocumentDigest::from_bytes([12; 32])),
            )
            .unwrap();
        let candidate = request.candidate().clone();
        runtime
            .publish_preview(
                request,
                PreparedPreviewAppearance::new(
                    candidate,
                    runtime.current().unwrap().prepared().clone(),
                ),
            )
            .unwrap();
        runtime.current().unwrap()
    });
    assert_ne!(preview.number(), replacement.number());
    mounted.assert_generation(&replacement, cx);
    let stopped = mounted.run(cx, |runtime, _| {
        runtime.stop_preview().unwrap();
        runtime.current().unwrap()
    });
    mounted.assert_generation(&stopped, cx);
    assert_eq!(color(&stopped), color(&updated));
    mounted.controls[1].reject.store(true, Ordering::SeqCst);
    mounted.run(cx, |runtime, fixture| {
        let committed = fixture
            .state
            .themes()
            .settings_identity(DomainRevision::new(17).unwrap(), None);
        let outcome = ConfirmedSettingsTheme::new(
            ThemeDraftIdentity::new(NonZeroU64::new(19).unwrap()),
            ThemeDraftRevision::INITIAL,
            committed,
            None,
            beryl_state::PreparedThemeAppearance::fallback(committed),
        );
        assert_eq!(
            runtime.consume_settings_outcome(SettingsThemeOutcome::Committed(outcome)),
            Err(ThemeRuntimeFailureClass::Publication)
        );
    });
    mounted.assert_generation(&stopped, cx);
    mounted.controls[1].reject.store(false, Ordering::SeqCst);
    let retried = mounted.run(cx, |runtime, _| {
        assert_eq!(
            runtime.retry_current_durable().unwrap(),
            RepositoryAppearanceResult::Published
        );
        runtime.current().unwrap()
    });
    mounted.assert_generation(&retried, cx);
    for (i, window) in mounted.windows.iter().enumerate() {
        window
            .read_with(cx, |root, app| {
                let owner = root.mount.read(app).contribution().unwrap();
                assert_eq!(
                    (
                        owner.entity_id(),
                        owner.read(app).surface_snapshot(app).unwrap()
                    ),
                    before[i]
                );
            })
            .unwrap();
    }
    mounted.run(cx, |runtime, _| runtime.retire());
    cx.run_until_parked();
    assert!(!mounted.target.snapshot().active);
    assert_eq!(mounted.target.snapshot().count, 0);
}

#[gpui::test]
fn phase294_reentrant_snapshot_and_retirement_preserve_whole_adoption(
    cx: &mut gpui::TestAppContext,
) {
    let mut mounted = Mounted::new(cx);
    let initial = mounted.runtime.as_ref().unwrap().current().unwrap();
    mounted.controls[1]
        .retire_during_prepare
        .store(true, Ordering::SeqCst);
    mounted.run(cx, |runtime, _| {
        let request = runtime
            .begin_preview(
                PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(1).unwrap()),
                PreviewCandidateIdentity::Digest(ThemeDocumentDigest::from_bytes([1; 32])),
            )
            .unwrap();
        let candidate = request.candidate().clone();
        assert!(
            runtime
                .publish_preview(
                    request,
                    PreparedPreviewAppearance::new(
                        candidate,
                        runtime.current().unwrap().prepared().clone()
                    )
                )
                .is_err()
        );
    });
    mounted.assert_generation(&initial, cx);
    assert!(!mounted.target.snapshot().active);

    let mut committing = Mounted::new(cx);
    committing.controls[0]
        .retire_during_commit
        .store(true, Ordering::SeqCst);
    let final_generation = committing.run(cx, |runtime, _| {
        let request = runtime
            .begin_preview(
                PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(2).unwrap()),
                PreviewCandidateIdentity::Digest(ThemeDocumentDigest::from_bytes([2; 32])),
            )
            .unwrap();
        let candidate = request.candidate().clone();
        runtime
            .publish_preview(
                request,
                PreparedPreviewAppearance::new(
                    candidate,
                    runtime.current().unwrap().prepared().clone(),
                ),
            )
            .unwrap();
        runtime.current().unwrap()
    });
    committing.assert_generation(&final_generation, cx);
    assert!(!committing.target.snapshot().active);
    assert_eq!(committing.target.snapshot().count, 0);
}

#[gpui::test]
fn phase294_final_validation_rejects_a_root_closed_during_later_preparation(
    cx: &mut gpui::TestAppContext,
) {
    let mut mounted = Mounted::new(cx);
    let initial = mounted.runtime.as_ref().unwrap().current().unwrap();
    *mounted.controls[1].close_during_prepare.borrow_mut() = Some(mounted.windows[0]);
    mounted.run(cx, |runtime, _| {
        let request = runtime
            .begin_preview(
                PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(3).unwrap()),
                PreviewCandidateIdentity::Digest(ThemeDocumentDigest::from_bytes([3; 32])),
            )
            .unwrap();
        let candidate = request.candidate().clone();
        assert!(
            runtime
                .publish_preview(
                    request,
                    PreparedPreviewAppearance::new(
                        candidate,
                        runtime.current().unwrap().prepared().clone()
                    )
                )
                .is_err()
        );
    });
    mounted.windows[1]
        .read_with(cx, |root, _| {
            assert!(Arc::ptr_eq(&root.generation, &initial))
        })
        .unwrap();
    assert!(Arc::ptr_eq(
        &mounted.runtime.as_ref().unwrap().current().unwrap(),
        &initial
    ));
}

#[gpui::test]
fn phase294_gui_call_rejects_before_waiting_or_reading_repository(cx: &mut gpui::TestAppContext) {
    let mut mounted = Mounted::new(cx);
    assert!(matches!(
        mounted
            .runtime
            .as_mut()
            .unwrap()
            .consume_change_hints(&mounted.fixture.store, &[ThemeChangeHint::ManifestChanged]),
        Err(ThemeRuntimeFailureClass::Publication)
    ));
    assert!(matches!(
        mounted.runtime.as_mut().unwrap().stop_preview(),
        Err(
            beryl_app::theme_runtime::PreviewPublicationError::WindowSet(
                AppearancePublicationFailure::Reentrant
            )
        )
    ));
    assert_eq!(mounted.target.snapshot().count, 2);
}

fn queue_preview(
    mounted: &mut Mounted,
) -> std::thread::JoinHandle<(
    ThemeRuntime,
    Result<
        beryl_app::theme_runtime::PreviewPublicationResult,
        beryl_app::theme_runtime::PreviewPublicationError,
    >,
)> {
    let mut runtime = mounted.runtime.take().unwrap();
    let worker = std::thread::Builder::new()
        .name("phase294-pending-publication".to_owned())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let request = runtime
                .begin_preview(
                    PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(51).unwrap()),
                    PreviewCandidateIdentity::Digest(ThemeDocumentDigest::from_bytes([51; 32])),
                )
                .unwrap();
            let candidate = request.candidate().clone();
            let result = runtime.publish_preview(
                request,
                PreparedPreviewAppearance::new(
                    candidate,
                    runtime.current().unwrap().prepared().clone(),
                ),
            );
            (runtime, result)
        })
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while mounted.target.pending_publications() == 0 {
        assert!(
            std::time::Instant::now() < deadline && !worker.is_finished(),
            "worker must queue one publication"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    worker
}

#[gpui::test]
fn phase294_pending_publication_has_one_slot_and_exact_epoch_and_retirement_fences(
    cx: &mut gpui::TestAppContext,
) {
    let mut mounted = Mounted::new(cx);
    let initial = mounted.runtime.as_ref().unwrap().current().unwrap();
    let worker = queue_preview(&mut mounted);
    let target = mounted.target.clone();
    let captured = target.snapshot();
    let overflow = std::thread::spawn(move || {
        target.publish(captured.epoch, captured.current.clone(), captured.current)
    })
    .join()
    .unwrap();
    assert_eq!(overflow, Err(AppearancePublicationFailure::CapacityReached));
    assert_eq!(mounted.target.pending_publications(), 1);
    cx.update(|app| {
        mounted.owner.update(app, |owner, _| {
            owner
                .unregister(WindowAdapterId::new(NonZeroU64::new(1).unwrap()))
                .unwrap()
        })
    });
    let (runtime, result) = join_worker(cx, worker);
    mounted.runtime = Some(runtime);
    assert!(matches!(
        result,
        Err(beryl_app::theme_runtime::PreviewPublicationError::Stale(
            beryl_app::theme_runtime::StalePublicationReason::WindowSetEpoch
        ))
    ));
    mounted.assert_generation(&initial, cx);
    assert_eq!(mounted.target.pending_publications(), 0);

    let (third_composer, prepared) = on_worker(cx, || {
        let composer = ComposerFixture::new(141);
        let prepared = composer.prepared();
        (composer, prepared)
    });
    let third = open_root(&third_composer, prepared, initial.clone(), cx);
    draw(third, cx);
    let worker = queue_preview(&mut mounted);
    cx.update(|app| {
        mounted.owner.update(app, |owner, app| {
            owner
                .register(
                    Box::new(fixture::TestAdapter {
                        id: WindowAdapterId::new(NonZeroU64::new(3).unwrap()),
                        window: third,
                        target: mounted.target.clone(),
                        controls: AdapterControls::default(),
                    }),
                    app,
                )
                .unwrap()
        })
    });
    let (runtime, result) = join_worker(cx, worker);
    mounted.runtime = Some(runtime);
    assert!(matches!(
        result,
        Err(beryl_app::theme_runtime::PreviewPublicationError::Stale(
            beryl_app::theme_runtime::StalePublicationReason::WindowSetEpoch
        ))
    ));
    third
        .read_with(cx, |root, _| {
            assert!(Arc::ptr_eq(&root.generation, &initial))
        })
        .unwrap();
    mounted.assert_generation(&initial, cx);
    assert_eq!(mounted.target.pending_publications(), 0);

    let worker = queue_preview(&mut mounted);
    mounted.target.retire();
    let (runtime, result) = join_worker(cx, worker);
    mounted.runtime = Some(runtime);
    assert!(matches!(
        result,
        Err(
            beryl_app::theme_runtime::PreviewPublicationError::WindowSet(
                AppearancePublicationFailure::Unavailable
            )
        )
    ));
    mounted.assert_generation(&initial, cx);
    cx.run_until_parked();
    assert!(!mounted.target.snapshot().active);
    assert_eq!(mounted.target.snapshot().count, 0);
    assert_eq!(mounted.target.pending_publications(), 0);
}
