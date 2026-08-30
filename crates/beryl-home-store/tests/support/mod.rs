#![allow(dead_code)]

use std::{convert::Infallible, error::Error, fmt, marker::PhantomData, path::Path};

use beryl_home_store::{
    CommandError, CommandOutcome, CommitReceipt, DomainCallbackError, DomainCallbackSource,
    DomainMutation, DomainReader, DomainSchemaVersion, HomeOpenOptions, HomeSchemaVersion,
    HomeStore, KeyspaceSchemaVersion, MutationBuildError, MutationBuilder, PointReadLimit,
    ReadError, ReconciliationReservation, RecordCodec, RecordFamily, RecordVersion, StorageDomain,
};

pub fn committed(outcome: CommandOutcome) -> CommitReceipt {
    match outcome {
        CommandOutcome::Committed {
            receipt,
            later_failure: None,
        } => receipt,
        CommandOutcome::Committed {
            later_failure: Some(error),
            ..
        } => panic!("command committed with unexpected later failure: {error}"),
        CommandOutcome::NotCommitted { evidence } => {
            panic!("command unexpectedly did not commit: {evidence}")
        }
        CommandOutcome::Indeterminate {
            failure,
            reconciliation: _,
        } => panic!("command outcome is unexpectedly indeterminate: {failure}"),
    }
}

pub fn not_committed(outcome: CommandOutcome) -> CommandError {
    match outcome {
        CommandOutcome::NotCommitted { evidence } => evidence,
        CommandOutcome::Committed {
            receipt: _,
            later_failure,
        } => panic!("command unexpectedly committed: {later_failure:?}"),
        CommandOutcome::Indeterminate {
            failure,
            reconciliation: _,
        } => panic!("command outcome is unexpectedly indeterminate: {failure}"),
    }
}

pub struct AlphaDomain;
pub struct BetaDomain;
pub struct AlphaDomainSchema2;
pub struct AlphaFamilySchema2;
pub struct DuplicateFamilyDomain;
pub struct EmptyDomain;
pub struct ValidatedDomain;

macro_rules! simple_domain {
    ($name:ident, $stable:literal) => {
        impl StorageDomain for $name {
            const NAME: &'static str = $stable;
            const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
            const FAMILIES: &'static [RecordFamily<Self>] =
                &[RecordFamily::new::<BytesRecord<Self>>(
                    KeyspaceSchemaVersion::new(1),
                )];
            type ValidationError = Infallible;
            type RuntimeAttachment = ();
            type RuntimeAttachmentError = Infallible;

            fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
                Ok(())
            }

            fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
                Ok(())
            }
        }
    };
}

simple_domain!(AlphaDomain, "alpha");
simple_domain!(BetaDomain, "beta");

impl StorageDomain for AlphaDomainSchema2 {
    const NAME: &'static str = "alpha";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(2);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<BytesRecord<Self>>(
        KeyspaceSchemaVersion::new(1),
    )];
    type ValidationError = Infallible;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = Infallible;

    fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

impl StorageDomain for AlphaFamilySchema2 {
    const NAME: &'static str = "alpha";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<BytesRecord<Self>>(
        KeyspaceSchemaVersion::new(2),
    )];
    type ValidationError = Infallible;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = Infallible;

    fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

impl StorageDomain for DuplicateFamilyDomain {
    const NAME: &'static str = "duplicates";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[
        RecordFamily::new::<BytesRecord<Self>>(KeyspaceSchemaVersion::new(1)),
        RecordFamily::new::<BytesRecord<Self>>(KeyspaceSchemaVersion::new(1)),
    ];
    type ValidationError = Infallible;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = Infallible;

    fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

impl StorageDomain for EmptyDomain {
    const NAME: &'static str = "empty";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[];
    type ValidationError = Infallible;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = Infallible;

    fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

#[derive(Debug)]
pub enum ValidatedDomainError {
    Read(ReadError),
    Rejected,
}

impl fmt::Display for ValidatedDomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(source) => source.fmt(formatter),
            Self::Rejected => formatter.write_str("fixture validator rejected marker"),
        }
    }
}

impl Error for ValidatedDomainError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(source) => Some(source),
            Self::Rejected => None,
        }
    }
}

impl DomainCallbackError for ValidatedDomainError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            semantic => Err(semantic),
        }
    }
}

