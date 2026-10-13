#[test]
fn literal_copy_strips_selected_literal_parts_and_preserves_prepared_display_spelling() {
    let cancelled = AtomicBool::new(false);
    let parts = vec![
        NarrativeCopyPart::Mapped("*escaped* → ".to_owned()),
        NarrativeCopyPart::Literal("**literal** [label](target)".to_owned()),
        NarrativeCopyPart::Mapped(" `authored`".to_owned()),
    ];
    assert_eq!(
        convert(parts, &cancelled).unwrap(),
        "*escaped* → literal label `authored`"
    );
    assert_eq!(
        convert(
            vec![NarrativeCopyPart::Literal("**partial".to_owned())],
            &cancelled
        )
        .unwrap(),
        "**partial"
    );
}

#[test]
fn literal_copy_refuses_cancelled_empty_and_combined_source_overflow() {
    let cancelled = AtomicBool::new(true);
    assert!(
        convert(
            vec![NarrativeCopyPart::Literal("**source**".to_owned())],
            &cancelled
        )
        .is_err()
    );
    cancelled.store(false, Ordering::Release);
    assert!(convert(Vec::new(), &cancelled).is_err());
    assert!(
        convert(
            vec![
                NarrativeCopyPart::Literal("x".repeat(COPY_TEXT_BYTES)),
                NarrativeCopyPart::Mapped("y".to_owned()),
            ],
            &cancelled
        )
        .is_err()
    );
}

#[test]
fn literal_copy_cancellation_keeps_work_slot_until_output_releases_and_overflow_fences_admission() {
    let mut copy = LiteralCopy::default();
    copy.busy.store(true, Ordering::Release);
    let slot = LiteralCopySlot(copy.busy.clone());
    let cancellation = Arc::new(AtomicBool::new(false));
    copy.cancelled = Some(cancellation.clone());
    copy.serial = Some(u64::MAX);
    copy.cancel();
    assert!(cancellation.load(Ordering::Acquire));
    assert!(copy.busy.load(Ordering::Acquire));
    assert_eq!(copy.serial, None);
    copy.cancel();
    assert_eq!(copy.serial, None);
    drop(slot);
    assert!(!copy.busy.load(Ordering::Acquire));
}
