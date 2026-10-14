use super::super::super::branch_discussion;
use super::*;

pub(in super::super) async fn context_copy(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
    let length = branch_discussion::SELECTED.len();
    let (copied, _) = writer(panel, cx);
    wait_until(
        window,
        cx,
        |cx| {
            if window
                .read_with(cx, |root, _| root.test_transcript_delivery_pending())
                .unwrap()
            {
                return false;
            }
            with_native(window, cx, |native, app| {
                native.draw_and_present_for_test(app);
                let current = panel.read(app);
                current.narrative_geometry_current_for_test(native)
                    && current
                        .literal_narrative_text_for_test()
                        .iter()
                        .any(|(id, text)| {
                            text == branch_discussion::SELECTED
                                && current.literal_narrative_position_for_test(id, 0).is_some()
                                && current
                                    .literal_narrative_position_for_test(id, length)
                                    .is_some()
                        })
            })
            .unwrap()
        },
        "created discussion context must paint current exact literal source",
    )
    .await;
    with_native(window, cx, |native, app| {
        native.draw_and_present_for_test(app);
        assert!(panel.read(app).narrative_geometry_current_for_test(native));
        let snapshot = panel.read(app).snapshot();
        let id = snapshot
            .records
            .iter()
            .find(|record| {
                matches!(
                    record.provenance.source,
                    ResidentRecordSource::LocalContext { .. }
                )
            })
            .unwrap()
            .id
            .clone();
        assert_eq!(
            panel
                .read(app)
                .diagnostic_snapshot()
                .resident_data
                .synthetic_context_group_count,
            1
        );
        panel.update(app, |panel, _| {
            assert!(matches!(
                panel.apply_realized_quote_target_for_record_ids(&[id.clone()]),
                crate::syndic_transcript::ResidentQuoteOutcome::Unavailable(_)
            ));
            assert!(matches!(
                panel.apply_realized_context_menu_target_for_record_id(&id),
                crate::syndic_transcript::ResidentContextMenuOutcome::Unavailable(_)
            ));
        });
        let first = panel
            .read(app)
            .literal_narrative_position_for_test(&id, 0)
            .unwrap();
        let last = panel
            .read(app)
            .literal_narrative_position_for_test(&id, length)
            .unwrap();
        drag(native, app, first, last);
        assert!(panel.read(app).narrative_geometry_current_for_test(native));
        assert!(
            panel
                .read(app)
                .narrative_selection_ranges_for_test()
                .is_some_and(|ranges| {
                    ranges
                        .iter()
                        .any(|(selected, range)| selected == &id && range == &(0..length))
                }),
            "literal context drag must retain its complete current source range"
        );
        key(native, app, "shift-f10");
        assert!(!panel.read(app).quote_menu_open_for_test());
        pointer(
            native,
            app,
            PlatformInput::MouseDown(gpui::MouseDownEvent {
                position: first,
                button: gpui::MouseButton::Right,
                click_count: 1,
                ..Default::default()
            }),
        );
        assert!(panel.read(app).resident_context_menu_target().is_none());
        assert!(!panel.read(app).quote_menu_open_for_test());
    })
    .unwrap();
    for (index, expected) in [branch_discussion::SELECTED, "café 🌸 words"]
        .into_iter()
        .enumerate()
    {
        with_native(window, cx, |native, app| key(native, app, "ctrl-c")).unwrap();
        wait_until(
            window,
            cx,
            |_| copied.borrow().len() == index + 1,
            "context paired Copy must settle through the ordinary worker",
        )
        .await;
        assert_eq!(copied.borrow()[index], expected);
    }
}
