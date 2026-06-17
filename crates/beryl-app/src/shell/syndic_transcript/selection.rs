use std::ops::Range;

use super::{
    frame::RealizedFrameRecord,
    provider::{ProjectionRecordId, ProviderRevision, SyndicSourceProvenance},
    snapshot::{
        ResidentPresentationRecord, ResidentPresentationRecordId, ResidentPresentationRecordKind,
        ResidentRecordSource,
    },
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResidentSelectionCommand {
    pub(crate) presentation_revision: u64,
    pub(crate) records: Vec<ResidentSelectionRecordGeometry>,
}

impl ResidentSelectionCommand {
    pub(crate) fn new(
        presentation_revision: u64,
        records: Vec<ResidentSelectionRecordGeometry>,
    ) -> Self {
        Self {
            presentation_revision,
            records,
        }
    }

    pub(crate) fn from_realized_frame_records(
        presentation_revision: u64,
        records: &[RealizedFrameRecord],
    ) -> Self {
        Self {
            presentation_revision,
            records: records
                .iter()
                .map(ResidentSelectionRecordGeometry::from_realized_frame_record)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResidentSelectionRecordGeometry {
    pub(crate) record_id: ResidentPresentationRecordId,
    pub(crate) top_px: f32,
    pub(crate) height_px: f32,
}

impl ResidentSelectionRecordGeometry {
    pub(crate) fn new(
        record_id: ResidentPresentationRecordId,
        top_px: f32,
        height_px: f32,
    ) -> Self {
        Self {
            record_id,
            top_px,
            height_px,
        }
    }

    fn from_realized_frame_record(record: &RealizedFrameRecord) -> Self {
        Self {
            record_id: record.record_id.clone(),
            top_px: record.top_px,
            height_px: record.height_px,
        }
    }

    pub(crate) fn is_stable(&self) -> bool {
        self.top_px.is_finite() && self.height_px.is_finite() && self.height_px > 0.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResidentTranscriptSelection {
    pub(crate) presentation_revision: u64,
    pub(crate) records: Vec<ResidentSelectedRecord>,
}

impl ResidentTranscriptSelection {
    pub(crate) fn new(presentation_revision: u64, records: Vec<ResidentSelectedRecord>) -> Self {
        Self {
            presentation_revision,
            records,
        }
    }

    pub(crate) fn record_ids(&self) -> Vec<ResidentPresentationRecordId> {
        self.records
            .iter()
            .map(|record| record.record_id.clone())
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResidentQuoteCommand {
    pub(crate) presentation_revision: u64,
    pub(crate) records: Vec<ResidentSelectionRecordGeometry>,
}

impl ResidentQuoteCommand {
    pub(crate) fn new(
        presentation_revision: u64,
        records: Vec<ResidentSelectionRecordGeometry>,
    ) -> Self {
        Self {
            presentation_revision,
            records,
        }
    }

    pub(crate) fn from_realized_frame_records(
        presentation_revision: u64,
        records: &[RealizedFrameRecord],
    ) -> Self {
        Self {
            presentation_revision,
            records: records
                .iter()
                .map(ResidentSelectionRecordGeometry::from_realized_frame_record)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResidentTranscriptQuoteTarget {
    pub(crate) presentation_revision: u64,
    pub(crate) records: Vec<ResidentSelectedRecord>,
}

impl ResidentTranscriptQuoteTarget {
    pub(crate) fn new(presentation_revision: u64, records: Vec<ResidentSelectedRecord>) -> Self {
        Self {
            presentation_revision,
            records,
        }
    }

    pub(crate) fn record_ids(&self) -> Vec<ResidentPresentationRecordId> {
        self.records
            .iter()
            .map(|record| record.record_id.clone())
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResidentSelectedRecord {
    pub(crate) record_id: ResidentPresentationRecordId,
    pub(crate) source: SyndicSourceProvenance,
    pub(crate) projection_id: ProjectionRecordId,
    pub(crate) projection_revision: ProviderRevision,
    pub(crate) copy_source_range: Range<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResidentTranscriptCopyPayload {
    pub(crate) presentation_revision: u64,
    pub(crate) markdown: String,
    pub(crate) plain_text: Option<String>,
    pub(crate) records: Vec<ResidentSelectedRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResidentTranscriptQuotePayload {
    pub(crate) presentation_revision: u64,
    pub(crate) quoted_markdown: String,
    pub(crate) records: Vec<ResidentSelectedRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResidentSelectionOutcome {
    Selected(ResidentTranscriptSelection),
    Cleared,
    Unavailable(ResidentSelectionUnavailable),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResidentQuoteOutcome {
    Targeted(ResidentTranscriptQuoteTarget),
    Cleared,
    Unavailable(ResidentSelectionUnavailable),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResidentSelectionUnavailable {
    NoActiveSelection,
    NoActiveQuoteTarget,
    NoRealizedFrame,
    EmptySelection,
    StalePresentationRevision {
        observed: u64,
        current: u64,
    },
    StaleRecord {
        record_id: ResidentPresentationRecordId,
    },
    DuplicateRecord {
        record_id: ResidentPresentationRecordId,
    },
    RecordNotResident {
        record_id: ResidentPresentationRecordId,
    },
    RecordNotRealized {
        record_id: ResidentPresentationRecordId,
    },
    UnstableGeometry {
        record_id: ResidentPresentationRecordId,
    },
    NonContentRecord {
        record_id: ResidentPresentationRecordId,
    },
    MissingStableProvenance {
        record_id: ResidentPresentationRecordId,
    },
    MissingCopySource {
        record_id: ResidentPresentationRecordId,
    },
}

pub(crate) fn resident_selected_record(
    record: &ResidentPresentationRecord,
) -> Result<ResidentSelectedRecord, ResidentSelectionUnavailable> {
    match &record.kind {
        ResidentPresentationRecordKind::TextChunk { text, .. } if !text.is_empty() => {}
        _ => {
            return Err(ResidentSelectionUnavailable::NonContentRecord {
                record_id: record.id.clone(),
            });
        }
    }

    let ResidentRecordSource::Syndic(source) = &record.provenance.source else {
        return Err(ResidentSelectionUnavailable::NonContentRecord {
            record_id: record.id.clone(),
        });
    };
    let Some(projection_id) = &record.provenance.projection_id else {
        return Err(ResidentSelectionUnavailable::MissingStableProvenance {
            record_id: record.id.clone(),
        });
    };
    let Some(projection_revision) = record.provenance.projection_revision else {
        return Err(ResidentSelectionUnavailable::MissingStableProvenance {
            record_id: record.id.clone(),
        });
    };
    let Some(copy_source_range) = record.provenance.copy_source_range.clone() else {
        return Err(ResidentSelectionUnavailable::MissingCopySource {
            record_id: record.id.clone(),
        });
    };

    Ok(ResidentSelectedRecord {
        record_id: record.id.clone(),
        source: source.clone(),
        projection_id: projection_id.clone(),
        projection_revision,
        copy_source_range,
    })
}

pub(crate) fn resident_copy_markdown(
    record: &ResidentPresentationRecord,
) -> Result<&str, ResidentSelectionUnavailable> {
    match &record.kind {
        ResidentPresentationRecordKind::TextChunk { text, .. } if !text.is_empty() => Ok(text),
        _ => Err(ResidentSelectionUnavailable::NonContentRecord {
            record_id: record.id.clone(),
        }),
    }
}

pub(crate) fn resident_quote_markdown(markdown: &str) -> Option<String> {
    let normalized = markdown.replace("\r\n", "\n").replace('\r', "\n");
    if normalized.is_empty() {
        return None;
    }

    let mut lines = normalized.lines().peekable();
    lines.peek()?;

    let mut quoted = String::new();
    for line in lines {
        if !quoted.is_empty() {
            quoted.push('\n');
        }
        quoted.push_str("> ");
        quoted.push_str(line);
    }
    Some(quoted)
}
