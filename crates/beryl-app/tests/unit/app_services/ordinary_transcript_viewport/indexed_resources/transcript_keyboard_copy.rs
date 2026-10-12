use super::*;
use crate::syndic_transcript::{ResidentPresentationRecordKind, ResidentSelectionOutcome};
use gpui::Focusable;

#[test]
fn native_transcript_keyboard_copy_is_markdown_then_stripped_and_resets_after_focus() {
    run_mounted_with_prepared_home_retained(
        1,
        0,
        |path| {
            seed_history_text(path, 1, Some("# Title\n\n**body**\n"));
        },
        |owner, _, cx| {
            Box::pin(async move {
                let window = windows(&owner)[0];
                let panel = window
                    .read_with(cx, |root, _| root.test_transcript_panel())
                    .unwrap();
                wait_until(window, cx, |cx| panel.read_with(cx, |panel, _| {
                panel.measured_frame_for_test().is_some() && panel.snapshot().records.iter().any(|record| {
                    matches!(&record.kind, ResidentPresentationRecordKind::TextChunk { text, .. } if text.contains("**body**"))
                })
            }).unwrap(), "ordinary source must be admitted and measured").await;
                let captured = Rc::new(RefCell::new(Vec::<String>::new()));
                let refuse = Rc::new(Cell::new(false));
                let sink = captured.clone();
                let fail = refuse.clone();
                panel
                    .update(cx, |panel, _| {
                        panel.set_keyboard_copy_writer_for_test(move |item, _, _| {
                            if fail.get() {
                                return Err(());
                            }
                            assert!(sink.borrow().len() < 8);
                            sink.borrow_mut().push(item.text().unwrap());
                            Ok(())
                        })
                    })
                    .unwrap();
                let expected = with_native(window, cx, |native, app| {
                panel.update(app, |panel, cx| {
                    let ids = panel.snapshot().records.iter().filter_map(|record| {
                        matches!(&record.kind, ResidentPresentationRecordKind::TextChunk { text, .. } if text.contains("Title") || text.contains("**body**"))
                            .then(|| record.id.clone())
                    }).collect::<Vec<_>>();
                    assert!(!ids.is_empty());
                    let outcome = panel.apply_realized_selection(&ids, cx);
                    assert!(matches!(outcome, ResidentSelectionOutcome::Selected(_)), "{outcome:?}");
                    native.focus(&panel.focus_handle(cx));
                });
                let expected = panel.read(app).resident_copy_payload().unwrap();
                key(native, app, "ctrl-c");
                assert_eq!(captured.borrow().as_slice(), &[expected.markdown.clone()]);
                native.dispatch_input_for_test(PlatformInput::KeyDown(gpui::KeyDownEvent { keystroke: gpui::Keystroke::parse("ctrl-c").unwrap(), is_held: true }), app);
                assert_eq!(captured.borrow().len(), 1);
                key(native, app, "ctrl-c");
                assert_eq!(captured.borrow()[1], expected.plain_text.as_ref().unwrap().as_str());
                assert!(captured.borrow()[1].contains("Title"));
                assert!(captured.borrow()[1].contains("body"));
                assert!(!captured.borrow()[1].contains("**"));
                expected.markdown
            }).unwrap();
                with_native(window, cx, |native, app| {
                    key(native, app, "ctrl-c");
                })
                .unwrap();
                assert_eq!(captured.borrow()[2], expected);
                let other = cx.update(|app| app.focus_handle()).unwrap();
                with_native(window, cx, |native, app| {
                    native.focus(&other);
                    native.draw_and_present_for_test(app);
                })
                .unwrap();
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(20))
                    .await;
                with_native(window, cx, |native, app| {
                    native.focus(&panel.read(app).focus_handle(app));
                    native.draw_and_present_for_test(app);
                })
                .unwrap();
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(20))
                    .await;
                with_native(window, cx, |native, app| {
                    key(native, app, "ctrl-c");
                })
                .unwrap();
                assert_eq!(captured.borrow()[3], expected);
                refuse.set(true);
                with_native(window, cx, |native, app| {
                    key(native, app, "ctrl-c");
                    native.draw_and_present_for_test(app);
                    assert!(panel.read(app).resident_copy_payload().is_ok());
                })
                .unwrap();
                assert_eq!(captured.borrow().len(), 4);
                retire_code_fixture(window, &panel, cx).await;
            })
        },
    );
}
