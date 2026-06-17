use std::ops::Range;

#[path = "support/syndic_transcript_core.rs"]
mod syndic_transcript_core;

use syndic_transcript_core::fixture_provider::InMemorySyndicTranscriptProvider;
use syndic_transcript_core::*;

const REVISION: ProviderRevision = ProviderRevision(73);

fn view_id() -> TranscriptViewId {
    TranscriptViewId("selection-view".to_string())
}

fn projection_id(name: &str) -> ProjectionRecordId {
    ProjectionRecordId(format!("projection-{name}"))
}

fn provenance(
    view_id: &TranscriptViewId,
    position: u64,
    projection_id: &ProjectionRecordId,
    copy_source_range: Option<Range<u64>>,
) -> SyndicSourceProvenance {
    SyndicSourceProvenance {
        view_id: view_id.clone(),
        position: Some(TranscriptViewPosition(position)),
        turn_id: Some(SyndicTurnId(format!("turn-{position}"))),
        item_id: Some(SyndicItemId(format!("item-{position}"))),
        projection_id: Some(projection_id.clone()),
        resource_id: None,
        source_range: Some(position..position + 10),
        resource_range: None,
        copy_source_range,
    }
}

fn resource_provenance(
    view_id: &TranscriptViewId,
    position: u64,
    projection_id: &ProjectionRecordId,
    resource_id: &ResourceId,
) -> SyndicSourceProvenance {
    let mut provenance = provenance(view_id, position, projection_id, None);
    provenance.resource_id = Some(resource_id.clone());
    provenance.resource_range = Some(0..128);
    provenance
}

fn missing_context_menu_provenance(
    view_id: &TranscriptViewId,
    position: u64,
    projection_id: &ProjectionRecordId,
) -> SyndicSourceProvenance {
    let mut provenance = provenance(
        view_id,
        position,
        projection_id,
        Some(position..position + 8),
    );
    provenance.projection_id = None;
    provenance
}

fn view_record(
    view_id: &TranscriptViewId,
    position: u64,
    name: &str,
    projection_id: ProjectionRecordId,
    copy_source_range: Option<Range<u64>>,
) -> TranscriptViewRecord {
    TranscriptViewRecord {
        id: TranscriptViewRecordId(format!("record-{name}")),
        position: TranscriptViewPosition(position),
        projection_id: projection_id.clone(),
        narrative_kind: TranscriptNarrativeKind::AssistantFinalAnswer,
        provenance: provenance(view_id, position, &projection_id, copy_source_range),
    }
}

fn view_record_with_provenance(
    position: u64,
    name: &str,
    projection_id: ProjectionRecordId,
    provenance: SyndicSourceProvenance,
) -> TranscriptViewRecord {
    TranscriptViewRecord {
        id: TranscriptViewRecordId(format!("record-{name}")),
        position: TranscriptViewPosition(position),
        projection_id,
        narrative_kind: TranscriptNarrativeKind::AssistantFinalAnswer,
        provenance,
    }
}

fn text_projection(
    view_id: &TranscriptViewId,
    position: u64,
    name: &str,
    text: &str,
    copy_source_range: Option<Range<u64>>,
) -> ProjectionRecord {
    let projection_id = projection_id(name);
    ProjectionRecord {
        id: projection_id.clone(),
        revision: ProviderRevision(0),
        kind: ProjectionRecordKind::TextChunk,
        payload: ProjectionPayload::Text {
            text: text.to_string(),
        },
        provenance: provenance(view_id, position, &projection_id, copy_source_range),
    }
}

fn text_projection_with_provenance(
    name: &str,
    text: &str,
    provenance: SyndicSourceProvenance,
) -> ProjectionRecord {
    ProjectionRecord {
        id: projection_id(name),
        revision: ProviderRevision(0),
        kind: ProjectionRecordKind::TextChunk,
        payload: ProjectionPayload::Text {
            text: text.to_string(),
        },
        provenance,
    }
}

fn resource_projection(
    view_id: &TranscriptViewId,
    position: u64,
    name: &str,
    resource_id: ResourceId,
    resource_kind: ResourceKind,
) -> ProjectionRecord {
    let projection_id = projection_id(name);
    ProjectionRecord {
        id: projection_id.clone(),
        revision: ProviderRevision(0),
        kind: ProjectionRecordKind::ResourceReference,
        payload: ProjectionPayload::ResourceReference {
            resource_id: resource_id.clone(),
            resource_kind,
            label: Some("resident resource".to_string()),
        },
        provenance: resource_provenance(view_id, position, &projection_id, &resource_id),
    }
}

fn handle_provider_request(
    core: &mut ResidentTranscriptCore,
    provider: &mut InMemorySyndicTranscriptProvider,
    request: TranscriptProviderRequest,
) -> ResidentProviderResponseEffect {
    let response = provider
        .handle_request(request)
        .expect("fixture provider request should not fail");
    core.handle_provider_response(response)
}

