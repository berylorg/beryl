use super::{MAX_REPORT_BYTES, ReportRecord, TRUNCATION};
use std::sync::atomic::Ordering;

#[test]
fn complete_report_preserves_payload_and_location() {
    let mut record = ReportRecord::empty();
    record.write_report("writer invariant failed", Some(("writer.rs", 123, 4)));
    let report = record.read().unwrap();
    assert!(report.contains("Panic at writer.rs:123:4"));
    assert!(report.ends_with("writer invariant failed"));
}

#[test]
fn oversized_multibyte_payload_has_explicit_valid_utf8_truncation() {
    let mut record = ReportRecord::empty();
    record.write_report(&"🙂".repeat(MAX_REPORT_BYTES), None);
    let report = record.read().unwrap();
    assert!(report.len() <= MAX_REPORT_BYTES);
    assert!(report.ends_with(TRUNCATION));
    assert!(report.contains("Panic location unavailable"));
}

#[test]
fn incomplete_publication_exposes_no_report() {
    let mut record = ReportRecord::empty();
    record.write_report("partial", None);
    record.complete.store(0, Ordering::Release);
    assert!(record.read().is_none());
}

#[test]
fn incompatible_record_exposes_no_report() {
    let mut record = ReportRecord::empty();
    record.write_report("payload", None);
    record.format[0] = 0;
    assert!(record.read().is_none());
}

#[test]
fn oversized_length_exposes_no_report() {
    let mut record = ReportRecord::empty();
    record.write_report("payload", None);
    record.length = (MAX_REPORT_BYTES + 1) as u32;
    assert!(record.read().is_none());
}

#[test]
fn invalid_utf8_exposes_no_report() {
    let mut record = ReportRecord::empty();
    record.write_report("payload", None);
    record.text[0] = 0xff;
    assert!(record.read().is_none());
}
