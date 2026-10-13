mod copy;
mod delivery;
mod measurement;
mod quote;
pub(super) use quote::QuoteState;
#[cfg(test)]
mod testing;

pub(super) use measurement::{measure, prune_candidates};

use std::{ops::Range, sync::Arc};

use gpui::{AnyElement, Context, Pixels, Point, Window, canvas, div, point, prelude::*, px};
use syndic_storage::narrative::NarrativeTreatment;

use super::super::{
    NarrativeOwner, NarrativePreparationResponse, NarrativeTranscriptDemand,
    ResidentTranscriptSnapshot,
};
use super::{
    ResidentPresentationRecord, ResidentPresentationRecordId, ResidentPresentationRecordKind,
    StagedTranscript, SyndicTranscriptHost, SyndicTranscriptPanel,
    narrative_appearance::{
        NarrativeAppearance, NarrativeBlockAppearance, NarrativeBlockStyle, NarrativeInlineStyle,
    },
    narrative_layout::{NarrativeLayout, NarrativeLayoutPreparation},
};

pub(super) struct NarrativeGeometry {
    pub(super) owner: NarrativeOwner,
    pub(super) block: NarrativeBlockAppearance,
    pub(super) layout: Option<Arc<NarrativeLayout>>,
    marker: Option<Arc<NarrativeLayout>>,
    marker_ready: bool,
    preparation: Option<NarrativeLayoutPreparation>,
    refused: bool,
}

impl NarrativeGeometry {
    fn content_height(&self) -> Pixels {
        self.layout
            .as_ref()
            .map_or(Pixels::ZERO, |layout| layout.height)
            .max(
                self.marker
                    .as_ref()
                    .map_or(Pixels::ZERO, |marker| marker.height),
            )
    }

    fn refuse(&mut self, host: &mut SyndicTranscriptHost) {
        self.layout = None;
        self.marker = None;
        self.preparation = None;
        self.marker_ready = true;
        self.refused = true;
        host.reserve_narrative_layout(&self.owner, 0);
    }

    pub(super) fn valid(&self) -> bool {
        self.layout.as_ref().is_none_or(|layout| layout.valid())
            && self.marker.as_ref().is_none_or(|layout| layout.valid())
    }

