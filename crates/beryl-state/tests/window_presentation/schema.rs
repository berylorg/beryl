use std::{convert::Infallible, io};

use super::*;
use beryl_home_store::{
    DomainCallbackError, DomainCallbackSource, DomainMutation, DomainReader, DomainSchemaVersion,
    HomeDomainRequirements, HomeOpenCandidate, HomeOpenOptions, HomeSchemaVersion,
    KeyspaceSchemaVersion, MutationBuildError, MutationBuilder, PointReadLimit,
    ReconciliationReservation, RecordCodec, RecordFamily, RecordVersion, StorageDomain,
};

struct RawDomain;
struct RawCodec;

impl StorageDomain for RawDomain {
    const NAME: &'static str = "beryl-window-presentation";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] =
        &[RecordFamily::new::<RawCodec>(KeyspaceSchemaVersion::new(1))];
    type ValidationError = Infallible;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = Infallible;
    fn create_runtime_attachment(
        _: &beryl_home_store::DomainRegistrationReader<'_, Self>,
    ) -> Result<(), Infallible> {
        Ok(())
    }
    fn validate(_: &DomainReader<'_, Self>) -> Result<(), Infallible> {
        Ok(())
    }
}

impl RecordCodec<RawDomain> for RawCodec {
    type Key = Vec<u8>;
    type Value = Vec<u8>;
    type Error = io::Error;
    const FAMILY: &'static str = "windows";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 16;
    const MAX_VALUE_BYTES: usize = 28;
    fn encode_key(value: &Vec<u8>) -> Result<Vec<u8>, io::Error> {
        Ok(value.clone())
    }
    fn decode_key(value: &[u8]) -> Result<Vec<u8>, io::Error> {
        Ok(value.to_vec())
    }
    fn encode_value(value: &Vec<u8>) -> Result<Vec<u8>, io::Error> {
        Ok(value.clone())
    }
    fn decode_value(value: &[u8]) -> Result<Vec<u8>, io::Error> {
        Ok(value.to_vec())
    }
}

struct RawWrite(Vec<u8>, Vec<u8>);
#[derive(Debug)]
struct RawWriteError(MutationBuildError);
impl std::fmt::Display for RawWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for RawWriteError {}
impl From<MutationBuildError> for RawWriteError {
    fn from(value: MutationBuildError) -> Self {
        Self(value)
    }
}
impl DomainCallbackError for RawWriteError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        Err(self)
    }
}
impl DomainMutation<RawDomain> for RawWrite {
    type Error = RawWriteError;
    type Prepared = Self;
    fn prepare(self, _: &DomainReader<'_, RawDomain>) -> Result<Self, Self::Error> {
        Ok(self)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, RawDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<RawCodec>(1)?;
        Ok(())
    }
    fn contribute(
        value: Self,
        mutations: &mut MutationBuilder<'_, RawDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<RawCodec>(&value.0, &value.1)?;
        Ok(())
    }
}

fn raw_home(
    path: &std::path::Path,
) -> (
    beryl_home_store::HomeStore,
    beryl_home_store::DomainHandle<RawDomain>,
) {
    let mut candidate =
        HomeOpenCandidate::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap();
    let raw = candidate.register_domain::<RawDomain>().unwrap();
    let home = candidate
        .prepare_publication(
            HomeDomainRequirements::new()
                .with_domain::<RawDomain>()
                .unwrap(),
        )
        .unwrap()
        .publish()
        .unwrap();
    (home, raw)
}

#[test]
fn height_codec_preserves_admitted_bits_and_refuses_invalid_scalars_and_shapes() {
    for value in [f32::from_bits(1), 1.0, 144.125, f32::MAX] {
        let directory = tempfile::tempdir().unwrap();
        let (home, state) = support::open(directory.path());
        let session = state.session();
        seed(&home, &session, 1);
        let window = snapshot(&home, &session).windows()[0].window_id();
        height(&home, &session, window, value);
        home.close().unwrap();
        let (home, raw) = raw_home(directory.path());
        let bytes = home
            .read_point::<RawDomain, RawCodec>(
                &raw,
                &window.as_bytes().to_vec(),
                PointReadLimit::new(32).unwrap(),
            )
            .unwrap()
            .unwrap();
        assert_eq!(bytes.len(), 28);
        assert_eq!(&bytes[..16], window.as_bytes());
        assert_eq!(&bytes[16..20], &value.to_bits().to_le_bytes());
        assert_eq!(&bytes[20..], &1u64.to_le_bytes());
        home.close().unwrap();
        let (home, state) = support::open(directory.path());
        assert_eq!(
            state
                .session()
                .window_presentation(&home, window)
                .unwrap()
                .unwrap()
                .activity_panel_height()
                .get()
                .to_bits(),
            value.to_bits()
        );
        home.close().unwrap();
    }
    let window = WindowId::from_bytes([7; 16]);
    let mut encoded = window.as_bytes().to_vec();
    encoded.extend_from_slice(&144.125f32.to_bits().to_le_bytes());
    encoded.extend_from_slice(&1u64.to_le_bytes());
    let mut malformed = Vec::new();
    for invalid in [0.0, -0.0, -1.0, f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
        assert!(ActivityPanelHeight::new(invalid).is_err());
        let mut bytes = encoded.clone();
        bytes[16..20].copy_from_slice(&invalid.to_bits().to_le_bytes());
        malformed.push((window.as_bytes().to_vec(), bytes));
    }
    let mut zero_revision = encoded.clone();
    zero_revision[20..].fill(0);
    malformed.push((window.as_bytes().to_vec(), zero_revision));
    malformed.push((window.as_bytes().to_vec(), encoded[..27].to_vec()));
    malformed.push((window.as_bytes()[..15].to_vec(), encoded.clone()));
    for (key, bytes) in malformed {
        let directory = tempfile::tempdir().unwrap();
        let (home, raw) = raw_home(directory.path());
        let mut command = HomeCommand::new(home.home_revision().unwrap());
        command
            .add(raw.contribution(home.domain_revision(&raw).unwrap(), RawWrite(key, bytes)))
            .unwrap();
        committed(home.execute(command));
        home.close().unwrap();
        let mut candidate = HomeOpenCandidate::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap();
        let error = match BerylState::register_with_schema_validation(&mut candidate) {
            Ok(_) => panic!("malformed presentation unexpectedly registered"),
            Err(error) => error,
        };
        let beryl_state::BerylStateRegistrationError::Domain { source, .. } = error else {
            panic!("unexpected registration refusal: {error}")
        };
        assert!(matches!(
            source,
            beryl_home_store::DomainRegistrationError::ValidationAccess {
                source: beryl_home_store::DomainCallbackSource::Read(_),
                ..
            }
        ));
        candidate.close().unwrap();
    }
    let directory = tempfile::tempdir().unwrap();
    let (home, raw) = raw_home(directory.path());
    let mut trailing = encoded;
    trailing.push(0);
    let mut command = HomeCommand::new(home.home_revision().unwrap());
    command
        .add(raw.contribution(
            home.domain_revision(&raw).unwrap(),
            RawWrite(window.as_bytes().to_vec(), trailing),
        ))
        .unwrap();
    assert!(matches!(
        home.execute(command),
        CommandOutcome::NotCommitted { .. }
    ));
    home.close().unwrap();
}