fn seed_text_presentation(records: &[(&str, &str, Option<Range<u64>>)]) -> ResidentTranscriptCore {
    let view_id = view_id();
    let mut provider = InMemorySyndicTranscriptProvider::new();
    let mut view_records = Vec::new();
    provider.set_revision(REVISION);

    for (index, (name, text, copy_source_range)) in records.iter().enumerate() {
        let position = ((index as u64) + 1) * 10;
        let projection_id = projection_id(name);
        view_records.push(view_record(
            &view_id,
            position,
            name,
            projection_id,
            copy_source_range.clone(),
        ));
        provider.insert_projection_record(text_projection(
            &view_id,
            position,
            name,
            text,
            copy_source_range.clone(),
        ));
    }

    provider.insert_view_records(view_id.clone(), view_records);
    let mut core = ResidentTranscriptCore::empty();
    let view_request = core.request_view_page(
        view_id.clone(),
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        ProviderRequestReason::ActivationSeed,
    );
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, view_request),
        ResidentProviderResponseEffect::ViewPageAdmitted {
            admitted_count: records.len()
        }
    );
    let projection_request = core
        .request_projection_records_for_resident_view(ProviderRequestReason::ProjectionAdmission)
        .expect("seeded view should need projection records");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, projection_request),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: records.len(),
            rejected_count: 0
        }
    );
    core
}

fn seed_text_and_resource_presentation() -> ResidentTranscriptCore {
    let view_id = view_id();
    let resource_id = ResourceId("resource-context-menu".to_string());
    let text_projection_id = projection_id("answer");
    let resource_projection_id = projection_id("resource");
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider.set_revision(REVISION);

    provider.insert_view_records(
        view_id.clone(),
        vec![
            view_record(
                &view_id,
                10,
                "answer",
                text_projection_id.clone(),
                Some(10..24),
            ),
            view_record_with_provenance(
                20,
                "resource",
                resource_projection_id.clone(),
                resource_provenance(&view_id, 20, &resource_projection_id, &resource_id),
            ),
        ],
    );
    provider.insert_projection_record(text_projection(
        &view_id,
        10,
        "answer",
        "open menu",
        Some(10..24),
    ));
    provider.insert_projection_record(resource_projection(
        &view_id,
        20,
        "resource",
        resource_id,
        ResourceKind::GeneratedImage,
    ));

    let mut core = ResidentTranscriptCore::empty();
    let view_request = core.request_view_page(
        view_id.clone(),
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        ProviderRequestReason::ActivationSeed,
    );
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, view_request),
        ResidentProviderResponseEffect::ViewPageAdmitted { admitted_count: 2 }
    );
    let projection_request = core
        .request_projection_records_for_resident_view(ProviderRequestReason::ProjectionAdmission)
        .expect("seeded view should need projection records");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, projection_request),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: 2,
            rejected_count: 0
        }
    );
    core
}

fn seed_missing_context_menu_provenance_presentation() -> ResidentTranscriptCore {
    let view_id = view_id();
    let projection_id = projection_id("missing-menu-provenance");
    let view_provenance = provenance(&view_id, 10, &projection_id, Some(10..18));
    let projection_provenance = missing_context_menu_provenance(&view_id, 10, &projection_id);
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider.set_revision(REVISION);
    provider.insert_view_records(
        view_id.clone(),
        vec![view_record_with_provenance(
            10,
            "missing-menu-provenance",
            projection_id,
            view_provenance,
        )],
    );
    provider.insert_projection_record(text_projection_with_provenance(
        "missing-menu-provenance",
        "missing provenance",
        projection_provenance,
    ));

    let mut core = ResidentTranscriptCore::empty();
    let view_request = core.request_view_page(
        view_id.clone(),
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        ProviderRequestReason::ActivationSeed,
    );
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, view_request),
        ResidentProviderResponseEffect::ViewPageAdmitted { admitted_count: 1 }
    );
    let projection_request = core
        .request_projection_records_for_resident_view(ProviderRequestReason::ProjectionAdmission)
        .expect("seeded view should need projection records");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, projection_request),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: 1,
            rejected_count: 0
        }
    );
    core
}

fn realize_selection_frame(core: &mut ResidentTranscriptCore) -> RealizedFrameWindow {
    realize_frame_with_viewport(core, 240.0)
}

fn realize_frame_with_viewport(
    core: &mut ResidentTranscriptCore,
    viewport_height_px: f32,
) -> RealizedFrameWindow {
    let snapshot = core.presentation_snapshot();
    let mut controller = RealizedFrameScrollController::new();
    let window = controller.realize(
        &snapshot,
        RealizedFrameRequest {
            viewport_height_px,
            overscan_height_px: 0.0,
            default_record_height_px: 20.0,
            manual_delta_px: 0.0,
            observed_presentation_revision: None,
        },
    );
    for fact in &window.demand_facts {
        core.push_demand_fact(fact.clone());
    }
    window
}

fn selection_command_for_frame_records(
    presentation_revision: u64,
    records: &[RealizedFrameRecord],
) -> ResidentSelectionCommand {
    ResidentSelectionCommand::from_realized_frame_records(presentation_revision, records)
}

fn quote_command_for_frame_records(
    presentation_revision: u64,
    records: &[RealizedFrameRecord],
) -> ResidentQuoteCommand {
    ResidentQuoteCommand::from_realized_frame_records(presentation_revision, records)
}

