use super::super::super::table_viewport::{TableCellId, TableViewport};
use super::table_export_tests::fixture;
use super::*;
use crate::syndic_transcript::{IndexedResourceResponse, IndexedResourceValue};
use syndic_storage::indexed_resource as index;

#[gpui::test]
fn table_inflight_body_and_continuation_survive_response_absent_render_frames(
    cx: &mut TestAppContext,
) {
    let fixture = fixture(
        &[vec![Some("h")], vec![Some("a")], vec![Some("b")]],
        "|---|",
    );
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Table));
    draw_frames(cx, window, 5);
    let owner = window
        .update(cx, |root, _, app| {
            root.panel.read(app).resource_demand().unwrap()
        })
        .unwrap();
    let mut viewport = TableViewport::new(owner, fixture.binding.resource.revision);
    viewport.adopt(Some(&fixture.page), Vec::new());
    let header_query = viewport.query(None);
    let crate::syndic_transcript::IndexedResourceQuery::Range {
        request: header_request,
        bytes,
        ..
    } = header_query.query
    else {
        panic!()
    };
    assert_eq!(bytes, 16_384);
    let IndexedResourceValue::Range(fixture_range) = &fixture.page.value else {
        panic!()
    };
    let mut header = IndexedResourceResponse {
        root: fixture.page.root.clone(),
        value: IndexedResourceValue::Range(index::Response {
            binding: fixture.binding.clone(),
            cursor: fixture_range.cursor,
            items: Vec::new(),
            request_complete: false,
            resource_complete: false,
            encoded_bytes: fixture.page.encoded_bytes,
            presentations: None,
        }),
        encoded_bytes: fixture.page.encoded_bytes,
    };
    let IndexedResourceValue::Range(range) = &mut header.value else {
        panic!()
    };
    range.cursor.request = header_request;
    range.cursor.next = index::Position::Table { row: 1, column: 0 };
    range.request_complete = true;
    let body = viewport.query(Some(&header));
    assert!(matches!(
        body.query,
        crate::syndic_transcript::IndexedResourceQuery::Range {
            request: index::Request::Table {
                role: index::RowRole::Body,
                ..
            },
            cursor: None,
            bytes: 16_384,
            ..
        }
    ));
    viewport.owner.indexed = Some(body.clone());
    for _ in 0..3 {
        assert_eq!(viewport.query(None), body);
    }
    let crate::syndic_transcript::IndexedResourceQuery::Range {
        request: body_request,
        ..
    } = body.query
    else {
        panic!()
    };
    let IndexedResourceValue::Range(range) = &mut header.value else {
        panic!()
    };
    range.cursor.request = body_request;
    range.request_complete = false;
    let continuation = viewport.query(Some(&header));
    assert!(matches!(
        continuation.query,
        crate::syndic_transcript::IndexedResourceQuery::Range {
            cursor: Some(_),
            bytes: 16_384,
            ..
        }
    ));
    viewport.owner.indexed = Some(continuation.clone());
    assert_eq!(viewport.query(None), continuation);
    let IndexedResourceValue::Range(range) = &mut header.value else {
        panic!()
    };
    range.cursor.next = index::Position::Table { row: 2, column: 0 };
    range.request_complete = true;
    assert_eq!(viewport.query(Some(&header)), continuation);
}

#[gpui::test]
fn table_painted_source_identity_refuses_recycled_mount_revision_and_root(cx: &mut TestAppContext) {
    let fixture = fixture(&[vec![Some("h")], vec![Some("v")]], "|---|");
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Table));
    draw_frames(cx, window, 5);
    let owner = window
        .update(cx, |root, _, app| {
            root.panel.read(app).resource_demand().unwrap()
        })
        .unwrap();
    let mut viewport = TableViewport::new(owner, fixture.binding.resource.revision);
    viewport.root = fixture.page.root.clone();
    let source = super::super::super::table_panel::RenderSource::for_viewport(1, &viewport);
    assert!(source.matches(1, &viewport));
    assert!(!source.matches(2, &viewport));
    viewport.revision = beryl_model::ProjectionRevision::new(viewport.revision.get() + 1).unwrap();
    assert!(!source.matches(1, &viewport));
    viewport.revision = fixture.binding.resource.revision;
    viewport.root = None;
    assert!(!source.matches(1, &viewport));
}

