use super::*;

fn encoded_png() -> Vec<u8> {
    fn crc(bytes: &[u8]) -> u32 {
        let mut crc = u32::MAX;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
            }
        }
        !crc
    }
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x02\0\0\0\x03\x08\x06\0\0\0".to_vec();
    bytes.extend_from_slice(&crc(&bytes[12..29]).to_be_bytes());
    let mut data = vec![0x78, 0x01, 0x01, 27, 0, 0xe4, 0xff];
    data.extend_from_slice(&[0; 27]);
    data.extend_from_slice(&[0, 27, 0, 1]);
    for (kind, data) in [(b"IDAT", data.as_slice()), (b"IEND", &[][..])] {
        bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = bytes.len();
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(data);
        bytes.extend_from_slice(&crc(&bytes[start..]).to_be_bytes());
    }
    bytes
}

#[gpui::test]
fn image_paste_storage_refusal_has_exact_dismissible_notice_and_preserves_editor(
    cx: &mut gpui::TestAppContext,
) {
    let (mounted, service) = composer_feedback::configured_mount(
        cx,
        211,
        syndic_storage::DraftMarkerAdmissionLimitsV1::new(64, u64::MAX, u64::MAX),
        |_| {},
    );
    let composer = composer_feedback::composer(&mounted, cx);
    composer_feedback::prepare_editor(&mounted, &composer, cx);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    let item = gpui::ClipboardItem::new_image(&gpui::Image::from_bytes(
        gpui::ImageFormat::Png,
        encoded_png(),
    ));
    mounted
        .window
        .update(cx, |_, window, app| {
            input.update(app, |input, cx| {
                let binding = input.surface().unwrap().binding();
                input
                    .rebind(
                        binding,
                        Some(gpui_text_input::RangeSourceSelection {
                            anchor: mutation_support::position(5),
                            head: mutation_support::position(1),
                        }),
                        window,
                        cx,
                    )
                    .unwrap();
                input.focus(window);
            });
            composer.update(app, |composer, _| {
                composer.test_set_checked_clipboard_reader(Box::new(move |_, _| {
                    Ok(gpui::CheckedClipboardSnapshot {
                        item: item.clone(),
                        sequence: 1,
                    })
                }));
            });
        })
        .unwrap();
    for _ in 0..16 {
        support::draw(cx);
    }
    let before = composer_feedback::editor_state(&composer, cx);
    let caret = input.read_with(cx, |input, _| input.surface().unwrap().caret());
    let assets = mounted.fixture.state.assets();
    assert!(assets.revision(&mounted.fixture.store).is_ok());
    mounted
        .fixture
        .faults
        .fail_next(beryl_home_store::test_faults::FaultPoint::BeforeSidecarWrite);
    cx.simulate_keystrokes(mounted.window.into(), "ctrl-v");
    support::drive_until(cx, |cx| {
        composer.read_with(cx, |composer, _| {
            !composer.paste_pending()
                && composer.clipboard_feedback().is_some_and(|feedback| {
                    feedback.kind == MainWindowComposerClipboardFeedbackKind::StorageUnavailable
                })
        })
    });
    support::draw(cx);
    let feedback = composer.read_with(cx, |composer, _| composer.clipboard_feedback().unwrap());
    assert_eq!(
        feedback.kind,
        MainWindowComposerClipboardFeedbackKind::StorageUnavailable
    );
    assert_eq!(feedback.selection, before.selection);
    assert!(feedback.paste);
    assert_eq!(composer_feedback::editor_state(&composer, cx), before);
    assert_eq!(
        input.read_with(cx, |input, _| input.surface().unwrap().caret()),
        caret
    );
    assert_eq!(service.selected_identity(), Some(before.selection));
    assert_eq!(
        mounted.fixture.store.health().state(),
        beryl_home_store::HomeHealthState::Failed
    );
    assert!(mounted.fixture.store.home_revision().is_err());
    assert!(assets.revision(&mounted.fixture.store).is_err());
    let (token, content) = composer_feedback::notice(&mounted, cx);
    assert_eq!(content.title().as_str(), "Paste storage failed");
    assert_eq!(
        content.detail().as_str(),
        "The image could not be stored. Your draft is unchanged."
    );
    assert_eq!(content.dismissal, NoticeDismissal::Dismissible);
    assert_eq!(content.commands().count(), 0);
    assert!(support::widget_diagnostics(mounted.window, cx).visible);
    let ingress = support::ingress(mounted.window, cx);
    cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(token), app))
        .unwrap();
    for _ in 0..4 {
        support::draw(cx);
    }
    assert!(projection(mounted.window, cx).is_none());
    assert_eq!(composer_feedback::editor_state(&composer, cx), before);
    drop((composer, input, service));
    support::finish(mounted, cx);
}