impl StorageDomain for ValidatedDomain {
    const NAME: &'static str = "validated";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<BytesRecord<Self>>(
        KeyspaceSchemaVersion::new(1),
    )];
    type ValidationError = ValidatedDomainError;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = Infallible;

    fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        let value = reader
            .point::<BytesRecord<Self>>(&1, PointReadLimit::new(1_028).unwrap())
            .map_err(ValidatedDomainError::Read)?;
        if value.as_deref() == Some(b"reject") {
            return Err(ValidatedDomainError::Rejected);
        }
        Ok(())
    }
}

pub struct BytesRecord<D>(PhantomData<fn() -> D>);
pub struct BytesRecordV2<D>(PhantomData<fn() -> D>);

fn decode_u64(encoded: &[u8]) -> Result<u64, FixtureCodecError> {
    let bytes: [u8; 8] = encoded
        .try_into()
        .map_err(|_| FixtureCodecError("fixture key is not eight bytes"))?;
    Ok(u64::from_be_bytes(bytes))
}

#[derive(Debug)]
pub struct FixtureCodecError(pub &'static str);

impl fmt::Display for FixtureCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for FixtureCodecError {}

macro_rules! bytes_codec {
    ($codec:ident, $version:literal) => {
        impl<D: StorageDomain> RecordCodec<D> for $codec<D> {
            type Key = u64;
            type Value = Vec<u8>;
            type Error = FixtureCodecError;

            const FAMILY: &'static str = "records";
            const VERSION: RecordVersion = RecordVersion::new($version);
            const MAX_KEY_BYTES: usize = 8;
            const MAX_VALUE_BYTES: usize = 1_024;

            fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
                Ok(key.to_be_bytes().to_vec())
            }

            fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
                decode_u64(encoded)
            }

            fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
                Ok(value.clone())
            }

            fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
                Ok(encoded.to_vec())
            }
        }
    };
}

bytes_codec!(BytesRecord, 1);
bytes_codec!(BytesRecordV2, 2);

#[derive(Debug)]
pub enum FixtureMutationError {
    Build(MutationBuildError),
    Rejected(&'static str),
}

impl fmt::Display for FixtureMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Build(source) => source.fmt(formatter),
            Self::Rejected(message) => formatter.write_str(message),
        }
    }
}

impl Error for FixtureMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Build(source) => Some(source),
            Self::Rejected(_) => None,
        }
    }
}

impl DomainCallbackError for FixtureMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        Err(self)
    }
}

impl From<MutationBuildError> for FixtureMutationError {
    fn from(source: MutationBuildError) -> Self {
        Self::Build(source)
    }
}

pub struct PutBytes<D, R = BytesRecord<D>> {
    pub key: u64,
    pub value: Vec<u8>,
    pub reject_validation: bool,
    pub reject_assembly: bool,
    _typed: PhantomData<fn() -> (D, R)>,
}

impl<D, R> PutBytes<D, R> {
    pub fn new(key: u64, value: impl Into<Vec<u8>>) -> Self {
        Self {
            key,
            value: value.into(),
            reject_validation: false,
            reject_assembly: false,
            _typed: PhantomData,
        }
    }

    pub fn rejecting_validation(mut self) -> Self {
        self.reject_validation = true;
        self
    }

    pub fn rejecting_assembly(mut self) -> Self {
        self.reject_assembly = true;
        self
    }
}

impl<D, R> DomainMutation<D> for PutBytes<D, R>
where
    D: StorageDomain,
    R: RecordCodec<D, Key = u64, Value = Vec<u8>>,
{
    type Error = FixtureMutationError;
    type Prepared = Self;

    fn prepare(self, _reader: &DomainReader<'_, D>) -> Result<Self::Prepared, Self::Error> {
        if self.reject_validation {
            return Err(FixtureMutationError::Rejected("fixture validation failure"));
        }
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, D>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<R>(1)?;
        Ok(())
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, D>,
    ) -> Result<(), Self::Error> {
        if prepared.reject_assembly {
            return Err(FixtureMutationError::Rejected("fixture assembly failure"));
        }
        mutations.put::<R>(&prepared.key, &prepared.value)?;
        Ok(())
    }
}

pub fn open_home(path: &Path) -> HomeStore {
    HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap()
}