#[gpui::test]
fn table_wide_small_font_tracks_prioritize_visible_columns_and_reach_end(cx: &mut TestAppContext) {
    let fixture = fixture(
        &[vec![Some("h"); 12], vec![Some("v"); 12]],
        "|---|---|---|---|---|---|---|---|---|---|---|---|",
    );
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Table));
    draw_frames(cx, window, 5);
    let owner = window
        .update(cx, |root, _, app| {
            root.panel.read(app).resource_demand().unwrap()
        })
        .unwrap();
    let mut viewport = TableViewport::new(owner, fixture.binding.resource.revision);
    viewport.root = fixture.page.root.clone();
    viewport.binding = Some(fixture.binding.clone());
    let mut appearance = super::super::super::table_appearance::TableAppearance::default();
    appearance.font_size = 4.0;
    {
        let mut scroll = viewport.scroll.borrow_mut();
        scroll.bounds.size = gpui::size(px(1600.0), px(280.0));
        scroll.rows = 1;
        scroll.columns = 12;
    }
    let width = appearance.column_width(1600.0);
    assert!(width >= 1600.0 / 7.0);
    viewport.set_tracks(28.0, width);
    let target = TableCellId::at(1, 11);
    viewport.reveal(target);
    let scroll = viewport.scroll.borrow();
    let columns = scroll.ranges().1;
    assert!(columns.end - columns.start <= 8);
    assert!(columns.start <= 11 && columns.end == 12);
    assert!(11.0 * width - scroll.horizontal < 1600.0);
    drop(scroll);
    let query = viewport.query(None);
    assert!(
        matches!(query.query,crate::syndic_transcript::IndexedResourceQuery::Range{request:index::Request::Table{columns,..},..} if columns.start<=11 && columns.end==12)
    );
}

#[gpui::test]
fn table_fallback_navigation_skips_known_rows_and_clears_newly_discovered_gap_targets(
    cx: &mut TestAppContext,
) {
    let fixture = fixture(
        &[
            vec![Some("h")],
            vec![Some("a")],
            vec![Some("surplus")],
            vec![Some("c")],
        ],
        "|---|",
    );
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Table));
    draw_frames(cx, window, 5);
    let owner = window
        .update(cx, |root, _, app| {
            root.panel.read(app).resource_demand().unwrap()
        })
        .unwrap();
    let mut viewport = TableViewport::new(owner, fixture.binding.resource.revision);
    let IndexedResourceValue::Range(range) = &fixture.page.value else {
        panic!();
    };
    let mut items = range.items.clone();
    let fallback_position = items
        .iter()
        .position(|item| {
            matches!(
                item,
                index::Item::Descriptor(index::IndexedDescriptor {
                    descriptor: index::Descriptor::TableCell {
                        role: index::RowRole::Body,
                        row: 1,
                        ..
                    },
                    ..
                })
            )
        })
        .unwrap();
    items[fallback_position] = index::Item::Descriptor(index::IndexedDescriptor {
        descriptor: index::Descriptor::TableRow {
            role: index::RowRole::Body,
            ordinal: 1,
            raw: index::ByteRange { start: 0, end: 10 },
            authored_cells: 2,
            disposition: index::RowDisposition::WholeRowFallback,
        },
        absolute_raw: Some(index::ByteRange {
            start: 137,
            end: 147,
        }),
    });
    let page = std::sync::Arc::new(IndexedResourceResponse {
        root: fixture.page.root.clone(),
        encoded_bytes: 4096,
        value: IndexedResourceValue::Range(index::Response {
            binding: range.binding.clone(),
            cursor: range.cursor,
            items,
            request_complete: true,
            resource_complete: true,
            encoded_bytes: 4096,
            presentations: None,
        }),
    });
    let initial = std::sync::Arc::new(IndexedResourceResponse {
        root: fixture.page.root.clone(),
        encoded_bytes: 4096,
        value: IndexedResourceValue::Range(index::Response {
            binding: range.binding.clone(),
            cursor: range.cursor,
            items: range
                .items
                .iter()
                .filter(|item| {
                    !matches!(
                        item,
                        index::Item::Descriptor(index::IndexedDescriptor {
                            descriptor: index::Descriptor::TableCell {
                                role: index::RowRole::Body,
                                row: 1,
                                ..
                            },
                            ..
                        })
                    )
                })
                .cloned()
                .collect(),
            request_complete: true,
            resource_complete: true,
            encoded_bytes: 4096,
            presentations: None,
        }),
    });
    viewport.adopt(Some(&initial), vec![initial.clone()]);
    assert!(viewport.select(TableCellId::at(1, 0), false));
    let coherent = viewport.selection.clone();
    assert!(!viewport.request_cell(TableCellId::at(2, 0), true));
    assert!(viewport.pending_focus.is_some());
    viewport.dragging = true;
    viewport.adopt(Some(&page), vec![page.clone()]);
    assert_eq!(viewport.selection, coherent);
    assert_eq!(viewport.focus, Some(TableCellId::at(1, 0)));
    assert!(viewport.pending_focus.is_none());
    assert!(!viewport.dragging);
    assert!(viewport.select(TableCellId::at(1, 0), false));
    let selected = viewport.selection.clone();
    assert_eq!(
        viewport.skip_fallback(TableCellId::at(2, 0), true),
        Some(TableCellId::at(3, 0))
    );
    assert!(!viewport.request_cell(TableCellId::at(2, 0), true));
    assert_eq!(viewport.selection, selected);
    assert!(viewport.pending_focus.is_none());
    assert!(!viewport.select(TableCellId::at(3, 0), true));
    assert_eq!(viewport.selection, selected);
}