fn context_menu_command_for_frame_record(
    presentation_revision: u64,
    record: &RealizedFrameRecord,
) -> ResidentContextMenuCommand {
    ResidentContextMenuCommand::from_realized_frame_record(presentation_revision, record)
}

fn assert_open_edit_branch_targets_unavailable(target: ResidentContextMenuCommandTarget) {
    assert_eq!(
        target,
        ResidentContextMenuCommandTarget::Unavailable(
            ResidentContextMenuUnavailable::NoActiveContextMenuTarget
        )
    );
    assert_eq!(
        ResidentEditCommandTarget::from_context_menu_command_target(target.clone()),
        ResidentEditCommandTarget::Unavailable(
            ResidentContextMenuUnavailable::NoActiveContextMenuTarget
        )
    );
    assert_eq!(
        ResidentBranchCommandTarget::from_context_menu_command_target(target),
        ResidentBranchCommandTarget::Unavailable(
            ResidentContextMenuUnavailable::NoActiveContextMenuTarget
        )
    );
}

fn assert_no_open_edit_branch_targets(core: &ResidentTranscriptCore) {
    assert_open_edit_branch_targets_unavailable(
        ResidentContextMenuCommandTarget::from_active_target(core.resident_context_menu_target()),
    );
}

fn record_id_for_text(
    core: &ResidentTranscriptCore,
    expected_text: &str,
) -> ResidentPresentationRecordId {
    core.presentation_snapshot()
        .records
        .into_iter()
        .find_map(|record| match record.kind {
            ResidentPresentationRecordKind::TextChunk { text, .. } if text == expected_text => {
                Some(record.id)
            }
            _ => None,
        })
        .expect("expected text presentation record")
}

fn text_records(core: &ResidentTranscriptCore) -> Vec<String> {
    core.presentation_snapshot()
        .records
        .into_iter()
        .filter_map(|record| match record.kind {
            ResidentPresentationRecordKind::TextChunk { text, .. } => Some(text),
            _ => None,
        })
        .collect()
}

fn seed_budget_fallback_presentation() -> ResidentTranscriptCore {
    let view_id = view_id();
    let rejected_id = projection_id("rejected");
    let mut provider = InMemorySyndicTranscriptProvider::new();
    provider
        .set_revision(REVISION)
        .insert_view_records(
            view_id.clone(),
            vec![view_record(
                &view_id,
                10,
                "rejected",
                rejected_id.clone(),
                Some(10..20),
            )],
        )
        .reject_projection_record(
            rejected_id,
            TranscriptProviderRejectionReason::BudgetExceeded,
        );

    let mut core = ResidentTranscriptCore::empty();
    let view_request = core.request_view_page(
        view_id.clone(),
        TranscriptPageAnchor::Start,
        TranscriptPageDirection::Forward,
        ProviderRequestReason::ActivationSeed,
    );
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, view_request),
        ResidentProviderResponseEffect::ViewPageAdmitted { admitted_count: 1 }
    );
    let projection_request = core
        .request_projection_records_for_resident_view(ProviderRequestReason::ProjectionAdmission)
        .expect("seeded rejected view should need projection records");
    assert_eq!(
        handle_provider_request(&mut core, &mut provider, projection_request),
        ResidentProviderResponseEffect::ProjectionRecordsAdmitted {
            admitted_count: 0,
            rejected_count: 1
        }
    );
    core
}

#[test]
fn single_record_selection_builds_resident_markdown_copy_payload() {
    let mut core = seed_text_presentation(&[("answer", "# Title\n\n", Some(10..18))]);
    let window = realize_selection_frame(&mut core);
    let command =
        selection_command_for_frame_records(window.presentation_revision, &window.records[0..1]);

    let outcome = core.apply_resident_selection(command);

    let selection = match outcome {
        ResidentSelectionOutcome::Selected(selection) => selection,
        other => panic!("expected resident selection, got {other:?}"),
    };
    assert_eq!(selection.records.len(), 1);
    assert_eq!(selection.records[0].copy_source_range, 10..18);

    let payload = core
        .resident_copy_payload()
        .expect("resident selection should produce copy payload");
    assert_eq!(payload.presentation_revision, window.presentation_revision);
    assert_eq!(payload.markdown, "# Title\n\n");
    assert_eq!(payload.plain_text, None);
    assert_eq!(payload.records, selection.records);
}

#[test]
fn multi_record_selection_preserves_resident_markdown_order() {
    let mut core = seed_text_presentation(&[
        ("first", "first ", Some(10..16)),
        ("second", "**second**", Some(20..30)),
        ("third", "\n\n- third", Some(30..39)),
    ]);
    let window = realize_selection_frame(&mut core);
    let reversed_records = vec![
        window.records[2].clone(),
        window.records[0].clone(),
        window.records[1].clone(),
    ];
    let command =
        selection_command_for_frame_records(window.presentation_revision, &reversed_records);

    let outcome = core.apply_resident_selection(command);

    assert!(matches!(outcome, ResidentSelectionOutcome::Selected(_)));
    let payload = core
        .resident_copy_payload()
        .expect("resident selection should produce copy payload");
    assert_eq!(payload.markdown, "first **second**\n\n- third");
    assert_eq!(
        payload
            .records
            .iter()
            .map(|record| record.copy_source_range.clone())
            .collect::<Vec<_>>(),
        vec![10..16, 20..30, 30..39]
    );
}

