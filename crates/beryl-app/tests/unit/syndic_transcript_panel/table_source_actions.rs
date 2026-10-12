use super::super::super::{
    copy_keyboard::CopyMethod,
    table_source_actions::TableSourceActions,
    table_viewport::{TableCellId, TableSelection, TableViewport},
};
use super::table_export_tests::fixture;
use super::*;
use crate::syndic_transcript::{
    IndexedResourceQuery, IndexedResourceResponse, IndexedResourceValue, IndexedSourceResponse,
};
use syndic_storage::indexed_resource as index;

#[gpui::test]
fn table_copy_source_queries_preserve_absolute_binding_split_utf8_and_selection_on_refusal(
    cx: &mut TestAppContext,
) {
    let large = "aa".to_owned() + &"🍒".repeat(2100) + "\r\n";
    let fixture = fixture(&[vec![Some(" h ")], vec![Some(&large)]], "|---|");
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Table));
    draw_frames(cx, window, 5);
    let owner = window
        .update(cx, |root, _, cx| {
            root.panel.read(cx).resource_demand().unwrap()
        })
        .unwrap();
    let mut viewport = TableViewport::new(owner.clone(), fixture.binding.resource.revision);
    viewport.adopt(Some(&fixture.page), vec![fixture.page.clone()]);
    let cell = TableCellId {
        role: index::RowRole::Body,
        row: 0,
        column: 0,
    };
    viewport.selection = Some(TableSelection {
        anchor: cell,
        head: cell,
    });
    let selection = viewport.selection.clone();
    let mut actions = TableSourceActions::default();
    let mut owner = owner;
    actions.observe(&owner, viewport.revision, Some(&fixture.page));
    assert!(actions.begin_copy(&viewport, CopyMethod::Markdown));
    let charge = actions.charge();
    let mut reads = 0;
    while let Some(query) = actions.query() {
        let IndexedResourceQuery::Source { range, .. } = query.query else {
            panic!();
        };
        assert!(range.start >= 137);
        assert!(range.end - range.start <= 8192);
        let page = IndexedResourceResponse {
            root: fixture.page.root.clone(),
            encoded_bytes: 16384,
            value: IndexedResourceValue::Source(IndexedSourceResponse {
                binding: fixture.binding.clone(),
                range,
                bytes: fixture.source[(range.start - 137) as usize..(range.end - 137) as usize]
                    .to_vec(),
            }),
        };
        owner.indexed = Some(query);
        owner.presentation_revision += 1;
        actions.accept_copy_page(&owner, Some(&page));
        reads += 1;
    }
    assert_eq!(reads, 2);
    assert_eq!(actions.assembled_copy_for_test(), Some(large.as_bytes()));
    assert_eq!(actions.charge(), charge);
    assert_eq!(viewport.selection, selection);
    actions.cancel_copy("");
    assert_eq!(actions.charge(), 4096);
    assert!(actions.begin_copy(&viewport, CopyMethod::Stripped));
    owner.indexed = actions.query();
    let mut stale = owner.clone();
    stale.indexed = None;
    assert!(!actions.refuse_query(&stale));
    assert!(actions.pending());
    assert!(actions.refuse_query(&owner));
    assert!(!actions.pending());
    assert_eq!(viewport.selection, selection);
    assert!(actions.feedback.contains("refused"));
    assert!(actions.begin_copy(&viewport, CopyMethod::Markdown));
    owner.indexed = actions.query();
    let query = owner.indexed.clone().unwrap();
    let IndexedResourceQuery::Source { range, .. } = query.query else {
        panic!();
    };
    let mut root = fixture.page.root.clone().unwrap();
    root.digest = [89; 32];
    let stale = IndexedResourceResponse {
        root: Some(root),
        encoded_bytes: 16384,
        value: IndexedResourceValue::Source(IndexedSourceResponse {
            binding: fixture.binding.clone(),
            range,
            bytes: vec![b'x'; (range.end - range.start) as usize],
        }),
    };
    actions.accept_copy_page(&owner, Some(&stale));
    assert!(!actions.pending());
    assert_eq!(viewport.selection, selection);
}

