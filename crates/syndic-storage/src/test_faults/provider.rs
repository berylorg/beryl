use std::{error::Error, fmt};

use beryl_home_store::{
    CurrentDomainCommand, DomainCallbackError, DomainCallbackSource, DomainMutation, DomainReader,
    MutationBuildError, MutationBuilder, ReadError, RecordCodec,
};

use crate::{
    ProviderItemBuildLifecycle, ProviderItemBuildRecord, ProviderNarrativeReference,
    ProviderNarrativeSpanRecord, ProviderStorageRecordError, SyndicStorage,
    codec::{
        ProviderItemBuildsCodec, ProviderItemBuildsFamily, ProviderNarrativeSpanKey,
        ProviderNarrativeSpansCodec, ProviderNarrativeSpansFamily,
    },
    domain::SyndicDomain,
};

/// One provider-storage family available to the bounded codec fixture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderFixtureFamily {
    ItemBuilds,
    NarrativeSpans,
}

impl ProviderFixtureFamily {
    /// Returns the exact ordered V5 physical-family names for registry assertions.
    #[must_use]
    pub fn domain_family_names() -> Vec<&'static str> {
        crate::domain::v7_family_names().collect()
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ItemBuilds => "provider-item-builds",
            Self::NarrativeSpans => "provider-narrative-spans",
        }
    }

    #[must_use]
    pub const fn record_version(self) -> u32 {
        match self {
            Self::ItemBuilds => ProviderItemBuildsCodec::VERSION.get(),
            Self::NarrativeSpans => ProviderNarrativeSpansCodec::VERSION.get(),
        }
    }

    #[must_use]
    pub const fn maximum_key_bytes(self) -> usize {
        match self {
            Self::ItemBuilds => ProviderItemBuildsCodec::MAX_KEY_BYTES,
            Self::NarrativeSpans => ProviderNarrativeSpansCodec::MAX_KEY_BYTES,
        }
    }

    #[must_use]
    pub const fn maximum_value_bytes(self) -> usize {
        match self {
            Self::ItemBuilds => ProviderItemBuildsCodec::MAX_VALUE_BYTES,
            Self::NarrativeSpans => ProviderNarrativeSpansCodec::MAX_VALUE_BYTES,
        }
    }
}

/// One exact typed provider-family record used only by the test-fault codec seam.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderFixtureRecord {
    ItemBuild(Box<ProviderItemBuildRecord>),
    NarrativeSpan(Box<ProviderNarrativeSpanRecord>),
}

impl ProviderFixtureRecord {
    #[must_use]
    pub const fn family(&self) -> ProviderFixtureFamily {
        match self {
            Self::ItemBuild(_) => ProviderFixtureFamily::ItemBuilds,
            Self::NarrativeSpan(_) => ProviderFixtureFamily::NarrativeSpans,
        }
    }
}

/// Returns the exact codec value length for one valid provider fixture record.
pub fn encoded_provider_fixture_value_bytes(
    record: &ProviderFixtureRecord,
) -> Result<usize, ProviderFixtureCodecError> {
    encode(record).map(|(_, value)| value.len())
}

/// One bounded structural corruption applied after exact family encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderFixtureCorruption {
    TruncatedKey,
    TruncatedValue,
    TrailingKey,
    TrailingValue,
    InvalidValueTag,
    ZeroNarrativeGeneration,
    KeyValueMismatch,
}

/// One codec-valid persisted provider-narrative fault for reopen validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistedProviderNarrativeCorruption {
    SourceDigest,
    StoredKey,
    ResultingChainDigest,
    StagedFrontier,
}

/// Why the narrow persisted provider-narrative fault could not be installed.
#[derive(Debug, thiserror::Error)]
pub enum PersistedProviderNarrativeCorruptionError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Build(#[from] MutationBuildError),
    #[error(transparent)]
    StorageRecord(#[from] ProviderStorageRecordError),
    #[error("persisted provider-narrative corruption requires one provable staged first span")]
    UnsupportedTarget,
    #[error("persisted provider-narrative corruption target changed before writer admission")]
    TargetChanged,
    #[error("persisted provider-narrative corrupted-key destination is already occupied")]
    DestinationOccupied,
}

impl DomainCallbackError for PersistedProviderNarrativeCorruptionError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            source => Err(source),
        }
    }
}

