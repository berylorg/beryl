mod support;

use std::{error::Error, fmt};

use beryl_home_store::{
    CommandOutcome, CursorDirection, CursorRange, CursorReadLimits, DomainCallbackError,
    DomainCallbackSource, DomainReader, DomainSchemaVersion, HomeOpenOptions, HomeSchemaVersion,
    HomeStore, KeyspaceSchemaVersion, PointReadLimit, ReadError, RecordCodec, RecordFamily,
    RecordVersion, StorageDomain, WholeHomeScrubTrigger,
};
use beryl_model::{AdmittedHostPath, PathFlavor};
use beryl_state::{
    ApplySettings, ApplySettingsError, BerylState, ExpectedSettingRevision, RecordRevision,
    SettingKey, SettingUpdate, SettingValue, SettingValueError, SettingsMutationError,
};
use tempfile::tempdir;

use support::{contributor_source, execute, open};

fn create(key: SettingKey, value: SettingValue) -> SettingUpdate {
    SettingUpdate::new(key, ExpectedSettingRevision::Absent, value)
}

fn replace(key: SettingKey, revision: RecordRevision, value: SettingValue) -> SettingUpdate {
    SettingUpdate::new(key, ExpectedSettingRevision::Exact(revision), value)
}

fn apply(store: &HomeStore, state: &BerylState, updates: Vec<SettingUpdate>) -> CommandOutcome {
    let contribution = state.settings().apply(
        state.settings().revision(store).unwrap(),
        ApplySettings::new(updates).unwrap(),
    );
    execute(store, contribution)
}

#[test]
fn every_closed_scalar_shape_persists_and_reopens() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    let sound =
        AdmittedHostPath::from_admitted(PathFlavor::Windows, r"C:\Sounds\done.wav").unwrap();

    match apply(
        &store,
        &state,
        vec![
            create(
                SettingKey::ActiveThemeId,
                SettingValue::active_theme_id("beryl-dark").unwrap(),
            ),
            create(
                SettingKey::ContextCompactionTimeout,
                SettingValue::context_compaction_timeout_millis(45_000),
            ),
            create(
                SettingKey::DraftAutosaveInterval,
                SettingValue::draft_autosave_interval_seconds(30),
            ),
            create(
                SettingKey::DeveloperInstructions,
                SettingValue::developer_instructions("Keep answers concise.").unwrap(),
            ),
            create(
                SettingKey::EndTurnSound,
                SettingValue::end_turn_sound(Some(sound.clone())),
            ),
        ],
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed settings command, got {outcome:?}"),
    }
    store.close().unwrap();

    let (store, state) = open(directory.path());
    let settings = state.settings();
    assert_eq!(
        settings
            .setting(&store, SettingKey::ActiveThemeId)
            .unwrap()
            .unwrap()
            .value()
            .as_active_theme_id(),
        Some("beryl-dark")
    );
    assert_eq!(
        settings
            .setting(&store, SettingKey::ContextCompactionTimeout)
            .unwrap()
            .unwrap()
            .value()
            .as_context_compaction_timeout_millis(),
        Some(45_000)
    );
    assert_eq!(
        settings
            .setting(&store, SettingKey::DraftAutosaveInterval)
            .unwrap()
            .unwrap()
            .value()
            .as_draft_autosave_interval_seconds(),
        Some(30)
    );
    assert_eq!(
        settings
            .setting(&store, SettingKey::DeveloperInstructions)
            .unwrap()
            .unwrap()
            .value()
            .as_developer_instructions(),
        Some("Keep answers concise.")
    );
    let sound_record = settings
        .setting(&store, SettingKey::EndTurnSound)
        .unwrap()
        .unwrap();
    assert_eq!(sound_record.value().as_end_turn_sound(), Some(&Some(sound)));
    assert_eq!(sound_record.revision(), RecordRevision::INITIAL);

    let page = settings
        .list(
            &store,
            None,
            CursorReadLimits::new(10, 1024 * 1024).unwrap(),
        )
        .unwrap();
    assert_eq!(page.records().len(), 5);
    assert!(!page.has_more());
    assert!(page.stored_bytes() > 0);
    assert!(page.decoded_bytes() > 0);
}