#[gpui::test]
fn table_whole_copy_and_raw_row_copy_use_distinct_source_authorities(cx: &mut TestAppContext) {
    let fixture = fixture(&[vec![Some("h")], vec![Some(" body ")]], "| :-----: |");
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Table));
    draw_frames(cx, window, 5);
    let owner = window
        .update(cx, |root, _, cx| {
            root.panel.read(cx).resource_demand().unwrap()
        })
        .unwrap();
    let mut viewport = TableViewport::new(owner.clone(), fixture.binding.resource.revision);
    viewport.adopt(Some(&fixture.page), vec![fixture.page.clone()]);
    let first = TableCellId {
        role: index::RowRole::Header,
        row: 0,
        column: 0,
    };
    let last = TableCellId {
        role: index::RowRole::Body,
        row: 0,
        column: 0,
    };
    viewport.selection = Some(TableSelection {
        anchor: first,
        head: last,
    });
    let mut actions = TableSourceActions::default();
    actions.observe(&owner, viewport.revision, Some(&fixture.page));
    assert!(actions.begin_copy(&viewport, CopyMethod::Markdown));
    assert!(
        matches!(actions.query().unwrap().query,IndexedResourceQuery::Source{range,..} if range==fixture.binding.range)
    );
    actions.cancel_copy("");
    assert!(!actions.begin_row_copy(&viewport, last, CopyMethod::Markdown));
    assert_eq!(
        viewport.selection,
        Some(TableSelection {
            anchor: first,
            head: last
        })
    );
}

#[gpui::test]
fn table_fallback_is_not_a_selectable_cell_and_explicit_copy_reads_the_entire_raw_row(
    cx: &mut TestAppContext,
) {
    let mut fixture = fixture(
        &[
            vec![Some("h"), Some("j")],
            vec![Some(" first "), Some(" second "), Some(" surplus ")],
        ],
        "|---|---|",
    );
    let row_start = fixture
        .cells
        .iter()
        .find_map(|(id, item)| {
            if id.role != index::RowRole::Body {
                return None;
            }
            match item {
                index::Item::Descriptor(value) => value.absolute_raw.map(|range| range.start - 1),
                _ => None,
            }
        })
        .unwrap();
    let raw = index::ByteRange {
        start: row_start,
        end: fixture.binding.range.end,
    };
    let page = std::sync::Arc::get_mut(&mut fixture.page).unwrap();
    let IndexedResourceValue::Range(range) = &mut page.value else {
        panic!();
    };
    range.items.retain(|item| {
        matches!(
            item,
            index::Item::Descriptor(index::IndexedDescriptor {
                descriptor: index::Descriptor::TableCell {
                    role: index::RowRole::Header,
                    ..
                },
                ..
            })
        )
    });
    range
        .items
        .push(index::Item::Descriptor(index::IndexedDescriptor {
            descriptor: index::Descriptor::TableRow {
                role: index::RowRole::Body,
                ordinal: 0,
                raw: index::ByteRange {
                    start: raw.start - 137,
                    end: raw.end - 137,
                },
                authored_cells: 3,
                disposition: index::RowDisposition::WholeRowFallback,
            },
            absolute_raw: Some(raw),
        }));
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Table));
    draw_frames(cx, window, 5);
    let owner = window
        .update(cx, |root, _, cx| {
            root.panel.read(cx).resource_demand().unwrap()
        })
        .unwrap();
    let mut viewport = TableViewport::new(owner.clone(), fixture.binding.resource.revision);
    viewport.adopt(Some(&fixture.page), vec![fixture.page.clone()]);
    let row = TableCellId {
        role: index::RowRole::Body,
        row: 0,
        column: 0,
    };
    viewport.selection = Some(TableSelection {
        anchor: row,
        head: row,
    });
    assert!(viewport.selection_cells().is_none());
    let mut actions = TableSourceActions::default();
    actions.observe(&owner, viewport.revision, Some(&fixture.page));
    assert!(!actions.begin_copy(&viewport, CopyMethod::Markdown));
    assert!(actions.begin_row_copy(&viewport, row, CopyMethod::Markdown));
    assert!(
        matches!(actions.query().unwrap().query, IndexedResourceQuery::Source { range,.. } if range == raw)
    );
    assert!(fixture.source[(raw.start - 137) as usize..].ends_with(b" surplus |\r\n"));
}

