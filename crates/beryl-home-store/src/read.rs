use std::error::Error;

use beryl_model::{DomainRevision, HomeRevision};
use fjall::Snapshot;
use thiserror::Error;

use crate::{
    CursorDirection, CursorPage, CursorRange, CursorReadLimits, CursorRecord, DomainHandle,
    PointReadLimit, RecordCodec, RecordVersion, StorageDomain,
    codec::RECORD_VERSION_BYTES,
    domain::{RegisteredDomain, RegisteredFamily},
    fault::FaultPoint,
    health::{ClassifiedFjallError, FailureSeverity},
    metadata::{
        DomainMetadata, HOME_REVISION_BYTES, MAX_DOMAIN_METADATA_BYTES, decode_home_revision,
    },
    store::{HomeStore, StoreGeneration},
};

mod execute;

pub(crate) use execute::{
    encode_stored_key, encode_value, validate_physical_family, validate_record_envelope,
};
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
    /// Read one key during exhaustive physical-family validation.
    PhysicalKey,
    /// Determine one value size during exhaustive physical-family validation.
    PhysicalValueSize,
    /// Read one value during exhaustive physical-family validation.
    PhysicalValue,
    /// Confirm one successful read against its admitted healthy generation.
    Confirmation,
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

    /// The family is owned by a different exact Rust codec type.
    #[error("record codec does not own family `{family}` in domain `{domain}`")]
    CodecTypeMismatch {
        /// Stable typed domain name.
        domain: &'static str,
        /// Logical family registered to another codec type.
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

    /// Encoding a caller-supplied typed key produced an empty or oversized key.
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

    /// A physical stored key is empty or exceeds its registered codec envelope.
    #[error(
        "stored key for `{domain}`/`{family}` has {actual} bytes; accepted range is 1..={maximum}"
    )]
    InvalidStoredKeySize {
        /// Stable typed domain name.
        domain: &'static str,
        /// Logical family name.
        family: &'static str,
        /// Registered codec-owned maximum.
        maximum: usize,
        /// Physical stored-key size.
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

    /// Caller-originated encoded bytes exceed an explicit materialization or codec bound.
    #[error(
        "encoded bytes for `{domain}`/`{family}` have {actual} bytes, exceeding limit {maximum}"
    )]
    BoundExceeded {
        /// Stable typed domain name.
        domain: &'static str,
        /// Logical family name.
        family: &'static str,
        /// Effective byte bound.
        maximum: usize,
        /// Encoded byte size.
        actual: usize,
    },

    /// A physical stored value exceeds its registered codec envelope.
    #[error(
        "stored value for `{domain}`/`{family}` has {actual} bytes, exceeding codec envelope {maximum}"
    )]
    InvalidStoredValueSize {
        /// Stable typed domain name.
        domain: &'static str,
        /// Logical family name.
        family: &'static str,
        /// Registered maximum including the store-owned version prefix.
        maximum: usize,
        /// Physical stored-value size.
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
    _typed: std::marker::PhantomData<fn(D) -> D>,
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
            let snapshot = generation
                .database
                .snapshot()
                .map_err(|source| fjall_storage(ReadStage::HomeRevision, source))?;
            read_home_revision(&snapshot, generation.header_keyspace())
        })
    }

    /// Reads the exact current revision of one registered typed domain.
    pub fn domain_revision<D: StorageDomain>(
        &self,
        handle: &DomainHandle<D>,
    ) -> Result<DomainRevision, ReadError> {
        self.execute_read(|generation| {
            let domain = generation
                .resolve_domain(handle)
                .ok_or(ReadError::ForeignDomain { domain: D::NAME })?;
            let snapshot = generation
                .database
                .snapshot()
                .map_err(|source| fjall_storage(ReadStage::DomainRevision, source))?;
            read_domain_metadata(&snapshot, generation.domains_keyspace(), domain.name)
                .map(|metadata| metadata.revision)
        })
    }

    /// Reads one typed point without exposing its physical keyspace or encoding.
    pub fn read_point<D: StorageDomain, R: RecordCodec<D>>(
        &self,
        handle: &DomainHandle<D>,
        key: &R::Key,
        limit: PointReadLimit,
    ) -> Result<Option<R::Value>, ReadError> {
        self.execute_read(|generation| {
            let domain = generation
                .resolve_domain(handle)
                .ok_or(ReadError::ForeignDomain { domain: D::NAME })?;
            let snapshot = generation
                .database
                .snapshot()
                .map_err(|source| fjall_storage(ReadStage::PointSize, source))?;
            read_point::<D, R>(&snapshot, domain, key, limit)
        })
    }

    /// Reads one typed finite cursor range without returning a raw iterator.
    pub fn read_cursor<D: StorageDomain, R: RecordCodec<D>>(
        &self,
        handle: &DomainHandle<D>,
        range: &CursorRange<R::Key>,
        direction: CursorDirection,
        limits: CursorReadLimits,
    ) -> Result<CursorPage<R::Key, R::Value>, ReadError> {
        self.execute_read(|generation| {
            let domain = generation
                .resolve_domain(handle)
                .ok_or(ReadError::ForeignDomain { domain: D::NAME })?;
            let snapshot = generation
                .database
                .snapshot()
                .map_err(|source| fjall_storage(ReadStage::CursorKey, source))?;
            read_cursor::<D, R>(&snapshot, domain, range, direction, limits)
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
        if let Err(source) = self.faults.check(FaultPoint::BeforeReadConfirmation) {
            admission.fail(FailureSeverity::Structural);
            return Err(storage(ReadStage::Confirmation, source));
        }
        admission.confirm_database(&generation.database, |source| {
            storage(ReadStage::Confirmation, source)
        })?;
        result
    }
}

pub(crate) fn read_home_revision(
    snapshot: &Snapshot,
    keyspace: &fjall::Keyspace,
) -> Result<HomeRevision, ReadError> {
    let point = snapshot
        .point(keyspace, crate::metadata::HOME_REVISION_KEY)
        .map_err(|source| fjall_storage(ReadStage::HomeRevision, source))?
        .ok_or_else(|| invalid_revision("home", "home revision record is missing"))?;
    if usize::try_from(point.stored_value_len()).ok() != Some(HOME_REVISION_BYTES) {
        return Err(invalid_revision(
            "home",
            "home revision record has an invalid stored length",
        ));
    }
    let pair = point
        .acquire()
        .map_err(|source| fjall_storage(ReadStage::HomeRevision, source))?;
    decode_home_revision(pair.value()).map_err(|source| ReadError::InvalidRevisionMetadata {
        kind: "home",
        source: Box::new(source),
    })
}

pub(crate) fn read_domain_metadata(
    snapshot: &Snapshot,
    keyspace: &fjall::Keyspace,
    domain: &'static str,
) -> Result<DomainMetadata, ReadError> {
    let point = snapshot
        .point(keyspace, domain.as_bytes())
        .map_err(|source| fjall_storage(ReadStage::DomainRevision, source))?
        .ok_or_else(|| invalid_revision("domain", "domain registration record is missing"))?;
    let stored_value_len = usize::try_from(point.stored_value_len())
        .expect("u32 always fits usize on supported targets");
    if stored_value_len > MAX_DOMAIN_METADATA_BYTES {
        return Err(invalid_revision(
            "domain",
            "domain registration record exceeds its stored byte bound",
        ));
    }
    let pair = point
        .acquire()
        .map_err(|source| fjall_storage(ReadStage::DomainRevision, source))?;
    DomainMetadata::decode(pair.value()).map_err(|source| ReadError::InvalidRevisionMetadata {
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

fn fjall_storage(stage: ReadStage, source: fjall::Error) -> ReadError {
    storage(stage, ClassifiedFjallError::direct(source))
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
        | ReadError::CodecTypeMismatch { .. }
        | ReadError::InvalidCodecContract { .. }
        | ReadError::InvalidKeySize { .. }
        | ReadError::ReversedRange { .. }
        | ReadError::BoundExceeded { .. } => None,
        ReadError::Storage { source, .. } => match source.downcast_ref::<ClassifiedFjallError>() {
            Some(source) => source.severity(),
            None => Some(FailureSeverity::Structural),
        },
        ReadError::GenerationPoisoned
        | ReadError::InvalidStoredKeySize { .. }
        | ReadError::InvalidStoredValueSize { .. }
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