#[test]
fn one_invalid_update_rejects_the_whole_apply() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    match apply(
        &store,
        &state,
        vec![create(
            SettingKey::ActiveThemeId,
            SettingValue::active_theme_id("old").unwrap(),
        )],
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed settings command, got {outcome:?}"),
    }
    let domain_before = state.settings().revision(&store).unwrap();

    let outcome = apply(
        &store,
        &state,
        vec![
            replace(
                SettingKey::ActiveThemeId,
                RecordRevision::INITIAL,
                SettingValue::active_theme_id("new").unwrap(),
            ),
            replace(
                SettingKey::DraftAutosaveInterval,
                RecordRevision::INITIAL,
                SettingValue::draft_autosave_interval_seconds(10),
            ),
        ],
    );
    let CommandOutcome::NotCommitted { evidence: error } = outcome else {
        panic!("expected rejected settings command, got {outcome:?}");
    };
    assert!(matches!(
        contributor_source::<SettingsMutationError>(&error),
        Some(SettingsMutationError::SettingMissing {
            key: SettingKey::DraftAutosaveInterval
        })
    ));
    assert_eq!(state.settings().revision(&store).unwrap(), domain_before);
    let active = state
        .settings()
        .setting(&store, SettingKey::ActiveThemeId)
        .unwrap()
        .unwrap();
    assert_eq!(active.value().as_active_theme_id(), Some("old"));
    assert_eq!(active.revision(), RecordRevision::INITIAL);
    assert!(
        state
            .settings()
            .setting(&store, SettingKey::DraftAutosaveInterval)
            .unwrap()
            .is_none()
    );
}

#[test]
fn record_revisions_advance_and_stale_updates_reject() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    match apply(
        &store,
        &state,
        vec![create(
            SettingKey::DraftAutosaveInterval,
            SettingValue::draft_autosave_interval_seconds(30),
        )],
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed settings command, got {outcome:?}"),
    }
    match apply(
        &store,
        &state,
        vec![replace(
            SettingKey::DraftAutosaveInterval,
            RecordRevision::INITIAL,
            SettingValue::draft_autosave_interval_seconds(45),
        )],
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed settings command, got {outcome:?}"),
    }
    let current = state
        .settings()
        .setting(&store, SettingKey::DraftAutosaveInterval)
        .unwrap()
        .unwrap();
    assert_eq!(current.revision().get(), 2);
    assert_eq!(
        current.value().as_draft_autosave_interval_seconds(),
        Some(45)
    );

    let outcome = apply(
        &store,
        &state,
        vec![replace(
            SettingKey::DraftAutosaveInterval,
            RecordRevision::INITIAL,
            SettingValue::draft_autosave_interval_seconds(60),
        )],
    );
    let CommandOutcome::NotCommitted { evidence: error } = outcome else {
        panic!("expected rejected settings command, got {outcome:?}");
    };
    assert!(matches!(
        contributor_source::<SettingsMutationError>(&error),
        Some(SettingsMutationError::RecordRevisionConflict {
            key: SettingKey::DraftAutosaveInterval,
            expected: RecordRevision::INITIAL,
            current
        }) if current.get() == 2
    ));
}

#[test]
fn apply_shape_rejects_empty_duplicate_mismatched_and_oversized_values() {
    assert!(matches!(
        ApplySettings::new(Vec::new()),
        Err(ApplySettingsError::Empty)
    ));
    assert!(matches!(
        ApplySettings::new(vec![
            create(
                SettingKey::ActiveThemeId,
                SettingValue::active_theme_id("one").unwrap(),
            ),
            create(
                SettingKey::ActiveThemeId,
                SettingValue::active_theme_id("two").unwrap(),
            ),
        ]),
        Err(ApplySettingsError::Duplicate {
            key: SettingKey::ActiveThemeId
        })
    ));
    assert!(matches!(
        ApplySettings::new(vec![create(
            SettingKey::ActiveThemeId,
            SettingValue::draft_autosave_interval_seconds(30),
        )]),
        Err(ApplySettingsError::KeyValueMismatch {
            key: SettingKey::ActiveThemeId,
            value_key: SettingKey::DraftAutosaveInterval,
        })
    ));
    assert!(matches!(
        SettingValue::active_theme_id("x".repeat(257)),
        Err(SettingValueError::TooLong {
            key: SettingKey::ActiveThemeId,
            max_bytes: 256,
            actual_bytes: 257,
        })
    ));
    assert!(matches!(
        SettingValue::developer_instructions("x".repeat(60 * 1024 + 1)),
        Err(SettingValueError::TooLong {
            key: SettingKey::DeveloperInstructions,
            actual_bytes,
            ..
        }) if actual_bytes == 60 * 1024 + 1
    ));
}