#[gpui::test]
fn table_copy_publication_checks_native_budget_refusal_selection_and_superseded_completion(
    cx: &mut TestAppContext,
) {
    let fixture = fixture(&[vec![Some("h")], vec![Some(" **🍒** ")]], "|---|");
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Table));
    draw_frames(cx, window, 5);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    let demand = panel.update(cx, |panel, _| {
        let demand = panel.resource_demand().unwrap();
        assert!(panel.stage_resource_response(&demand, super::metadata_response(&demand), None));
        demand
    });
    draw_frames(cx, window, 5);
    window
        .update(cx, |root, window, cx| {
            root.panel.update(cx, |panel, _| {
                assert!(panel.measurement_current(window));
                panel.publish_resource_candidate(&demand);
            });
        })
        .unwrap();
    draw_frames(cx, window, 5);
    let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let refuse = std::rc::Rc::new(std::cell::Cell::new(false));
    panel.update(cx, |panel, cx| {
        assert!(panel.staged.is_none());
        let table = panel.table_panels.first_mut().unwrap();
        let id = table.id;
        let owner = panel
            .host
            .indexed_resource_owner(&table.viewport.owner.resource_id)
            .unwrap();
        table.viewport.owner = owner.clone();
        table.viewport.revision = fixture.binding.resource.revision;
        table
            .viewport
            .adopt(Some(&fixture.page), vec![fixture.page.clone()]);
        let selected = TableCellId {
            role: index::RowRole::Body,
            row: 0,
            column: 0,
        };
        table.viewport.selection = Some(TableSelection {
            anchor: selected,
            head: selected,
        });
        let selection = table.viewport.selection.clone();
        table
            .actions
            .observe(&owner, table.viewport.revision, Some(&fixture.page));
        let sink = calls.clone();
        let fail = refuse.clone();
        table.actions.writer = Some(Box::new(move |item, limits, _| {
            let text = item.text().unwrap();
            assert!(limits.text_bytes >= (text.encode_utf16().count() + 1) * 2);
            assert!(limits.total_bytes >= limits.text_bytes);
            sink.borrow_mut().push(text);
            if fail.get() {
                Err(gpui::ClipboardError::Write(
                    gpui::ClipboardRepresentation::Text,
                ))
            } else {
                Ok(())
            }
        }));
        assert!(
            table
                .actions
                .begin_copy(&table.viewport, CopyMethod::Markdown)
        );
        let original = table.actions.sequence;
        assert!(
            table
                .actions
                .begin_copy(&table.viewport, CopyMethod::Stripped)
        );
        let stripped = table.actions.sequence;
        assert_ne!(original, stripped);
        panel.finish_table_copy(id, original, Ok("| **🍒** |".into()), cx);
        assert!(calls.borrow().is_empty());
        panel.finish_table_copy(id, stripped, Ok("🍒".into()), cx);
        assert_eq!(&*calls.borrow(), &["🍒".to_owned()]);
        let table = panel.table_panels.first_mut().unwrap();
        assert_eq!(table.viewport.selection, selection);
        assert!(
            table
                .actions
                .begin_copy(&table.viewport, CopyMethod::Markdown)
        );
        let sequence = table.actions.sequence;
        refuse.set(true);
        panel.finish_table_copy(id, sequence, Ok("| **🍒** |".into()), cx);
        let table = panel.table_panels.first_mut().unwrap();
        assert_eq!(table.viewport.selection, selection);
        assert!(table.actions.feedback.contains("clipboard refused"));
        assert!(!table.actions.pending());
        let before = calls.borrow().len();
        assert!(
            table
                .actions
                .begin_copy(&table.viewport, CopyMethod::Markdown)
        );
        let stale = table.actions.sequence;
        table.viewport.selection = Some(TableSelection {
            anchor: TableCellId {
                role: index::RowRole::Header,
                row: 0,
                column: 0,
            },
            head: selected,
        });
        panel.finish_table_copy(id, stale, Ok("obsolete".into()), cx);
        assert_eq!(calls.borrow().len(), before);
        let table = panel.table_panels.first_mut().unwrap();
        assert!(table.actions.feedback.contains("changed"));
        assert!(
            table
                .actions
                .begin_copy(&table.viewport, CopyMethod::Markdown)
        );
        let stale = table.actions.sequence;
        table.viewport.root.as_mut().unwrap().digest = [89; 32];
        panel.finish_table_copy(id, stale, Ok("obsolete root".into()), cx);
        assert_eq!(calls.borrow().len(), before);
    });
}
