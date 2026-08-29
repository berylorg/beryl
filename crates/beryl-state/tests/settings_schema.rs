mod support;

use std::{convert::Infallible, error::Error, fmt};

use beryl_home_store::{
    CodecOperation, CommandOutcome, DomainCallbackError, DomainCallbackSource, DomainMutation,
    DomainReader, DomainRegistrationError, DomainSchemaVersion, HomeCommand, HomeOpenOptions,
    HomeSchemaVersion, HomeStore, KeyspaceSchemaVersion, MutationBuildError, MutationBuilder,
    ReadError, ReconciliationReservation, RecordCodec, RecordFamily, RecordVersion, StorageDomain,
};
use beryl_state::{BerylState, BerylStateRegistrationError};
use tempfile::tempdir;

const SETTINGS_FAMILIES: &[RecordFamily<RawSettingsDomain>] = &[RecordFamily::new::<
    RawSettingRecordV1,
>(KeyspaceSchemaVersion::new(1))];

struct RawSettingsDomain;
struct RawSettingRecordV1;

impl StorageDomain for RawSettingsDomain {
    const NAME: &'static str = "beryl-settings";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = SETTINGS_FAMILIES;
    type ValidationError = Infallible;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = std::convert::Infallible;

    fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(_reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        Ok(())
    }
}

impl RecordCodec<RawSettingsDomain> for RawSettingRecordV1 {
    type Key = u8;
    type Value = Vec<u8>;
    type Error = ProbeCodecError;

    const FAMILY: &'static str = "records";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 1;
    const MAX_VALUE_BYTES: usize = 64 * 1024;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![*key])
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        encoded
            .first()
            .copied()
            .filter(|_| encoded.len() == 1)
            .ok_or(ProbeCodecError)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(value.clone())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        Ok(encoded.to_vec())
    }
}

#[derive(Debug)]
struct ProbeCodecError;

impl std::fmt::Display for ProbeCodecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("probe setting key is not one byte")
    }
}

impl std::error::Error for ProbeCodecError {}

struct PutRawSetting {
    key: u8,
    value: Vec<u8>,
}

#[derive(Debug)]
struct RawMutationError(MutationBuildError);

impl fmt::Display for RawMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Error for RawMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

impl From<MutationBuildError> for RawMutationError {
    fn from(source: MutationBuildError) -> Self {
        Self(source)
    }
}

impl DomainCallbackError for RawMutationError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        Err(self)
    }
}

impl DomainMutation<RawSettingsDomain> for PutRawSetting {
    type Error = RawMutationError;

    fn validate(&self, _reader: &DomainReader<'_, RawSettingsDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, RawSettingsDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<RawSettingRecordV1>(1)?;
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, RawSettingsDomain>,
        mutations: &mut MutationBuilder<'_, RawSettingsDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<RawSettingRecordV1>(&self.key, &self.value)?;
        Ok(())
    }
}

#[test]
fn routine_reopen_defers_unknown_setting_schema_but_typed_read_and_explicit_validation_reject_it() {
    let directory = tempdir().unwrap();
    let mut store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let raw = store.register_domain::<RawSettingsDomain>().unwrap();
    let raw_revision = store.domain_revision(&raw).unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(raw.contribution(
            raw_revision,
            PutRawSetting {
                key: 0,
                value: [vec![0], 2_u32.to_be_bytes().to_vec()].concat(),
            },
        ))
        .unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed settings fixture command, got {outcome:?}"),
    }
    store.close().unwrap();

    let mut store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = BerylState::register(&mut store)
        .expect("routine registration must not scan the dormant malformed setting");
    assert!(matches!(
        state
            .settings()
            .setting(&store, beryl_state::SettingKey::ActiveThemeId),
        Err(ReadError::Codec {
            domain: "beryl-settings",
            family: "records",
            operation: CodecOperation::DecodeValue,
            ..
        })
    ));
    store.close().unwrap();

    let mut store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let error = match BerylState::register_with_schema_validation(&mut store) {
        Err(error) => error,
        Ok(_) => panic!("unknown setting schema unexpectedly registered"),
    };
    let BerylStateRegistrationError::Domain { domain, source } = error else {
        panic!("expected domain registration failure, got {error}");
    };
    assert_eq!(domain, "beryl-settings");
    let DomainRegistrationError::ValidationAccess {
        source: DomainCallbackSource::Read(source),
        ..
    } = source
    else {
        panic!("expected settings validation-access failure, got {source}");
    };
    assert!(source.to_string().contains("uses schema 2"));
}
