use std::error::Error;

use beryl_model::{DomainRevision, HomeRevision};
use fjall::{Readable, Snapshot};
use thiserror::Error;

use crate::{
    CursorDirection, CursorPage, CursorRange, CursorReadLimits, CursorRecord, DomainHandle,
    PointReadLimit, RecordCodec, RecordVersion, StorageDomain,
    domain::RegisteredDomain,
    health::FailureSeverity,
    metadata::{DomainMetadata, decode_home_revision},
    store::{HomeStore, StoreGeneration},
};

const RECORD_VERSION_BYTES: usize = 4;

mod execute;

pub(crate) use execute::{encode_key, encode_value};
use execute::{read_cursor, read_point};

/// Codec operation that rejected a typed value or stored record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecOperation {
    /// Encode a caller-supplied typed key.
    EncodeKey,
    /// Decode a stored key.
    DecodeKey,
    /// Encode a caller-supplied typed value.
    EncodeValue,
    /// Decode a stored value.
    DecodeValue,
}

/// Engine operation performed by a bounded typed read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadStage {
    /// Read a fixed home revision record.
    HomeRevision,
    /// Read a fixed domain registration record.
    DomainRevision,
    /// Determine a point value's stored size.
    PointSize,
    /// Read a bounded point value.
    PointValue,
    /// Walk one bounded cursor key.
    CursorKey,
    /// Determine one cursor value's stored size.
    CursorValueSize,
    /// Read one bounded cursor value.
    CursorValue,
}

