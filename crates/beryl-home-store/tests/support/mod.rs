#![allow(dead_code)]

use std::{convert::Infallible, error::Error, fmt, marker::PhantomData, path::Path};

use beryl_home_store::{
    DomainMutation, DomainReader, DomainSchemaVersion, HomeOpenOptions, HomeSchemaVersion,
    HomeStore, KeyspaceFamily, KeyspaceSchemaVersion, MutationBuildError, MutationBuilder,
    PointReadLimit, ReadError, RecordCodec, RecordVersion, StorageDomain,
};

pub const FAMILIES_V1: &[KeyspaceFamily] = &[KeyspaceFamily::new(
    "records",
    KeyspaceSchemaVersion::new(1),
)];
pub const FAMILIES_V2: &[KeyspaceFamily] = &[KeyspaceFamily::new(
    "records",
    KeyspaceSchemaVersion::new(2),
)];
pub const DUPLICATE_FAMILIES: &[KeyspaceFamily] = &[
    KeyspaceFamily::new("records", KeyspaceSchemaVersion::new(1)),
    KeyspaceFamily::new("records", KeyspaceSchemaVersion::new(1)),
];

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
            const KEYSPACES: &'static [KeyspaceFamily] = FAMILIES_V1;
            type ValidationError = Infallible;

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
    const KEYSPACES: &'static [KeyspaceFamily] = FAMILIES_V1;
    type ValidationError = Infallible;

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

impl StorageDomain for AlphaFamilySchema2 {
    const NAME: &'static str = "alpha";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const KEYSPACES: &'static [KeyspaceFamily] = FAMILIES_V2;
    type ValidationError = Infallible;

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

impl StorageDomain for DuplicateFamilyDomain {
    const NAME: &'static str = "duplicates";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const KEYSPACES: &'static [KeyspaceFamily] = DUPLICATE_FAMILIES;
    type ValidationError = Infallible;

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

impl StorageDomain for EmptyDomain {
    const NAME: &'static str = "empty";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const KEYSPACES: &'static [KeyspaceFamily] = &[];
    type ValidationError = Infallible;

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

impl StorageDomain for ValidatedDomain {
    const NAME: &'static str = "validated";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const KEYSPACES: &'static [KeyspaceFamily] = FAMILIES_V1;
    type ValidationError = ValidatedDomainError;

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

    fn validate(&self, _reader: &DomainReader<'_, D>) -> Result<(), Self::Error> {
        if self.reject_validation {
            return Err(FixtureMutationError::Rejected("fixture validation failure"));
        }
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, D>,
        mutations: &mut MutationBuilder<'_, D>,
    ) -> Result<(), Self::Error> {
        if self.reject_assembly {
            return Err(FixtureMutationError::Rejected("fixture assembly failure"));
        }
        mutations.put::<R>(&self.key, &self.value)?;
        Ok(())
    }
}

pub fn open_home(path: &Path) -> HomeStore {
    HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap()
}
