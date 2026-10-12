use super::*;

#[gpui::test]
fn code_continuation_can_admit_the_first_fragment_of_an_oversized_later_line(
    cx: &mut TestAppContext,
) {
    let window = mounted_panel(cx, activation());
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    publish_metadata(cx, window, &panel);
    let first = panel.update(cx, |panel, _| {
        let demand = panel.resource_demand().unwrap();
        assert!(panel.stage_resource_delivery(
            &demand,
            ResourceDeliveryResponse::Indexed(Arc::new(page(0, 15))),
            None
        ));
        demand
    });
    publish_candidate(cx, window, &first);
    let last = panel.update(cx, |panel, _| {
        let demand = panel.resource_demand().unwrap();
        let mut response = page(15, 16);
        let IndexedResourceValue::Range(range) = &mut response.value else {
            unreachable!();
        };
        range.request_complete = false;
        range.cursor.next = index::Position::Code(15);
        range.cursor.fragment_offset = 4;
        range.items[0] = index::Item::Descriptor(index::IndexedDescriptor {
            descriptor: index::Descriptor::CodeLine {
                ordinal: 15,
                raw: index::ByteRange {
                    start: 75,
                    end: 9076,
                },
                content: index::ByteRange {
                    start: 75,
                    end: 9075,
                },
                terminator: index::ByteRange {
                    start: 9075,
                    end: 9076,
                },
            },
            absolute_raw: Some(index::ByteRange {
                start: 75,
                end: 9076,
            }),
        });
        range.presentations.as_mut().unwrap()[0]
            .fragment
            .as_mut()
            .unwrap()
            .remaining
            .end = 9075;
        assert!(panel.stage_resource_delivery(
            &demand,
            ResourceDeliveryResponse::Indexed(Arc::new(response)),
            None
        ));
        demand
    });
    publish_candidate(cx, window, &last);
    panel.read_with(cx, |panel, _| {
        assert_eq!(
            panel.diagnostic_snapshot().code_panels[0].realized_visual_lines,
            15
        );
        assert_eq!(
            panel.diagnostic_snapshot().code_panels[0].layout_state,
            "pending"
        );
        assert!(matches!(
            panel.resource_demand().unwrap().indexed.unwrap().query,
            IndexedResourceQuery::Display {
                request: index::DisplayRequest {
                    target: index::Target::Code(15),
                    ..
                },
                ..
            }
        ));
        assert_eq!(
            panel.diagnostic_snapshot().code_panels[0].source_bytes,
            Some(10000)
        );
    });
}

#[gpui::test]
fn sixteen_single_line_code_pages_charge_every_retained_page_to_residency(cx: &mut TestAppContext) {
    let window = mounted_panel(cx, activation());
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    publish_metadata(cx, window, &panel);
    let baseline = panel.read_with(cx, |panel, _| {
        let diagnostics = panel.diagnostic_snapshot();
        diagnostics.resident_data.resource_bytes - diagnostics.code_panels[0].layout_charged_bytes
    });
    let mut retained = Vec::new();
    for ordinal in 0..16 {
        let response = Arc::new(page(ordinal, ordinal + 1));
        retained.push(Arc::downgrade(&response));
        let demand = panel.update(cx, |panel, _| {
            let demand = panel.resource_demand().unwrap();
            assert!(panel.stage_resource_delivery(
                &demand,
                ResourceDeliveryResponse::Indexed(response),
                None
            ));
            demand
        });
        publish_candidate(cx, window, &demand);
        panel.read_with(cx, |panel, _| {
            let diagnostics = panel.diagnostic_snapshot();
            assert_eq!(
                diagnostics.code_panels[0].realized_visual_lines,
                ordinal as usize + 1
            );
            assert_eq!(
                diagnostics.resident_data.resource_bytes
                    - diagnostics.code_panels[0].layout_charged_bytes,
                baseline
                    + if ordinal == 15 {
                        0
                    } else {
                        (ordinal as usize + 1) * 1024
                    }
            );
            assert!(retained.iter().all(|page| page.upgrade().is_some()));
            assert!(
                diagnostics.code_panels[0].layout_charged_bytes >= (ordinal as usize + 1) * 1024
            );
            assert!(
                diagnostics.resident_data.resource_bytes
                    >= baseline + (ordinal as usize + 1) * 1024
            );
        });
    }
    panel.read_with(cx, |panel, _| assert_bounded_source_demand(panel));
}

