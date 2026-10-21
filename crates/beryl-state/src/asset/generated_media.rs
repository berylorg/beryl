use std::{error::Error, fmt};

use beryl_home_store::{
    DomainCallbackError, DomainCallbackSource, DomainHandle, HomeStore, MutationBuildError,
    ReadError,
};
use beryl_model::{AssetId, SyndicItemId, SyndicResourceId, SyndicThreadId, SyndicTurnId};

use super::{AssetDimensions, AssetDomain, AssetMediaType, AssetMetadataRecord, AssetState};
use crate::RecordRevision;

mod codec;
mod mutation;
pub(super) use codec::GeneratedMediaCodec;
pub use mutation::{GeneratedMediaCommandFootprint, GeneratedMediaPreparation};

pub const GENERATED_MEDIA_ADMISSION_MAX_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratedMediaTarget {
    resource: SyndicResourceId,
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    item: SyndicItemId,
    fingerprint: [u8; 32],
}

impl GeneratedMediaTarget {
    pub const fn new(
        resource: SyndicResourceId,
        thread: SyndicThreadId,
        turn: SyndicTurnId,
        item: SyndicItemId,
        fingerprint: [u8; 32],
    ) -> Self {
        Self {
            resource,
            thread,
            turn,
            item,
            fingerprint,
        }
    }
    pub const fn resource(self) -> SyndicResourceId {
        self.resource
    }
    pub const fn thread(self) -> SyndicThreadId {
        self.thread
    }
    pub const fn turn(self) -> SyndicTurnId {
        self.turn
    }
    pub const fn item(self) -> SyndicItemId {
        self.item
    }
    pub const fn fingerprint(self) -> [u8; 32] {
        self.fingerprint
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedMediaFacts {
    asset: AssetId,
    media_type: AssetMediaType,
    dimensions: Option<AssetDimensions>,
}

impl GeneratedMediaFacts {
    pub fn new(
        asset: AssetId,
        media_type: AssetMediaType,
        dimensions: Option<AssetDimensions>,
    ) -> Result<Self, GeneratedMediaError> {
        if dimensions.is_some_and(|d| {
            u32::try_from(d.width().get()).is_err() || u32::try_from(d.height().get()).is_err()
        }) {
            return Err(GeneratedMediaError::Rejected);
        }
        Ok(Self {
            asset,
            media_type,
            dimensions,
        })
    }
    pub const fn asset(&self) -> AssetId {
        self.asset
    }
    pub const fn media_type(&self) -> &AssetMediaType {
        &self.media_type
    }
    pub const fn dimensions(&self) -> Option<AssetDimensions> {
        self.dimensions
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratedMediaLifecycle {
    Prepared,
    Published,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GeneratedMediaRecord {
    thread: SyndicThreadId,
    turn: SyndicTurnId,
    item: SyndicItemId,
    fingerprint: [u8; 32],
    facts: GeneratedMediaFacts,
    lifecycle: GeneratedMediaLifecycle,
    revision: RecordRevision,
}

impl GeneratedMediaRecord {
    fn matches_target(&self, target: GeneratedMediaTarget) -> bool {
        self.thread == target.thread
            && self.turn == target.turn
            && self.item == target.item
            && self.fingerprint == target.fingerprint
    }
    fn matches_metadata(&self, metadata: &AssetMetadataRecord) -> bool {
        metadata.asset_id() == self.facts.asset
            && metadata.media_type() == &self.facts.media_type
            && metadata.dimensions() == self.facts.dimensions
    }
}

#[derive(Clone, Debug)]
pub struct GeneratedMediaAdmission {
    scope: DomainHandle<AssetDomain>,
    target: GeneratedMediaTarget,
    record: GeneratedMediaRecord,
}

impl GeneratedMediaAdmission {
    pub const fn target(&self) -> GeneratedMediaTarget {
        self.target
    }
    pub const fn facts(&self) -> &GeneratedMediaFacts {
        &self.record.facts
    }
    pub const fn lifecycle(&self) -> GeneratedMediaLifecycle {
        self.record.lifecycle
    }
    pub const fn revision(&self) -> RecordRevision {
        self.record.revision
    }
}

#[derive(Debug)]
pub enum GeneratedMediaError {
    Read(ReadError),
    Build(MutationBuildError),
    Rejected,
    ConcurrentChange,
    NoEffect,
    RevisionExhausted,
}
impl fmt::Display for GeneratedMediaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(e) => e.fmt(f),
            Self::Build(e) => e.fmt(f),
            Self::Rejected => {
                f.write_str("generated media source, admission or metadata disagrees")
            }
            Self::NoEffect => f.write_str("exact generated media admission is already durable"),
            Self::ConcurrentChange => {
                f.write_str("generated media admission changed during reading")
            }
            Self::RevisionExhausted => f.write_str("generated media revision is exhausted"),
        }
    }
}
impl Error for GeneratedMediaError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(e) => Some(e),
            Self::Build(e) => Some(e),
            _ => None,
        }
    }
}
impl DomainCallbackError for GeneratedMediaError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(e) => Ok(DomainCallbackSource::Read(e)),
            e => Err(e),
        }
    }
}
impl From<ReadError> for GeneratedMediaError {
    fn from(e: ReadError) -> Self {
        Self::Read(e)
    }
}
impl From<MutationBuildError> for GeneratedMediaError {
    fn from(e: MutationBuildError) -> Self {
        Self::Build(e)
    }
}

