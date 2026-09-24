use beryl_app::discussion_handoff_limits::*;
use beryl_state::{HANDOFF_JOB_RECORD_MAX_ENCODED_BYTES, HANDOFF_LIVE_RECORD_MAX_ENCODED_BYTES};

fn configuration() -> HandoffScanConfiguration {
    HandoffScanConfiguration {
        handoff_recovery_page_items: 2,
        handoff_recovery_page_encoded_bytes: HANDOFF_LIVE_RECORD_MAX_ENCODED_BYTES,
        handoff_job_record_encoded_bytes: HANDOFF_JOB_RECORD_MAX_ENCODED_BYTES,
        handoff_reconcile_slots: 3,
        handoff_ready_job_items: 4,
    }
}

#[test]
fn scan_caps_are_positive_and_preserve_full_record_and_key_envelopes() {
    let valid = configuration();
    let limits = HandoffScanLimits::try_from(valid).unwrap();
    assert_eq!(limits.page().max_items(), 2);
    assert_eq!(
        limits.page().max_bytes(),
        HANDOFF_LIVE_RECORD_MAX_ENCODED_BYTES
    );
    assert_eq!(
        limits.job_record_encoded_bytes().get(),
        HANDOFF_JOB_RECORD_MAX_ENCODED_BYTES
    );
    assert_eq!(limits.reconcile_slots().get(), 3);
    assert_eq!(limits.ready_job_items().get(), 4);
    for field in 0..5 {
        let mut invalid = valid;
        match field {
            0 => invalid.handoff_recovery_page_items = 0,
            1 => invalid.handoff_recovery_page_encoded_bytes = 0,
            2 => invalid.handoff_job_record_encoded_bytes = 0,
            3 => invalid.handoff_reconcile_slots = 0,
            _ => invalid.handoff_ready_job_items = 0,
        }
        assert!(matches!(
            HandoffScanLimits::try_from(invalid),
            Err(HandoffScanLimitError::Zero { .. })
        ));
    }
    let mut invalid = valid;
    invalid.handoff_job_record_encoded_bytes -= 1;
    assert!(matches!(
        HandoffScanLimits::try_from(invalid),
        Err(HandoffScanLimitError::RecordTooSmall { .. })
    ));
    let mut invalid = valid;
    invalid.handoff_recovery_page_encoded_bytes -= 1;
    assert!(matches!(
        HandoffScanLimits::try_from(invalid),
        Err(HandoffScanLimitError::PageTooSmall { .. })
    ));
    let mut invalid = valid;
    invalid.handoff_job_record_encoded_bytes += 1;
    assert!(matches!(
        HandoffScanLimits::try_from(invalid),
        Err(HandoffScanLimitError::PageTooSmall { .. })
    ));
    invalid.handoff_recovery_page_encoded_bytes += 1;
    assert!(HandoffScanLimits::try_from(invalid).is_ok());
    invalid.handoff_job_record_encoded_bytes = usize::MAX;
    invalid.handoff_recovery_page_encoded_bytes = usize::MAX;
    assert_eq!(
        HandoffScanLimits::try_from(invalid),
        Err(HandoffScanLimitError::RecordEnvelopeOverflow)
    );
}
