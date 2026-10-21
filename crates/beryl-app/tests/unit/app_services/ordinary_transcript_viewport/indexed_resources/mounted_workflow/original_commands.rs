use super::*;
use crate::main_window::image_inspection::original_commands::{
    TestOriginalClipboardWriter, TestOriginalSavePicker,
};
use std::{cell::Cell, collections::VecDeque};

#[test]
fn native_original_image_commands_use_exact_bytes_focus_and_late_picker_cancellation() {
    require_fixture_ledger();
    run_mounted_with_prepared_home_retained_and_workers_and_deadline(
        1,
        0,
        9,
        Duration::from_secs(120),
        |path| {
            super::image_markers::fixture::seed_png(path);
        },
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let root = cx.update(|app| window.entity(app).unwrap()).unwrap();
                let panel = root
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                let media = owner.borrow().test_services().media_process().unwrap();
                let attachment = owner.borrow().media_attachment().unwrap();
                let copies = Rc::new(RefCell::new(Vec::new()));
                let recorded = copies.clone();
                let reject = Rc::new(Cell::new(false));
                let reject_write = reject.clone();
                let picks = Rc::new(RefCell::new(VecDeque::new()));
                let receiver_queue = picks.clone();
                cx.update(|app| {
                    app.set_global(TestOriginalClipboardWriter(Rc::new(RefCell::new(
                        Box::new(move |item, _| {
                            let [gpui::ClipboardEntry::Image(image)] = item.entries() else {
                                panic!("Copy must publish one exact encoded image")
                            };
                            recorded.borrow_mut().push(image.bytes().to_vec());
                            if reject_write.get() {
                                Err(gpui::ClipboardError::Ownership)
                            } else {
                                Ok(())
                            }
                        }),
                    ))));
                    app.set_global(TestOriginalSavePicker(Rc::new(RefCell::new(Box::new(
                        move || {
                            receiver_queue
                                .borrow_mut()
                                .pop_front()
                                .expect("every native picker is injected")
                        },
                    )))));
                })
                .unwrap();
                leading(window, &panel, cx).await;
                ready(window, &panel, cx, &["before", "after"]).await;
                let markers = panel
                    .read_with(cx, |panel, _| {
                        panel
                            .snapshot()
                            .records
                            .iter()
                            .flat_map(|record| record.authored_parts())
                            .filter(|part| {
                                matches!(part.kind, ResidentPresentationRecordKind::ImageMarker(_))
                            })
                            .map(|part| part.id.clone())
                            .collect::<Vec<_>>()
                    })
                    .unwrap();
                open_preview(window, &panel, &markers[0], cx).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        root.read_with(cx, |root, _| {
                            root.test_image_original_command(false)
                                .is_some_and(|(reason, _)| reason.is_none())
                        })
                        .unwrap()
                    },
                    "original Copy availability must prepare independently",
                )
                .await;
                media.domain().lose_device_for_test();
                with_native(window, cx, |native, _| native.refresh()).unwrap();
                preview_action(window, "transcript-image-preview-actions", false, cx).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        root.read_with(cx, |root, _| {
                            root.test_image_original_command(false) == Some((None, "Copied."))
                        })
                        .unwrap()
                    },
                    "checked Copy must acknowledge once while the image device is unavailable",
                )
                .await;
                assert_eq!(
                    &*copies.borrow(),
                    &[super::image_markers::fixture::png_bytes()]
                );
                with_native(window, cx, |native, app| {
                    assert!(root.read(app).test_image_original_anchor_focused(native))
                })
                .unwrap();
                reject.set(true);
                preview_action(window, "transcript-image-preview-actions", false, cx).await;
                wait_until(window, cx, |cx| root.read_with(cx, |root, _| root.test_image_original_command(false) == Some((None, "Copy failed."))).unwrap(), "native Copy rejection must report failure without changing admitted availability").await;
                assert_eq!(copies.borrow().len(), 2);
                reject.set(false);
                let directory = output_directory();
                let destination = directory.join("original.bin");
                let (send, receive) = futures_channel::oneshot::channel();
                picks.borrow_mut().push_back(receive);
                preview_action(window, "transcript-image-preview-actions", true, cx).await;
                wait_until(
                    window,
                    cx,
                    |_| media.diagnostics().unwrap().original_commands == 1,
                    "Save must reserve its single slot while awaiting native choice",
                )
                .await;
                let state = media.diagnostics().unwrap();
                assert_eq!(
                    (
                        state.path_slots,
                        state.verification_slots,
                        state.original_encoded_bytes
                    ),
                    (0, 0, 0)
                );
                send.send(Ok(None)).unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        root.read_with(cx, |root, _| {
                            root.test_image_original_command(true)
                                == Some((None, "Save cancelled."))
                        })
                        .unwrap()
                    },
                    "picker cancellation must return without a filesystem effect",
                )
                .await;
                assert!(!destination.exists());
                let (send, receive) = futures_channel::oneshot::channel();
                picks.borrow_mut().push_back(receive);
                preview_action(window, "transcript-image-preview-actions", true, cx).await;
                send.send(Ok(Some(gpui::SavePathChoice {
                    path: destination.clone(),
                    replace_existing: false,
                })))
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        root.read_with(cx, |root, _| {
                            root.test_image_original_command(true) == Some((None, "Saved."))
                        })
                        .unwrap()
                    },
                    "Save must acknowledge only after exact original publication",
                )
                .await;
                assert_eq!(
                    std::fs::read(&destination).unwrap(),
                    super::image_markers::fixture::png_bytes()
                );
                let (send, receive) = futures_channel::oneshot::channel();
                picks.borrow_mut().push_back(receive);
                preview_action(window, "transcript-image-preview-actions", true, cx).await;
                click_selector(window, "transcript-image-preview-close", cx).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        root.read_with(cx, |root, _| root.test_image_marker_inspection().is_none())
                            .unwrap()
                    },
                    "the original command must cancel with its owning inspection",
                )
                .await;
                assert_eq!(
                    media.diagnostics().unwrap().original_commands,
                    1,
                    "an actual pending picker receipt retains admission after cancellation"
                );
                open_preview(window, &panel, &markers[1], cx).await;
                let successor_focus =
                    with_native(window, cx, |native, app| native.focused(app).unwrap()).unwrap();
                let late_destination = directory.join("late.bin");
                send.send(Ok(Some(gpui::SavePathChoice {
                    path: late_destination.clone(),
                    replace_existing: false,
                })))
                .unwrap();
                wait_until(
                    window,
                    cx,
                    |cx| {
                        root.read_with(cx, |root, _| {
                            root.test_image_original_command(false)
                                .is_some_and(|(reason, _)| reason.is_none())
                        })
                        .unwrap()
                    },
                    "late cancelled choice must release admission for the successor surface",
                )
                .await;
                assert!(!late_destination.exists());
                with_native(window, cx, |native, app| {
                    assert_eq!(native.focused(app).unwrap(), successor_focus)
                })
                .unwrap();
                click_selector(window, "transcript-image-preview-close", cx).await;
                media.domain().recover_device_for_test();
                with_native(window, cx, |native, _| native.refresh()).unwrap();
                open_direct_menu(window, &panel, &markers[0], cx).await;
                with_native(window, cx, |native, app| key(native, app, "escape")).unwrap();
                wait_until(window, cx, |cx| {
                    panel.read_with(cx, |panel, _| panel.test_image_direct_original_command(false).is_none()).unwrap()
                }, "dismissing a direct image menu must drop its availability owner and busy waiter").await;
                open_direct_menu(window, &panel, &markers[0], cx).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        panel
                            .read_with(cx, |panel, _| {
                                panel
                                    .test_image_direct_original_command(false)
                                    .is_some_and(|(reason, _)| reason.is_none())
                            })
                            .unwrap()
                    },
                    "direct transcript menu must prepare its own exact original availability",
                )
                .await;
                click_selector(window, "transcript-image-copy", cx).await;
                wait_until(
                    window,
                    cx,
                    |_| copies.borrow().len() == 3,
                    "direct transcript Copy must execute through the checked original owner",
                )
                .await;
                assert!(
                    copies
                        .borrow()
                        .iter()
                        .all(|bytes| *bytes == super::image_markers::fixture::png_bytes())
                );
                edit(window, &panel, cx).await;
                let resident = composer(window, cx).await;
                replacement_ready(window, &resident, &panel, true, cx).await;
                let input = resident
                    .read_with(cx, |resident, _| resident.gpui_input())
                    .unwrap();
                super::image_inspection::open_composer_view(window, &resident, &input, 0, cx).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        resident
                            .read_with(cx, |resident, _| {
                                resident
                                    .test_image_original_command(false)
                                    .is_some_and(|(reason, _)| reason.is_none())
                            })
                            .unwrap()
                    },
                    "composer original availability must bind its current candidate",
                )
                .await;
                preview_action(window, "conversation-composer-image-commands", false, cx).await;
                wait_until(
                    window,
                    cx,
                    |cx| {
                        resident
                            .read_with(cx, |resident, _| {
                                resident.test_image_original_command(false)
                                    == Some((None, "Copied."))
                            })
                            .unwrap()
                    },
                    "composer Copy must acknowledge its exact original",
                )
                .await;
                assert_eq!(copies.borrow().len(), 4);
                assert_eq!(
                    copies.borrow()[3],
                    super::image_markers::fixture::png_bytes()
                );
                click_selector(window, "conversation-composer-image-preview-close", cx).await;
                super::image_inspection::drain_media(window, &media, &attachment, cx).await;
                dispose(window, &panel, cx).await;
            })
        },
    );
}

