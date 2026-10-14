use super::*;

impl SyndicTranscriptPanel {
    #[cfg(test)]
    pub(crate) fn owned_presentation_counts_for_test(&self) -> (usize, usize, usize, usize) {
        (
            self.narrative_geometry.len()
                + self.narrative_candidate_geometry.len()
                + self.inline_parts_geometry.len()
                + self.inline_parts_candidate_geometry.len()
                + self.literal_geometry.len()
                + self.literal_candidate_geometry.len(),
            self.code_panels.len() + self.retired_code_panels.len(),
            self.table_panels.len() + self.retired_table_panels.len(),
            self.measurement_diagnostics().1,
        )
    }

    pub(crate) fn narrative_hit_for_test(&self, position: Point<Pixels>) -> Option<(ResidentPresentationRecordId, usize)> {
        self.narrative_hit(position).map(|endpoint| (endpoint.record, endpoint.display))
    }

    pub(crate) fn narrative_hit_diagnostic_for_test(&self, position: Point<Pixels>) -> String {
        let groups = self.inline_parts_geometry.iter().map(|group| {
            let glyphs = group.text.iter().map(|text| {
                let hit = group.bounds.get().and_then(|bounds| text.layout.as_ref().and_then(|layout|
                    layout.hit(position - bounds.origin - point(px(text.block.indent), px(0.0)))));
                (text.owner.record_id.clone(), hit, text.valid())
            }).collect::<Vec<_>>();
            (group.bounds.get(), self.host.inline_parts_current(&group.record), glyphs)
        }).collect::<Vec<_>>();
        format!("position={position:?}, viewport={:?}, groups={groups:?}", self.narrative_viewport_bounds)
    }

    pub(crate) fn narrative_selection_ranges_for_test(&self) -> Option<Vec<(ResidentPresentationRecordId, Range<usize>)>> {
        self.narrative_ranges(self.narrative_selection.as_ref()?)
    }

    pub(crate) fn narrative_geometry_current_for_test(&self, window: &Window) -> bool {
        self.character_geometry_current(window)
    }

    pub(crate) fn narrative_marker_geometry_for_test(
        &self,
        id: &ResidentPresentationRecordId,
    ) -> Option<(f32, f32, f32)> {
        let value = self.narrative_geometry.iter().find(|value| {
            &value.owner.record_id == id && self.host.narrative_current(&value.owner)
        })?;
        Some((
            f32::from(value.layout.as_ref()?.height),
            f32::from(value.marker.as_ref()?.height),
            f32::from(value.content_height()),
        ))
    }

    #[cfg(test)]
    pub(crate) fn narrative_kind_for_test(
        &self,
        id: &ResidentPresentationRecordId,
    ) -> Option<super::super::super::TranscriptNarrativeKind> {
        self.host.narrative_kind_for_test(id)
    }

    #[cfg(test)]
    pub(crate) fn literal_narrative_position_for_test(
        &self,
        id: &ResidentPresentationRecordId,
        index: usize,
    ) -> Option<Point<Pixels>> {
        let value = self.literal_geometry.iter().find(|value| {
            &value.owner.record_id == id && self.host.literal_narrative_current(&value.owner)
        })?;
        Some(
            value.bounds.get()?.origin
                + value.layout.as_ref()?.position(index)?
                + point(px(0.0), px(1.0)),
        )
    }

