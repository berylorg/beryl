use super::support::{self, drive, drive_until};
use gpui::{EntityInputHandler, TestAppContext, px};

#[gpui::test]
fn shutdown_gate_preserves_admitted_edits_copy_and_residency(cx: &mut TestAppContext) {
    exercise(cx, true);
}

#[gpui::test]
fn releasing_shutdown_preserves_the_resident_close_gate(cx: &mut TestAppContext) {
    exercise(cx, false);
}

fn exercise(cx: &mut TestAppContext, close_first: bool) {
    let (fixture, cx) = support::mounted(cx, "shutdown-resident", 219);
    let composer = fixture
        .mount
        .read_with(cx, |mount, _| mount.contribution())
        .unwrap();
    let input = composer.read_with(cx, |composer, _| composer.gpui_input());
    composer.update(cx, |composer, _| {
        composer.test_set_checked_clipboard_writer(Box::new(|text, metadata, app| {
            let item = match metadata {
                Some(metadata) => gpui::ClipboardItem::new_string_with_metadata(
                    text.to_owned(),
                    metadata.to_owned(),
                ),
                None => gpui::ClipboardItem::new_string(text.to_owned()),
            };
            app.write_to_clipboard(item);
            gpui_text_input::ClipboardWriteOutcome::Written
        }));
    });
    let text = (0..40)
        .map(|line| format!("line {line}: resident text admitted before shutdown\n"))
        .collect::<String>();
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.focus(window);
            input.replace_and_mark_text_in_range(None, &text, None, window, cx);
        });
        composer.update(app, |composer, cx| {
            composer
                .test_set_shutdown_interaction_gated(true, cx)
                .unwrap();
            composer
                .test_set_shutdown_interaction_gated(true, cx)
                .unwrap();
        });
    });
    drive_until(cx, "edit admitted before shutdown settles", |cx| {
        assert!(composer.read_with(cx, |composer, _| composer.last_error().is_none()));
        fixture
            .service
            .selected_identity()
            .is_some_and(|selection| {
                selection.binding().logical_extent().logical_utf8_bytes() == text.len() as u64
            })
            && input.read_with(cx, |input, _| input.is_quiescent())
    });
    let saved = fixture.service.selected_identity().unwrap();
    let input_id = input.entity_id();
    let composer_id = composer.entity_id();
    assert!(input.read_with(cx, |input, _| input.is_enabled()));
    cx.simulate_keystrokes("ctrl-home shift-right shift-right");
    drive_until(cx, "selection during shutdown", |cx| {
        input.read_with(cx, |input, _| input.is_quiescent())
    });
    let selected = input.read_with(cx, |input, _| input.surface().unwrap().selection());
    composer.update(cx, |composer, cx| {
        composer
            .test_set_shutdown_interaction_gated(true, cx)
            .unwrap();
        assert!(composer.invoke_marker_remove(cx).is_err());
    });
    assert_eq!(
        input.read_with(cx, |input, _| input.surface().unwrap().selection()),
        selected
    );
    cx.update(|window, app| {
        input.update(app, |input, cx| {
            input.replace_and_mark_text_in_range(None, "forbidden", None, window, cx);
        })
    });
    cx.simulate_keystrokes("backspace shift-enter enter ctrl-z ctrl-y ctrl-x ctrl-v");
    drive(cx, 8);
    assert_eq!(fixture.service.selected_identity(), Some(saved));
    assert!(!fixture.mount.read_with(cx, |mount, _| {
        mount.test_submission_diagnostics().active_ticket()
    }));
    let generation = fixture
        .mount
        .read_with(cx, |mount, _| mount.test_submission_start_generation());
    cx.update(|window, app| {
        fixture.mount.update(app, |mount, cx| {
            mount.test_begin_submission_start(saved, window, cx)
        })
    })
    .unwrap();
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.test_submission_start_generation()),
        generation
    );
    cx.simulate_keystrokes("ctrl-a");
    drive_until(cx, "select shutdown text for copy", |cx| {
        input.read_with(cx, |input, _| input.is_quiescent())
    });
    cx.simulate_keystrokes("ctrl-c");
    drive_until(cx, "copy during shutdown", |cx| {
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref()
            == Some(text.as_str())
    });
    input
        .update(cx, |input, cx| input.request_absolute_scroll(px(32.), cx))
        .unwrap();
    drive_until(cx, "scroll resident shutdown text", |cx| {
        input.read_with(cx, |input, _| {
            input.is_quiescent() && input.surface().unwrap().scroll_block() > px(0.)
        })
    });
    let close = cx
        .update(|window, app| {
            fixture
                .mount
                .update(app, |mount, cx| mount.begin_window_close(window, cx))
        })
        .unwrap();
    if close_first {
        assert!(
            cx.update(|window, app| fixture.mount.update(app, |mount, cx| {
                mount.release_window_close(close.ticket, window, cx)
            }))
            .unwrap()
        );
    } else {
        composer
            .update(cx, |composer, cx| {
                composer.test_set_shutdown_interaction_gated(false, cx)
            })
            .unwrap();
    }
    cx.simulate_keystrokes("backspace ctrl-x ctrl-v enter");
    drive(cx, 8);
    assert_eq!(fixture.service.selected_identity(), Some(saved));
    assert!(!fixture.mount.read_with(cx, |mount, _| {
        mount.test_submission_diagnostics().active_ticket()
    }));
    if close_first {
        composer
            .update(cx, |composer, cx| {
                composer.test_set_shutdown_interaction_gated(false, cx)
            })
            .unwrap();
    } else {
        assert!(
            cx.update(|window, app| fixture.mount.update(app, |mount, cx| {
                mount.release_window_close(close.ticket, window, cx)
            }))
            .unwrap()
        );
    }
    assert_eq!(
        fixture
            .mount
            .read_with(cx, |mount, _| mount.contribution())
            .unwrap()
            .entity_id(),
        composer_id
    );
    assert_eq!(
        composer
            .read_with(cx, |composer, _| composer.gpui_input())
            .entity_id(),
        input_id
    );
    cx.simulate_keystrokes("ctrl-z");
    drive_until(cx, "edit after all gates release", |_| {
        fixture
            .service
            .selected_identity()
            .is_some_and(|selection| selection.binding().logical_extent().logical_utf8_bytes() == 0)
    });
    drop((input, composer));
    support::finish(fixture, cx);
}
