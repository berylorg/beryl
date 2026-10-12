use super::super::super::super::code_panel::{selection, source_actions::CodeSourceActions};
use super::*;
use crate::syndic_transcript::{IndexedResourceDemand, IndexedSourceResponse};

#[gpui::test]
fn code_action_source_and_anchor_refusals_release_only_the_matching_slot(cx: &mut TestAppContext) {
    with_code_layout(cx, |_, viewport, _| {
        let first = code_line_response(1, 100);
        let mut actions = CodeSourceActions::default();
        let mut owner = viewport.owner.clone();
        actions.observe(&owner, viewport.revision, Some(&first));
        actions.select(20, false);
        actions.select(80, true);
        let selected = actions.selection;
        let (range, budget) = actions.copy_budget(true).unwrap();
        assert!(actions.begin_copy(&owner, range, budget));
        let query = actions.query().unwrap();
        owner.indexed = Some(IndexedResourceDemand {
            query: IndexedResourceQuery::Source {
                range: index::ByteRange { start: 21, end: 80 },
                bytes: 16384,
            },
            ..query.clone()
        });
        assert!(!actions.refuse_query(&owner));
        assert!(actions.copy.is_some());
        owner.indexed = Some(query.clone());
        assert!(actions.refuse_query(&owner));
        assert_eq!(actions.charge(), 4096);
        assert_eq!(actions.selection, selected);
        assert!(actions.query().is_none());
        let (range, budget) = actions.copy_budget(true).unwrap();
        assert!(actions.begin_copy(&owner, range, budget));
        let IndexedResourceQuery::Source { range, .. } = query.query else {
            panic!();
        };
        let mut bytes = vec![b'x'; 60];
        bytes[5] = 0;
        let response = IndexedResourceResponse {
            root: first.root.clone(),
            encoded_bytes: 572,
            value: IndexedResourceValue::Source(IndexedSourceResponse {
                binding: actions.binding.clone().unwrap(),
                range,
                bytes,
            }),
        };
        actions.accept_copy_page(&owner, Some(&response));
        assert!(actions.copy.is_none());
        assert_eq!(actions.selection, selected);
        assert!(actions.feedback.contains("NUL"));
        let successor = code_line_response(2, 60);
        let revision = beryl_model::ProjectionRevision::new(2).unwrap();
        actions.observe(&owner, revision, Some(&successor));
        owner.indexed = actions.query();
        assert!(matches!(
            owner.indexed.as_ref().unwrap().query,
            IndexedResourceQuery::Anchor { .. }
        ));
        assert!(actions.refuse_query(&owner));
        assert!(actions.selection.is_none());
        assert!(actions.query().is_none());
        assert_eq!(actions.charge(), 4096);
        actions.select(30, false);
        actions.select(45, true);
        assert_eq!(actions.selected_range(), Some(30..45));
        assert!(actions.copy_budget(true).is_some());
    });
}

#[gpui::test]
fn code_copy_assembles_exact_raw_utf8_crlf_pages_and_rejects_stale_effect_owner(
    cx: &mut TestAppContext,
) {
    with_code_layout(cx, |_, viewport, _| {
        let raw = "aa".to_owned() + &"🍒".repeat(2048) + "xxx\r\n";
        let response = code_line_response(1, raw.len() as u64);
        let mut actions = CodeSourceActions::default();
        let mut owner = viewport.owner.clone();
        actions.observe(&owner, viewport.revision, Some(&response));
        actions.select(2, false);
        actions.select(raw.len() as u64, true);
        let selection = actions.selection;
        let (range, budget) = actions.copy_budget(false).unwrap();
        assert!(budget >= 3 * raw.len() + 2 + 64 * 1024);
        assert!(actions.begin_copy(&owner, range, budget));
        let charge = actions.charge();
        let mut pages = 0;
        while let Some(query) = actions.query() {
            let IndexedResourceQuery::Source { range, .. } = query.query else {
                panic!();
            };
            assert!(range.end - range.start <= 8192);
            let source = IndexedResourceResponse {
                root: response.root.clone(),
                encoded_bytes: range.end as usize - range.start as usize + 512,
                value: IndexedResourceValue::Source(IndexedSourceResponse {
                    binding: actions.binding.clone().unwrap(),
                    range,
                    bytes: raw.as_bytes()[range.start as usize..range.end as usize].to_vec(),
                }),
            };
            owner.indexed = Some(query);
            owner.presentation_revision += 1;
            actions.accept_copy_page(&owner, Some(&source));
            assert_eq!(actions.charge(), charge);
            pages += 1;
        }
        assert_eq!(pages, 2);
        assert_eq!(actions.copy.as_ref().unwrap().bytes, raw.as_bytes());
        assert!(actions.ready_copy(&owner));
        owner.activation_revision += 1;
        assert!(!actions.ready_copy(&owner));
        actions.cancel_copy("Copy unavailable");
        assert_eq!(actions.selection, selection);
        assert_eq!(actions.charge(), 4096);
    });
}

