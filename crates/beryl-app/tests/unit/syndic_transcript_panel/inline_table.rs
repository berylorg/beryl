use super::super::{
    copy_keyboard::CopyMethod,
    table_export::TableExport,
    table_panel::RenderSource,
    table_viewport::{TableCellId, TableViewport},
};
use super::*;
use crate::syndic_transcript::core::InlineTableOwner;
use std::sync::Arc;
use syndic_storage::indexed_resource as index;

fn prepared(source: &str) -> Arc<syndic_storage::inline_table::PreparedInlineTable> {
    Arc::new(
        syndic_storage::inline_table::prepare_inline_table(
            syndic_storage::ProjectionPayload::inline_markdown(
                syndic_storage::MarkdownBlockId::from_bytes([73; 32]),
                syndic_storage::MarkdownBlockKind::Table,
                1,
                syndic_storage::ProjectionSourceRange::new(0, source.len() as u64).unwrap(),
                source,
            )
            .unwrap(),
            131072,
        )
        .unwrap(),
    )
}

fn activation(source: &str) -> PreparedTranscriptActivation {
    let mut seed = seed(1);
    let Some(TranscriptProviderResponseKind::ProjectionRecords(records)) =
        seed.projection_records_response.as_mut()
    else {
        panic!();
    };
    let record = &mut records.records[0];
    record.kind = ProjectionRecordKind::InlineTable;
    record.payload = ProjectionPayload::InlineTable {
        prepared: prepared(source),
    };
    record.provenance.block_id = Some([73; 32]);
    record.provenance.source_range = Some(0..source.len() as u64);
    record.provenance.copy_source_range = Some(0..source.len() as u64);
    record.provenance.item_id = Some(crate::syndic_transcript::provider::SyndicItemId(
        "inline-item".to_owned(),
    ));
    seed
}

fn viewport(source: &str) -> TableViewport {
    TableViewport::inline(InlineTableOwner {
        view_id: TranscriptViewId("inline-view".to_owned()),
        record_id: ResidentPresentationRecordId("inline-record".to_owned()),
        activation_revision: 1,
        provenance: super::super::super::ResidentRecordProvenance {
            source: super::super::super::ResidentRecordSource::LocalUi,
            projection_id: None,
            projection_revision: Some(ProviderRevision(1)),
            presentation_revision: 1,
            copy_source_range: None,
        },
        prepared: prepared(source),
    })
}

fn maximum_source() -> String {
    let mut source = format!(
        "|{}|\r\n|{}|\r\n",
        vec!["h"; 12].join("|"),
        vec![":---:"; 12].join("|")
    );
    for _ in 0..32 {
        source.push_str("|a|b|c|d|e|f|g|h|i|j|k|l|\r\n");
    }
    source
}

#[test]
fn inline_maximum_selects_all_offscreen_positions_without_expanding_paint_or_resource_owner() {
    let source = maximum_source();
    let mut viewport = viewport(&source);
    viewport.scroll.borrow_mut().bounds = gpui::Bounds::new(
        gpui::point(px(0.0), px(0.0)),
        gpui::size(px(640.0), px(300.0)),
    );
    assert_eq!(viewport.cells().len(), 396);
    assert!(viewport.owner.indexed().is_none());
    assert!(
        viewport.root.is_none() && viewport.binding.is_none() && viewport.source_pages().is_empty()
    );
    assert!(viewport.select(TableCellId::at(0, 0), false));
    assert!(viewport.request_cell(TableCellId::at(32, 11), true));
    assert!(viewport.pending_focus.is_none());
    assert_eq!(viewport.selection_cells().unwrap().len(), 396);
    assert!(viewport.whole_table_selection());
    let model = &viewport.owner.inline().unwrap().prepared;
    let export = TableExport::inline(viewport.selection_cells().unwrap(), model, true).unwrap();
    let bytes = export.assemble_inline(&source).unwrap();
    assert_eq!(export.render(bytes, CopyMethod::Markdown).unwrap(), source);
    viewport.reveal(TableCellId::at(32, 11));
    let (rows, columns) = viewport.realized_ranges();
    assert!(rows.end - rows.start <= 15 && columns.end - columns.start <= 8);
    assert!(viewport.paint_cells().len() <= 128);
}

#[test]
fn inline_partial_export_preserves_selected_raw_cells_and_missing_positions() {
    let source = "| h | i | j |\r\n|:----|---:|:---:|\r\n| **a** | b\\|c | |\r\n| lone | |\r\n";
    let mut viewport = viewport(source);
    assert!(viewport.select(TableCellId::at(1, 1), false));
    assert!(viewport.select(TableCellId::at(2, 2), true));
    let cells = viewport.selection_cells().unwrap();
    assert_eq!(cells.len(), 4);
    assert!(!viewport.whole_table_selection());
    let model = &viewport.owner.inline().unwrap().prepared;
    let export = TableExport::inline(cells.clone(), model, false).unwrap();
    let bytes = export.assemble_inline(source).unwrap();
    assert_eq!(
        export.render(bytes, CopyMethod::Markdown).unwrap(),
        "| b\\|c | |\n| ||"
    );
    let export = TableExport::inline(cells, model, false).unwrap();
    let bytes = export.assemble_inline(source).unwrap();
    assert_eq!(export.render(bytes, CopyMethod::Stripped).unwrap(), "b|c");
}