impl SyndicStorage {
    /// Builds one exact current-domain command that installs a bounded semantic narrative fault.
    pub fn current_corrupt_staged_provider_narrative(
        &self,
        build: &ProviderItemBuildRecord,
        span: ProviderNarrativeSpanRecord,
        corruption: PersistedProviderNarrativeCorruption,
    ) -> Result<CurrentDomainCommand, PersistedProviderNarrativeCorruptionError> {
        let mutation = PersistedProviderNarrativeFault::new(build, span, corruption)?;
        Ok(self.handle.current_command(mutation))
    }
}

/// Exact provider fixture codec rejection, retained as bounded diagnostic text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderFixtureCodecError(Box<str>);

impl fmt::Display for ProviderFixtureCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Error for ProviderFixtureCodecError {}

/// Round-trips one typed fixture through the exact family key and value codec.
pub fn roundtrip_provider_fixture(
    record: &ProviderFixtureRecord,
) -> Result<ProviderFixtureRecord, ProviderFixtureCodecError> {
    let (key, value) = encode(record)?;
    decode(record.family(), &key, &value)
}

/// Applies one closed corruption and then invokes the exact family decoder.
pub fn decode_corrupted_provider_fixture(
    record: &ProviderFixtureRecord,
    corruption: ProviderFixtureCorruption,
) -> Result<ProviderFixtureRecord, ProviderFixtureCodecError> {
    let (mut key, mut value) = encode(record)?;
    apply_corruption(record.family(), &mut key, &mut value, corruption)?;
    decode(record.family(), &key, &value)
}

#[derive(Clone, Debug)]
struct PersistedProviderNarrativeFault {
    expected_build: ProviderItemBuildRecord,
    expected_span: ProviderNarrativeSpanRecord,
    replacement: PersistedProviderNarrativeReplacement,
}

#[derive(Clone, Debug)]
enum PersistedProviderNarrativeReplacement {
    Span(Box<ProviderNarrativeSpanRecord>),
    MovedKey(ProviderNarrativeSpanKey),
    Build(Box<ProviderItemBuildRecord>),
}

impl PersistedProviderNarrativeFault {
    fn new(
        build: &ProviderItemBuildRecord,
        span: ProviderNarrativeSpanRecord,
        corruption: PersistedProviderNarrativeCorruption,
    ) -> Result<Self, PersistedProviderNarrativeCorruptionError> {
        let target = build
            .target()
            .narrative()
            .ok_or(PersistedProviderNarrativeCorruptionError::UnsupportedTarget)?;
        if build.lifecycle() != ProviderItemBuildLifecycle::Staging
            || build.staged_narrative() != Some(target)
            || build.staged_encoded_bytes() >= build.target().content().summary().encoded_bytes()
            || span.content_id() != target.content_id()
            || span.generation() != target.generation()
            || span.logical_start() != 0
            || span.logical_end() <= 1
            || span.logical_end() != target.logical_utf8_bytes()
            || target.span_count() != 1
            || span.resulting_chain_digest() != target.chain_digest()
            || span.frame_ordinal() != build.target().frame().ordinal()
            || span.frame_encoded_digest() != build.target().frame().encoded_digest()
            || span.source_end() > build.staged_encoded_bytes()
        {
            return Err(PersistedProviderNarrativeCorruptionError::UnsupportedTarget);
        }

        let replacement = match corruption {
            PersistedProviderNarrativeCorruption::SourceDigest => {
                let mut digest = span.source_digest();
                digest[0] ^= 1;
                let previous =
                    crate::provider_narrative_chain_seed(target.content_id(), target.generation());
                PersistedProviderNarrativeReplacement::Span(Box::new(
                    ProviderNarrativeSpanRecord::new(
                        span.content_id(),
                        span.generation(),
                        span.logical_start(),
                        span.logical_end(),
                        span.frame_ordinal(),
                        span.frame_encoded_digest(),
                        span.source_start(),
                        span.source_end(),
                        digest,
                        previous,
                    )?,
                ))
            }
            PersistedProviderNarrativeCorruption::StoredKey => {
                let logical_start = span
                    .logical_start()
                    .checked_add(1)
                    .ok_or(PersistedProviderNarrativeCorruptionError::UnsupportedTarget)?;
                PersistedProviderNarrativeReplacement::MovedKey(ProviderNarrativeSpanKey::new(
                    span.content_id(),
                    span.generation(),
                    logical_start,
                ))
            }
            PersistedProviderNarrativeCorruption::ResultingChainDigest => {
                let mut digest = span.resulting_chain_digest();
                digest[0] ^= 1;
                PersistedProviderNarrativeReplacement::Span(Box::new(
                    ProviderNarrativeSpanRecord::from_stored_parts(
                        span.content_id(),
                        span.generation(),
                        span.logical_start(),
                        span.logical_end(),
                        span.frame_ordinal(),
                        span.frame_encoded_digest(),
                        span.source_start(),
                        span.source_end(),
                        span.source_digest(),
                        digest,
                    )?,
                ))
            }
            PersistedProviderNarrativeCorruption::StagedFrontier => {
                let narrative = ProviderNarrativeReference::new(
                    target.content_id(),
                    target.generation(),
                    target.span_count(),
                    target.logical_utf8_bytes() - 1,
                    target.chain_digest(),
                )?;
                PersistedProviderNarrativeReplacement::Build(Box::new(
                    ProviderItemBuildRecord::new(
                        build.item_id(),
                        build.turn_id(),
                        build.source().clone(),
                        build.source_event(),
                        build.revision(),
                        build.prior().cloned(),
                        build.target().clone(),
                        build.staged_chunk_count(),
                        build.staged_encoded_bytes(),
                        build.staged_chain_digest(),
                        Some(narrative),
                        build.completion_check(),
                        ProviderItemBuildLifecycle::Staging,
                    )?,
                ))
            }
        };
        Ok(Self {
            expected_build: build.clone(),
            expected_span: span,
            replacement,
        })
    }