#[test]
fn resident_quote_target_builds_quoted_markdown_from_resident_copy_payload() {
    let mut core = seed_text_presentation(&[
        ("first", "alpha\r\nbeta", Some(10..20)),
        ("second", "\n\n**gamma**", Some(20..31)),
    ]);
    let window = realize_selection_frame(&mut core);
    let reversed_records = vec![window.records[1].clone(), window.records[0].clone()];
    let command = quote_command_for_frame_records(window.presentation_revision, &reversed_records);

    let outcome = core.apply_resident_quote_target(command);

    let target = match outcome {
        ResidentQuoteOutcome::Targeted(target) => target,
        other => panic!("expected resident quote target, got {other:?}"),
    };
    assert_eq!(
        target
            .records
            .iter()
            .map(|record| record.copy_source_range.clone())
            .collect::<Vec<_>>(),
        vec![10..20, 20..31]
    );

    let payload = core
        .resident_quote_payload()
        .expect("resident quote target should produce quote payload");
    assert_eq!(payload.presentation_revision, window.presentation_revision);
    assert_eq!(payload.quoted_markdown, "> alpha\n> beta\n> \n> **gamma**");
    assert_eq!(payload.records, target.records);

    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.resident.active_quote_pins, target.record_ids());
    assert!(snapshot.resident.active_selection_pins.is_empty());
    assert_eq!(snapshot.resident.active_pin_count, 2);
}

#[test]
fn quote_target_rejects_stale_missing_fallback_missing_copy_source_and_unstable_records() {
    let mut core = seed_text_presentation(&[("answer", "quote me", Some(10..18))]);
    assert_eq!(
        core.resident_quote_payload(),
        Err(ResidentSelectionUnavailable::NoActiveQuoteTarget)
    );
    let window = realize_selection_frame(&mut core);
    let stale_command = quote_command_for_frame_records(
        window.presentation_revision.saturating_sub(1),
        &window.records[0..1],
    );

    assert_eq!(
        core.apply_resident_quote_target(stale_command),
        ResidentQuoteOutcome::Unavailable(
            ResidentSelectionUnavailable::StalePresentationRevision {
                observed: window.presentation_revision.saturating_sub(1),
                current: window.presentation_revision,
            },
        )
    );

    let missing_id = ResidentPresentationRecordId("missing-quote-record".to_string());
    let missing_command = ResidentQuoteCommand::new(
        window.presentation_revision,
        vec![ResidentSelectionRecordGeometry::new(
            missing_id.clone(),
            0.0,
            20.0,
        )],
    );
    assert_eq!(
        core.apply_resident_quote_target(missing_command),
        ResidentQuoteOutcome::Unavailable(ResidentSelectionUnavailable::RecordNotResident {
            record_id: missing_id,
        })
    );

    let unstable_record = ResidentSelectionRecordGeometry::new(
        window.records[0].record_id.clone(),
        window.records[0].top_px,
        0.0,
    );
    let unstable_command =
        ResidentQuoteCommand::new(window.presentation_revision, vec![unstable_record]);
    assert_eq!(
        core.apply_resident_quote_target(unstable_command),
        ResidentQuoteOutcome::Unavailable(ResidentSelectionUnavailable::UnstableGeometry {
            record_id: window.records[0].record_id.clone()
        })
    );

    let mut fallback_core = seed_budget_fallback_presentation();
    let fallback_window = realize_selection_frame(&mut fallback_core);
    let fallback_command = quote_command_for_frame_records(
        fallback_window.presentation_revision,
        &fallback_window.records[0..1],
    );
    assert!(matches!(
        fallback_core.apply_resident_quote_target(fallback_command),
        ResidentQuoteOutcome::Unavailable(ResidentSelectionUnavailable::NonContentRecord { .. })
    ));

    let mut missing_copy_core = seed_text_presentation(&[("answer", "quote me", None)]);
    let missing_copy_window = realize_selection_frame(&mut missing_copy_core);
    let missing_copy_command = quote_command_for_frame_records(
        missing_copy_window.presentation_revision,
        &missing_copy_window.records[0..1],
    );
    assert!(matches!(
        missing_copy_core.apply_resident_quote_target(missing_copy_command),
        ResidentQuoteOutcome::Unavailable(ResidentSelectionUnavailable::MissingCopySource { .. })
    ));
    assert!(
        missing_copy_core
            .core_snapshot()
            .resident
            .active_quote_pins
            .is_empty()
    );
}

