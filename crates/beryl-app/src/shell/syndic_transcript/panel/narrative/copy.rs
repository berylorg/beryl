use super::*;

impl SyndicTranscriptPanel {
    pub(in super::super) fn narrative_copy_text(&self, stripped: bool) -> Option<String> {
        if self.staged.is_some() {
            return None;
        }
        let selection = self.narrative_selection.as_ref()?;
        let ranges = self.narrative_ranges(selection)?;
        let mut parts = Vec::new();
        let mut bytes = 0usize;
        for (record, range) in &ranges {
            let geometry = self.mapped_geometry(record)?;
            let text = if stripped {
                geometry.selected_display(range.clone())?
            } else {
                geometry
                    .source()
                    .get(geometry.source_range(range.clone())?)?
            };
            bytes = bytes
                .checked_add(text.len())
                .filter(|bytes| *bytes <= 65_536)?;
            parts.push(text);
        }
        if bytes == 0 {
            return None;
        }
        let mut result = String::new();
        result.try_reserve_exact(bytes).ok()?;
        for text in parts {
            result.push_str(text);
        }
        Some(result)
    }

    pub(in super::super) fn narrative_copy_capture(
        &self,
    ) -> Option<super::super::copy_keyboard::NarrativeCopyCapture> {
        use super::super::copy_keyboard::{NarrativeCopyCapture, NarrativeCopyPart};
        if self.staged.is_some() {
            return None;
        }
        let selection = self.narrative_selection.clone()?;
        let ranges = self.narrative_ranges(&selection)?;
        let mut bytes = 0usize;
        let mut source_bytes = 0usize;
        let mut metadata_bytes = ranges
            .len()
            .checked_mul(
                std::mem::size_of::<NarrativeCopyBinding>()
                    + std::mem::size_of::<NarrativeCopyPart>(),
            )?
            .checked_add(std::mem::size_of::<NarrativeCopyCapture>())?
            .checked_add(selection.anchor.record.0.len())?
            .checked_add(selection.head.record.0.len())?;
        for (record, range) in &ranges {
            let geometry = self.mapped_geometry(record)?;
            let provenance = match &geometry {
                MappedGeometry::Prepared(value) => &value.owner.provenance,
                MappedGeometry::Literal(value) => &value.owner.provenance,
            };
            metadata_bytes = metadata_bytes
                .checked_add(record.0.len())?
                .checked_add(NarrativeCopyProvenance::metadata_bytes(provenance))?;
            let selected_source = geometry
                .source()
                .get(geometry.source_range(range.clone())?)?;
            source_bytes = source_bytes
                .checked_add(selected_source.len())
                .filter(|bytes| *bytes <= 65_536)?;
            let text = match &geometry {
                MappedGeometry::Prepared(_) => geometry.selected_display(range.clone())?,
                MappedGeometry::Literal(_) => geometry
                    .source()
                    .get(geometry.source_range(range.clone())?)?,
            };
            bytes = bytes
                .checked_add(text.len())
                .filter(|bytes| *bytes <= 65_536)?;
        }
        source_bytes
            .checked_add(metadata_bytes)
            .filter(|bytes| *bytes <= 65_536)?;
        bytes
            .checked_add(metadata_bytes)
            .filter(|bytes| *bytes <= 65_536)?;
        if bytes == 0 {
            return None;
        }
        let mut parts = Vec::new();
        let mut bindings = Vec::new();
        parts.try_reserve_exact(ranges.len()).ok()?;
        bindings.try_reserve_exact(ranges.len()).ok()?;
        for (record, range) in ranges {
            let geometry = self.mapped_geometry(&record)?;
            let (part, binding) = match geometry {
                MappedGeometry::Prepared(value) => {
                    let text = value.owner.prepared.selected_display(range.clone())?;
                    let mut captured = String::new();
                    captured.try_reserve_exact(text.len()).ok()?;
                    captured.push_str(text);
                    (
                        NarrativeCopyPart::Mapped(captured),
                        NarrativeCopyBinding {
                            record: value.owner.record_id.clone(),
                            activation: value.owner.activation_revision,
                            provenance: NarrativeCopyProvenance::capture(&value.owner.provenance)?,
                            display: range,
                            source_identity: Arc::as_ptr(&value.owner.prepared) as usize,
                            source_len: value.owner.prepared.source().len(),
                            prepared: true,
                            layout: Arc::as_ptr(value.layout.as_ref()?) as usize,
                        },
                    )
                }
                MappedGeometry::Literal(value) => {
                    let model = value.model.as_ref()?;
                    let text = model
                        .source()
                        .get(model.source_range_for_display(range.clone())?)?;
                    let mut captured = String::new();
                    captured.try_reserve_exact(text.len()).ok()?;
                    captured.push_str(text);
                    (
                        NarrativeCopyPart::Literal(captured),
                        NarrativeCopyBinding {
                            record: value.owner.record_id.clone(),
                            activation: value.owner.activation_revision,
                            provenance: NarrativeCopyProvenance::capture(&value.owner.provenance)?,
                            display: range,
                            source_identity: value.owner.source.as_ptr() as usize,
                            source_len: value.owner.source.len(),
                            prepared: false,
                            layout: Arc::as_ptr(value.layout.as_ref()?) as usize,
                        },
                    )
                }
            };
            parts.push(part);
            bindings.push(binding);
        }
        Some(NarrativeCopyCapture {
            selection,
            parts,
            bindings,
        })
    }

    pub(in super::super) fn narrative_copy_capture_current(
        &self,
        selection: &NarrativeSelection,
        bindings: &[NarrativeCopyBinding],
    ) -> bool {
        if self.staged.is_some() || self.narrative_selection.as_ref() != Some(selection) {
            return false;
        }
        let Some(ranges) = self.narrative_ranges(selection) else {
            return false;
        };
        if ranges.len() != bindings.len() {
            return false;
        }
        ranges
            .iter()
            .zip(bindings)
            .all(
                |((record, range), binding)| match self.mapped_geometry(record) {
                    Some(MappedGeometry::Prepared(value)) => {
                        binding.prepared
                            && record == &binding.record
                            && range == &binding.display
                            && value.owner.activation_revision == binding.activation
                            && binding.provenance.matches(&value.owner.provenance)
                            && Arc::as_ptr(&value.owner.prepared) as usize
                                == binding.source_identity
                            && value.owner.prepared.source().len() == binding.source_len
                            && value.layout.as_ref().is_some_and(|current| {
                                Arc::as_ptr(current) as usize == binding.layout
                            })
                    }
                    Some(MappedGeometry::Literal(value)) => {
                        !binding.prepared
                            && record == &binding.record
                            && range == &binding.display
                            && value.owner.activation_revision == binding.activation
                            && binding.provenance.matches(&value.owner.provenance)
                            && value.owner.source.as_ptr() as usize == binding.source_identity
                            && value.owner.source.len() == binding.source_len
                            && value.layout.as_ref().is_some_and(|current| {
                                Arc::as_ptr(current) as usize == binding.layout
                            })
                    }
                    _ => false,
                },
            )
    }
}
