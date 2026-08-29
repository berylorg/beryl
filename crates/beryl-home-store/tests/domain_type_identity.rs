use std::{convert::Infallible, error::Error, fmt};

use beryl_home_store::{
    DomainCallbackError, DomainCallbackSource, DomainHandleError, DomainMutation, DomainReader,
    DomainRegistrationError, DomainSchemaVersion, HomeCommand, HomeHealthState, HomeOpenOptions,
    HomeSchemaVersion, HomeStore, KeyspaceSchemaVersion, MutationBuildError, MutationBuilder,
    PointReadLimit, ReadError, RecordCodec, RecordFamily, RecordVersion, StorageDomain,
};
use tempfile::tempdir;

struct OwnerDomain;
struct ImpostorDomain;
struct OwnerCodec;
struct AliasCodec;
struct ImpostorCodec;

macro_rules! byte_codec {
    ($codec:ident, $domain:ident) => {
        impl RecordCodec<$domain> for $codec {
            type Key = u8;
            type Value = u8;
            type Error = Infallible;

            const FAMILY: &'static str = "records";
            const VERSION: RecordVersion = RecordVersion::new(1);
            const MAX_KEY_BYTES: usize = 1;
            const MAX_VALUE_BYTES: usize = 1;

            fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
                Ok(vec![*key])
            }

            fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
                Ok(encoded[0])
            }

            fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
                Ok(vec![*value])
            }

            fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
                Ok(encoded[0])
            }
        }
    };
}

byte_codec!(OwnerCodec, OwnerDomain);
byte_codec!(AliasCodec, OwnerDomain);
byte_codec!(ImpostorCodec, ImpostorDomain);

impl StorageDomain for OwnerDomain {
    const NAME: &'static str = "typed_owner";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<OwnerCodec>(
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

impl StorageDomain for ImpostorDomain {
    const NAME: &'static str = "typed_owner";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<ImpostorCodec>(
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

#[derive(Debug)]
struct MutationError(MutationBuildError);

impl fmt::Display for MutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Error for MutationError {}

impl DomainCallbackError for MutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        Err(self)
    }
}

struct AliasPut;

impl DomainMutation<OwnerDomain> for AliasPut {
    type Error = MutationError;

    fn validate(&self, _reader: &DomainReader<'_, OwnerDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, OwnerDomain>,
    ) -> Result<(), Self::Error> {
        reservation
            .reserve_records::<AliasCodec>(1)
            .map_err(MutationError)?;
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, OwnerDomain>,
        mutations: &mut MutationBuilder<'_, OwnerDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<AliasCodec>(&1, &1).map_err(MutationError)
    }
}

#[test]
fn stable_names_cannot_alias_live_domain_or_family_rust_owners() {
    let directory = tempdir().unwrap();
    let mut store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let owner = store.register_domain::<OwnerDomain>().unwrap();

    assert!(matches!(
        store.register_domain::<ImpostorDomain>(),
        Err(DomainRegistrationError::OwnerTypeMismatch {
            domain: "typed_owner"
        })
    ));
    assert!(matches!(
        store.domain_handle::<ImpostorDomain>(),
        Err(DomainHandleError::OwnerTypeMismatch {
            domain: "typed_owner"
        })
    ));
    assert!(matches!(
        store.read_point::<OwnerDomain, AliasCodec>(&owner, &1, PointReadLimit::new(5).unwrap(),),
        Err(ReadError::CodecTypeMismatch {
            domain: "typed_owner",
            family: "records"
        })
    ));

    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(owner.contribution(store.domain_revision(&owner).unwrap(), AliasPut))
        .unwrap();
    let error = match store.execute(command) {
        beryl_home_store::CommandOutcome::NotCommitted { evidence } => evidence,
        other => panic!("expected definitive non-commit, got {other:?}"),
    };
    assert!(
        error
            .to_string()
            .contains("record codec does not own family `records`")
    );
    assert_eq!(store.health().state(), HomeHealthState::Healthy);
}