async fn open_direct_menu(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    marker: &crate::syndic_transcript::ResidentPresentationRecordId,
    cx: &mut AsyncApp,
) {
    with_native(window, cx, |native, app| {
        let bounds = panel.read(app).image_marker_bounds(marker, native).unwrap();
        pointer(
            native,
            app,
            PlatformInput::MouseDown(gpui::MouseDownEvent {
                position: bounds.center(),
                button: gpui::MouseButton::Right,
                click_count: 1,
                ..Default::default()
            }),
        );
    })
    .unwrap();
    wait_until(
        window,
        cx,
        |cx| {
            panel
                .read_with(cx, |panel, _| {
                    panel.test_image_direct_original_command(false).is_some()
                })
                .unwrap()
        },
        "the exact marker context menu must mount before its next input",
    )
    .await;
}
async fn open_preview(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    marker: &crate::syndic_transcript::ResidentPresentationRecordId,
    cx: &mut AsyncApp,
) {
    with_native(window, cx, |native, app| {
        let bounds = panel.read(app).image_marker_bounds(marker, native).unwrap();
        click(native, app, bounds.center());
    })
    .unwrap();
    wait_until(
        window,
        cx,
        |cx| {
            window
                .read_with(cx, |root, _| {
                    root.test_image_marker_inspection()
                        .is_some_and(|inspection| inspection.target.record.record_id == *marker)
                })
                .unwrap()
        },
        "the exact transcript inspection must mount",
    )
    .await;
}
async fn preview_action(
    window: WindowHandle<MainWindowShellRoot>,
    anchor: &'static str,
    save: bool,
    cx: &mut AsyncApp,
) {
    click_selector(window, anchor, cx).await;
    click_selector(
        window,
        if save {
            "image-original-save"
        } else {
            "image-original-copy"
        },
        cx,
    )
    .await;
}
async fn click_selector(
    window: WindowHandle<MainWindowShellRoot>,
    selector: &'static str,
    cx: &mut AsyncApp,
) {
    wait_until(
        window,
        cx,
        |cx| {
            with_native(window, cx, |native, _| {
                native.debug_bounds_for_test(selector).is_some()
            })
            .unwrap()
        },
        "the exact command control must be painted before activation",
    )
    .await;
    with_native(window, cx, |native, app| {
        let bounds = native.debug_bounds_for_test(selector).unwrap();
        click(native, app, bounds.center());
    })
    .unwrap();
}
fn output_directory() -> std::path::PathBuf {
    let path = tempfile::Builder::new()
        .prefix("native-original-image-save-")
        .tempdir()
        .unwrap()
        .keep();
    let ledger = std::env::var_os("BERYL_NATIVE_RETAINED_FIXTURE_LEDGER")
        .expect("native output requires its retained fixture ledger");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ledger)
        .unwrap();
    std::io::Write::write_all(&mut file, format!("{}\n", path.display()).as_bytes()).unwrap();
    path
}
