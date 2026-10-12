use super::*;

#[test]
fn inline_table_worker_publishes_complete_source_and_charge_without_resource_authority() {
    let fixture = Fixture::new();
    let source = "| h | i | j |\r\n|:---|---:|---|\r\n| **a** | b\\|c |\r\n| lone | |\r\n";
    fixture.seed_projection_kind(source, 1, syndic_storage::MarkdownBlockKind::Table);
    let cancel = AtomicBool::new(false);
    let request = fixture.request(1);
    let prepared = fixture
        .reader
        .prepare_attachment(request.clone(), &cancel)
        .unwrap();
    fixture
        .reader
        .publish_if_current(prepared, &request, &cancel, |seed| {
            let Some(crate::syndic_transcript::TranscriptProviderResponseKind::ProjectionRecords(
                records,
            )) = seed.projection_records_response
            else {
                panic!("missing projection");
            };
            assert_eq!(records.records.len(), 1);
            let record = &records.records[0];
            let crate::syndic_transcript::ProjectionPayload::InlineTable { prepared } =
                &record.payload
            else {
                panic!("recognized Table lost typed payload");
            };
            assert_eq!(prepared.source(), source);
            assert_eq!(prepared.body_rows(), 2);
            assert_eq!(prepared.columns(), 3);
            assert_eq!(prepared.cells().len(), 9);
            assert!(prepared.retained_bytes() > source.len());
            assert!(record.provenance.resource_id.is_none());
            assert!(record.provenance.resource_range.is_none());
            assert_eq!(record.provenance.source_range, Some(0..source.len() as u64));
            assert!(
                prepared
                    .cell(syndic_storage::indexed_resource::RowRole::Body, 1, 2)
                    .unwrap()
                    .is_missing()
            );
        })
        .unwrap();
    assert!(fixture.reader.retire());
}

#[test]
fn invalid_inline_table_remains_typed_unavailable_and_preserves_source() {
    let fixture = Fixture::new();
    let source = "recognized descriptor with invalid table structure";
    fixture.seed_projection_kind(source, 1, syndic_storage::MarkdownBlockKind::Table);
    let cancel = AtomicBool::new(false);
    let request = fixture.request(1);
    let prepared = fixture
        .reader
        .prepare_attachment(request.clone(), &cancel)
        .unwrap();
    fixture
        .reader
        .publish_if_current(prepared, &request, &cancel, |seed| {
            let Some(crate::syndic_transcript::TranscriptProviderResponseKind::ProjectionRecords(
                records,
            )) = seed.projection_records_response
            else {
                panic!("missing projection");
            };
            let crate::syndic_transcript::ProjectionPayload::InlineTableUnavailable {
                source: retained,
                reason,
            } = &records.records[0].payload
            else {
                panic!("invalid table became text");
            };
            assert_eq!(retained.as_ref(), source);
            assert_eq!(
                *reason,
                syndic_storage::inline_table::InlineTablePreparationErrorKind::InvalidStructure
            );
        })
        .unwrap();
    assert!(fixture.reader.retire());
}
