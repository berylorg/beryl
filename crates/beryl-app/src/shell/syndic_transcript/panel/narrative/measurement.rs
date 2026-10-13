use super::*;

pub(in super::super) fn prune_candidates(
    prepared: &mut Vec<NarrativeGeometry>,
    literal: &mut Vec<super::super::narrative_literal_geometry::LiteralGeometry>,
    host: &mut SyndicTranscriptHost,
    retains: impl Fn(&ResidentPresentationRecordId) -> bool,
) {
    prepared.retain(|geometry| {
        let retained = retains(&geometry.owner.record_id);
        if !retained {
            host.reserve_narrative_layout(&geometry.owner, 0);
        }
        retained
    });
    literal.retain(|geometry| {
        let retained = retains(&geometry.owner.record_id);
        if !retained {
            host.reserve_literal_narrative_layout(&geometry.owner, 0);
        }
        retained
    });
}

pub(in super::super) fn measure(
    geometries: &mut Vec<NarrativeGeometry>,
    host: &mut SyndicTranscriptHost,
    snapshot: &ResidentTranscriptSnapshot,
    record: &ResidentPresentationRecord,
    appearance: &NarrativeAppearance,
    width: Pixels,
    budget: usize,
    window: &mut Window,
    cx: &mut Context<SyndicTranscriptPanel>,
) -> (Option<f32>, usize) {
    if budget == 0 {
        return (None, 0);
    }
    let ResidentPresentationRecordKind::NarrativeReady { prepared } = &record.kind else {
        return (None, 0);
    };
    let owner = NarrativeOwner {
        view_id: match &record.provenance.source {
            super::super::super::ResidentRecordSource::Syndic(source) => source.view_id.clone(),
            _ => return (None, 0),
        },
        record_id: record.id.clone(),
        activation_revision: snapshot.activation_revision,
        provenance: record.provenance.clone(),
        prepared: prepared.clone(),
    };
    let index = geometries
        .iter()
        .position(|value| value.owner == owner)
        .unwrap_or_else(|| {
            let kind = match prepared.treatment() {
                NarrativeTreatment::Heading(level) => NarrativeBlockStyle::Heading(level),
                NarrativeTreatment::Quote => NarrativeBlockStyle::Quote,
                NarrativeTreatment::List => NarrativeBlockStyle::List,
                NarrativeTreatment::ThematicBreak => NarrativeBlockStyle::ThematicBreak,
                _ => NarrativeBlockStyle::Paragraph,
            };
            geometries.push(NarrativeGeometry {
                owner: owner.clone(),
                block: appearance.block(kind),
                layout: None,
                marker: None,
                marker_ready: prepared.list_marker().is_none(),
                preparation: None,
                refused: false,
            });
            geometries.len() - 1
        });
    let value = &mut geometries[index];
    if value.refused {
        return measure_refusal(value, snapshot, record, width, 0, budget, window, cx);
    }
    if let Some(layout) = &value.layout {
        if !value.marker_ready {
            let allowance = host.narrative_layout_allowance(&owner);
            if !host.reserve_narrative_layout(&owner, allowance) {
                value.refuse(host);
                return measure_refusal(value, snapshot, record, width, 0, budget, window, cx);
            }
            if budget == 0 {
                return (None, 0);
            }
            let source: Arc<str> = prepared.list_marker().unwrap().into();
            let model = super::super::narrative_literal::PreparedLiteralNarrative::prepare(
                source, allowance,
            );
            let mut remaining = budget;
            let result = model.ok().and_then(|model| {
                let mut pending = NarrativeLayoutPreparation::new(
                    snapshot.presentation_revision,
                    px(value.block.indent.max(1.0)),
                    px(value.block.text.size),
                    px(value.block.text.size * 20.0 / 14.0),
                    allowance.saturating_sub(value.charge() + model.retained_bytes()),
                );
                pending
                    .advance(&model, window.text_system(), &mut remaining, |_, len| {
                        value.block.text.run(len)
                    })
                    .ok()
                    .flatten()
            });
            value.marker = result;
            value.marker_ready = true;
            if value.marker.is_none() || !host.reserve_narrative_layout(&owner, value.charge()) {
                value.refuse(host);
                return measure_refusal(
                    value,
                    snapshot,
                    record,
                    width,
                    budget - remaining,
                    budget,
                    window,
                    cx,
                );
            }
            let used = budget - remaining;
            if remaining == 0 {
                return (None, used);
            }
            let (top, bottom) = super::super::record_vertical_padding(snapshot, record);
            return (
                Some(f32::from(value.content_height()) + top + bottom),
                used + 1,
            );
        }
        let (top, bottom) = super::super::record_vertical_padding(snapshot, record);
        return (Some(f32::from(value.content_height()) + top + bottom), 1);
    }
    let allowance = host.narrative_layout_allowance(&owner);
    if !host.reserve_narrative_layout(&owner, allowance) || allowance < 1024 {
        value.refuse(host);
        return measure_refusal(value, snapshot, record, width, 0, budget, window, cx);
    }
    let pending = value.preparation.get_or_insert_with(|| {
        NarrativeLayoutPreparation::new(
            snapshot.presentation_revision,
            (width - px(32.0 + value.block.indent)).max(px(1.0)),
            px(value.block.text.size),
            px(value.block.text.size * (20.0 / 14.0)),
            allowance.saturating_sub(1024),
        )
    });
    let mut remaining = budget;
    let result = pending.advance(
        prepared.as_ref(),
        window.text_system(),
        &mut remaining,
        |style, len| {
            appearance
                .inline(
                    &value.block,
                    NarrativeInlineStyle {
                        emphasis: style.emphasis,
                        strong: style.strong,
                        strikethrough: style.strikethrough,
                        code: style.code,
                        link_label: style.link_label,
                    },
                )
                .run(len)
        },
    );
    let used = budget - remaining;
    match result {
        Ok(Some(layout)) => {
            if !host.reserve_narrative_layout(&owner, layout.charge + 1024) {
                value.refuse(host);
                return measure_refusal(value, snapshot, record, width, used, budget, window, cx);
            }
            let (top, bottom) = super::super::record_vertical_padding(snapshot, record);
            let height = f32::from(layout.height) + top + bottom;
            value.layout = Some(layout);
            value.preparation = None;
            if !value.marker_ready {
                return (None, used);
            }
            if remaining == 0 {
                (None, used)
            } else {
                (Some(height), used + 1)
            }
        }
        Ok(None) => {
            if host.reserve_narrative_layout(&owner, pending.charge() + 1024) {
                (None, used)
            } else {
                value.refuse(host);
                measure_refusal(value, snapshot, record, width, used, budget, window, cx)
            }
        }
        Err(_) => {
            value.refuse(host);
            measure_refusal(value, snapshot, record, width, used, budget, window, cx)
        }
    }
}

fn measure_refusal(
    value: &NarrativeGeometry,
    snapshot: &ResidentTranscriptSnapshot,
    record: &ResidentPresentationRecord,
    width: Pixels,
    used: usize,
    budget: usize,
    window: &mut Window,
    cx: &mut Context<SyndicTranscriptPanel>,
) -> (Option<f32>, usize) {
    if used == budget {
        return (None, used);
    }
    let (top, bottom) = super::super::record_vertical_padding(snapshot, record);
    let mut element = div()
        .w(width)
        .px(px(16.0))
        .pt(px(top))
        .pb(px(bottom))
        .child(render(value, None))
        .into_any_element();
    let height = element
        .layout_as_root(
            gpui::size(
                gpui::AvailableSpace::Definite(width),
                gpui::AvailableSpace::MinContent,
            ),
            window,
            cx,
        )
        .height;
    (Some(f32::from(height)), used + 1)
}
