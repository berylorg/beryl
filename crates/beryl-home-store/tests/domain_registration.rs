#[path = "support/fjall.rs"]
mod fjall_support;
mod support;

use std::collections::BTreeSet;

use beryl_home_store::{
    DomainDefinitionError, DomainRegistrationError, DomainValidationError, HomeCommand,
    HomeHealthState, HomeOpenError, HomeOpenOptions, HomeSchemaVersion, HomeStore,
    HomeUnreadableStage, KeyspaceSchemaVersion, WholeHomeScrubTrigger,
};
use fjall::{Database, PersistMode};
use tempfile::tempdir;

use support::{
    AlphaDomain, AlphaDomainSchema2, AlphaFamilySchema2, DuplicateFamilyDomain, EmptyDomain,
    PutBytes, ValidatedDomain, committed, open_home,
};

#[test]
fn fresh_registration_reopens_and_rejects_a_duplicate_generation_registration() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();

    assert_eq!(store.home_revision().unwrap().get(), 1);
    assert_eq!(store.domain_revision(&alpha).unwrap().get(), 1);
    assert!(matches!(
        store.register_domain::<AlphaDomain>(),
        Err(DomainRegistrationError::DuplicateDomain { domain: "alpha" })
    ));
    store.close().unwrap();

    let mut reopened = open_home(directory.path());
    let alpha = reopened.register_domain::<AlphaDomain>().unwrap();
    assert_eq!(reopened.domain_revision(&alpha).unwrap().get(), 1);
    reopened
        .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
        .unwrap();
    reopened.close().unwrap();
}

#[test]
fn invalid_static_family_declarations_fail_before_registration() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());

    assert!(matches!(
        store.register_domain::<DuplicateFamilyDomain>(),
        Err(DomainRegistrationError::InvalidDefinition(
            DomainDefinitionError::DuplicateKeyspace {
                domain: "duplicates",
                family: "records"
            }
        ))
    ));
    assert!(matches!(
        store.register_domain::<EmptyDomain>(),
        Err(DomainRegistrationError::InvalidDefinition(
            DomainDefinitionError::NoKeyspaces { domain: "empty" }
        ))
    ));
    store.close().unwrap();
}

#[test]
fn persistent_domain_and_family_versions_are_exact() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    store.register_domain::<AlphaDomain>().unwrap();
    store.close().unwrap();

    let mut reopened = open_home(directory.path());
    assert!(matches!(
        reopened.register_domain::<AlphaDomainSchema2>(),
        Err(DomainRegistrationError::UnsupportedDomainSchema {
            domain: "alpha",
            ..
        })
    ));
    reopened.close().unwrap();

    let mut reopened = open_home(directory.path());
    assert!(matches!(
        reopened.register_domain::<AlphaFamilySchema2>(),
        Err(DomainRegistrationError::UnsupportedKeyspaceSchema {
            domain: "alpha",
            family,
            supported,
            found,
        }) if family == "records"
            && supported == KeyspaceSchemaVersion::new(2)
            && found == KeyspaceSchemaVersion::new(1)
    ));
    reopened.close().unwrap();
}

#[test]
fn fresh_registration_adopts_only_an_empty_interrupted_family() {
    let empty_directory = tempdir().unwrap();
    open_home(empty_directory.path()).close().unwrap();
    create_unregistered_alpha_family(empty_directory.path(), None);

    let mut reopened = open_home(empty_directory.path());
    let alpha = reopened.register_domain::<AlphaDomain>().unwrap();
    assert_eq!(reopened.domain_revision(&alpha).unwrap().get(), 1);
    reopened.close().unwrap();
    let mut persisted = open_home(empty_directory.path());
    persisted.register_domain::<AlphaDomain>().unwrap();
    persisted.close().unwrap();

    let nonempty_directory = tempdir().unwrap();
    open_home(nonempty_directory.path()).close().unwrap();
    create_unregistered_alpha_family(
        nonempty_directory.path(),
        Some((b"unregistered-key", b"unregistered-value")),
    );

    let mut reopened = open_home(nonempty_directory.path());
    let error = reopened.register_domain::<AlphaDomain>().unwrap_err();
    assert!(matches!(
        error,
        DomainRegistrationError::UnexpectedKeyspace { ref keyspace }
            if keyspace == "d.alpha.records"
    ));
    assert_eq!(
        error.to_string(),
        "physical keyspace `d.alpha.records` cannot be adopted by a fresh domain registration"
    );
    assert_eq!(reopened.health().state(), HomeHealthState::Failed);
}