#[test]
fn resident_context_menu_target_accepts_text_and_resource_content_records() {
    let mut core = seed_text_and_resource_presentation();
    let window = realize_selection_frame(&mut core);

    let text_outcome = core.apply_resident_context_menu_target(
        context_menu_command_for_frame_record(window.presentation_revision, &window.records[0]),
    );
    let text_target = match text_outcome {
        ResidentContextMenuOutcome::Targeted(target) => target,
        other => panic!("expected resident text context-menu target, got {other:?}"),
    };
    assert_eq!(
        text_target.record.source.position,
        Some(TranscriptViewPosition(10))
    );
    assert_eq!(
        text_target.record.content_kind,
        ResidentContextMenuContentKind::TextChunk
    );
    assert_eq!(
        text_target.record.geometry.record_id,
        window.records[0].record_id
    );

    let resource_outcome = core.apply_resident_context_menu_target(
        context_menu_command_for_frame_record(window.presentation_revision, &window.records[1]),
    );
    let resource_target = match resource_outcome {
        ResidentContextMenuOutcome::Targeted(target) => target,
        other => panic!("expected resident resource context-menu target, got {other:?}"),
    };
    assert!(matches!(
        resource_target.record.content_kind,
        ResidentContextMenuContentKind::ResourceReference {
            resource_kind: ResourceKind::GeneratedImage,
            ..
        }
    ));

    let snapshot = core.core_snapshot();
    assert_eq!(
        snapshot.resident.active_menu_pins,
        resource_target.record_ids()
    );
    assert!(snapshot.resident.active_selection_pins.is_empty());
    assert!(snapshot.resident.active_quote_pins.is_empty());
    assert_eq!(snapshot.resident.active_pin_count, 1);

    assert_eq!(
        core.clear_resident_context_menu_target(),
        ResidentContextMenuOutcome::Cleared
    );
    assert!(core.resident_context_menu_target().is_none());
    assert!(core.core_snapshot().resident.active_menu_pins.is_empty());
}

#[test]
fn resident_context_menu_command_target_requires_active_target() {
    let mut core = seed_text_presentation(&[("answer", "menu me", Some(10..18))]);
    assert_no_open_edit_branch_targets(&core);

    let window = realize_selection_frame(&mut core);
    let outcome = core.apply_resident_context_menu_target(context_menu_command_for_frame_record(
        window.presentation_revision,
        &window.records[0],
    ));
    let active_target = match outcome {
        ResidentContextMenuOutcome::Targeted(target) => target,
        other => panic!("expected active resident context-menu target, got {other:?}"),
    };

    assert_eq!(
        ResidentContextMenuCommandTarget::from_active_target(core.resident_context_menu_target()),
        ResidentContextMenuCommandTarget::Targeted(active_target)
    );
}

#[test]
fn realized_context_menu_target_drives_open_edit_and_branch_command_targets() {
    let mut core = seed_text_and_resource_presentation();
    let window = realize_selection_frame(&mut core);
    let snapshot = core.presentation_snapshot();
    let resource_record_id = window.records[1].record_id.clone();
    let command = resident_context_menu_command_for_realized_record_id(
        &snapshot,
        &window,
        &resource_record_id,
    )
    .expect("resident realized resource should produce context-menu command");

    let active_target = match core.apply_resident_context_menu_target(command) {
        ResidentContextMenuOutcome::Targeted(target) => target,
        other => panic!("expected realized resident target, got {other:?}"),
    };
    let open_target =
        ResidentContextMenuCommandTarget::from_active_target(core.resident_context_menu_target());
    let edit_target =
        ResidentEditCommandTarget::from_context_menu_command_target(open_target.clone());
    let branch_target =
        ResidentBranchCommandTarget::from_context_menu_command_target(open_target.clone());

    assert_eq!(
        open_target,
        ResidentContextMenuCommandTarget::Targeted(active_target.clone())
    );
    let ResidentEditCommandTarget::Targeted(edit_target) = edit_target else {
        panic!("expected edit action target from active context-menu target");
    };
    let ResidentBranchCommandTarget::Targeted(branch_target) = branch_target else {
        panic!("expected branch action target from active context-menu target");
    };

    assert_eq!(active_target.record.record_id, resource_record_id);
    assert_eq!(active_target.record_ids(), vec![resource_record_id.clone()]);
    assert_eq!(edit_target.record_ids(), vec![resource_record_id.clone()]);
    assert_eq!(branch_target.record_ids(), vec![resource_record_id]);
    assert_eq!(edit_target.provenance, branch_target.provenance);
    assert_eq!(
        edit_target.provenance.presentation_revision,
        active_target.presentation_revision
    );
    assert_eq!(edit_target.provenance.source, active_target.record.source);
    assert_eq!(
        edit_target.provenance.content_kind,
        active_target.record.content_kind
    );
    assert!(matches!(
        edit_target.provenance.content_kind,
        ResidentContextMenuContentKind::ResourceReference {
            resource_kind: ResourceKind::GeneratedImage,
            ..
        }
    ));
}