    fn span_key(&self) -> ProviderNarrativeSpanKey {
        ProviderNarrativeSpanKey::new(
            self.expected_span.content_id(),
            self.expected_span.generation(),
            self.expected_span.logical_start(),
        )
    }
}

enum PreparedPersistedProviderNarrativeFault {
    Span {
        key: ProviderNarrativeSpanKey,
        span: ProviderNarrativeSpanRecord,
    },
    MovedKey {
        source: ProviderNarrativeSpanKey,
        destination: ProviderNarrativeSpanKey,
        span: ProviderNarrativeSpanRecord,
    },
    Build(ProviderItemBuildRecord),
}

impl DomainMutation<SyndicDomain> for PersistedProviderNarrativeFault {
    type Error = PersistedProviderNarrativeCorruptionError;
    type Prepared = PreparedPersistedProviderNarrativeFault;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let build = reader.point::<ProviderItemBuildsCodec>(
            &self.expected_build.item_id(),
            crate::codec::family_point_limit::<ProviderItemBuildsFamily>(),
        )?;
        let span_key = self.span_key();
        let span = reader.point::<ProviderNarrativeSpansCodec>(
            &span_key,
            crate::codec::family_point_limit::<ProviderNarrativeSpansFamily>(),
        )?;
        if build.as_ref() != Some(&self.expected_build)
            || span.as_ref() != Some(&self.expected_span)
        {
            return Err(PersistedProviderNarrativeCorruptionError::TargetChanged);
        }
        if let PersistedProviderNarrativeReplacement::MovedKey(destination) = &self.replacement
            && reader
                .point::<ProviderNarrativeSpansCodec>(
                    destination,
                    crate::codec::family_point_limit::<ProviderNarrativeSpansFamily>(),
                )?
                .is_some()
        {
            return Err(PersistedProviderNarrativeCorruptionError::DestinationOccupied);
        }
        match self.replacement {
            PersistedProviderNarrativeReplacement::Span(span) => {
                Ok(PreparedPersistedProviderNarrativeFault::Span {
                    key: span_key,
                    span: *span,
                })
            }
            PersistedProviderNarrativeReplacement::MovedKey(destination) => {
                Ok(PreparedPersistedProviderNarrativeFault::MovedKey {
                    source: span_key,
                    destination,
                    span: self.expected_span,
                })
            }
            PersistedProviderNarrativeReplacement::Build(build) => {
                Ok(PreparedPersistedProviderNarrativeFault::Build(*build))
            }
        }
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match &self.replacement {
            PersistedProviderNarrativeReplacement::Span(_) => {
                reservation.reserve_records::<ProviderNarrativeSpansCodec>(1)?;
            }
            PersistedProviderNarrativeReplacement::MovedKey(_) => {
                reservation.reserve_records::<ProviderNarrativeSpansCodec>(2)?;
            }
            PersistedProviderNarrativeReplacement::Build(_) => {
                reservation.reserve_records::<ProviderItemBuildsCodec>(1)?;
            }
        }
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        builder: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match prepared {
            PreparedPersistedProviderNarrativeFault::Span { key, span } => {
                builder.put::<ProviderNarrativeSpansCodec>(&key, &span)?;
            }
            PreparedPersistedProviderNarrativeFault::MovedKey {
                source,
                destination,
                span,
            } => {
                builder.delete::<ProviderNarrativeSpansCodec>(&source)?;
                builder.put::<ProviderNarrativeSpansCodec>(&destination, &span)?;
            }
            PreparedPersistedProviderNarrativeFault::Build(build) => {
                builder.put::<ProviderItemBuildsCodec>(&build.item_id(), &build)?;
            }
        }
        Ok(())
    }
}