fn assert_bounded_source_demand(panel: &SyndicTranscriptPanel) {
    let IndexedResourceQuery::Source { range, bytes } =
        panel.resource_demand().unwrap().indexed.unwrap().query
    else {
        panic!("completed foreground must yield to the resource extent scan");
    };
    assert!(range.end - range.start <= 8192);
    assert_eq!(bytes, 16384);
}

fn page(start: u64, end: u64) -> IndexedResourceResponse {
    let mut response = range_response(binding(900), 2);
    response.encoded_bytes = 1024;
    let IndexedResourceValue::Range(range) = &mut response.value else {
        unreachable!();
    };
    range.encoded_bytes = 1024;
    range.cursor.next = index::Position::Code(end);
    range.request_complete = end == 16;
    range.items = (start..end)
        .map(|ordinal| index::Item::Descriptor(descriptor(ordinal)))
        .collect();
    range.presentations = Some(
        (start..end)
            .map(|ordinal| index::Presentation {
                fragment: Some(index::Fragment {
                    text: "code".into(),
                    source: index::ByteRange {
                        start: ordinal * 5,
                        end: ordinal * 5 + 4,
                    },
                    remaining: index::ByteRange {
                        start: ordinal * 5 + 4,
                        end: ordinal * 5 + 4,
                    },
                    next_offset: 4,
                }),
                alignment: None,
                literal_fallback: false,
            })
            .collect(),
    );
    response
}

#[gpui::test]
fn partial_code_pages_remain_painted_until_the_exact_continuation_completes(
    cx: &mut TestAppContext,
) {
    let window = mounted_panel(cx, activation());
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    publish_metadata(cx, window, &panel);
    let first = Arc::new(page(0, 15));
    let retained = Arc::downgrade(&first);
    let demand = panel.update(cx, |panel, _| {
        let demand = panel.resource_demand().unwrap();
        assert!(panel.stage_resource_delivery(
            &demand,
            ResourceDeliveryResponse::Indexed(first),
            None
        ));
        demand
    });
    publish_candidate(cx, window, &demand);
    let (continuation, key) = panel.read_with(cx, |panel, _| {
        let demand = panel.resource_demand().unwrap();
        assert!(matches!(
            demand.indexed.as_ref().unwrap().query,
            IndexedResourceQuery::Range {
                cursor: Some(index::Cursor {
                    next: index::Position::Code(15),
                    fragment_offset: 0,
                    ..
                }),
                ..
            }
        ));
        let diagnostics = &panel.diagnostic_snapshot().code_panels[0];
        assert_eq!(diagnostics.realized_visual_lines, 15);
        assert_eq!(diagnostics.source_bytes, Some(10000));
        assert!(retained.upgrade().is_some());
        (demand, diagnostics.key)
    });
    draw_frames(cx, window, 10);
    panel.update(cx, |panel, _| {
        let mut wrong_root = page(15, 16);
        wrong_root.root.as_mut().unwrap().digest = [1; 32];
        assert!(!panel.stage_resource_delivery(
            &continuation,
            ResourceDeliveryResponse::Indexed(Arc::new(wrong_root)),
            None
        ));
        assert!(panel.stage_resource_delivery(
            &continuation,
            ResourceDeliveryResponse::Indexed(Arc::new(page(15, 16))),
            None
        ));
    });
    publish_candidate(cx, window, &continuation);
    panel.read_with(cx, |panel, _| {
        let diagnostics = &panel.diagnostic_snapshot().code_panels[0];
        assert_eq!(diagnostics.key, key);
        assert_eq!(diagnostics.realized_visual_lines, 16);
        assert_eq!(diagnostics.realized_source_range, Some(0..79));
        assert_eq!(panel.code_panels[0].viewport.anchor.unwrap().position, 0);
        assert_bounded_source_demand(panel);
    });
    panel.update(cx, |panel, cx| {
        panel.code_panels[0]
            .viewport
            .scroll
            .borrow_mut()
            .set_offset(900.0 * 20.0);
        cx.notify();
    });
    draw_frames(cx, window, 5);
    panel.read_with(cx, |panel, _| {
        assert!(retained.upgrade().is_none());
        assert_eq!(
            panel.diagnostic_snapshot().code_panels[0].realized_visual_lines,
            0
        );
        assert!(matches!(
            panel.resource_demand().unwrap().indexed.unwrap().query,
            IndexedResourceQuery::Range { cursor: None, .. }
        ));
    });
}