    #[cfg(test)]
    pub(crate) fn literal_narrative_text_for_test(
        &self,
    ) -> Vec<(ResidentPresentationRecordId, String)> {
        self.literal_geometry
            .iter()
            .filter(|value| {
                value.layout.is_some() && self.host.literal_narrative_current(&value.owner)
            })
            .map(|value| {
                (
                    value.owner.record_id.clone(),
                    value.owner.source.to_string(),
                )
            })
            .collect()
    }
    #[cfg(test)]
    pub(crate) fn narrative_provider_revision_for_test(
        &self,
        id: &ResidentPresentationRecordId,
    ) -> Option<super::super::super::ProviderRevision> {
        self.mapped_geometry(id)?;
        self.host.provider_revision_for_test()
    }
    #[cfg(test)]
    pub(crate) fn narrative_states_for_test(&self) -> (usize, usize, usize, usize, usize, bool) {
        let snapshot = self.host.shared_snapshot();
        let pending = snapshot
            .records
            .iter()
            .filter(|record| {
                matches!(
                    record.kind,
                    ResidentPresentationRecordKind::NarrativePending { .. }
                )
            })
            .count();
        let ready = snapshot
            .records
            .iter()
            .filter(|record| {
                matches!(
                    record.kind,
                    ResidentPresentationRecordKind::NarrativeReady { .. }
                )
            })
            .count();
        let unavailable = snapshot
            .records
            .iter()
            .filter(|record| {
                matches!(
                    record.kind,
                    ResidentPresentationRecordKind::NarrativeUnavailable { .. }
                )
            })
            .count();
        (
            pending,
            ready,
            unavailable,
            self.narrative_geometry.len(),
            self.narrative_candidate_geometry.len(),
            self.staged.is_some(),
        )
    }

    #[cfg(test)]
    pub(crate) fn narrative_model_identity_for_test(
        &self,
        id: &ResidentPresentationRecordId,
    ) -> Option<usize> {
        self.narrative_geometry
            .iter()
            .find(|geometry| {
                &geometry.owner.record_id == id && self.host.narrative_current(&geometry.owner)
            })
            .map(|geometry| Arc::as_ptr(&geometry.owner.prepared) as usize)
    }
    #[cfg(test)]
    pub(crate) fn narrative_layout_facts_for_test(
        &self,
    ) -> Vec<(ResidentPresentationRecordId, String, String, usize)> {
        self.narrative_geometry
            .iter()
            .chain(self.inline_parts_geometry.iter().flat_map(|group| group.text.iter()))
            .filter(|value| value.layout.is_some() && self.host.narrative_current(&value.owner))
            .map(|value| {
                (
                    value.owner.record_id.clone(),
                    value.owner.prepared.source().to_owned(),
                    value.owner.prepared.display().to_owned(),
                    value.owner.prepared.fragments().len(),
                )
            })
            .collect()
    }
    #[cfg(test)]
    pub(crate) fn narrative_text_for_test(
        &self,
    ) -> Vec<(ResidentPresentationRecordId, String, String)> {
        self.narrative_geometry
            .iter()
            .chain(self.inline_parts_geometry.iter().flat_map(|group| group.text.iter()))
            .filter(|value| value.layout.is_some() && self.host.narrative_current(&value.owner))
            .map(|value| {
                (
                    value.owner.record_id.clone(),
                    value.owner.prepared.source().to_owned(),
                    value.owner.prepared.display().to_owned(),
                )
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn narrative_position_for_test(
        &self,
        id: &ResidentPresentationRecordId,
        index: usize,
    ) -> Option<Point<Pixels>> {
        if let Some((group, text)) = self.inline_parts_geometry.iter().find_map(|group| {
            group.text.iter().find(|text| &text.owner.record_id == id && self.host.narrative_current(&text.owner))
                .map(|text| (group, text))
        }) {
            return Some(group.bounds.get()?.origin + point(px(text.block.indent), px(1.0)) + text.layout.as_ref()?.position(index)?);
        }
        let bounds = self.narrative_viewport_bounds?;
        let row = self
            .last_frame_window
            .as_ref()?
            .records
            .iter()
            .find(|row| &row.record_id == id)?;
        let geometry = self.narrative_geometry.iter().find(|value| {
            &value.owner.record_id == id && self.host.narrative_current(&value.owner)
        })?;
        Some(
            bounds.origin
                + point(
                    px(16.0 + geometry.block.indent),
                    px(row.top_px
                        + self
                            .published_snapshot
                            .as_ref()
                            .and_then(|snapshot| {
                                snapshot.records.iter().find(|record| record.id == *id).map(
                                    |record| {
                                        super::super::record_vertical_padding(snapshot, record).0
                                    },
                                )
                            })
                            .unwrap_or(0.0)),
                )
                + geometry.layout.as_ref()?.position(index)?
                + point(px(0.0), px(1.0)),
        )
    }
}
