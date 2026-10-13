#[test]
fn literal_narrative_keeps_shared_source_and_utf8_fragment_boundaries() {
    let source: Arc<str> = format!("{}🌸é\r\n**literal**", "x".repeat(4095)).into();
    let charge = PreparedLiteralNarrative::required_charge(&source).unwrap();
    let prepared = PreparedLiteralNarrative::prepare(source.clone(), charge).unwrap();
    assert!(Arc::ptr_eq(&source, &prepared.source));
    assert_eq!(prepared.source(), prepared.display());
    assert_eq!(prepared.retained_bytes(), charge);
    assert_eq!(prepared.fragments().len(), 2);
    let mut offset = 0;
    for fragment in prepared.fragments() {
        assert_eq!(fragment.display.start, offset);
        assert!(fragment.display.len() <= NARRATIVE_FRAGMENT_MAX_BYTES);
        assert!(source.get(fragment.display.clone()).is_some());
        assert_eq!(
            prepared.runs()[fragment.runs.start].display,
            fragment.display
        );
        offset = fragment.display.end;
    }
    assert_eq!(offset, source.len());
    assert_eq!(prepared.snap_display_endpoint(4096, false), Some(4095));
    assert_eq!(prepared.snap_display_endpoint(4096, true), Some(4099));
    assert_eq!(
        prepared.source_range_for_display(4095..4101),
        Some(4095..4101)
    );
    assert_eq!(prepared.selected_display(4095..4101), Some("🌸é"));
    assert_eq!(prepared.source_range_for_display(4096..4101), None);
    assert_eq!(
        prepared.snap_display_endpoint(source.len() + 1, false),
        None
    );
}

#[test]
fn literal_narrative_refuses_before_adopting_over_capacity_source_or_metadata() {
    let source: Arc<str> = "source".into();
    let charge = PreparedLiteralNarrative::required_charge(&source).unwrap();
    assert_eq!(
        PreparedLiteralNarrative::prepare(source.clone(), charge - 1).unwrap_err(),
        LiteralPreparationError::RetainedCapacity
    );
    assert_eq!(Arc::strong_count(&source), 1);
    let maximum: Arc<str> = "🌸".repeat(NARRATIVE_MAX_BYTES / 4).into();
    let charge = PreparedLiteralNarrative::required_charge(&maximum).unwrap();
    let prepared = PreparedLiteralNarrative::prepare(maximum, charge).unwrap();
    assert_eq!(
        prepared.fragments().len(),
        NARRATIVE_MAX_BYTES / NARRATIVE_FRAGMENT_MAX_BYTES
    );
    let over: Arc<str> = "x".repeat(NARRATIVE_MAX_BYTES + 1).into();
    assert_eq!(
        PreparedLiteralNarrative::prepare(over, usize::MAX).unwrap_err(),
        LiteralPreparationError::SourceCapacity
    );
}

#[test]
fn empty_literal_narrative_has_no_fabricated_run_mapping_or_fragment() {
    let source: Arc<str> = "".into();
    let charge = PreparedLiteralNarrative::required_charge(&source).unwrap();
    let prepared = PreparedLiteralNarrative::prepare(source, charge).unwrap();
    assert!(prepared.runs().is_empty());
    assert!(prepared.mappings().is_empty());
    assert!(prepared.fragments().is_empty());
    assert_eq!(prepared.source_range_for_display(0..0), Some(0..0));
    assert_eq!(prepared.snap_display_endpoint(0, false), Some(0));
}