#[gpui::test]
fn table_selection_uses_whole_stable_cells_and_preserves_rectangle_at_a_gap(
    cx: &mut TestAppContext,
) {
    let fixture = fixture(
        &[
            vec![Some("h0"), Some("h1")],
            vec![Some("**text**"), Some("")],
            vec![Some("tail"), None],
        ],
        "|---|---|",
    );
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Table));
    draw_frames(cx, window, 5);
    let owner = window
        .update(cx, |root, _, app| {
            root.panel.read(app).resource_demand().unwrap()
        })
        .unwrap();
    let mut viewport = TableViewport::new(owner, fixture.binding.resource.revision);
    viewport.adopt(Some(&fixture.page), vec![fixture.page.clone()]);
    assert!(viewport.select(TableCellId::at(1, 0), false));
    assert_eq!(viewport.selection_cells().unwrap().len(), 1);
    assert!(viewport.select(TableCellId::at(2, 1), true));
    assert_eq!(viewport.selection_cells().unwrap().len(), 4);
    let rectangle = viewport.selection.clone();
    assert!(!viewport.select(TableCellId::at(3, 1), true));
    assert_eq!(viewport.selection, rectangle);
    assert!(viewport.select(TableCellId::at(0, 0), false));
    assert!(viewport.select(TableCellId::at(2, 1), true));
    assert!(viewport.whole_table_selection());
    let mut root = fixture.page.root.clone().unwrap();
    root.digest = [99; 32];
    let replacement = IndexedResourceResponse {
        root: Some(root),
        value: IndexedResourceValue::Pending,
        encoded_bytes: 1,
    };
    viewport.adopt(Some(&replacement), Vec::new());
    assert!(viewport.selection.is_none());
    assert!(viewport.cells().is_empty());
}

#[gpui::test]
fn table_source_anchor_resolves_same_source_successor_and_invalidates_other_source(
    cx: &mut TestAppContext,
) {
    let fixture = fixture(
        &[
            vec![Some("h0"), Some("h1")],
            vec![Some("a"), Some("b")],
            vec![Some("c"), Some("d")],
        ],
        "|---|---|",
    );
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Table));
    draw_frames(cx, window, 5);
    let owner = window
        .update(cx, |root, _, app| {
            root.panel.read(app).resource_demand().unwrap()
        })
        .unwrap();
    let mut viewport = TableViewport::new(owner.clone(), fixture.binding.resource.revision);
    viewport.adopt(Some(&fixture.page), vec![fixture.page.clone()]);
    viewport.scroll.borrow_mut().vertical = 7.5;
    viewport.scroll.borrow_mut().horizontal = 3.25;
    viewport.select(TableCellId::at(1, 0), false);
    viewport.rebind(owner.clone(), fixture.binding.resource.revision);
    assert!(viewport.remapping);
    assert!(viewport.selection.is_none());
    let anchor = viewport.anchor.unwrap();
    let descriptor = fixture
        .cells
        .iter()
        .find(|(cell, _)| cell.role == index::RowRole::Body && cell.row == 0 && cell.column == 0)
        .unwrap()
        .1
        .clone();
    let index::Item::Descriptor(descriptor) = descriptor else {
        panic!();
    };
    let response = IndexedResourceResponse {
        root: fixture.page.root.clone(),
        encoded_bytes: 4096,
        value: IndexedResourceValue::Anchor(index::AnchorResponse {
            binding: fixture.binding.clone(),
            resolved: Some(index::ResolvedAnchor { descriptor, anchor }),
            encoded_bytes: 4096,
        }),
    };
    viewport.adopt(Some(&response), Vec::new());
    assert!(!viewport.remapping);
    assert_eq!(viewport.source_continuity, "preserved");
    assert!((viewport.scroll.borrow().vertical - 8.0).abs() < 0.01);
    assert_eq!(viewport.scroll.borrow().horizontal, 3.25);
    viewport.adopt(Some(&fixture.page), vec![fixture.page.clone()]);
    viewport.rebind(owner, fixture.binding.resource.revision);
    let refused = IndexedResourceResponse {
        root: fixture.page.root.clone(),
        encoded_bytes: 4096,
        value: IndexedResourceValue::Anchor(index::AnchorResponse {
            binding: fixture.binding.clone(),
            resolved: None,
            encoded_bytes: 4096,
        }),
    };
    viewport.adopt(Some(&refused), Vec::new());
    assert_eq!(viewport.source_continuity, "invalidated");
    assert!(viewport.anchor.is_none());
    assert_eq!(viewport.scroll.borrow().vertical, 0.0);
}