#[gpui::test]
fn code_selection_successor_publishes_both_admitted_anchor_endpoints_together(
    cx: &mut TestAppContext,
) {
    with_code_layout(cx, |_, viewport, _| {
        let first = code_line_response(1, 100);
        let successor = code_line_response(2, 60);
        let mut actions = CodeSourceActions::default();
        let mut owner = viewport.owner.clone();
        actions.observe(&owner, viewport.revision, Some(&first));
        actions.select(20, false);
        actions.select(80, true);
        let prior = actions.selection;
        let revision = beryl_model::ProjectionRevision::new(2).unwrap();
        actions.observe(&owner, revision, Some(&successor));
        assert_eq!(actions.selected_range(), None);
        for (attempt, position) in [20, 60].into_iter().enumerate() {
            let query = actions.query().unwrap();
            let IndexedResourceQuery::Anchor { anchor, .. } = query.query else {
                panic!();
            };
            let IndexedResourceValue::Range(value) = &successor.value else {
                panic!();
            };
            let index::Item::Descriptor(descriptor) = &value.items[0] else {
                panic!();
            };
            let response = IndexedResourceResponse {
                root: successor.root.clone(),
                encoded_bytes: 512,
                value: IndexedResourceValue::Anchor(index::AnchorResponse {
                    binding: value.binding.clone(),
                    encoded_bytes: 512,
                    resolved: Some(index::ResolvedAnchor {
                        descriptor: descriptor.clone(),
                        anchor: index::SourceAnchor {
                            source: value.binding.source,
                            position,
                            ..anchor
                        },
                    }),
                }),
            };
            owner.indexed = Some(query);
            actions.observe(&owner, revision, Some(&response));
            if attempt == 0 {
                assert_eq!(actions.selection, prior);
                assert_eq!(actions.selected_range(), None);
            }
        }
        assert_eq!(actions.selected_range(), Some(20..60));
        assert!(actions.query().is_none());
        let (range, budget) = actions.copy_budget(true).unwrap();
        assert!(actions.begin_copy(&owner, range, budget));
        actions.select(30, false);
        assert!(actions.copy.is_none());
    });
}

#[gpui::test]
fn code_copy_preflight_refuses_empty_and_over_clipboard_cap_without_source_query(
    cx: &mut TestAppContext,
) {
    with_code_layout(cx, |_, viewport, _| {
        let response = code_line_response(1, 262145);
        let mut actions = CodeSourceActions::default();
        actions.observe(&viewport.owner, viewport.revision, Some(&response));
        actions.select(0, false);
        assert!(actions.copy_budget(true).is_none());
        assert!(actions.copy_budget(false).is_none());
        assert!(actions.query().is_none());
        assert_eq!(actions.charge(), 4096);
        actions.select(128, true);
        assert!(actions.copy_budget(true).is_some());
    });
}