impl AssetState {
    #[cfg(feature = "test-faults")]
    pub fn generated_media_record_bytes_for_test(
        target: GeneratedMediaTarget,
        facts: GeneratedMediaFacts,
        lifecycle: GeneratedMediaLifecycle,
        revision: RecordRevision,
    ) -> Vec<u8> {
        use beryl_home_store::RecordCodec;
        GeneratedMediaCodec::encode_value(&GeneratedMediaRecord {
            thread: target.thread,
            turn: target.turn,
            item: target.item,
            fingerprint: target.fingerprint,
            facts,
            lifecycle,
            revision,
        })
        .expect("bounded test record")
    }

    #[cfg(feature = "test-faults")]
    pub fn generated_media_record_valid_for_test(key: &[u8], bytes: &[u8]) -> bool {
        use beryl_home_store::RecordCodec;
        GeneratedMediaCodec::decode_key(key).is_ok()
            && GeneratedMediaCodec::decode_value(bytes).is_ok()
    }

    pub fn generated_media_admission(
        &self,
        store: &HomeStore,
        target: GeneratedMediaTarget,
    ) -> Result<Option<GeneratedMediaAdmission>, GeneratedMediaError> {
        let revision = self.revision(store)?;
        let record = store.read_point::<AssetDomain, GeneratedMediaCodec>(
            &self.handle,
            &target.resource,
            codec::point_limit(),
        )?;
        if let Some(record) = &record {
            let metadata = self
                .metadata(store, record.facts.asset)?
                .ok_or(GeneratedMediaError::Rejected)?;
            if !record.matches_target(target) || !record.matches_metadata(&metadata) {
                return Err(GeneratedMediaError::Rejected);
            }
        }
        if self.revision(store)? != revision {
            return Err(GeneratedMediaError::ConcurrentChange);
        }
        Ok(record.map(|record| GeneratedMediaAdmission {
            scope: self.handle.clone(),
            target,
            record,
        }))
    }
}

pub(super) fn validate(
    reader: &beryl_home_store::DomainReader<'_, AssetDomain>,
) -> Result<(), super::AssetValidationError> {
    use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits};
    let mut after = None;
    loop {
        let end = SyndicResourceId::from_bytes([u8::MAX; 16]);
        let range = match after {
            Some(key) => CursorRange::after(key, end),
            None => CursorRange::closed(SyndicResourceId::from_bytes([0; 16]), end),
        };
        let page = reader.cursor::<GeneratedMediaCodec>(
            &range,
            CursorDirection::Forward,
            CursorReadLimits::new(128, 65_536).expect("fixed admission page"),
        )?;
        for entry in page.records() {
            let metadata = reader
                .point::<super::codec::AssetMetadataCodec>(
                    &entry.value().facts.asset,
                    super::metadata_point_limit(),
                )?
                .ok_or(super::AssetValidationError::Invariant(
                    "generated media admission has no committed metadata",
                ))?;
            if !entry.value().matches_metadata(&metadata) {
                return Err(super::AssetValidationError::Invariant(
                    "generated media admission disagrees with committed metadata",
                ));
            }
        }
        if !page.has_more() {
            return Ok(());
        }
        after = page.records().last().map(|entry| *entry.key());
        if after.is_none() {
            return Err(super::AssetValidationError::Invariant(
                "generated media cursor has no progress",
            ));
        }
    }
}
