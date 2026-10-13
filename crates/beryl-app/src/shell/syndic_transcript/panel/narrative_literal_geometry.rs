use std::{cell::Cell, ops::Range, rc::Rc, sync::Arc};

use gpui::{AnyElement, Bounds, Context, Pixels, Window, canvas, div, prelude::*, px};

use super::super::{LiteralNarrativeOwner, ResidentRecordSource, ResidentTranscriptSnapshot};
use super::{
    ResidentPresentationRecord, ResidentPresentationRecordKind, SyndicTranscriptHost,
    SyndicTranscriptPanel,
    context_appearance::ContextAppearance,
    narrative_appearance::{NarrativeAppearance, NarrativeBlockAppearance, NarrativeBlockStyle},
    narrative_layout::{NarrativeLayout, NarrativeLayoutPreparation},
    narrative_literal::PreparedLiteralNarrative,
};

pub(super) struct LiteralGeometry {
    pub(super) owner: LiteralNarrativeOwner,
    pub(super) model: Option<PreparedLiteralNarrative>,
    pub(super) layout: Option<Arc<NarrativeLayout>>,
    pub(super) bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    block: NarrativeBlockAppearance,
    preparation: Option<NarrativeLayoutPreparation>,
    refused: bool,
}

impl LiteralGeometry {
    pub(super) fn valid(&self) -> bool {
        self.layout.as_ref().is_none_or(|layout| layout.valid())
    }

    pub(super) fn charge(&self) -> usize {
        self.model
            .as_ref()
            .map_or(0, |model| model.retained_bytes())
            + self.layout.as_ref().map_or(0, |layout| layout.charge)
            + std::mem::size_of::<Self>()
    }
}

pub(super) fn measure(
    geometries: &mut Vec<LiteralGeometry>,
    host: &mut SyndicTranscriptHost,
    snapshot: &ResidentTranscriptSnapshot,
    record: &ResidentPresentationRecord,
    appearance: &NarrativeAppearance,
    context: &ContextAppearance,
    width: Pixels,
    budget: usize,
    window: &mut Window,
    cx: &mut Context<SyndicTranscriptPanel>,
) -> (Option<f32>, usize) {
    if budget == 0 {
        return (None, 0);
    }
    let ResidentPresentationRecordKind::TextChunk { text, .. } = &record.kind else {
        return (None, 0);
    };
    let owner = LiteralNarrativeOwner {
        record_id: record.id.clone(),
        activation_revision: snapshot.activation_revision,
        provenance: record.provenance.clone(),
        source: text.clone(),
    };
    let index = geometries
        .iter()
        .position(|value| value.owner == owner)
        .unwrap_or_else(|| {
            let mut block = appearance.block(NarrativeBlockStyle::Paragraph);
            if matches!(
                record.provenance.source,
                ResidentRecordSource::LocalContext { .. }
            ) {
                block.text.font = context.body.font.clone();
                block.text.size = context.body.size;
                block.text.color = context.body.color;
            }
            geometries.push(LiteralGeometry {
                owner: owner.clone(),
                block,
                model: None,
                layout: None,
                preparation: None,
                refused: false,
                bounds: Rc::new(Cell::new(None)),
            });
            geometries.len() - 1
        });
    let value = &mut geometries[index];
    let mut used = 0;
    if !value.refused && value.layout.is_none() {
        let allowance = host.literal_narrative_layout_allowance(&owner);
        if !host.reserve_literal_narrative_layout(&owner, allowance) {
            value.refused = true;
        } else {
            if value.model.is_none() {
                value.model = PreparedLiteralNarrative::prepare(text.clone(), allowance).ok();
                value.refused = value.model.is_none();
            }
            if let Some(model) = &value.model {
                let literal_charge =
                    model.retained_bytes() + std::mem::size_of::<LiteralGeometry>();
                let inner_width = if matches!(
                    record.provenance.source,
                    ResidentRecordSource::LocalContext { .. }
                ) {
                    width - px(32.0 + 28.0 * context.scale() + 2.0)
                } else {
                    width - px(32.0)
                };
                let pending = value.preparation.get_or_insert_with(|| {
                    NarrativeLayoutPreparation::new(
                        snapshot.presentation_revision,
                        inner_width.max(px(1.0)),
                        px(value.block.text.size),
                        px(value.block.text.size * 20.0 / 14.0),
                        allowance.saturating_sub(literal_charge),
                    )
                });
                let mut remaining = budget;
                let result =
                    pending.advance(model, window.text_system(), &mut remaining, |_, len| {
                        value.block.text.run(len)
                    });
                used = budget - remaining;
                match result {
                    Ok(Some(layout)) => {
                        if host.reserve_literal_narrative_layout(
                            &owner,
                            literal_charge + layout.charge,
                        ) {
                            value.layout = Some(layout);
                            value.preparation = None;
                        } else {
                            value.refused = true;
                        }
                    }
                    Ok(None) => {
                        if host.reserve_literal_narrative_layout(
                            &owner,
                            literal_charge + pending.charge(),
                        ) {
                            return (None, budget - remaining);
                        }
                        value.refused = true;
                    }
                    Err(_) => value.refused = true,
                }
                if remaining == 0 && !value.refused {
                    return (None, budget);
                }
                if !value.refused {
                    let height = measure_body(value, record, context, width, window, cx);
                    return (Some(height), budget - remaining + 1);
                }
            }
        }
        if value.refused {
            value.model = None;
            value.layout = None;
            value.preparation = None;
            host.reserve_literal_narrative_layout(&owner, 0);
        }
    }
    if used == budget {
        return (None, used);
    }
    (
        Some(measure_body(value, record, context, width, window, cx)),
        used + 1,
    )
}

fn measure_body(
    value: &LiteralGeometry,
    record: &ResidentPresentationRecord,
    context: &ContextAppearance,
    width: Pixels,
    window: &mut Window,
    cx: &mut Context<SyndicTranscriptPanel>,
) -> f32 {
    let mut element = div()
        .w(width)
        .px(px(16.0))
        .py(px(
            if matches!(
                record.provenance.source,
                ResidentRecordSource::LocalContext { .. }
            ) {
                0.0
            } else {
                8.0
            },
        ))
        .child(render(value, record, context, None))
        .into_any_element();
    f32::from(
        element
            .layout_as_root(
                gpui::size(
                    gpui::AvailableSpace::Definite(width),
                    gpui::AvailableSpace::MinContent,
                ),
                window,
                cx,
            )
            .height,
    )
}

pub(super) fn render(
    value: &LiteralGeometry,
    record: &ResidentPresentationRecord,
    context: &ContextAppearance,
    selected: Option<Range<usize>>,
) -> AnyElement {
    let body = if let Some(layout) = value.layout.clone() {
        let stored = value.bounds.clone();
        div()
            .w_full()
            .h(layout.height)
            .child(
                canvas(
                    move |bounds, _, _| {
                        stored.set(Some(bounds));
                    },
                    move |bounds, _, window, cx| {
                        if layout
                            .paint(bounds.origin, selected.clone(), window, cx)
                            .is_err()
                        {
                            window.request_animation_frame();
                        }
                    },
                )
                .size_full(),
            )
            .into_any_element()
    } else {
        div()
            .child("Narrative layout unavailable")
            .into_any_element()
    };
    if matches!(
        record.provenance.source,
        ResidentRecordSource::LocalContext { .. }
    ) {
        super::render_context_record(record, context, body)
    } else {
        body
    }
}