#[gpui::test]
fn code_source_hits_cover_tabs_oversize_atoms_and_terminal_boundaries(cx: &mut TestAppContext) {
    with_code_layout(cx, |mount, viewport, window| {
        for raw in ["ab\t🍒".to_owned(), "a".to_owned() + &"\u{301}".repeat(600)] {
            let mut response = code_line_response(1, raw.len() as u64);
            let response_mut = Arc::get_mut(&mut response).unwrap();
            let IndexedResourceValue::Range(value) = &mut response_mut.value else {
                panic!();
            };
            value.presentations.as_mut().unwrap()[0]
                .fragment
                .as_mut()
                .unwrap()
                .text = raw.clone();
            mount.reset();
            viewport.scroll.borrow_mut().bounds = gpui::Bounds::new(
                gpui::point(gpui::px(0.0), gpui::px(0.0)),
                gpui::size(gpui::px(400.0), gpui::px(240.0)),
            );
            drive_code_layout(
                mount,
                viewport,
                &response,
                &CodeAppearance::default(),
                window,
            );
            let prepared = mount.prepared().next().unwrap();
            let mut saw_atom = false;
            for segment in &prepared.fragments {
                for fragment in segment.fragments.iter() {
                    if let gpui::StreamingLayoutFragment::OversizeAtom(atom) = fragment {
                        saw_atom = true;
                        let maps = atom.maps();
                        for map in maps {
                            assert_eq!(
                                selection::hit(
                                    mount,
                                    viewport,
                                    gpui::point(map.position.x + gpui::px(10.0), gpui::px(10.0))
                                ),
                                Some(map.logical_position.byte_offset)
                            );
                        }
                        for (fraction, expected) in [
                            (0.25, maps[0].logical_position.byte_offset),
                            (0.75, maps[1].logical_position.byte_offset),
                        ] {
                            let x = maps[0].position.x
                                + (maps[1].position.x - maps[0].position.x) * fraction
                                + gpui::px(10.0);
                            assert_eq!(
                                selection::hit(mount, viewport, gpui::point(x, gpui::px(10.0))),
                                Some(expected)
                            );
                        }
                    }
                }
            }
            assert!(saw_atom);
            let width = mount.complete_width(1).unwrap();
            assert_eq!(
                selection::hit(
                    mount,
                    viewport,
                    gpui::point(gpui::px(width + 20.0), gpui::px(10.0))
                ),
                Some(raw.len() as u64)
            );
        }
        let mut empty_line = code_line_response(1, 1);
        let IndexedResourceValue::Range(range) = &mut Arc::get_mut(&mut empty_line).unwrap().value
        else {
            panic!()
        };
        let index::Item::Descriptor(item) = &mut range.items[0] else {
            panic!()
        };
        item.descriptor = index::Descriptor::CodeLine {
            ordinal: 0,
            raw: index::ByteRange { start: 0, end: 1 },
            content: index::ByteRange { start: 0, end: 0 },
            terminator: index::ByteRange { start: 0, end: 1 },
        };
        range.presentations.as_mut().unwrap()[0].fragment = Some(index::Fragment {
            text: String::new(),
            source: index::ByteRange { start: 0, end: 0 },
            remaining: index::ByteRange { start: 0, end: 0 },
            next_offset: 0,
        });
        mount.reset();
        viewport.adopt(&empty_line);
        mount.reconcile_range(viewport);
        mount
            .adopt(
                viewport,
                vec![empty_line],
                &CodeAppearance::default(),
                window,
                window.text_system().font_environment_key().unwrap(),
            )
            .unwrap();
        for _ in 0..16 {
            mount
                .advance(viewport, &CodeAppearance::default(), window)
                .unwrap();
            mount.publish_candidate(viewport);
            if mount.state(viewport) == "ready" {
                break;
            }
        }
        assert_eq!(mount.state(viewport), "ready");
        assert_eq!(
            selection::hit(
                mount,
                viewport,
                gpui::point(gpui::px(120.0), gpui::px(10.0))
            ),
            Some(0)
        );
    });
}

#[gpui::test]
fn code_text_hit_owns_last_glyph_tails_at_canonical_and_terminal_boundaries(
    cx: &mut TestAppContext,
) {
    with_code_layout(cx, |mount, viewport, window| {
        let response = code_line_response(1, 1100);
        viewport.scroll.borrow_mut().bounds = gpui::Bounds::new(
            gpui::point(gpui::px(0.0), gpui::px(0.0)),
            gpui::size(gpui::px(20000.0), gpui::px(240.0)),
        );
        drive_code_layout(
            mount,
            viewport,
            &response,
            &CodeAppearance::default(),
            window,
        );
        let mut boundaries = Vec::new();
        for prepared in mount.prepared() {
            for segment in &prepared.fragments {
                for fragment in segment.fragments.iter() {
                    if let gpui::StreamingLayoutFragment::Text(text) = fragment {
                        let last = text.maps().last().unwrap();
                        boundaries.push(last.logical_position.byte_offset);
                        assert_eq!(
                            selection::hit(
                                mount,
                                viewport,
                                gpui::point(last.position.x + gpui::px(9.999), gpui::px(10.0))
                            ),
                            Some(last.logical_position.byte_offset)
                        );
                    }
                }
            }
        }
        assert_eq!(boundaries, vec![1024, 1100]);
    });
}
