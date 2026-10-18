use super::*;

#[gpui::test]
fn later_native_refusal_preserves_every_prior_generation_after_original_window_growth(
    cx: &mut gpui::TestAppContext,
) {
    let mut mounted = Mounted::new(cx);
    let initial = mounted.runtime.as_ref().unwrap().current().unwrap();
    let window = mounted.windows[0];
    let before = window
        .update(cx, |root, window, app| {
            let resident = root.mount.read(app).contribution().unwrap();
            (window.viewport_size(), resident.entity_id())
        })
        .unwrap();
    let minimum = gpui::size(
        before.0.width + gpui::px(80.),
        before.0.height + gpui::px(40.),
    );
    mounted.controls[0].native_minimum.set(Some(minimum));
    mounted.controls[1]
        .refuse_native
        .store(true, Ordering::SeqCst);
    mounted.run(cx, |runtime, _| {
        let request = runtime
            .begin_preview(
                PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(44).unwrap()),
                PreviewCandidateIdentity::Digest(ThemeDocumentDigest::from_bytes([44; 32])),
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
    window
        .update(cx, |root, window, app| {
            assert!(window.viewport_size().width >= minimum.width);
            assert!(window.viewport_size().height >= minimum.height);
            assert_eq!(
                root.mount.read(app).contribution().unwrap().entity_id(),
                before.1
            );
        })
        .unwrap();
    assert!(Arc::ptr_eq(&mounted.target.snapshot().current, &initial));
}

#[gpui::test]
fn hidden_registration_reprepares_current_generation_after_original_native_readiness(
    cx: &mut gpui::TestAppContext,
) {
    let mut mounted = Mounted::new(cx);
    let initial = mounted.runtime.as_ref().unwrap().current().unwrap();
    let (composer, prepared) = on_worker(cx, || {
        let composer = ComposerFixture::new(51);
        let prepared = composer.prepared();
        (composer, prepared)
    });
    let window =
        fixture::open_root_with_visibility(&composer, prepared, initial.clone(), false, cx);
    let original = window.entity(cx).unwrap().entity_id();
    let controls = AdapterControls::default();
    controls.native_hold.set(true);
    let minimum = window
        .update(cx, |_, window, _| {
            gpui::size(
                window.viewport_size().width + gpui::px(80.),
                window.viewport_size().height + gpui::px(40.),
            )
        })
        .unwrap();
    controls.native_minimum.set(Some(minimum));
    let registration = cx.update(|app| {
        GpuiAppearanceWindowSet::test_begin_hidden_registration(
            mounted.owner.clone(),
            Box::new(fixture::TestAdapter {
                id: WindowAdapterId::new(NonZeroU64::new(3).unwrap()),
                window,
                target: mounted.target.clone(),
                controls: controls.clone(),
            }),
            app,
        )
    });
    let result = std::rc::Rc::new(std::cell::RefCell::new(None));
    let completion = result.clone();
    cx.update(|app| {
        app.spawn(async move |_| *completion.borrow_mut() = Some(registration.await))
            .detach()
    });
    draw(window, cx);
    assert!(controls.native_ready.get());
    assert!(result.borrow().is_none());
    assert_eq!(mounted.target.snapshot().count, 2);
    assert!(!cx.window_visibility(window.into()).is_visible);
    let current = mounted.run(cx, |runtime, _| {
        let request = runtime
            .begin_preview(
                PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(45).unwrap()),
                PreviewCandidateIdentity::Digest(ThemeDocumentDigest::from_bytes([45; 32])),
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
    controls.native_hold.set(false);
    if let Some(waker) = controls.native_waker.borrow_mut().take() {
        waker.wake();
    }
    draw(window, cx);
    assert!(matches!(result.borrow().as_ref(), Some(Ok(()))));
    assert_eq!(mounted.target.snapshot().count, 3);
    assert_eq!(window.entity(cx).unwrap().entity_id(), original);
    assert!(!cx.window_visibility(window.into()).is_visible);
    window
        .read_with(cx, |root, _| {
            assert!(Arc::ptr_eq(&root.generation, &current))
        })
        .unwrap();
}

#[gpui::test]
fn owner_release_during_native_readiness_unblocks_publication_without_partial_commit(
    cx: &mut gpui::TestAppContext,
) {
    interrupt_native_readiness(cx, true);
}

#[gpui::test]
fn explicit_retirement_during_native_readiness_unblocks_publication_without_partial_commit(
    cx: &mut gpui::TestAppContext,
) {
    interrupt_native_readiness(cx, false);
}

fn interrupt_native_readiness(cx: &mut gpui::TestAppContext, release_owner: bool) {
    let mut mounted = Mounted::new(cx);
    let initial = mounted.runtime.as_ref().unwrap().current().unwrap();
    let editors = mounted.windows.map(|window| {
        window
            .read_with(cx, |root, app| {
                root.mount.read(app).contribution().unwrap().entity_id()
            })
            .unwrap()
    });
    let minimum = mounted.windows[0]
        .update(cx, |_, window, _| {
            gpui::size(
                window.viewport_size().width + gpui::px(80.),
                window.viewport_size().height + gpui::px(40.),
            )
        })
        .unwrap();
    mounted.controls[0].native_minimum.set(Some(minimum));
    mounted.controls[0].native_hold.set(true);
    let mut runtime = mounted.runtime.take().unwrap();
    let worker = std::thread::Builder::new()
        .name("appearance-publication-cancellation".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let request = runtime
                .begin_preview(
                    PreviewSource::DynamicTool(PreviewSourceIdentity::try_new(46).unwrap()),
                    PreviewCandidateIdentity::Digest(ThemeDocumentDigest::from_bytes([46; 32])),
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
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while !mounted.controls[0].native_ready.get() {
        assert!(
            std::time::Instant::now() < deadline,
            "native preparation did not begin"
        );
        cx.run_until_parked();
        std::thread::yield_now();
    }
    assert_eq!(mounted.target.pending_publications(), 1);
    assert!(!worker.is_finished());
    assert!(mounted.controls[0].native_hold.get());
    assert_eq!(
        mounted
            .owner
            .read_with(cx, |owner, _| owner.target().snapshot().count),
        2
    );
    mounted.assert_generation(&initial, cx);
    if release_owner {
        cx.update(|app| {
            let replacement =
                GpuiAppearanceWindowSet::new(initial.clone(), NonZeroUsize::new(1).unwrap(), app);
            drop(std::mem::replace(&mut mounted.owner, replacement));
        });
    } else {
        cx.update(|app| mounted.owner.update(app, |owner, _| owner.retire()));
    }
    cx.run_until_parked();
    assert!(!mounted.target.snapshot().active);
    assert_eq!(mounted.target.snapshot().count, 0);
    assert_eq!(mounted.target.pending_publications(), 0);
    let (runtime, result) = join_worker(cx, worker);
    mounted.runtime = Some(runtime);
    assert!(result.is_err());
    mounted.assert_generation(&initial, cx);
    mounted.windows[0]
        .update(cx, |_, window, _| {
            assert_eq!(window.minimum_size_for_test(), Some(minimum));
        })
        .unwrap();
    for (window, expected) in mounted.windows.into_iter().zip(editors) {
        window
            .read_with(cx, |root, app| {
                assert_eq!(
                    root.mount.read(app).contribution().unwrap().entity_id(),
                    expected
                )
            })
            .unwrap();
    }
    assert!(!mounted.target.snapshot().active);
    assert_eq!(mounted.target.snapshot().count, 0);
}
