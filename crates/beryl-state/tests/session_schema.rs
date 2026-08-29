mod support;

use std::{convert::Infallible, error::Error, fmt};

use beryl_home_store::{
    CommandOutcome, DomainCallbackError, DomainCallbackSource, DomainMutation, DomainReader,
    DomainRegistrationError, DomainSchemaVersion, HomeCommand, HomeOpenOptions, HomeSchemaVersion,
    HomeStore, KeyspaceSchemaVersion, MutationBuildError, MutationBuilder, PointReadLimit,
    ReconciliationReservation, RecordCodec, RecordFamily, RecordVersion, StorageDomain,
};
use beryl_model::{RootId, RuntimeId};
use beryl_state::{
    BeginSessionRestore, BerylState, BerylStateBootstrap, BerylStateRegistrationError,
    RememberedTarget, SESSION_HEADER_V1_BYTES, SESSION_WINDOW_V1_BYTES,
};
use tempfile::tempdir;

const RAW_SESSION_FAMILIES: &[RecordFamily<RawSessionDomain>] = &[
    RecordFamily::new::<RawHeaderCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<RawWindowCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<RawClaimByWindowCodec>(KeyspaceSchemaVersion::new(1)),
    RecordFamily::new::<RawClaimByThreadCodec>(KeyspaceSchemaVersion::new(1)),
];

struct RawSessionDomain;
struct RawHeaderCodec;
struct RawWindowCodec;
struct RawClaimByWindowCodec;
struct RawClaimByThreadCodec;

impl StorageDomain for RawSessionDomain {
    const NAME: &'static str = "beryl-session";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = RAW_SESSION_FAMILIES;
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

#[derive(Debug)]
struct RawCodecError;

impl std::fmt::Display for RawCodecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("invalid raw session fixture")
    }
}

impl std::error::Error for RawCodecError {}

impl RecordCodec<RawSessionDomain> for RawHeaderCodec {
    type Key = u8;
    type Value = Vec<u8>;
    type Error = RawCodecError;

    const FAMILY: &'static str = "active-header";
    const VERSION: RecordVersion = RecordVersion::new(1);
    const MAX_KEY_BYTES: usize = 1;
    const MAX_VALUE_BYTES: usize = SESSION_HEADER_V1_BYTES;

    fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
        Ok(vec![*key])
    }

    fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
        encoded
            .first()
            .copied()
            .filter(|_| encoded.len() == 1)
            .ok_or(RawCodecError)
    }

    fn encode_value(value: &Self::Value) -> Result<Vec<u8>, Self::Error> {
        Ok(value.clone())
    }

    fn decode_value(encoded: &[u8]) -> Result<Self::Value, Self::Error> {
        Ok(encoded.to_vec())
    }
}