#[test]
fn missing_registered_keyspace_is_not_recreated() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let keyspaces = directory.path().join("state").join("keyspaces");
    let before = physical_keyspace_directories(&keyspaces);
    store.register_domain::<AlphaDomain>().unwrap();
    store.close().unwrap();

    let after = physical_keyspace_directories(&keyspaces);
    let added: Vec<_> = after.difference(&before).cloned().collect();
    assert_eq!(added.len(), 1, "registration creates one physical family");
    std::fs::remove_dir_all(&added[0]).unwrap();

    let error = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        HomeOpenError::Unreadable {
            stage: HomeUnreadableStage::RecoverDatabase,
            ..
        }
    ));
    assert!(!added[0].exists(), "missing family must not be recreated");
}

#[test]
fn missing_home_revision_makes_existing_home_unreadable() {
    let directory = tempdir().unwrap();
    open_home(directory.path()).close().unwrap();
    let database =
        Database::recover(fjall_support::config(&directory.path().join("state"))).unwrap();
    let keyspace = database.open_keyspace("_beryl_home").unwrap();
    keyspace.remove(b"revision").unwrap();
    database.persist(PersistMode::SyncAll).unwrap();
    drop(keyspace);
    drop(database);

    let error = HomeStore::open(HomeOpenOptions::new(
        directory.path(),
        HomeSchemaVersion::CURRENT,
    ))
    .unwrap_err();
    assert!(matches!(
        error,
        HomeOpenError::Unreadable {
            stage: HomeUnreadableStage::MissingHomeRevision,
            ..
        }
    ));
}

fn physical_keyspace_directories(root: &std::path::Path) -> BTreeSet<std::path::PathBuf> {
    std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect()
}

fn create_unregistered_alpha_family(home: &std::path::Path, record: Option<(&[u8], &[u8])>) {
    let database = Database::recover(fjall_support::config(&home.join("state"))).unwrap();
    let keyspace = database.create_keyspace("d.alpha.records").unwrap();
    if let Some((key, value)) = record {
        keyspace.insert(key, value).unwrap();
    }
    database.persist(PersistMode::SyncAll).unwrap();
}

#[test]
fn persisted_registration_distinguishes_routine_reacquisition_from_schema_validation() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let domain = store.register_domain::<ValidatedDomain>().unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(
            store.domain_revision(&domain).unwrap(),
            PutBytes::<ValidatedDomain>::new(1, b"reject".to_vec()),
        ))
        .unwrap();
    committed(store.execute(command));

    let mut rejected = HomeCommand::new(store.home_revision().unwrap());
    rejected
        .add(domain.contribution(
            store.domain_revision(&domain).unwrap(),
            PutBytes::<ValidatedDomain>::new(2, b"later".to_vec()),
        ))
        .unwrap();
    committed(store.execute(rejected));
    assert!(matches!(
        store
            .scrub_whole_home(WholeHomeScrubTrigger::Explicit)
            .unwrap_err()
            .validation_error(),
        DomainValidationError::Rejected {
            domain: "validated",
            ..
        }
    ));
    store.close().unwrap();

    let mut reopened = open_home(directory.path());
    reopened.register_domain::<ValidatedDomain>().unwrap();
    reopened.close().unwrap();

    let mut validating = open_home(directory.path());
    assert!(matches!(
        validating.register_domain_with_schema_validation::<ValidatedDomain>(),
        Err(DomainRegistrationError::Validation {
            domain: "validated",
            ..
        })
    ));
}