/// Why a typed point or cursor read could not complete.
#[derive(Debug, Error)]
pub enum ReadError {
    /// The process-wide health gate is not accepting state-dependent work.
    #[error(transparent)]
    HealthGate(#[from] crate::HealthGateError),

    /// A panic poisoned the in-process home generation lock.
    #[error("the Beryl-home generation lock is poisoned")]
    GenerationPoisoned,

    /// The handle belongs to another home generation or registration.
    #[error("domain handle `{domain}` does not belong to this home generation")]
    ForeignDomain {
        /// Stable typed domain name.
        domain: &'static str,
    },

    /// The codec names no family in its owning domain.
    #[error("record codec names unknown family `{family}` in domain `{domain}`")]
    UnknownFamily {
        /// Stable typed domain name.
        domain: &'static str,
        /// Unknown logical family.
        family: &'static str,
    },

    /// A codec declared unusable bounds.
    #[error("record codec for `{domain}`/`{family}` has an invalid static bound")]
    InvalidCodecContract {
        /// Stable typed domain name.
        domain: &'static str,
        /// Logical family name.
        family: &'static str,
    },

    /// The typed codec produced an empty or oversized key.
    #[error(
        "encoded key for `{domain}`/`{family}` has {actual} bytes; accepted range is 1..={maximum}"
    )]
    InvalidKeySize {
        /// Stable typed domain name.
        domain: &'static str,
        /// Logical family name.
        family: &'static str,
        /// Codec-owned maximum.
        maximum: usize,
        /// Actual encoded size.
        actual: usize,
    },

    /// The finite cursor endpoints are reversed after typed encoding.
    #[error("encoded cursor range for `{domain}`/`{family}` is reversed")]
    ReversedRange {
        /// Stable typed domain name.
        domain: &'static str,
        /// Logical family name.
        family: &'static str,
    },

    /// One stored value exceeds either the codec or caller byte bound.
    #[error("stored value for `{domain}`/`{family}` has {actual} bytes, exceeding limit {maximum}")]
    BoundExceeded {
        /// Stable typed domain name.
        domain: &'static str,
        /// Logical family name.
        family: &'static str,
        /// Effective byte bound.
        maximum: usize,
        /// Stored byte size.
        actual: usize,
    },

    /// A stored record carries a version this exact codec does not accept.
    #[error(
        "record in `{domain}`/`{family}` uses version {found}, but this codec accepts {supported}"
    )]
    UnsupportedRecordVersion {
        /// Stable typed domain name.
        domain: &'static str,
        /// Logical family name.
        family: &'static str,
        /// Exact supported record version.
        supported: RecordVersion,
        /// Exact stored record version.
        found: u32,
    },

    /// A stored record is too short to contain its version prefix.
    #[error("record in `{domain}`/`{family}` has a malformed value envelope")]
    MalformedRecord {
        /// Stable typed domain name.
        domain: &'static str,
        /// Logical family name.
        family: &'static str,
    },

    /// The domain-owned codec failed.
    #[error("codec failed during {operation:?} for `{domain}`/`{family}`: {source}")]
    Codec {
        /// Stable typed domain name.
        domain: &'static str,
        /// Logical family name.
        family: &'static str,
        /// Codec operation.
        operation: CodecOperation,
        /// Domain-owned source.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },

    /// A fixed internal revision record is malformed.
    #[error("invalid {kind} revision metadata: {source}")]
    InvalidRevisionMetadata {
        /// Revision record kind.
        kind: &'static str,
        /// Decoder failure.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },

    /// Fjall failed while servicing a bounded typed read.
    #[error("typed read failed during {stage:?}: {source}")]
    Storage {
        /// Read stage.
        stage: ReadStage,
        /// Engine source hidden behind the package boundary.
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
}

/// Short-lived typed view used by domain validators and mutation contributors.
pub struct DomainReader<'a, D: StorageDomain> {
    snapshot: &'a Snapshot,
    domain: &'a RegisteredDomain,
    _typed: std::marker::PhantomData<fn() -> D>,
}

impl<'a, D: StorageDomain> DomainReader<'a, D> {
    pub(crate) const fn new(snapshot: &'a Snapshot, domain: &'a RegisteredDomain) -> Self {
        Self {
            snapshot,
            domain,
            _typed: std::marker::PhantomData,
        }
    }

    /// Reads one typed point under an explicit stored-byte bound.
    pub fn point<R: RecordCodec<D>>(
        &self,
        key: &R::Key,
        limit: PointReadLimit,
    ) -> Result<Option<R::Value>, ReadError> {
        read_point::<D, R>(self.snapshot, self.domain, key, limit)
    }

    /// Reads one finite typed key range under explicit item and byte bounds.
    pub fn cursor<R: RecordCodec<D>>(
        &self,
        range: &CursorRange<R::Key>,
        direction: CursorDirection,
        limits: CursorReadLimits,
    ) -> Result<CursorPage<R::Key, R::Value>, ReadError> {
        read_cursor::<D, R>(self.snapshot, self.domain, range, direction, limits)
    }
}

impl HomeStore {
    /// Reads the exact current home revision from one short-lived snapshot.
    pub fn home_revision(&self) -> Result<HomeRevision, ReadError> {
        self.execute_read(|generation| {
            read_home_revision(
                &generation.database.snapshot(),
                generation.header_keyspace(),
            )
        })
    }

    /// Reads the exact current revision of one registered typed domain.
    pub fn domain_revision<D: StorageDomain>(
        &self,
        handle: DomainHandle<D>,
    ) -> Result<DomainRevision, ReadError> {
        self.execute_read(|generation| {
            let domain = generation
                .resolve_domain(handle)
                .ok_or(ReadError::ForeignDomain { domain: D::NAME })?;
            read_domain_metadata(
                &generation.database.snapshot(),
                generation.domains_keyspace(),
                domain.name,
            )
            .map(|metadata| metadata.revision)
        })
    }

    /// Reads one typed point without exposing its physical keyspace or encoding.
    pub fn read_point<D: StorageDomain, R: RecordCodec<D>>(
        &self,
        handle: DomainHandle<D>,
        key: &R::Key,
        limit: PointReadLimit,
    ) -> Result<Option<R::Value>, ReadError> {
        self.execute_read(|generation| {
            let domain = generation
                .resolve_domain(handle)
                .ok_or(ReadError::ForeignDomain { domain: D::NAME })?;
            read_point::<D, R>(&generation.database.snapshot(), domain, key, limit)
        })
    }

    /// Reads one typed finite cursor range without returning a raw iterator.
    pub fn read_cursor<D: StorageDomain, R: RecordCodec<D>>(
        &self,
        handle: DomainHandle<D>,
        range: &CursorRange<R::Key>,
        direction: CursorDirection,
        limits: CursorReadLimits,
    ) -> Result<CursorPage<R::Key, R::Value>, ReadError> {
        self.execute_read(|generation| {
            let domain = generation
                .resolve_domain(handle)
                .ok_or(ReadError::ForeignDomain { domain: D::NAME })?;
            read_cursor::<D, R>(
                &generation.database.snapshot(),
                domain,
                range,
                direction,
                limits,
            )
        })
    }

    fn execute_read<T>(
        &self,
        operation: impl FnOnce(&StoreGeneration) -> Result<T, ReadError>,
    ) -> Result<T, ReadError> {
        let admission = self.health.admit()?;
        let generation = match self.generation.read() {
            Ok(generation) => generation,
            Err(_) => {
                admission.fail(FailureSeverity::Structural);
                return Err(ReadError::GenerationPoisoned);
            }
        };
        let generation = match generation.as_ref() {
            Some(generation) => generation,
            None => {
                admission.fail(FailureSeverity::Structural);
                return Err(ReadError::GenerationPoisoned);
            }
        };
        let result = operation(generation);
        if let Err(error) = &result {
            if let Some(severity) = read_failure_severity(error) {
                admission.fail(severity);
            }
            return result;
        }
        admission.confirm()?;
        result
    }
}

pub(crate) fn read_home_revision(
    snapshot: &Snapshot,
    keyspace: &fjall::Keyspace,
) -> Result<HomeRevision, ReadError> {
    let encoded = snapshot
        .get(keyspace, crate::metadata::HOME_REVISION_KEY)
        .map_err(|source| storage(ReadStage::HomeRevision, source))?
        .ok_or_else(|| invalid_revision("home", "home revision record is missing"))?;
    decode_home_revision(&encoded).map_err(|source| ReadError::InvalidRevisionMetadata {
        kind: "home",
        source: Box::new(source),
    })
}

pub(crate) fn read_domain_metadata(
    snapshot: &Snapshot,
    keyspace: &fjall::Keyspace,
    domain: &'static str,
) -> Result<DomainMetadata, ReadError> {
    let encoded = snapshot
        .get(keyspace, domain.as_bytes())
        .map_err(|source| storage(ReadStage::DomainRevision, source))?
        .ok_or_else(|| invalid_revision("domain", "domain registration record is missing"))?;
    DomainMetadata::decode(&encoded).map_err(|source| ReadError::InvalidRevisionMetadata {
        kind: "domain",
        source: Box::new(source),
    })
}

fn storage(stage: ReadStage, source: impl Error + Send + Sync + 'static) -> ReadError {
    ReadError::Storage {
        stage,
        source: Box::new(source),
    }
}

fn invalid_revision(kind: &'static str, message: &'static str) -> ReadError {
    ReadError::InvalidRevisionMetadata {
        kind,
        source: Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    }
}

fn read_failure_severity(error: &ReadError) -> Option<FailureSeverity> {
    match error {
        ReadError::HealthGate(_)
        | ReadError::ForeignDomain { .. }
        | ReadError::UnknownFamily { .. }
        | ReadError::InvalidCodecContract { .. }
        | ReadError::InvalidKeySize { .. }
        | ReadError::ReversedRange { .. }
        | ReadError::BoundExceeded { .. } => None,
        ReadError::Storage { .. } => Some(FailureSeverity::Verify),
        ReadError::GenerationPoisoned
        | ReadError::UnsupportedRecordVersion { .. }
        | ReadError::MalformedRecord { .. }
        | ReadError::InvalidRevisionMetadata { .. } => Some(FailureSeverity::Structural),
        ReadError::Codec { operation, .. } => match operation {
            CodecOperation::DecodeKey | CodecOperation::DecodeValue => {
                Some(FailureSeverity::Structural)
            }
            CodecOperation::EncodeKey | CodecOperation::EncodeValue => None,
        },
    }
}
