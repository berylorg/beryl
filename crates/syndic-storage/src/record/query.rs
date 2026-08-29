use sha2::{Digest, Sha256};

use beryl_model::{
    SealedAssetReferenceSetProof, SyndicAcceptedInputId, SyndicItemId, SyndicThreadId,
};

use crate::{ImageLabelOrdinal, SyndicRecordError};

mod activity;

pub use activity::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageLabelAuthorityHeadV1 {
    thread_id: SyndicThreadId,
    revision: u64,
    inherited: crate::ImageLabelFrontier,
    permanent: crate::ImageLabelFrontier,
    digest: [u8; 32],
}

impl ImageLabelAuthorityHeadV1 {
    pub fn new(
        thread_id: SyndicThreadId,
        revision: u64,
        inherited: crate::ImageLabelFrontier,
        permanent: crate::ImageLabelFrontier,
    ) -> Result<Self, SyndicRecordError> {
        if revision == 0 || permanent < inherited {
            return Err(SyndicRecordError::InvalidImageLabelFrontier);
        }
        let digest = Self::digest_for(thread_id, revision, inherited, permanent);
        Ok(Self {
            thread_id,
            revision,
            inherited,
            permanent,
            digest,
        })
    }

    pub const fn thread_id(self) -> SyndicThreadId {
        self.thread_id
    }
    pub const fn revision(self) -> u64 {
        self.revision
    }
    pub const fn inherited(self) -> crate::ImageLabelFrontier {
        self.inherited
    }
    pub const fn permanent(self) -> crate::ImageLabelFrontier {
        self.permanent
    }
    pub const fn digest(self) -> [u8; 32] {
        self.digest
    }

    pub fn is_exact(self) -> bool {
        self.revision != 0
            && self.permanent >= self.inherited
            && self.digest
                == Self::digest_for(
                    self.thread_id,
                    self.revision,
                    self.inherited,
                    self.permanent,
                )
    }

    pub fn advanced(self, permanent: crate::ImageLabelFrontier) -> Result<Self, SyndicRecordError> {
        if permanent < self.permanent {
            return Err(SyndicRecordError::InvalidImageLabelFrontier);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(SyndicRecordError::InvalidImageLabelFrontier)?;
        Self::new(self.thread_id, revision, self.inherited, permanent)
    }

    fn digest_for(
        thread_id: SyndicThreadId,
        revision: u64,
        inherited: crate::ImageLabelFrontier,
        permanent: crate::ImageLabelFrontier,
    ) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"syndic/image-label-authority-head/v1");
        hasher.update(thread_id.as_bytes());
        hasher.update(revision.to_be_bytes());
        hasher.update(inherited.get().to_be_bytes());
        hasher.update(permanent.get().to_be_bytes());
        hasher.finalize().into()
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
