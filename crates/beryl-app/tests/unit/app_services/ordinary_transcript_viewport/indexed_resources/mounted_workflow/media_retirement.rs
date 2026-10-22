use super::*;
use crate::image_processing::{ImageMediaError, ImageMediaFailure};
use crate::main_window::image_inspection::original_commands::TestOriginalSavePicker;
use std::collections::VecDeque;

#[test]
fn native_image_exit_joins_pending_original_and_painted_resources_before_home_reopen() {
    require_fixture_ledger();
    let retained = Rc::new(RefCell::new(None));
    let evidence = retained.clone();
    let expected = Rc::new(RefCell::new(None));
    let prepared = expected.clone();
    run_mounted_with_prepared_home_retained_and_workers_and_deadline(
        1,
        0,
        9,
        Duration::from_secs(120),
        move |path| {
            super::image_markers::fixture::seed_png(path);
            *prepared.borrow_mut() = Some(durable_asset(path));
        },
        move |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let root = cx.update(|app| window.entity(app).unwrap()).unwrap();
                let panel = root
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                let attachment = owner.borrow().media_attachment().unwrap();
                let media = owner.borrow().test_services().media_process().unwrap();
                let (asset, proof, sidecar) = expected.borrow_mut().take().unwrap();
                let (path, old_home) = {
                    let retained = owner.borrow();
                    let graph = retained.test_services().graph().unwrap();
                    (
                        graph.home().canonical_path().to_path_buf(),
                        graph.home().service_reference(),
                    )
                };
                let picks = Rc::new(RefCell::new(VecDeque::new()));
                let receiver_queue = picks.clone();
                cx.update(|app| {
                    app.set_global(TestOriginalSavePicker(Rc::new(RefCell::new(Box::new(
                        move || {
                            receiver_queue
                                .borrow_mut()
                                .pop_front()
                                .expect("every pending image picker is injected")
                        },
                    )))));
                })
                .unwrap();
                ready(window, &panel, cx, &["before", "after"]).await;
                settled_transcript(window, &panel, cx).await;
                let marker = panel
                    .read_with(cx, |panel, _| {
                        panel
                            .snapshot()
                            .records
                            .iter()
                            .flat_map(|record| record.authored_parts())
                            .find(|part| {
                                matches!(part.kind, ResidentPresentationRecordKind::ImageMarker(_))
                            })
                            .unwrap()
                            .id
                            .clone()
                    })
                    .unwrap();
                let mut plateau = None;
                for _ in 0..2 {
                    super::original_commands::open_preview(window, &panel, &marker, cx).await;
                    wait_until(
                        window,
                        cx,
                        |cx| {
                            root.read_with(cx, |root, _| {
                                root.test_image_marker_rendition()
                                    == Some((true, true, true, false))
                            })
                            .unwrap()
                        },
                        "the repeated inspection must paint admitted original pixels",
                    )
                    .await;
                    with_native(window, cx, |native, app| {
                        assert_eq!(
                            root.read(app).test_image_marker_pixel().unwrap().unwrap(),
                            [255, 0, 0, 255]
                        );
                        key(native, app, "escape");
                    })
                    .unwrap();
                    wait_until(
                        window,
                        cx,
                        |_| {
                            media.diagnostics().unwrap().original_commands == 0
                                && media.domain().diagnostics().unique_pins == 0
                        },
                        "closed inspections must release original admission and image pins",
                    )
                    .await;
                    let work = media.diagnostics().unwrap();
                    let native = media.domain().diagnostics();
                    let current = (
                        work.cpu.entries,
                        work.cpu.bytes,
                        native.resident_textures,
                        native.texture_bytes,
                    );
                    if let Some(plateau) = plateau {
                        assert_eq!(current, plateau);
                    }
                    plateau = Some(current);
                }
                super::original_commands::open_preview(window, &panel, &marker, cx).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        root.read_with(cx, |root, _| {
                            root.test_image_marker_rendition() == Some((true, true, true, false))
                                && root
                                    .test_image_original_command(true)
                                    .is_some_and(|(reason, _)| reason.is_none())
                        })
                        .unwrap()
                    },
                    "the final inspection must retain Ready pixels and exact original authority",
                )
                .await;
                let (send, receive) = futures_channel::oneshot::channel();
                picks.borrow_mut().push_back(receive);
                super::original_commands::preview_action(
                    window,
                    "transcript-image-preview-actions",
                    true,
                    cx,
                )
                .await;
                assert_eq!(media.diagnostics().unwrap().original_commands, 1);
                assert!(media.domain().diagnostics().unique_pins > 0);
                activate_exit(window, cx);
                wait_until(
                    window,
                    cx,
                    |_| attachment.is_retired(),
                    "ordinary Exit must retire the published attachment before its picker drains",
                )
                .await;
                assert_eq!(media.diagnostics().unwrap().original_commands, 1);
                with_native(window, cx, |native, _| {
                    assert!(matches!(
                        attachment.new_window(native),
                        Err(ImageMediaError(ImageMediaFailure::Retired))
                    ));
                })
                .unwrap();
                assert!(!owner.borrow().test_process().windows.shells().is_empty());
                send.send(Ok(None)).unwrap();
                *evidence.borrow_mut() = Some((
                    owner, media, attachment, path, asset, proof, old_home, sidecar,
                ));
            })
        },
    );
    let (owner, media, attachment, path, asset, proof, old_home, sidecar) =
        retained.borrow_mut().take().unwrap();
    assert!(owner.borrow().test_process().windows.shells().is_empty());
    assert!(owner.borrow().test_services().graph().is_none());
    assert!(owner.borrow().test_final_teardown_detail().is_none());
    assert!(attachment.is_retired());
    assert!(old_home.home_revision().is_err());
    assert!(media.domain().failure().is_none());
    let work = media.diagnostics().unwrap();
    assert_eq!(
        (
            work.queued,
            work.active_workers,
            work.flights,
            work.path_slots,
            work.verification_slots,
            work.original_commands,
            work.original_encoded_bytes,
            work.cpu.entries,
            work.cpu.bytes,
        ),
        (0, 0, 0, 0, 0, 0, 0, 0, 0)
    );
    let native = media.domain().diagnostics();
    assert_eq!(
        (
            native.staging_buffers,
            native.staging_bytes,
            native.reserved_textures,
            native.resident_textures,
            native.texture_bytes,
            native.unique_pins,
            native.frame_uses,
        ),
        (0, 0, 0, 0, 0, 0, 0)
    );
    assert_eq!(durable_asset(&path), (asset, proof, sidecar.clone()));
    assert_eq!(
        std::fs::read(sidecar).unwrap(),
        super::image_markers::fixture::png_bytes()
    );
}

fn durable_asset(
    path: &std::path::Path,
) -> (
    beryl_model::AssetId,
    beryl_model::SealedAssetReferenceSetProof,
    std::path::PathBuf,
) {
    let StartupHomeOpen::Ready {
        candidate,
        state,
        syndic,
    } = support::open(path)
    else {
        panic!("the image lifecycle fixture Home must reopen with fresh authority");
    };
    let home = candidate.publish().unwrap();
    let assets = state.assets();
    let proof = assets
        .owner_head(
            &home,
            beryl_state::AssetOwner::SubmittedTurnItem(beryl_model::SyndicItemId::from_bytes(
                [0xB3; 16],
            )),
        )
        .unwrap()
        .unwrap()
        .set();
    let asset = assets
        .marker_reference(
            &home,
            proof,
            beryl_model::SyndicDraftMarkerId::from_bytes([0xB1; 16]),
        )
        .unwrap()
        .unwrap()
        .asset_id();
    let sidecar = assets
        .verify_sidecar(&home, asset)
        .unwrap()
        .path()
        .to_path_buf();
    drop((assets, state, syndic));
    home.close().unwrap();
    (asset, proof, sidecar)
}
