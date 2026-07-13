mod support;

use beryl_home_store::{
    CommandError, DomainDefinitionError, DomainRegistrationError, HomeCommand, HomeOpenError,
    HomeOpenOptions, HomeSchemaVersion, HomeStore, HomeUnreadableStage, KeyspaceSchemaVersion,
};
use fjall::{Config, Database, KeyspaceCreateOptions, PersistMode};
use tempfile::tempdir;

use support::{
    AlphaDomain, AlphaDomainSchema2, AlphaFamilySchema2, DuplicateFamilyDomain, EmptyDomain,
    PutBytes, ValidatedDomain, open_home,
};

#[test]
fn fresh_registration_reopens_and_rejects_a_duplicate_generation_registration() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let alpha = store.register_domain::<AlphaDomain>().unwrap();

    assert_eq!(store.home_revision().unwrap().get(), 1);
    assert_eq!(store.domain_revision(alpha).unwrap().get(), 1);
    assert!(matches!(
        store.register_domain::<AlphaDomain>(),
        Err(DomainRegistrationError::DuplicateDomain { domain: "alpha" })
    ));
    store.close().unwrap();

    let mut reopened = open_home(directory.path());
    let alpha = reopened.register_domain::<AlphaDomain>().unwrap();
    assert_eq!(reopened.domain_revision(alpha).unwrap().get(), 1);
    reopened.validate_registered_domains().unwrap();
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
fn missing_registered_keyspace_is_not_recreated() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    store.register_domain::<AlphaDomain>().unwrap();
    store.close().unwrap();

    {
        let database = Database::recover(Config::new(&directory.path().join("state"))).unwrap();
        let keyspace = database
            .keyspace("d.alpha.records", KeyspaceCreateOptions::default)
            .unwrap();
        database.delete_keyspace(keyspace).unwrap();
        database.persist(PersistMode::SyncAll).unwrap();
    }

    let mut reopened = open_home(directory.path());
    assert!(matches!(
        reopened.register_domain::<AlphaDomain>(),
        Err(DomainRegistrationError::MissingKeyspace {
            domain: "alpha",
            keyspace,
        }) if keyspace == "d.alpha.records"
    ));
    reopened.close().unwrap();
}

#[test]
fn missing_control_keyspace_or_home_revision_makes_existing_home_unreadable() {
    for remove_registry in [false, true] {
        let directory = tempdir().unwrap();
        open_home(directory.path()).close().unwrap();
        {
            let database = Database::recover(Config::new(&directory.path().join("state"))).unwrap();
            if remove_registry {
                let keyspace = database
                    .keyspace("_beryl_domains", KeyspaceCreateOptions::default)
                    .unwrap();
                database.delete_keyspace(keyspace).unwrap();
            } else {
                let keyspace = database
                    .keyspace("_beryl_home", KeyspaceCreateOptions::default)
                    .unwrap();
                keyspace.remove(b"revision").unwrap();
            }
            database.persist(PersistMode::SyncAll).unwrap();
        }

        let error = HomeStore::open(HomeOpenOptions::new(
            directory.path(),
            HomeSchemaVersion::CURRENT,
        ))
        .unwrap_err();
        assert!(
            matches!(
                &error,
                HomeOpenError::Unreadable {
                    stage: HomeUnreadableStage::MissingDomainRegistryKeyspace,
                    ..
                } if remove_registry
            ) || matches!(
                &error,
                HomeOpenError::Unreadable {
                    stage: HomeUnreadableStage::MissingHomeRevision,
                    ..
                } if !remove_registry
            )
        );
    }
}

#[test]
fn existing_domain_validator_runs_before_registration_is_published() {
    let directory = tempdir().unwrap();
    let mut store = open_home(directory.path());
    let domain = store.register_domain::<ValidatedDomain>().unwrap();
    let mut command = HomeCommand::new(store.home_revision().unwrap());
    command
        .add(domain.contribution(
            store.domain_revision(domain).unwrap(),
            PutBytes::<ValidatedDomain>::new(1, b"reject".to_vec()),
        ))
        .unwrap();
    store.execute(command).unwrap();

    let mut rejected = HomeCommand::new(store.home_revision().unwrap());
    rejected
        .add(domain.contribution(
            store.domain_revision(domain).unwrap(),
            PutBytes::<ValidatedDomain>::new(2, b"later".to_vec()),
        ))
        .unwrap();
    assert!(matches!(
        store.execute(rejected),
        Err(CommandError::DomainValidation {
            domain: "validated",
            ..
        })
    ));
    store.close().unwrap();

    let mut reopened = open_home(directory.path());
    assert!(matches!(
        reopened.register_domain::<ValidatedDomain>(),
        Err(DomainRegistrationError::Validation {
            domain: "validated",
            ..
        })
    ));
    reopened.close().unwrap();
}