#[test]
fn resident_edit_and_branch_targets_use_context_menu_provenance() {
    let mut core = seed_text_and_resource_presentation();
    assert_no_open_edit_branch_targets(&core);

    let window = realize_selection_frame(&mut core);
    let outcome = core.apply_resident_context_menu_target(context_menu_command_for_frame_record(
        window.presentation_revision,
        &window.records[1],
    ));
    let active_target = match outcome {
        ResidentContextMenuOutcome::Targeted(target) => target,
        other => panic!("expected active resident context-menu target, got {other:?}"),
    };
    let edit_target = match ResidentEditCommandTarget::from_context_menu_command_target(
        ResidentContextMenuCommandTarget::from_active_target(core.resident_context_menu_target()),
    ) {
        ResidentEditCommandTarget::Targeted(target) => target,
        other => panic!("expected resident edit target, got {other:?}"),
    };
    let branch_target = match ResidentBranchCommandTarget::from_context_menu_command_target(
        ResidentContextMenuCommandTarget::from_active_target(core.resident_context_menu_target()),
    ) {
        ResidentBranchCommandTarget::Targeted(target) => target,
        other => panic!("expected resident branch target, got {other:?}"),
    };

    assert_eq!(edit_target.record_ids(), active_target.record_ids());
    assert_eq!(branch_target.record_ids(), active_target.record_ids());
    assert_eq!(edit_target.provenance, branch_target.provenance);
    assert_eq!(
        edit_target.provenance.presentation_revision,
        active_target.presentation_revision
    );
    assert_eq!(
        edit_target.provenance.record_id,
        active_target.record.record_id
    );
    assert_eq!(edit_target.provenance.source, active_target.record.source);
    assert_eq!(
        edit_target.provenance.projection_id,
        active_target.record.projection_id
    );
    assert_eq!(
        edit_target.provenance.projection_revision,
        active_target.record.projection_revision
    );
    assert_eq!(
        edit_target.provenance.content_kind,
        active_target.record.content_kind
    );
    assert_eq!(
        edit_target.provenance.source_range,
        active_target.record.source_range
    );
    assert_eq!(
        edit_target.provenance.resource_range,
        active_target.record.resource_range
    );
}

#[test]
fn context_menu_target_rejects_stale_missing_fallback_local_unstable_and_missing_provenance() {
    let mut core = seed_text_presentation(&[("answer", "menu me", Some(10..18))]);
    assert!(core.resident_context_menu_target().is_none());
    let window = realize_selection_frame(&mut core);
    let stale_command = context_menu_command_for_frame_record(
        window.presentation_revision.saturating_sub(1),
        &window.records[0],
    );

    assert_eq!(
        core.apply_resident_context_menu_target(stale_command),
        ResidentContextMenuOutcome::Unavailable(
            ResidentContextMenuUnavailable::StalePresentationRevision {
                observed: window.presentation_revision.saturating_sub(1),
                current: window.presentation_revision,
            },
        )
    );
    assert_no_open_edit_branch_targets(&core);

    let missing_id = ResidentPresentationRecordId("missing-menu-record".to_string());
    let missing_command = ResidentContextMenuCommand::new(
        window.presentation_revision,
        ResidentSelectionRecordGeometry::new(missing_id.clone(), 0.0, 20.0),
    );
    assert_eq!(
        core.apply_resident_context_menu_target(missing_command),
        ResidentContextMenuOutcome::Unavailable(
            ResidentContextMenuUnavailable::RecordNotResident {
                record_id: missing_id,
            },
        )
    );
    assert_no_open_edit_branch_targets(&core);

    let mut not_realized_core = seed_text_presentation(&[
        ("first", "first resident", Some(10..24)),
        ("second", "second resident", Some(20..35)),
    ]);
    let not_realized_window = realize_frame_with_viewport(&mut not_realized_core, 20.0);
    let not_realized_id = record_id_for_text(&not_realized_core, "second resident");
    assert!(
        !not_realized_window
            .records
            .iter()
            .any(|record| record.record_id == not_realized_id)
    );
    let not_realized_command = ResidentContextMenuCommand::new(
        not_realized_window.presentation_revision,
        ResidentSelectionRecordGeometry::new(not_realized_id.clone(), 20.0, 20.0),
    );
    assert_eq!(
        not_realized_core.apply_resident_context_menu_target(not_realized_command),
        ResidentContextMenuOutcome::Unavailable(
            ResidentContextMenuUnavailable::RecordNotRealized {
                record_id: not_realized_id,
            },
        )
    );
    assert_no_open_edit_branch_targets(&not_realized_core);

    let unstable_command = ResidentContextMenuCommand::new(
        window.presentation_revision,
        ResidentSelectionRecordGeometry::new(window.records[0].record_id.clone(), 0.0, 0.0),
    );
    assert_eq!(
        core.apply_resident_context_menu_target(unstable_command),
        ResidentContextMenuOutcome::Unavailable(ResidentContextMenuUnavailable::UnstableGeometry {
            record_id: window.records[0].record_id.clone(),
        },)
    );
    assert_no_open_edit_branch_targets(&core);

    let mut fallback_core = seed_budget_fallback_presentation();
    let fallback_window = realize_selection_frame(&mut fallback_core);
    let fallback_command = context_menu_command_for_frame_record(
        fallback_window.presentation_revision,
        &fallback_window.records[0],
    );
    assert!(matches!(
        fallback_core.apply_resident_context_menu_target(fallback_command),
        ResidentContextMenuOutcome::Unavailable(
            ResidentContextMenuUnavailable::NonContentRecord { .. }
        )
    ));
    assert_no_open_edit_branch_targets(&fallback_core);

    let local_record_id = ResidentPresentationRecordId("local-affordance".to_string());
    let local_record = ResidentPresentationRecord {
        id: local_record_id.clone(),
        kind: ResidentPresentationRecordKind::LocalAffordance,
        provenance: ResidentRecordProvenance {
            source: ResidentRecordSource::LocalUi,
            projection_id: None,
            projection_revision: None,
            presentation_revision: window.presentation_revision,
            copy_source_range: None,
        },
        estimated_bytes: 0,
    };
    assert_eq!(
        resident_context_menu_record(
            &local_record,
            ResidentSelectionRecordGeometry::new(local_record_id.clone(), 0.0, 20.0),
        ),
        Err(ResidentContextMenuUnavailable::NonContentRecord {
            record_id: local_record_id,
        })
    );
    assert_no_open_edit_branch_targets(&core);

    let mut missing_provenance_core = seed_missing_context_menu_provenance_presentation();
    let missing_provenance_window = realize_selection_frame(&mut missing_provenance_core);
    let missing_provenance_command = context_menu_command_for_frame_record(
        missing_provenance_window.presentation_revision,
        &missing_provenance_window.records[0],
    );
    assert!(matches!(
        missing_provenance_core.apply_resident_context_menu_target(missing_provenance_command),
        ResidentContextMenuOutcome::Unavailable(
            ResidentContextMenuUnavailable::MissingStableProvenance { .. }
        )
    ));
    assert_no_open_edit_branch_targets(&missing_provenance_core);
    assert!(
        missing_provenance_core
            .core_snapshot()
            .resident
            .active_menu_pins
            .is_empty()
    );
}