fn encode(record: &ProviderFixtureRecord) -> Result<(Vec<u8>, Vec<u8>), ProviderFixtureCodecError> {
    match record {
        ProviderFixtureRecord::ItemBuild(value) => {
            let key = value.item_id();
            ProviderItemBuildsFamily::validate_key_value(&key, value).map_err(fixture_error)?;
            Ok((
                ProviderItemBuildsCodec::encode_key(&key).map_err(fixture_error)?,
                ProviderItemBuildsCodec::encode_value(value).map_err(fixture_error)?,
            ))
        }
        ProviderFixtureRecord::NarrativeSpan(value) => {
            let key = ProviderNarrativeSpanKey::new(
                value.content_id(),
                value.generation(),
                value.logical_start(),
            );
            ProviderNarrativeSpansFamily::validate_key_value(&key, value).map_err(fixture_error)?;
            Ok((
                ProviderNarrativeSpansCodec::encode_key(&key).map_err(fixture_error)?,
                ProviderNarrativeSpansCodec::encode_value(value).map_err(fixture_error)?,
            ))
        }
    }
}

fn decode(
    family: ProviderFixtureFamily,
    key: &[u8],
    value: &[u8],
) -> Result<ProviderFixtureRecord, ProviderFixtureCodecError> {
    match family {
        ProviderFixtureFamily::ItemBuilds => {
            let key = ProviderItemBuildsCodec::decode_key(key).map_err(fixture_error)?;
            ProviderItemBuildsCodec::validate_stored_key(&key).map_err(fixture_error)?;
            let value = ProviderItemBuildsCodec::decode_value(value).map_err(fixture_error)?;
            ProviderItemBuildsFamily::validate_key_value(&key, &value).map_err(fixture_error)?;
            Ok(ProviderFixtureRecord::ItemBuild(Box::new(value)))
        }
        ProviderFixtureFamily::NarrativeSpans => {
            let key = ProviderNarrativeSpansCodec::decode_key(key).map_err(fixture_error)?;
            ProviderNarrativeSpansCodec::validate_stored_key(&key).map_err(fixture_error)?;
            let value = ProviderNarrativeSpansCodec::decode_value(value).map_err(fixture_error)?;
            ProviderNarrativeSpansFamily::validate_key_value(&key, &value)
                .map_err(fixture_error)?;
            Ok(ProviderFixtureRecord::NarrativeSpan(Box::new(value)))
        }
    }
}

fn apply_corruption(
    family: ProviderFixtureFamily,
    key: &mut Vec<u8>,
    value: &mut Vec<u8>,
    corruption: ProviderFixtureCorruption,
) -> Result<(), ProviderFixtureCodecError> {
    match corruption {
        ProviderFixtureCorruption::TruncatedKey => {
            key.pop();
        }
        ProviderFixtureCorruption::TruncatedValue => {
            value.pop();
        }
        ProviderFixtureCorruption::TrailingKey => key.push(0),
        ProviderFixtureCorruption::TrailingValue => value.push(0),
        ProviderFixtureCorruption::InvalidValueTag => {
            if family != ProviderFixtureFamily::ItemBuilds {
                return Err(fixture_message(
                    "invalid value-tag corruption requires the item-build family",
                ));
            }
            let tag = value
                .last_mut()
                .ok_or_else(|| fixture_message("provider fixture value is unexpectedly empty"))?;
            *tag = u8::MAX;
        }
        ProviderFixtureCorruption::ZeroNarrativeGeneration => {
            if family != ProviderFixtureFamily::NarrativeSpans {
                return Err(fixture_message(
                    "zero narrative-generation corruption requires the narrative-span family",
                ));
            }
            key[16..24].fill(0);
        }
        ProviderFixtureCorruption::KeyValueMismatch => {
            key[0] ^= 1;
        }
    }
    Ok(())
}

fn fixture_error(error: impl fmt::Display) -> ProviderFixtureCodecError {
    fixture_message(&error.to_string())
}

fn fixture_message(message: &str) -> ProviderFixtureCodecError {
    ProviderFixtureCodecError(message.into())
}
