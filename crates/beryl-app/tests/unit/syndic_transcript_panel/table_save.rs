use super::super::super::code_panel::save::{SaveResult, SaveWriterOptions, TextResourceSave};
use super::table_export_tests::fixture;
use super::*;

#[gpui::test]
fn table_save_captures_original_nonzero_range_root_and_elects_publication_once(
    cx: &mut TestAppContext,
) {
    let fixture = fixture(&[vec![Some(" h ")], vec![Some(" value ")]], "|---|");
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Table));
    draw_frames(cx, window, 5);
    let mut owner = window
        .update(cx, |root, _, cx| {
            root.panel.read(cx).resource_demand().unwrap()
        })
        .unwrap();
    let root = fixture.page.root.as_ref().unwrap().digest;
    let sequence = 9;
    let mut save = TextResourceSave::new_exact(
        &owner,
        fixture.binding.clone(),
        root,
        fixture.binding.resource.revision,
        sequence,
        fixture.binding.range,
    )
    .unwrap();
    let terminal = save.attach_writer(&cx.background_executor, SaveWriterOptions::default());
    assert!(save.current_exact(
        &owner,
        Some(&fixture.binding),
        Some(root),
        Some(fixture.binding.resource.revision),
        sequence
    ));
    owner.presentation_revision += 1;
    assert!(save.current_exact(
        &owner,
        Some(&fixture.binding),
        Some(root),
        Some(fixture.binding.resource.revision),
        sequence
    ));
    assert!(!save.current_exact(
        &owner,
        Some(&fixture.binding),
        Some([98; 32]),
        Some(fixture.binding.resource.revision),
        sequence
    ));
    assert!(!save.current_exact(
        &owner,
        Some(&fixture.binding),
        Some(root),
        Some(fixture.binding.resource.revision),
        sequence + 1
    ));
    let mut retired_owner = owner.clone();
    retired_owner.resource_id = crate::syndic_transcript::ResourceId("replacement-resource".into());
    assert!(!save.current_exact(
        &retired_owner,
        Some(&fixture.binding),
        Some(root),
        Some(fixture.binding.resource.revision),
        sequence
    ));
    save.acknowledged(SaveResult::Opened);
    let query = save.query_exact(Some(&fixture.binding)).unwrap();
    assert_eq!(query.root, Some(root));
    assert!(
        matches!(query.query,crate::syndic_transcript::IndexedResourceQuery::Source{range,..} if range==fixture.binding.range)
    );
    save.acknowledged(SaveResult::Wrote(fixture.source.len()));
    save.acknowledged(SaveResult::Prepared);
    assert!(save.elect());
    assert!(!save.elect());
    drop(save);
    cx.background_executor.block_test(terminal).unwrap();
    let row_range = syndic_storage::indexed_resource::ByteRange {
        start: fixture.binding.range.end - 5,
        end: fixture.binding.range.end,
    };
    let mut row_save = TextResourceSave::new_exact(
        &owner,
        fixture.binding.clone(),
        root,
        fixture.binding.resource.revision,
        sequence + 1,
        row_range,
    )
    .unwrap();
    let terminal = row_save.attach_writer(&cx.background_executor, SaveWriterOptions::default());
    row_save.acknowledged(SaveResult::Opened);
    assert!(
        matches!(row_save.query_exact(Some(&fixture.binding)).unwrap().query,
        crate::syndic_transcript::IndexedResourceQuery::Source { range,.. } if range == row_range)
    );
    drop(row_save);
    cx.background_executor.block_test(terminal).unwrap();
}

#[gpui::test]
fn table_save_streams_a_captured_large_range_without_the_copy_limit(cx: &mut TestAppContext) {
    let large = "🍒\r\n".repeat(20_000);
    let fixture = fixture(&[vec![Some("h")], vec![Some(&large)]], "|---|");
    assert!(fixture.source.len() > super::super::super::table_export::COPY_BYTES);
    let window = mounted_panel(cx, resource_seed(1, ResourceKind::Table));
    draw_frames(cx, window, 5);
    let owner = window
        .update(cx, |root, _, cx| {
            root.panel.read(cx).resource_demand().unwrap()
        })
        .unwrap();
    let root = fixture.page.root.as_ref().unwrap().digest;
    let mut save = TextResourceSave::new_exact(
        &owner,
        fixture.binding.clone(),
        root,
        fixture.binding.resource.revision,
        1,
        fixture.binding.range,
    )
    .unwrap();
    let terminal = save.attach_writer(&cx.background_executor, SaveWriterOptions::default());
    save.acknowledged(SaveResult::Opened);
    let mut cursor = fixture.binding.range.start;
    let mut pages = 0;
    while let Some(query) = save.query_exact(Some(&fixture.binding)) {
        assert_eq!(query.root, Some(root));
        let crate::syndic_transcript::IndexedResourceQuery::Source { range, .. } = query.query
        else {
            panic!();
        };
        assert_eq!(range.start, cursor);
        assert!(range.end - range.start <= 8192);
        cursor = range.end;
        save.acknowledged(SaveResult::Wrote((range.end - range.start) as usize));
        pages += 1;
    }
    assert!(pages > 8);
    assert_eq!(cursor, fixture.binding.range.end);
    drop(save);
    cx.background_executor.block_test(terminal).unwrap();
}
