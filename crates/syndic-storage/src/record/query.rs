use beryl_model::{
    SealedAssetReferenceSetProof, SyndicAcceptedInputId, SyndicItemId, SyndicThreadId,
};

use crate::{ImageLabelOrdinal, SyndicRecordError};

mod activity;

pub use activity::*;

/// Compact inherited/current permanent image-label authority on one thread.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreadImageLabelFrontiers {
    inherited: crate::ImageLabelFrontier,
    current: crate::ImageLabelFrontier,
}

impl ThreadImageLabelFrontiers {
    pub fn new(
        inherited: crate::ImageLabelFrontier,
        current: crate::ImageLabelFrontier,
    ) -> Result<Self, SyndicRecordError> {
        if inherited > current {
            return Err(SyndicRecordError::InvalidImageLabelFrontier);
        }
        Ok(Self { inherited, current })
    }

    #[must_use]
    pub const fn empty() -> Self {
        Self {
            inherited: crate::ImageLabelFrontier::EMPTY,
            current: crate::ImageLabelFrontier::EMPTY,
        }
    }

    #[must_use]
    pub const fn inherited(self) -> crate::ImageLabelFrontier {
        self.inherited
    }

    #[must_use]
    pub const fn current(self) -> crate::ImageLabelFrontier {
        self.current
    }
}

/// Immutable admitted owner of one local image-label frontier advance.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ImageLabelOriginOwner {
    AcceptedInput(SyndicAcceptedInputId),
    CanonicalItem(SyndicItemId),
}

/// Immutable local origin evidence for one permanently reserved image-label span.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageLabelOriginSpanRecord {
    thread_id: SyndicThreadId,
    start_label: ImageLabelOrdinal,
    end_label: ImageLabelOrdinal,
    admitted_owner: ImageLabelOriginOwner,
    asset_reference_set: SealedAssetReferenceSetProof,
}

impl ImageLabelOriginSpanRecord {
    pub const fn new(
        thread_id: SyndicThreadId,
        start_label: ImageLabelOrdinal,
        end_label: ImageLabelOrdinal,
        admitted_owner: ImageLabelOriginOwner,
        asset_reference_set: SealedAssetReferenceSetProof,
    ) -> Result<Self, SyndicRecordError> {
        if start_label.get() > end_label.get() {
            return Err(SyndicRecordError::InvalidImageLabelFrontier);
        }
        Ok(Self {
            thread_id,
            start_label,
            end_label,
            admitted_owner,
            asset_reference_set,
        })
    }

    #[must_use]
    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }
    #[must_use]
    pub const fn start_label(self) -> ImageLabelOrdinal {
        self.start_label
    }
    #[must_use]
    pub const fn end_label(self) -> ImageLabelOrdinal {
        self.end_label
    }
    #[must_use]
    pub const fn admitted_owner(self) -> ImageLabelOriginOwner {
        self.admitted_owner
    }
    #[must_use]
    pub const fn asset_reference_set(self) -> SealedAssetReferenceSetProof {
        self.asset_reference_set
    }
    #[must_use]
    pub const fn contains(self, label: ImageLabelOrdinal) -> bool {
        self.start_label.get() <= label.get() && label.get() <= self.end_label.get()
    }
}