    pub(super) fn charge(&self) -> usize {
        1024 + self.layout.as_ref().map_or(0, |layout| layout.charge)
            + self.marker.as_ref().map_or(0, |layout| layout.charge)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NarrativeEndpoint {
    pub(super) record: ResidentPresentationRecordId,
    pub(super) display: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NarrativeSelection {
    pub(super) revision: u64,
    pub(super) activation: u64,
    pub(super) anchor: NarrativeEndpoint,
    pub(super) head: NarrativeEndpoint,
}

enum MappedGeometry<'a> {
    Prepared(&'a NarrativeGeometry),
    Literal(&'a super::narrative_literal_geometry::LiteralGeometry),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NarrativeCopyBinding {
    record: ResidentPresentationRecordId,
    activation: u64,
    provenance: NarrativeCopyProvenance,
    display: Range<usize>,
    source_identity: usize,
    source_len: usize,
    layout: usize,
    prepared: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum NarrativeCopySource {
    Syndic(super::super::SyndicSourceProvenance),
    Context { group: usize, chunk: usize },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NarrativeCopyProvenance {
    source: NarrativeCopySource,
    projection_id: Option<super::super::ProjectionRecordId>,
    projection_revision: Option<super::super::ProviderRevision>,
    presentation_revision: u64,
    copy_source_range: Option<Range<u64>>,
}

impl NarrativeCopyProvenance {
    fn matches(&self, provenance: &super::super::ResidentRecordProvenance) -> bool {
        let source = match (&self.source, &provenance.source) {
            (
                NarrativeCopySource::Syndic(old),
                super::super::ResidentRecordSource::Syndic(current),
            ) => old == current,
            (
                NarrativeCopySource::Context { group, chunk },
                super::super::ResidentRecordSource::LocalContext {
                    group: current,
                    chunk_index,
                },
            ) => *group == Arc::as_ptr(current) as usize && chunk == chunk_index,
            _ => false,
        };
        source
            && self.projection_id == provenance.projection_id
            && self.projection_revision == provenance.projection_revision
            && self.presentation_revision == provenance.presentation_revision
            && self.copy_source_range == provenance.copy_source_range
    }
    fn capture(provenance: &super::super::ResidentRecordProvenance) -> Option<Self> {
        let source = match &provenance.source {
            super::super::ResidentRecordSource::Syndic(source) => {
                NarrativeCopySource::Syndic(source.clone())
            }
            super::super::ResidentRecordSource::LocalContext { group, chunk_index } => {
                NarrativeCopySource::Context {
                    group: Arc::as_ptr(group) as usize,
                    chunk: *chunk_index,
                }
            }
            _ => return None,
        };
        Some(Self {
            source,
            projection_id: provenance.projection_id.clone(),
            projection_revision: provenance.projection_revision,
            presentation_revision: provenance.presentation_revision,
            copy_source_range: provenance.copy_source_range.clone(),
        })
    }

    fn metadata_bytes(provenance: &super::super::ResidentRecordProvenance) -> usize {
        let own = provenance.projection_id.as_ref().map_or(0, |id| id.0.len());
        let source = match &provenance.source {
            super::super::ResidentRecordSource::Syndic(source) => {
                source.view_id.0.len()
                    + source.turn_id.as_ref().map_or(0, |id| id.0.len())
                    + source.item_id.as_ref().map_or(0, |id| id.0.len())
                    + source.projection_id.as_ref().map_or(0, |id| id.0.len())
                    + source.resource_id.as_ref().map_or(0, |id| id.0.len())
            }
            _ => 0,
        };
        own.saturating_add(source)
    }
}

impl<'a> MappedGeometry<'a> {
    fn source(&self) -> &'a str {
        match self {
            Self::Prepared(value) => value.owner.prepared.source(),
            Self::Literal(value) => value.model.as_ref().unwrap().source(),
        }
    }
    fn display(&self) -> &'a str {
        match self {
            Self::Prepared(value) => value.owner.prepared.display(),
            Self::Literal(value) => value.model.as_ref().unwrap().display(),
        }
    }
    fn source_range(&self, range: Range<usize>) -> Option<Range<usize>> {
        match self {
            Self::Prepared(value) => value.owner.prepared.source_range_for_display(range),
            Self::Literal(value) => value.model.as_ref()?.source_range_for_display(range),
        }
    }
    fn selected_display(&self, range: Range<usize>) -> Option<&'a str> {
        match self {
            Self::Prepared(value) => value.owner.prepared.selected_display(range),
            Self::Literal(value) => value.model.as_ref()?.selected_display(range),
        }
    }
    fn snap(&self, index: usize) -> Option<usize> {
        match self {
            Self::Prepared(value) => value.owner.prepared.snap_display_endpoint(index, false),
            Self::Literal(value) => value.model.as_ref()?.snap_display_endpoint(index, false),
        }
    }
    fn layout(&self) -> &NarrativeLayout {
        match self {
            Self::Prepared(value) => value.layout.as_ref().unwrap(),
            Self::Literal(value) => value.layout.as_ref().unwrap(),
        }
    }
}

pub(super) fn render(geometry: &NarrativeGeometry, selected: Option<Range<usize>>) -> AnyElement {
    let Some(layout) = geometry.layout.clone() else {
        return div()
            .child("Narrative layout unavailable")
            .into_any_element();
    };
    let indent = geometry.block.indent;
    let border_width = geometry.block.border_width;
    let border = geometry.block.border;
    let quote_border = geometry.owner.prepared.treatment() == NarrativeTreatment::Quote;
    let marker = geometry.marker.clone();
    div()
        .w_full()
        .h(geometry.content_height())
        .relative()
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, cx| {
                    if let Some(marker) = &marker {
                        if marker.paint(bounds.origin, None, window, cx).is_err() {
                            window.request_animation_frame();
                            return;
                        }
                    }
                    if border_width > 0.0 {
                        let size = if quote_border {
                            gpui::size(px(border_width), bounds.size.height)
                        } else {
                            gpui::size(bounds.size.width, px(border_width))
                        };
                        window
                            .paint_quad(gpui::fill(gpui::Bounds::new(bounds.origin, size), border));
                    }
                    if layout
                        .paint(
                            bounds.origin + point(px(indent), px(0.0)),
                            selected.clone(),
                            window,
                            cx,
                        )
                        .is_err()
                    {
                        window.request_animation_frame();
                    }
                },
            )
            .size_full(),
        )
        .into_any_element()
}

impl SyndicTranscriptPanel {
    fn mapped_geometry(&self, id: &ResidentPresentationRecordId) -> Option<MappedGeometry<'_>> {
        if let Some(value) = self.narrative_geometry.iter().find(|value| {
            &value.owner.record_id == id
                && value.layout.is_some()
                && !value.refused
                && value.valid()
                && self.host.narrative_current(&value.owner)
        }) {
            return Some(MappedGeometry::Prepared(value));
        }
        self.literal_geometry
            .iter()
            .find(|value| {
                &value.owner.record_id == id
                    && value.layout.is_some()
                    && value.model.is_some()
                    && value.valid()
                    && self.host.literal_narrative_current(&value.owner)
            })
            .map(MappedGeometry::Literal)
    }
    pub(super) fn narrative_hit_available(&self, position: Point<Pixels>, window: &Window) -> bool {
        self.character_geometry_current(window) && self.narrative_hit(position).is_some()
    }

