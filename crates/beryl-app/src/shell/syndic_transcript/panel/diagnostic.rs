use super::*;
use serde_json::{Value, json};

impl SyndicTranscriptPanel {
    pub(crate) fn diagnostic_ui_facts(&self, limit: usize) -> Value {
        let diagnostics = self.host.diagnostic_snapshot();
        let (preparation_pending, retained_measurements, work) = self.measurement_diagnostics();
        let frame = self.last_frame_window.as_ref();
        let published = self.published_snapshot.as_ref();
        let tail = frame.and_then(|frame| frame.records.last()).filter(|row| {
            published.is_some_and(|snapshot| row.index == snapshot.records.len().saturating_sub(1)
                && snapshot.records.get(row.index).is_some_and(|record| record.id == row.record_id))
        }).map(|row| json!({ "index": row.index,
            "recordId": crate::diagnostic_child_protocol::truncate_protocol_message(row.record_id.0.clone()),
            "bottomPx": row.top_px + row.height_px }));
        let rows: Vec<_> = frame.into_iter().flat_map(|frame| frame.records.iter().filter(|row| frame.visible_range.contains(&row.index))).take(limit.min(64)).filter_map(|row| {
            let record = published?.records.get(row.index)?;
            (record.id == row.record_id).then(|| json!({ "index": row.index, "recordId": crate::diagnostic_child_protocol::truncate_protocol_message(record.id.0.clone()), "kind": record_kind(&record.kind), "topPx": row.top_px, "heightPx": row.height_px }))
        }).collect();
        let images = published
            .into_iter()
            .flat_map(|snapshot| snapshot.records.iter())
            .map(image_markers)
            .sum::<usize>();
        let image_resources = published
            .into_iter()
            .flat_map(|snapshot| snapshot.records.iter())
            .filter(|record| {
                matches!(
                    &record.kind,
                    ResidentPresentationRecordKind::ResourceReference {
                        resource_kind: ResourceKind::Image | ResourceKind::GeneratedImage,
                        ..
                    }
                )
            })
            .count();
        json!({
            "publishedViewId": self.published_view_id.as_ref().map(|id| &id.0),
            "viewportHeightPx": self.published_viewport_height,
            "tailPlacement": tail,
            "activationRevision": published.map(|snapshot| snapshot.activation_revision),
            "presentationRevision": published.map(|snapshot| snapshot.presentation_revision),
            "staged": self.staged.is_some(),
            "measurementPreparationPending": preparation_pending,
            "retainedPreparationMeasurements": retained_measurements,
            "layoutObservationsLastFrame": work,
            "visibleRange": frame.map(|frame| [frame.visible_range.start, frame.visible_range.end]),
            "realizedRange": frame.map(|frame| [frame.overscan_range.start, frame.overscan_range.end]),
            "anchor": frame.and_then(|frame| frame.anchor.as_ref()).map(|anchor| json!({ "index": anchor.index, "recordId": crate::diagnostic_child_protocol::truncate_protocol_message(anchor.record_id.0.clone()), "viewportYPx": anchor.viewport_y_px, "position": anchor.position.map(|position| position.0) })),
            "manualScrollTotalPx": frame.map(|frame| frame.manual_scroll_total_px),
            "scrollMode": diagnostics.frame.scroll_mode,
            "renderedRecordCount": frame.map_or(0, |frame| frame.records.len()),
            "visibleRows": rows,
            "visibleRowsTruncated": frame.is_some_and(|frame| frame.visible_range.len() > limit.min(64)),
            "residentRecords": diagnostics.resident_data.resident_record_count,
            "estimatedResidentBytes": diagnostics.resident_data.estimated_resident_bytes,
            "resourceMetadata": diagnostics.resident_data.resident_resource_metadata_count,
            "resourceSlices": diagnostics.resident_data.resident_resource_slice_count,
            "resourceRejections": diagnostics.resident_data.resident_resource_rejection_count,
            "fallbackRecords": diagnostics.resident_data.fallback_record_count,
            "decodedOrUploadedMediaBytes": diagnostics.resident_data.decoded_or_uploaded_media_bytes,
            "imageMarkers": images,
            "imageResources": image_resources,
            "codeReady": self.code_panels.iter().filter(|code| code.layout.state(&code.viewport) == "ready").count(),
            "codePending": self.code_panels.iter().filter(|code| code.layout.state(&code.viewport) == "pending").count(),
            "codeRefused": self.code_panels.iter().filter(|code| code.layout.state(&code.viewport) == "refused").count(),
        })
    }
}

fn image_markers(record: &ResidentPresentationRecord) -> usize {
    match &record.kind {
        ResidentPresentationRecordKind::ImageMarker(_)
        | ResidentPresentationRecordKind::GeneratedImage(_) => 1,
        ResidentPresentationRecordKind::AuthoredInlineParts { parts } => parts
            .iter()
            .filter(|part| matches!(part.kind, ResidentPresentationRecordKind::ImageMarker(_)))
            .count(),
        _ => 0,
    }
}

fn record_kind(kind: &ResidentPresentationRecordKind) -> &'static str {
    match kind {
        ResidentPresentationRecordKind::AuthoredInlineParts { .. } => "inline_parts",
        ResidentPresentationRecordKind::ImageMarker(_) => "image_marker",
        ResidentPresentationRecordKind::GeneratedImage(_) => "generated_image",
        ResidentPresentationRecordKind::NarrativePending { .. } => "narrative_pending",
        ResidentPresentationRecordKind::NarrativeReady { .. } => "narrative_ready",
        ResidentPresentationRecordKind::NarrativeUnavailable { .. } => "narrative_unavailable",
        ResidentPresentationRecordKind::InlineTable { .. } => "inline_table",
        ResidentPresentationRecordKind::InlineTableUnavailable { .. } => "inline_table_unavailable",
        ResidentPresentationRecordKind::TextChunk { .. } => "text",
        ResidentPresentationRecordKind::ResourceReference { .. } => "resource",
        ResidentPresentationRecordKind::LocalUiFallback { .. } => "fallback",
        ResidentPresentationRecordKind::LocalAffordance => "affordance",
        ResidentPresentationRecordKind::ContextUnavailable => "context_unavailable",
    }
}