#[test]
fn unstable_geometry_rejects_selection_and_leaves_no_copy_payload() {
    let mut core = seed_text_presentation(&[("answer", "copy me", Some(10..17))]);
    let window = realize_selection_frame(&mut core);
    let unstable_record = ResidentSelectionRecordGeometry::new(
        window.records[0].record_id.clone(),
        window.records[0].top_px,
        0.0,
    );
    let command =
        ResidentSelectionCommand::new(window.presentation_revision, vec![unstable_record]);

    let outcome = core.apply_resident_selection(command);

    assert_eq!(
        outcome,
        ResidentSelectionOutcome::Unavailable(ResidentSelectionUnavailable::UnstableGeometry {
            record_id: window.records[0].record_id.clone()
        })
    );
    assert_eq!(
        core.resident_copy_payload(),
        Err(ResidentSelectionUnavailable::NoActiveSelection)
    );
    assert!(
        core.core_snapshot()
            .resident
            .active_selection_pins
            .is_empty()
    );
}

#[test]
fn stale_missing_fallback_and_missing_copy_source_records_are_unavailable() {
    let mut core = seed_text_presentation(&[("answer", "copy me", Some(10..17))]);
    let window = realize_selection_frame(&mut core);
    let stale_command = selection_command_for_frame_records(
        window.presentation_revision.saturating_sub(1),
        &window.records[0..1],
    );

    assert_eq!(
        core.apply_resident_selection(stale_command),
        ResidentSelectionOutcome::Unavailable(
            ResidentSelectionUnavailable::StalePresentationRevision {
                observed: window.presentation_revision.saturating_sub(1),
                current: window.presentation_revision,
            },
        )
    );

    let missing_id = ResidentPresentationRecordId("missing-record".to_string());
    let missing_command = ResidentSelectionCommand::new(
        window.presentation_revision,
        vec![ResidentSelectionRecordGeometry::new(
            missing_id.clone(),
            0.0,
            20.0,
        )],
    );
    assert_eq!(
        core.apply_resident_selection(missing_command),
        ResidentSelectionOutcome::Unavailable(ResidentSelectionUnavailable::RecordNotResident {
            record_id: missing_id,
        })
    );

    let mut fallback_core = seed_budget_fallback_presentation();
    let fallback_window = realize_selection_frame(&mut fallback_core);
    assert!(matches!(
        fallback_core.presentation_snapshot().records[0].kind,
        ResidentPresentationRecordKind::LocalUiFallback { .. }
    ));
    let fallback_command = selection_command_for_frame_records(
        fallback_window.presentation_revision,
        &fallback_window.records[0..1],
    );
    assert!(matches!(
        fallback_core.apply_resident_selection(fallback_command),
        ResidentSelectionOutcome::Unavailable(
            ResidentSelectionUnavailable::NonContentRecord { .. }
        )
    ));

    let mut missing_copy_core = seed_text_presentation(&[("answer", "copy me", None)]);
    let missing_copy_window = realize_selection_frame(&mut missing_copy_core);
    let missing_copy_command = selection_command_for_frame_records(
        missing_copy_window.presentation_revision,
        &missing_copy_window.records[0..1],
    );
    assert!(matches!(
        missing_copy_core.apply_resident_selection(missing_copy_command),
        ResidentSelectionOutcome::Unavailable(
            ResidentSelectionUnavailable::MissingCopySource { .. }
        )
    ));
}

