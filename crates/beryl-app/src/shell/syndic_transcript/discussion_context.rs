use std::{ops::Range, sync::Arc};

use super::{
    ResidentPresentationRecord, ResidentPresentationRecordId, ResidentPresentationRecordKind,
    ResidentRecordProvenance, ResidentRecordSource, TranscriptNarrativeKind, TranscriptViewId,
};

pub(crate) const CONTEXT_CHUNK_MAX_BYTES: usize = 2048;
pub(crate) const CONTEXT_SOURCE_MAX_BYTES: usize = 65_536;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SyntheticContextProvenance {
    pub(crate) view_id: TranscriptViewId,
    pub(crate) group_id: String,
    pub(crate) insertion_after: u64,
    pub(crate) insertion_parent: beryl_model::SyndicTurnId,
    pub(crate) descriptor: Option<syndic_storage::DiscussionContextDescriptor>,
    pub(crate) envelope_revision: u64,
    pub(crate) chunk_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SyntheticContextChunk {
    pub(crate) range: Range<u64>,
    pub(crate) text: Arc<str>,
    pub(crate) plain_text: Arc<str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SyntheticContextGroup {
    pub(crate) provenance: Arc<SyntheticContextProvenance>,
    pub(crate) chunks: Vec<SyntheticContextChunk>,
}

impl SyntheticContextGroup {
    pub(crate) fn presentation_records(&self, revision: u64) -> Vec<ResidentPresentationRecord> {
        if self.chunks.is_empty() {
            return vec![self.record(0, 0..0, None, revision)];
        }
        self.chunks
            .iter()
            .enumerate()
            .map(|(index, chunk)| self.record(index, chunk.range.clone(), Some(chunk), revision))
            .collect()
    }

    fn record(
        &self,
        index: usize,
        range: Range<u64>,
        chunk: Option<&SyntheticContextChunk>,
        revision: u64,
    ) -> ResidentPresentationRecord {
        ResidentPresentationRecord {
            id: ResidentPresentationRecordId(format!(
                "context:{}:{index:04}",
                self.provenance.group_id
            )),
            kind: match chunk {
                Some(chunk) => ResidentPresentationRecordKind::TextChunk {
                    narrative_kind: TranscriptNarrativeKind::AssistantText,
                    text: chunk.text.clone(),
                    plain_text: Some(chunk.plain_text.clone()),
                },
                None => ResidentPresentationRecordKind::ContextUnavailable,
            },
            provenance: ResidentRecordProvenance {
                source: ResidentRecordSource::LocalContext {
                    group: self.provenance.clone(),
                    chunk_index: index,
                },
                projection_id: None,
                projection_revision: None,
                presentation_revision: revision,
                copy_source_range: chunk.map(|_| range),
            },
            estimated_bytes: 512
                + self.provenance.group_id.len()
                + self.provenance.view_id.0.len()
                + chunk.map_or(0, |chunk| chunk.text.len() + chunk.plain_text.len()),
        }
    }
}
