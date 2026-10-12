use super::super::core::InlineTableOwner;
use super::super::{
    ProviderRevision, ResidentPresentationRecordId, ResourceTranscriptDemand, TranscriptViewId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum TableOwner {
    Indexed(ResourceTranscriptDemand),
    Inline(InlineTableOwner),
}

impl TableOwner {
    pub fn indexed(&self) -> Option<&ResourceTranscriptDemand> {
        match self {
            Self::Indexed(owner) => Some(owner),
            Self::Inline(_) => None,
        }
    }
    #[cfg(test)]
    pub fn indexed_mut(&mut self) -> Option<&mut ResourceTranscriptDemand> {
        match self {
            Self::Indexed(owner) => Some(owner),
            Self::Inline(_) => None,
        }
    }
    pub fn inline(&self) -> Option<&InlineTableOwner> {
        match self {
            Self::Inline(owner) => Some(owner),
            Self::Indexed(_) => None,
        }
    }
    pub fn record_id(&self) -> &ResidentPresentationRecordId {
        match self {
            Self::Indexed(owner) => &owner.record_id,
            Self::Inline(owner) => &owner.record_id,
        }
    }
    pub fn view_id(&self) -> &TranscriptViewId {
        match self {
            Self::Indexed(owner) => &owner.view_id,
            Self::Inline(owner) => &owner.view_id,
        }
    }
    pub fn provider_revision(&self) -> ProviderRevision {
        match self {
            Self::Indexed(owner) => owner.revision,
            Self::Inline(owner) => owner
                .provenance
                .projection_revision
                .expect("admitted inline revision"),
        }
    }
}