#[test]
fn inline_fallback_crossing_and_remeasurement_preserve_last_coherent_selection() {
    let source = "| h | i |\r\n|---|---|\r\n| a | b |\r\n| surplus | c | extra |\r\n| d | e |\r\n";
    let mut viewport = viewport(source);
    assert!(viewport.select(TableCellId::at(1, 0), false));
    let selection = viewport.selection.clone();
    assert!(!viewport.request_cell(TableCellId::at(3, 1), true));
    assert_eq!(viewport.selection, selection);
    assert!(viewport.pending_focus.is_none());
    assert!(!viewport.whole_table_selection());
    assert!(viewport.set_tracks(36.0, 240.0));
    assert_eq!(viewport.selection, selection);
    assert_eq!(viewport.focus, Some(TableCellId::at(1, 0)));
    let raw = viewport.fallback_rows()[0].1.raw.unwrap();
    let export = TableExport::raw(raw).unwrap();
    let bytes = export.assemble_inline(source).unwrap();
    assert_eq!(
        export.render(bytes, CopyMethod::Markdown).unwrap(),
        "| surplus | c | extra |\r\n"
    );
}

#[gpui::test]
fn mounted_inline_table_charges_projection_presentation_layout_and_releases_on_retirement(
    cx: &mut TestAppContext,
) {
    let source = maximum_source();
    let window = mounted_panel(cx, activation(&source));
    draw_frames(cx, window, 6);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    panel.update(cx, |panel, _| {
        assert_eq!(panel.table_panels.len(), 1);
        let table = &panel.table_panels[0];
        let owner = table.viewport.owner.inline().unwrap().clone();
        let source_charge = owner.prepared.retained_bytes() + 2 * std::mem::size_of::<usize>();
        assert_eq!(
            panel.host.shared_snapshot().records[0].estimated_bytes,
            source_charge
        );
        assert_eq!(
            panel
                .host
                .diagnostic_snapshot()
                .resident_data
                .projection_bytes,
            source_charge
        );
        assert_eq!(
            panel
                .host
                .diagnostic_snapshot()
                .resident_data
                .presentation_bytes,
            source_charge
        );
        assert_eq!(
            panel
                .host
                .diagnostic_snapshot()
                .resident_data
                .resource_bytes,
            table.viewport.charge() + table.actions.charge()
        );
        assert!(
            !panel
                .host
                .reserve_inline_table_layout(&owner, 8 * 1024 * 1024 + 1)
        );
        assert!(panel.host.resource_demands().is_empty());
        assert!(panel.host.shared_snapshot().resources.metadata.is_empty());
        panel.retire();
        assert!(!panel.host.inline_table_current(&owner));
        assert_eq!(
            panel
                .host
                .diagnostic_snapshot()
                .resident_data
                .resource_bytes,
            0
        );
    });
    draw_frames(cx, window, 3);
    panel.read_with(cx, |panel, _| {
        assert!(panel.table_panels.is_empty() && panel.retired_table_panels.is_empty())
    });
}

#[gpui::test]
fn inline_source_replacement_retires_callbacks_selection_and_preserves_block_anchor(
    cx: &mut TestAppContext,
) {
    let window = mounted_panel(cx, activation("|h|i|\n|---|---|\n|a|b|\n"));
    draw_frames(cx, window, 6);
    let panel = window.update(cx, |root, _, _| root.panel.clone()).unwrap();
    let (old_id, record, callback) = panel.update(cx, |panel, _| {
        let table = &mut panel.table_panels[0];
        assert!(table.viewport.select(TableCellId::at(1, 0), false));
        (
            table.id,
            table.viewport.owner.record_id().clone(),
            RenderSource::for_viewport(table.id, &table.viewport),
        )
    });
    panel.update(cx, |panel, _| {
        panel.stage_activation(&activation("|h|i|\n|---|---|\n|changed|b|\n"), None)
    });
    draw_frames(cx, window, 6);
    panel.read_with(cx, |panel, _| {
        let table = &panel.table_panels[0];
        assert_ne!(table.id, old_id);
        assert_eq!(table.viewport.owner.record_id(), &record);
        assert!(table.viewport.selection.is_none());
        assert!(!callback.matches(table.id, &table.viewport));
    });
    let mut successor = activation("|h|i|\n|---|---|\n|changed|b|\n");
    let Some(TranscriptProviderResponseKind::ProjectionRecords(records)) =
        successor.projection_records_response.as_mut()
    else {
        panic!();
    };
    records.records[0].kind = ProjectionRecordKind::ResourceReference;
    records.records[0].payload = ProjectionPayload::ResourceReference {
        resource_id: ResourceId("promoted-table".to_owned()),
        resource_kind: ResourceKind::Table,
        label: None,
    };
    panel.update(cx, |panel, _| panel.stage_activation(&successor, None));
    draw_frames(cx, window, 6);
    panel.read_with(cx, |panel, _| {
        assert!(panel.table_panels.is_empty());
        assert_eq!(panel.host.shared_snapshot().records[0].id, record);
        assert!(panel.inline_table_source_for_test().is_none());
    });
}
