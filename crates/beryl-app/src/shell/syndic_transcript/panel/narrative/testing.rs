use super::*;

impl SyndicTranscriptPanel {
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
