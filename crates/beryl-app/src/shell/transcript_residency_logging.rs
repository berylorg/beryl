use tracing::info;

use super::transcript_history::{TranscriptResidencyRetainedCounts, TranscriptResidencyTargetPlan};
use super::transcript_residency_pins::TranscriptResidencyAdmissionSummary;

pub(super) fn log_transcript_transport_page_received(
    thread_id: &str,
    summary: &TranscriptResidencyAdmissionSummary,
) {
    if summary.transport_turns == 0 {
        return;
    }

    info!(
        thread_id,
        request_kind = summary.request_kind,
        source_range_start = summary.source_range.start,
        source_range_end = summary.source_range.end,
        transport_turns = summary.transport_turns,
        transport_payload_bytes = summary.transport_payload_bytes,
        "Fetched transcript transport page"
    );
}

pub(super) fn log_transcript_resident_turns_admitted(
    thread_id: &str,
    summary: &TranscriptResidencyAdmissionSummary,
) {
    if summary.admitted_turns == 0 && summary.oversized_fallback_turns == 0 {
        return;
    }

    info!(
        thread_id,
        request_kind = summary.request_kind,
        source_range_start = summary.source_range.start,
        source_range_end = summary.source_range.end,
        admitted_turns = summary.admitted_turns,
        admitted_payload_bytes = summary.admitted_payload_bytes,
        transport_turns = summary.transport_turns,
        transport_payload_bytes = summary.transport_payload_bytes,
        oversized_fallback_turns = summary.oversized_fallback_turns,
        "Admitted transcript resident turns"
    );
}

pub(super) fn log_transcript_residency_target_decision(
    thread_id: Option<&str>,
    plan: &TranscriptResidencyTargetPlan,
    counts: &TranscriptResidencyRetainedCounts,
) {
    let desired_turns = plan.desired_full_turn_ids.len();
    let release_intents = plan.release_turn_ids.len();
    let missing_transport_ranges = plan.missing_transport_ranges.len();
    if desired_turns == 0 && release_intents == 0 && missing_transport_ranges == 0 {
        return;
    }

    let target_window_shrunk_by_budget = !plan.diagnostics.viewport_margin_satisfied
        && (plan.diagnostics.resident_turn_limit
            || plan.diagnostics.resident_byte_limit
            || plan.diagnostics.oversized_turn_fallback);

    info!(
        thread_id = thread_id.unwrap_or_default(),
        desired_turns,
        desired_bytes = plan.diagnostics.desired_resident_bytes,
        release_intents,
        missing_transport_ranges,
        resident_turns = counts.resident_turns,
        resident_bytes = counts.resident_bytes,
        resident_payload_bytes = counts.resident_payload_bytes,
        resident_derived_bytes = counts.resident_derived_bytes,
        leading_viewport_margins = counts.leading_viewport_margins,
        trailing_viewport_margins = counts.trailing_viewport_margins,
        viewport_margin_satisfied = plan.diagnostics.viewport_margin_satisfied,
        target_window_shrunk_by_budget,
        budget_reason = plan.diagnostics.limiting_reason.label(),
        in_flight_requests = counts.in_flight_requests,
        oversized_fallback_turns = plan.oversized_turn_fallback_ids.len(),
        "Planned transcript residency target"
    );
}

pub(super) fn log_transcript_resident_turns_released(
    thread_id: Option<&str>,
    released_turns: usize,
    release_kind: &'static str,
) {
    if released_turns == 0 {
        return;
    }

    if let Some(thread_id) = thread_id {
        info!(
            thread_id,
            released_turns, release_kind, "Released transcript resident turns"
        );
    } else {
        info!(
            released_turns,
            release_kind, "Released transcript resident turns"
        );
    }
}