#[gpui::test]
fn persistent_mutation_notice_blocks_paste_acquisition_and_remains_visible(
    cx: &mut gpui::TestAppContext,
) {
    let (mounted, service) = composer_feedback::configured_mount(
        cx,
        221,
        syndic_storage::DraftMarkerAdmissionLimitsV1::new(64, u64::MAX, u64::MAX),
        |_| {},
    );
    let composer = composer_feedback::composer(&mounted, cx);
    composer_feedback::prepare_editor(&mounted, &composer, cx);
    let asset = composer_feedback::asset(&mounted, cx);
    let before = composer_feedback::editor_state(&composer, cx);
    mounted
        .fixture
        .faults
        .fail_next(beryl_home_store::test_faults::FaultPoint::AfterPersist);
    composer_feedback::insert(&mounted, &composer, asset, 1, cx);
    let feedback = composer.read_with(cx, |composer, _| composer.mutation_feedback().unwrap());
    assert_eq!(
        feedback.kind,
        MainWindowComposerMutationFeedbackKind::AdmittedWorkUnavailable
    );
    let (token, content) = composer_feedback::notice(&mounted, cx);
    assert_eq!(content.dismissal, NoticeDismissal::Persistent);
    let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = reads.clone();
    let item = gpui::ClipboardItem::new_image(&gpui::Image::from_bytes(
        gpui::ImageFormat::Png,
        encoded_png(),
    ));
    composer.update(cx, |composer, _| {
        composer.test_set_checked_clipboard_reader(Box::new(move |_, _| {
            observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(gpui::CheckedClipboardSnapshot {
                item: item.clone(),
                sequence: 1,
            })
        }));
    });
    cx.simulate_keystrokes(mounted.window.into(), "ctrl-v");
    for _ in 0..32 {
        support::draw(cx);
    }
    assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.mutation_feedback()),
        Some(feedback)
    );
    assert_eq!(composer_feedback::notice(&mounted, cx).0, token);
    assert_eq!(composer_feedback::editor_state(&composer, cx), before);
    let ingress = support::ingress(mounted.window, cx);
    assert!(matches!(
        cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(token.clone()), app)),
        Err(MainWindowNoticeRouteRejection::Notice(
            NoticeRejection::Persistent
        ))
    ));
    support::draw(cx);
    assert_eq!(composer_feedback::notice(&mounted, cx).0, token);
    drop((composer, service));
    support::finish(mounted, cx);
}

#[gpui::test]
fn clipboard_write_failure_is_dismissible_and_repeated_cut_preserves_editor(
    cx: &mut gpui::TestAppContext,
) {
    let (mounted, _service) = composer_feedback::configured_mount(
        cx,
        201,
        syndic_storage::DraftMarkerAdmissionLimitsV1::new(64, u64::MAX, u64::MAX),
        |_| {},
    );
    let composer = composer_feedback::composer(&mounted, cx);
    composer_feedback::prepare_editor(&mounted, &composer, cx);
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    mounted
        .window
        .update(cx, |_, window, app| {
            input.update(app, |input, cx| {
                let binding = input.surface().unwrap().binding();
                let start = gpui_text_input::SourcePosition::new(
                    gpui_text_input::ByteOffset::new(0),
                    gpui_text_input::InlineObjectGap::NoObjects,
                );
                let end = gpui_text_input::SourcePosition::new(
                    gpui_text_input::ByteOffset::new(5),
                    gpui_text_input::InlineObjectGap::NoObjects,
                );
                input
                    .rebind(
                        binding,
                        Some(gpui_text_input::RangeSourceSelection {
                            anchor: start,
                            head: end,
                        }),
                        window,
                        cx,
                    )
                    .unwrap();
            });
            composer.update(app, |composer, _| {
                composer.test_set_checked_clipboard_writer(Box::new(|_, _, _| {
                    gpui_text_input::ClipboardWriteOutcome::Failed
                }))
            });
        })
        .unwrap();
    for _ in 0..16 {
        support::draw(cx);
    }
    let before = composer_feedback::editor_state(&composer, cx);
    cx.simulate_keystrokes(mounted.window.into(), "ctrl-x");
    for _ in 0..64 {
        support::draw(cx);
    }
    assert_eq!(composer_feedback::editor_state(&composer, cx), before);
    let feedback = composer.read_with(cx, |composer, _| composer.clipboard_feedback().unwrap());
    assert_eq!(
        feedback.kind,
        MainWindowComposerClipboardFeedbackKind::Failed
    );
    let (first, content) = composer_feedback::notice(&mounted, cx);
    assert_eq!(content.dismissal, NoticeDismissal::Dismissible);
    assert_eq!(content.commands().count(), 0);
    let ingress = support::ingress(mounted.window, cx);
    cx.update(|app| ingress.dispatch(MainWindowNoticeWidgetEvent::Dismiss(first.clone()), app))
        .unwrap();
    for _ in 0..4 {
        support::draw(cx);
    }
    assert!(projection(mounted.window, cx).is_none());
    cx.simulate_keystrokes(mounted.window.into(), "ctrl-x");
    for _ in 0..64 {
        support::draw(cx);
    }
    let repeated = composer.read_with(cx, |composer, _| composer.clipboard_feedback().unwrap());
    assert_ne!(feedback.operation, repeated.operation);
    assert_eq!(composer_feedback::editor_state(&composer, cx), before);
    let (second, _) = composer_feedback::notice(&mounted, cx);
    assert!(!first.record().same_identity(second.record()));
    drop(composer);
    drop(input);
    drop(_service);
    support::finish(mounted, cx);
}
