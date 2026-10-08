use std::{convert::Infallible, error::Error, fmt};

use beryl_home_store::{
    DomainCallbackError, DomainCallbackSource, DomainHandle, DomainMutation, DomainReader,
    DomainSchemaVersion, KeyspaceSchemaVersion, MutationBuildError, MutationBuilder,
    PointReadLimit, ReadError, ReconciliationReservation, RecordCodec, RecordFamily, RecordVersion,
    StorageDomain,
};

use super::*;

pub(super) struct SourceDomain;
pub(super) struct SourceRecord;

impl StorageDomain for SourceDomain {
    const NAME: &'static str = "catalog-invalidation-source";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = &[RecordFamily::new::<SourceRecord>(
        KeyspaceSchemaVersion::new(1),
    )];
    type ValidationError = SourceError;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = Infallible;

    fn create_runtime_attachment(
        _: &beryl_home_store::DomainRegistrationReader<'_, Self>,
    ) -> Result<(), Infallible> {
        Ok(())
    }

    fn validate(_: &DomainReader<'_, Self>) -> Result<(), SourceError> {
        Ok(())
    }

    fn reconcile(
        reader: &beryl_home_store::ReconciliationReader<'_, Self>,
    ) -> Result<DomainReconciliation, SourceError> {
        let mut classification = crate::reconciliation::ReconciliationClassification::new();
        crate::reconciliation::classify_records::<Self, SourceRecord>(reader, &mut classification)
            .map_err(SourceError::Read)?;
        Ok(classification.finish())
    }
}

impl RecordCodec<SourceDomain> for SourceRecord {
    type Key = u8;
    type Value = u8;
    type Error = Infallible;
    const FAMILY: &'static str = "source";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 1;
    const MAX_VALUE_BYTES: usize = 1;

    fn encode_key(key: &u8) -> Result<Vec<u8>, Infallible> {
        Ok(vec![*key])
    }
    fn decode_key(bytes: &[u8]) -> Result<u8, Infallible> {
        Ok(bytes[0])
    }
    fn encode_value(value: &u8) -> Result<Vec<u8>, Infallible> {
        Ok(vec![*value])
    }
    fn decode_value(bytes: &[u8]) -> Result<u8, Infallible> {
        Ok(bytes[0])
    }
}

#[derive(Debug)]
pub(super) enum SourceError {
    Read(ReadError),
    Build(MutationBuildError),
    Changed {
        expected: Option<u8>,
        actual: Option<u8>,
    },
}

impl fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => write!(formatter, "{error}"),
            Self::Build(error) => write!(formatter, "{error}"),
            Self::Changed { expected, actual } => write!(
                formatter,
                "source changed: expected {expected:?}, found {actual:?}"
            ),
        }
    }
}

impl Error for SourceError {}

impl DomainCallbackError for SourceError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        match self {
            Self::Read(source) => Ok(DomainCallbackSource::Read(source)),
            semantic => Err(semantic),
        }
    }
}

pub(super) struct ChangeSource {
    pub(super) key: u8,
    pub(super) expected: Option<u8>,
    pub(super) next: u8,
}

impl DomainMutation<SourceDomain> for ChangeSource {
    type Error = SourceError;
    type Prepared = Self;

    fn prepare(self, reader: &DomainReader<'_, SourceDomain>) -> Result<Self, SourceError> {
        let actual = reader
            .point::<SourceRecord>(&self.key, PointReadLimit::new(16).unwrap())
            .map_err(SourceError::Read)?;
        if actual != self.expected {
            return Err(SourceError::Changed {
                expected: self.expected,
                actual,
            });
        }
        Ok(self)
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SourceDomain>,
    ) -> Result<(), SourceError> {
        reservation
            .reserve_records::<SourceRecord>(1)
            .map_err(SourceError::Build)
    }

    fn contribute(
        prepared: Self,
        builder: &mut MutationBuilder<'_, SourceDomain>,
    ) -> Result<(), SourceError> {
        builder
            .put::<SourceRecord>(&prepared.key, &prepared.next)
            .map_err(SourceError::Build)
    }
}

pub(super) fn open_source(
    path: &std::path::Path,
    faults: FaultController,
) -> (HomeStore, CatalogState, DomainHandle<SourceDomain>) {
    let mut candidate = HomeOpenCandidate::open_with_faults(
        HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT),
        faults,
    )
    .unwrap();
    let catalog = CatalogState::register(&mut candidate).unwrap();
    let source = candidate.register_domain::<SourceDomain>().unwrap();
    let store = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<CatalogDomain>()
                .unwrap()
                .with_domain::<SourceDomain>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    (store, catalog, source)
}

pub(super) fn value(store: &HomeStore, source: &DomainHandle<SourceDomain>, key: u8) -> Option<u8> {
    store
        .read_point::<SourceDomain, SourceRecord>(source, &key, PointReadLimit::new(16).unwrap())
        .unwrap()
}

pub(super) fn recover_source(
    store: HomeStore,
) -> (HomeStore, CatalogState, DomainHandle<SourceDomain>) {
    assert_eq!(
        store.health().state(),
        beryl_home_store::HomeHealthState::Failed
    );
    let recovery = store.recover_same_home().unwrap();
    let catalog = CatalogState::reacquire_candidate(&recovery).unwrap();
    let source = recovery.domain_handle::<SourceDomain>().unwrap();
    (recovery.publish().unwrap(), catalog, source)
}