    pub(super) fn narrative_pointer(
        &mut self,
        position: Point<Pixels>,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.character_geometry_current(window) {
            self.close_invalid_geometry();
            cx.notify();
            return;
        }
        let Some(endpoint) = self.narrative_hit(position) else {
            return;
        };
        let Some(snapshot) = self.published_snapshot.as_ref() else {
            return;
        };
        let anchor = if extend {
            self.narrative_selection
                .as_ref()
                .filter(|selection| selection.revision == snapshot.presentation_revision)
                .map(|selection| selection.anchor.clone())
                .unwrap_or_else(|| endpoint.clone())
        } else {
            endpoint.clone()
        };
        let selection = NarrativeSelection {
            revision: snapshot.presentation_revision,
            activation: snapshot.activation_revision,
            anchor,
            head: endpoint,
        };
        if self.narrative_ranges(&selection).is_none() {
            return;
        }
        self.host.clear_resident_selection();
        self.reset_keyboard_copy();
        if self.narrative_selection.as_ref() != Some(&selection) {
            self.quote_state.capture = None;
        }
        self.narrative_selection = Some(selection);
        self.focus_handle.focus(window);
        cx.notify();
    }

    fn narrative_hit(&self, position: Point<Pixels>) -> Option<NarrativeEndpoint> {
        let bounds = self.narrative_viewport_bounds?;
        if !bounds.contains(&position) {
            return None;
        }
        let local = position - bounds.origin;
        let frame = self.last_frame_window.as_ref()?;
        let record = frame.records.iter().find(|row| {
            f32::from(local.y) >= row.top_px && f32::from(local.y) < row.top_px + row.height_px
        })?;
        let geometry = self.mapped_geometry(&record.record_id)?;
        let point = match &geometry {
            MappedGeometry::Prepared(value) => {
                let snapshot = self.published_snapshot.as_ref()?;
                let source = snapshot.records.get(record.index)?;
                let origin = point(
                    px(16.0 + value.block.indent),
                    px(record.top_px + super::record_vertical_padding(snapshot, source).0),
                );
                let layout = value.layout.as_ref()?;
                let body = gpui::Bounds::new(origin, gpui::size(layout.width, layout.height));
                if !body.contains(&local) {
                    return None;
                }
                local - origin
            }
            MappedGeometry::Literal(value) => {
                let bounds = value.bounds.get()?;
                if !bounds.contains(&position) {
                    return None;
                }
                position - bounds.origin
            }
        };
        let offset = geometry.layout().hit(point)?;
        let display = geometry.snap(offset)?;
        Some(NarrativeEndpoint {
            record: record.record_id.clone(),
            display,
        })
    }

    pub(super) fn narrative_ranges(
        &self,
        selection: &NarrativeSelection,
    ) -> Option<Vec<(ResidentPresentationRecordId, Range<usize>)>> {
        let snapshot = self.published_snapshot.as_ref()?;
        let frame = self.last_frame_window.as_ref()?;
        if selection.revision != snapshot.presentation_revision
            || selection.activation != snapshot.activation_revision
        {
            return None;
        }
        let anchor = frame
            .records
            .iter()
            .position(|record| record.record_id == selection.anchor.record)?;
        let head = frame
            .records
            .iter()
            .position(|record| record.record_id == selection.head.record)?;
        let (start_index, start, end_index, end) =
            if (anchor, selection.anchor.display) <= (head, selection.head.display) {
                (
                    anchor,
                    selection.anchor.display,
                    head,
                    selection.head.display,
                )
            } else {
                (
                    head,
                    selection.head.display,
                    anchor,
                    selection.anchor.display,
                )
            };
        let mut ranges = Vec::new();
        for (index, row) in frame
            .records
            .iter()
            .enumerate()
            .take(end_index + 1)
            .skip(start_index)
        {
            if index > start_index && frame.records[index - 1].index + 1 != row.index {
                return None;
            }
            let geometry = self.mapped_geometry(&row.record_id)?;
            let start = if index == start_index { start } else { 0 };
            let end = if index == end_index {
                end
            } else {
                geometry.display().len()
            };
            geometry.source_range(start..end)?;
            ranges.push((row.record_id.clone(), start..end));
        }
        Some(ranges)
    }
}