#[test]
fn resident_selection_pins_selected_records_during_obsolete_release() {
    let mut core = seed_text_presentation(&[
        ("a", "text-a", Some(10..16)),
        ("b", "text-b", Some(20..26)),
        ("c", "text-c", Some(30..36)),
        ("d", "text-d", Some(40..46)),
    ]);
    let window = realize_selection_frame(&mut core);
    let selected_id = record_id_for_text(&core, "text-c");
    let selected_frame_record = window
        .records
        .iter()
        .find(|record| record.record_id == selected_id)
        .cloned()
        .expect("selected record should be realized");

    assert!(matches!(
        core.apply_resident_selection(selection_command_for_frame_records(
            window.presentation_revision,
            &[selected_frame_record],
        )),
        ResidentSelectionOutcome::Selected(_)
    ));
    core.push_demand_fact(DemandFact::new(
        window.presentation_revision,
        DemandFactKind::VisibleRange { range: 0..1 },
    ));
    core.push_demand_fact(DemandFact::new(
        window.presentation_revision,
        DemandFactKind::OverscanRange { range: 0..1 },
    ));
    core.push_demand_fact(DemandFact::new(
        window.presentation_revision,
        DemandFactKind::ObsoleteRange { range: 0..4 },
    ));

    assert_eq!(core.release_obsolete_resident_data(), 1);

    assert_eq!(text_records(&core), vec!["text-a", "text-c"]);
    assert_eq!(
        core.core_snapshot().resident.active_selection_pins,
        vec![selected_id]
    );
    assert_eq!(
        core.resident_copy_payload()
            .expect("selected resident record should remain copyable")
            .markdown,
        "text-c"
    );
}

#[test]
fn resident_quote_pin_preserves_target_during_obsolete_release() {
    let mut core = seed_text_presentation(&[
        ("a", "text-a", Some(10..16)),
        ("b", "text-b", Some(20..26)),
        ("c", "text-c", Some(30..36)),
        ("d", "text-d", Some(40..46)),
    ]);
    let window = realize_selection_frame(&mut core);
    let quote_id = record_id_for_text(&core, "text-c");
    let quote_frame_record = window
        .records
        .iter()
        .find(|record| record.record_id == quote_id)
        .cloned()
        .expect("quote target record should be realized");

    assert!(matches!(
        core.apply_resident_quote_target(quote_command_for_frame_records(
            window.presentation_revision,
            &[quote_frame_record],
        )),
        ResidentQuoteOutcome::Targeted(_)
    ));
    core.push_demand_fact(DemandFact::new(
        window.presentation_revision,
        DemandFactKind::VisibleRange { range: 0..1 },
    ));
    core.push_demand_fact(DemandFact::new(
        window.presentation_revision,
        DemandFactKind::OverscanRange { range: 0..1 },
    ));
    core.push_demand_fact(DemandFact::new(
        window.presentation_revision,
        DemandFactKind::ObsoleteRange { range: 0..4 },
    ));

    assert_eq!(core.release_obsolete_resident_data(), 1);

    assert_eq!(text_records(&core), vec!["text-a", "text-c"]);
    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.resident.active_quote_pins, vec![quote_id]);
    assert_eq!(
        snapshot
            .resident
            .release_decisions
            .last()
            .expect("quote release should record a decision")
            .released_presentation_record_count,
        2
    );
    assert_eq!(
        core.resident_quote_payload()
            .expect("quote target resident record should remain quoteable")
            .quoted_markdown,
        "> text-c"
    );
}

#[test]
fn resident_context_menu_pin_preserves_target_during_obsolete_release() {
    let mut core = seed_text_presentation(&[
        ("a", "text-a", Some(10..16)),
        ("b", "text-b", Some(20..26)),
        ("c", "text-c", Some(30..36)),
        ("d", "text-d", Some(40..46)),
    ]);
    let window = realize_selection_frame(&mut core);
    let menu_id = record_id_for_text(&core, "text-c");
    let menu_frame_record = window
        .records
        .iter()
        .find(|record| record.record_id == menu_id)
        .cloned()
        .expect("context-menu target record should be realized");

    assert!(matches!(
        core.apply_resident_context_menu_target(context_menu_command_for_frame_record(
            window.presentation_revision,
            &menu_frame_record,
        )),
        ResidentContextMenuOutcome::Targeted(_)
    ));
    core.push_demand_fact(DemandFact::new(
        window.presentation_revision,
        DemandFactKind::VisibleRange { range: 0..1 },
    ));
    core.push_demand_fact(DemandFact::new(
        window.presentation_revision,
        DemandFactKind::OverscanRange { range: 0..1 },
    ));
    core.push_demand_fact(DemandFact::new(
        window.presentation_revision,
        DemandFactKind::ObsoleteRange { range: 0..4 },
    ));

    assert_eq!(core.release_obsolete_resident_data(), 1);

    assert_eq!(text_records(&core), vec!["text-a", "text-c"]);
    let snapshot = core.core_snapshot();
    assert_eq!(snapshot.resident.active_menu_pins, vec![menu_id.clone()]);
    assert_eq!(
        snapshot
            .resident
            .release_decisions
            .last()
            .expect("context-menu release should record a decision")
            .released_presentation_record_count,
        2
    );
    assert_eq!(
        core.resident_context_menu_target()
            .expect("context-menu target should remain resident")
            .record
            .record_id,
        menu_id
    );
}