macro_rules! raw_identity_codec {
    ($codec:ident, $family:literal, $maximum:expr) => {
        impl RecordCodec<RawSessionDomain> for $codec {
            type Key = [u8; 16];
            type Value = Vec<u8>;
            type Error = RawCodecError;

            const FAMILY: &'static str = $family;
            const VERSION: RecordVersion = RecordVersion::new(1);
            const MAX_KEY_BYTES: usize = 16;
            const MAX_VALUE_BYTES: usize = $maximum;

            fn encode_key(key: &Self::Key) -> Result<Vec<u8>, Self::Error> {
                Ok(key.to_vec())
            }

            fn decode_key(encoded: &[u8]) -> Result<Self::Key, Self::Error> {
                encoded.try_into().map_err(|_| RawCodecError)
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

raw_identity_codec!(RawWindowCodec, "windows", SESSION_WINDOW_V1_BYTES);
raw_identity_codec!(RawClaimByWindowCodec, "claims-by-window", 49);
raw_identity_codec!(RawClaimByThreadCodec, "claims-by-thread", 49);

#[derive(Default)]
struct RawMutation {
    header: Option<Vec<u8>>,
    windows: Vec<([u8; 16], Vec<u8>)>,
    by_window: Vec<([u8; 16], Vec<u8>)>,
    by_thread: Vec<([u8; 16], Vec<u8>)>,
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

impl DomainMutation<RawSessionDomain> for RawMutation {
    type Error = RawMutationError;

    fn validate(&self, _reader: &DomainReader<'_, RawSessionDomain>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, RawSessionDomain>,
    ) -> Result<(), Self::Error> {
        if self.header.is_some() {
            reservation.reserve_records::<RawHeaderCodec>(1)?;
        }
        if !self.windows.is_empty() {
            reservation.reserve_records::<RawWindowCodec>(self.windows.len())?;
        }
        if !self.by_window.is_empty() {
            reservation.reserve_records::<RawClaimByWindowCodec>(self.by_window.len())?;
        }
        if !self.by_thread.is_empty() {
            reservation.reserve_records::<RawClaimByThreadCodec>(self.by_thread.len())?;
        }
        Ok(())
    }

    fn contribute(
        &self,
        _reader: &DomainReader<'_, RawSessionDomain>,
        mutations: &mut MutationBuilder<'_, RawSessionDomain>,
    ) -> Result<(), Self::Error> {
        if let Some(header) = &self.header {
            mutations.put::<RawHeaderCodec>(&0, header)?;
        }
        for (key, value) in &self.windows {
            mutations.put::<RawWindowCodec>(key, value)?;
        }
        for (key, value) in &self.by_window {
            mutations.put::<RawClaimByWindowCodec>(key, value)?;
        }
        for (key, value) in &self.by_thread {
            mutations.put::<RawClaimByThreadCodec>(key, value)?;
        }
        Ok(())
    }
}

fn write_raw(path: &std::path::Path, mutation: RawMutation) {
    let mut store =
        HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap();
    let raw = store.register_domain::<RawSessionDomain>().unwrap();
    let raw_revision = store.domain_revision(&raw).unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(raw.contribution(raw_revision, mutation))
        .unwrap();
    match store.execute(command) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session fixture command, got {outcome:?}"),
    }
    store.close().unwrap();
}

fn header_bytes(
    revision: u64,
    fallback: Option<RememberedTarget>,
    windows: &[([u8; 16], u64)],
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(SESSION_HEADER_V1_BYTES);
    bytes.extend_from_slice(&revision.to_be_bytes());
    bytes.push(0);
    match fallback {
        Some(target) => {
            bytes.push(1);
            bytes.extend_from_slice(target.runtime_id().as_bytes());
            bytes.extend_from_slice(target.root_id().as_bytes());
        }
        None => {
            bytes.push(0);
            bytes.extend_from_slice(&[0; 32]);
        }
    }
    bytes.extend_from_slice(&(windows.len() as u16).to_be_bytes());
    for (window_id, record_revision) in windows {
        bytes.extend_from_slice(window_id);
        bytes.extend_from_slice(&record_revision.to_be_bytes());
    }
    for _ in windows.len()..256 {
        bytes.extend_from_slice(&[0; 24]);
    }
    assert_eq!(bytes.len(), SESSION_HEADER_V1_BYTES);
    bytes
}

fn window_bytes(
    window_id: [u8; 16],
    target: RememberedTarget,
    thread_id: [u8; 16],
    generation: u64,
    claim_revision: u64,
    record_revision: u64,
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(SESSION_WINDOW_V1_BYTES);
    bytes.extend_from_slice(&window_id);
    bytes.push(1);
    bytes.extend_from_slice(target.runtime_id().as_bytes());
    bytes.extend_from_slice(target.root_id().as_bytes());
    bytes.push(1);
    bytes.extend_from_slice(&thread_id);
    bytes.extend_from_slice(&generation.to_be_bytes());
    bytes.extend_from_slice(&claim_revision.to_be_bytes());
    bytes.extend_from_slice(&0_i32.to_be_bytes());
    bytes.extend_from_slice(&0_i32.to_be_bytes());
    bytes.extend_from_slice(&800_u32.to_be_bytes());
    bytes.extend_from_slice(&600_u32.to_be_bytes());
    bytes.push(0);
    bytes.push(0);
    bytes.extend_from_slice(&0_u16.to_be_bytes());
    bytes.extend_from_slice(&[0; 512]);
    bytes.extend_from_slice(&[0; 16]);
    bytes.push(0);
    bytes.extend_from_slice(&[0; 16]);
    bytes.extend_from_slice(&record_revision.to_be_bytes());
    assert_eq!(bytes.len(), SESSION_WINDOW_V1_BYTES);
    bytes
}

fn claim_bytes(
    window_id: [u8; 16],
    thread_id: [u8; 16],
    generation: u64,
    revision: u64,
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(49);
    bytes.extend_from_slice(&window_id);
    bytes.extend_from_slice(&thread_id);
    bytes.extend_from_slice(&generation.to_be_bytes());
    bytes.push(0);
    bytes.extend_from_slice(&revision.to_be_bytes());
    bytes
}

fn registration_error(path: &std::path::Path) -> BerylStateRegistrationError {
    let mut store =
        HomeStore::open(HomeOpenOptions::new(path, HomeSchemaVersion::CURRENT)).unwrap();
    match BerylState::register_with_schema_validation(&mut store) {
        Ok(_) => panic!("malformed session unexpectedly registered"),
        Err(error) => error,
    }
}

fn validation_message(error: &BerylStateRegistrationError) -> String {
    match error {
        BerylStateRegistrationError::Domain { source, .. } => match source {
            DomainRegistrationError::Validation { source, .. } => source.to_string(),
            DomainRegistrationError::ValidationAccess {
                source: DomainCallbackSource::Read(source),
                ..
            } => source.to_string(),
            other => panic!("expected session validation error, got {other}"),
        },
        other => panic!("expected domain registration error, got {other}"),
    }
}

#[test]
fn canonical_optional_padding_and_exact_fixed_record_sizes_are_enforced() {
    let directory = tempdir().unwrap();
    let mut header = header_bytes(1, None, &[]);
    header[10] = 1;
    write_raw(
        directory.path(),
        RawMutation {
            header: Some(header),
            ..RawMutation::default()
        },
    );
    let message = validation_message(&registration_error(directory.path()));
    assert!(message.contains("absent remembered target has nonzero padding"));
}

#[test]
fn active_selected_window_without_exact_reverse_claims_fails_reopen() {
    let directory = tempdir().unwrap();
    let window_id = [1; 16];
    let thread_id = [2; 16];
    let target = RememberedTarget::new(RuntimeId::from_bytes([3; 16]), RootId::from_bytes([4; 16]));
    write_raw(
        directory.path(),
        RawMutation {
            header: Some(header_bytes(5, Some(target), &[(window_id, 1)])),
            windows: vec![(
                window_id,
                window_bytes(window_id, target, thread_id, 5, 1, 1),
            )],
            ..RawMutation::default()
        },
    );
    let message = validation_message(&registration_error(directory.path()));
    assert!(message.contains("selected window has no forward claim"));
}

#[test]
fn paired_stale_claims_are_readable_and_begin_restore_deletes_both_copies() {
    let directory = tempdir().unwrap();
    let target = RememberedTarget::new(RuntimeId::from_bytes([3; 16]), RootId::from_bytes([4; 16]));
    let window_id = [8; 16];
    let thread_id = [9; 16];
    let claim = claim_bytes(window_id, thread_id, 4, 2);
    write_raw(
        directory.path(),
        RawMutation {
            header: Some(header_bytes(5, Some(target), &[])),
            by_window: vec![(window_id, claim.clone())],
            by_thread: vec![(thread_id, claim)],
            ..RawMutation::default()
        },
    );

    let mut store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let state = BerylStateBootstrap::register(&mut store).unwrap();
    let snapshot = state.session().minimal_bootstrap(&store).unwrap().unwrap();
    assert!(snapshot.windows().is_empty());
    match support::execute(
        &store,
        state.session().begin_restore(
            state.session().revision(&store).unwrap(),
            BeginSessionRestore::new(snapshot.header().revision()),
        ),
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed session restore command, got {outcome:?}"),
    }
    store.close().unwrap();

    let mut raw_store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let raw = raw_store.register_domain::<RawSessionDomain>().unwrap();
    assert!(
        raw_store
            .read_point::<RawSessionDomain, RawClaimByWindowCodec>(
                &raw,
                &window_id,
                PointReadLimit::new(53).unwrap(),
            )
            .unwrap()
            .is_none()
    );
    assert!(
        raw_store
            .read_point::<RawSessionDomain, RawClaimByThreadCodec>(
                &raw,
                &thread_id,
                PointReadLimit::new(53).unwrap(),
            )
            .unwrap()
            .is_none()
    );
}

#[test]
fn disagreeing_reverse_claim_copies_fail_validation() {
    let directory = tempdir().unwrap();
    let target = RememberedTarget::new(RuntimeId::from_bytes([3; 16]), RootId::from_bytes([4; 16]));
    let window_id = [1; 16];
    let thread_id = [2; 16];
    write_raw(
        directory.path(),
        RawMutation {
            header: Some(header_bytes(5, Some(target), &[(window_id, 1)])),
            windows: vec![(
                window_id,
                window_bytes(window_id, target, thread_id, 5, 1, 1),
            )],
            by_window: vec![(window_id, claim_bytes(window_id, thread_id, 5, 1))],
            by_thread: vec![(thread_id, claim_bytes(window_id, thread_id, 4, 1))],
        },
    );
    let message = validation_message(&registration_error(directory.path()));
    assert!(message.contains("claim reverse copies"));
}

#[test]
fn claim_generation_newer_than_the_active_header_fails_validation() {
    let directory = tempdir().unwrap();
    let target = RememberedTarget::new(RuntimeId::from_bytes([3; 16]), RootId::from_bytes([4; 16]));
    let window_id = [8; 16];
    let thread_id = [9; 16];
    let claim = claim_bytes(window_id, thread_id, 6, 1);
    write_raw(
        directory.path(),
        RawMutation {
            header: Some(header_bytes(5, Some(target), &[])),
            by_window: vec![(window_id, claim.clone())],
            by_thread: vec![(thread_id, claim)],
            ..RawMutation::default()
        },
    );
    let message = validation_message(&registration_error(directory.path()));
    assert!(message.contains("claim generation is newer"));
}

#[test]
fn a_257th_window_record_exceeds_the_hard_validation_bound() {
    let directory = tempdir().unwrap();
    let target = RememberedTarget::new(RuntimeId::from_bytes([3; 16]), RootId::from_bytes([4; 16]));
    let thread_id = [9; 16];
    let windows = (0_u16..=256)
        .map(|ordinal| {
            let mut window_id = [0; 16];
            window_id[14..].copy_from_slice(&ordinal.to_be_bytes());
            (
                window_id,
                window_bytes(window_id, target, thread_id, 1, 1, 1),
            )
        })
        .collect();
    write_raw(
        directory.path(),
        RawMutation {
            header: Some(header_bytes(1, None, &[])),
            windows,
            ..RawMutation::default()
        },
    );
    let message = validation_message(&registration_error(directory.path()));
    assert!(message.contains("more than 256 window records"));
}