#[test]
fn settings_cursor_obeys_item_and_byte_bounds() {
    let directory = tempdir().unwrap();
    let (store, state) = open(directory.path());
    match apply(
        &store,
        &state,
        vec![
            create(
                SettingKey::ActiveThemeId,
                SettingValue::active_theme_id("theme").unwrap(),
            ),
            create(
                SettingKey::ContextCompactionTimeout,
                SettingValue::context_compaction_timeout_millis(1_000),
            ),
            create(
                SettingKey::DraftAutosaveInterval,
                SettingValue::draft_autosave_interval_seconds(30),
            ),
        ],
    ) {
        CommandOutcome::Committed {
            later_failure: None,
            ..
        } => {}
        outcome => panic!("expected committed settings command, got {outcome:?}"),
    }

    let first = state
        .settings()
        .list(&store, None, CursorReadLimits::new(1, 1024 * 1024).unwrap())
        .unwrap();
    assert_eq!(first.records().len(), 1);
    assert!(first.has_more());
    let first_key = first.records()[0].key();
    let rest = state
        .settings()
        .list(
            &store,
            Some(first_key),
            CursorReadLimits::new(10, 1024 * 1024).unwrap(),
        )
        .unwrap();
    assert_eq!(rest.records().len(), 2);
    assert!(!rest.has_more());

    assert!(matches!(
        state
            .settings()
            .list(&store, None, CursorReadLimits::new(10, 4).unwrap(),),
        Err(ReadError::BoundExceeded { .. })
    ));
}

const SETTINGS_FAMILIES: &[RecordFamily<SettingsV2Probe>] =
    &[RecordFamily::new::<SettingRecordV2>(
        KeyspaceSchemaVersion::new(1),
    )];

struct SettingsV2Probe;
struct SettingRecordV2;

impl StorageDomain for SettingsV2Probe {
    const NAME: &'static str = "beryl-settings";
    const SCHEMA_VERSION: DomainSchemaVersion = DomainSchemaVersion::new(1);
    const FAMILIES: &'static [RecordFamily<Self>] = SETTINGS_FAMILIES;
    type ValidationError = ProbeError;
    type RuntimeAttachment = ();
    type RuntimeAttachmentError = std::convert::Infallible;

    fn create_runtime_attachment() -> Result<(), Self::RuntimeAttachmentError> {
        Ok(())
    }

    fn validate(reader: &DomainReader<'_, Self>) -> Result<(), Self::ValidationError> {
        reader
            .cursor::<SettingRecordV2>(
                &CursorRange::closed(0, u8::MAX),
                CursorDirection::Forward,
                CursorReadLimits::new(16, 1024 * 1024).unwrap(),
            )
            .map(|_| ())
            .map_err(ProbeError)
    }
}

impl RecordCodec<SettingsV2Probe> for SettingRecordV2 {
    type Key = u8;
    type Value = Vec<u8>;
    type Error = ProbeCodecError;

    const FAMILY: &'static str = "records";
    const VERSION: RecordVersion = RecordVersion::new(2);
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
struct ProbeError(ReadError);

impl fmt::Display for ProbeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Error for ProbeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

impl DomainCallbackError for ProbeError {
    fn into_callback_source(self) -> Result<DomainCallbackSource, Self> {
        Ok(DomainCallbackSource::Read(self.0))
    }
}

#[derive(Debug)]
struct ProbeCodecError;

impl fmt::Display for ProbeCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("probe setting key is not one byte")
    }
}

impl Error for ProbeCodecError {}

#[test]
fn routine_reopen_defers_an_unsupported_setting_record_version_to_explicit_scrub() {
    let directory = tempdir().unwrap();
    let mut store = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let probe = store.register_domain::<SettingsV2Probe>().unwrap();
    store
        .inject_persisted_corrupt_record::<SettingsV2Probe, SettingRecordV2>(
            &probe,
            &[0],
            &1_u32.to_be_bytes(),
        )
        .unwrap();
    store.close().unwrap();

    let mut reopened = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap();
    let probe = reopened.register_domain::<SettingsV2Probe>().unwrap();
    assert!(matches!(
        reopened.read_point::<SettingsV2Probe, SettingRecordV2>(
            &probe,
            &0,
            PointReadLimit::new(64 * 1024 + 4).unwrap(),
        ),
        Err(ReadError::UnsupportedRecordVersion {
            supported,
            found: 1,
            ..
        }) if supported == RecordVersion::new(2)
    ));
    assert!(
        reopened
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .is_err()
    );
}
