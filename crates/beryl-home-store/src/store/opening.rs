use std::{io, path::Path};

use beryl_model::BerylHomeId;
use fjall::{Config, Database, KeyspaceCreateOptions, PersistMode, Readable};

use super::{HomeControl, OpenedDatabase};
use crate::{
    HomeHeader, HomeOpenError, HomeOpenStage, HomeSchemaVersion, HomeUnreadableStage,
    layout::HomeLayout,
    metadata::{
        DOMAINS_KEYSPACE, HEADER_KEY, HEADER_KEYSPACE, HOME_REVISION_KEY, decode_home_revision,
        encode_home_revision,
    },
};

pub(super) fn create_fresh_database(
    configured_path: &Path,
    layout: &HomeLayout,
    schema: HomeSchemaVersion,
) -> Result<OpenedDatabase, HomeOpenError> {
    let database = Database::open(Config::new(&layout.database_path)).map_err(|source| {
        HomeOpenError::open(configured_path, HomeOpenStage::CreateDatabase, source)
    })?;
    let header_keyspace = database
        .keyspace(HEADER_KEYSPACE, KeyspaceCreateOptions::default)
        .map_err(|source| {
            HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
        })?;
    let domains_keyspace = database
        .keyspace(DOMAINS_KEYSPACE, KeyspaceCreateOptions::default)
        .map_err(|source| {
            HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
        })?;

    let mut identity = [0; 16];
    getrandom::fill(&mut identity).map_err(|source| {
        HomeOpenError::open(configured_path, HomeOpenStage::GenerateHomeIdentity, source)
    })?;
    let header = HomeHeader {
        schema,
        home_id: BerylHomeId::from_bytes(identity),
    };

    let mut batch = database.batch();
    batch.insert(&header_keyspace, HEADER_KEY, header.encode());
    batch.insert(
        &header_keyspace,
        HOME_REVISION_KEY,
        encode_home_revision(beryl_model::HomeRevision::new(1).expect("one is nonzero")),
    );
    batch.commit().map_err(|source| {
        HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
    })?;
    database.persist(PersistMode::SyncAll).map_err(|source| {
        HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
    })?;

    let snapshot = database.snapshot();
    let persisted = snapshot
        .get(&header_keyspace, HEADER_KEY)
        .map_err(|source| {
            HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
        })?;
    let persisted = persisted.ok_or_else(|| {
        HomeOpenError::open(
            configured_path,
            HomeOpenStage::InitializeHeader,
            io::Error::new(
                io::ErrorKind::InvalidData,
                "fresh home header was not readable after persistence",
            ),
        )
    })?;
    let verified = HomeHeader::decode(&persisted).map_err(|source| {
        HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
    })?;
    let revision = snapshot
        .get(&header_keyspace, HOME_REVISION_KEY)
        .map_err(|source| {
            HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
        })?
        .ok_or_else(|| {
            HomeOpenError::open(
                configured_path,
                HomeOpenStage::InitializeHeader,
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "fresh home revision was not readable after persistence",
                ),
            )
        })?;
    decode_home_revision(&revision).map_err(|source| {
        HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
    })?;
    drop(snapshot);

    Ok(OpenedDatabase {
        database,
        control: HomeControl {
            header: header_keyspace,
            domains: domains_keyspace,
        },
        header: verified,
    })
}

pub(crate) fn open_existing_database(
    configured_path: &Path,
    layout: &HomeLayout,
) -> Result<OpenedDatabase, HomeOpenError> {
    let database = Database::recover(Config::new(&layout.database_path)).map_err(|source| {
        HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::RecoverDatabase,
            source,
        )
    })?;
    if !database.keyspace_exists(HEADER_KEYSPACE) {
        return Err(HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::MissingHeaderKeyspace,
            io::Error::new(
                io::ErrorKind::InvalidData,
                "required Beryl-home header keyspace is missing",
            ),
        ));
    }
    if !database.keyspace_exists(DOMAINS_KEYSPACE) {
        return Err(HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::MissingDomainRegistryKeyspace,
            io::Error::new(
                io::ErrorKind::InvalidData,
                "required Beryl-home domain registry keyspace is missing",
            ),
        ));
    }

    let header_keyspace = database
        .keyspace(HEADER_KEYSPACE, KeyspaceCreateOptions::default)
        .map_err(|source| {
            HomeOpenError::unreadable(
                configured_path,
                HomeUnreadableStage::OpenHeaderKeyspace,
                source,
            )
        })?;
    let domains_keyspace = database
        .keyspace(DOMAINS_KEYSPACE, KeyspaceCreateOptions::default)
        .map_err(|source| {
            HomeOpenError::unreadable(
                configured_path,
                HomeUnreadableStage::OpenDomainRegistryKeyspace,
                source,
            )
        })?;
    let snapshot = database.snapshot();
    let encoded = snapshot
        .get(&header_keyspace, HEADER_KEY)
        .map_err(|source| {
            HomeOpenError::unreadable(
                configured_path,
                HomeUnreadableStage::OpenHeaderKeyspace,
                source,
            )
        })?;
    let encoded = encoded.ok_or_else(|| {
        HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::MissingHeaderRecord,
            io::Error::new(
                io::ErrorKind::InvalidData,
                "required Beryl-home header record is missing",
            ),
        )
    })?;
    let header = HomeHeader::decode(&encoded).map_err(|source| {
        HomeOpenError::unreadable(configured_path, HomeUnreadableStage::DecodeHeader, source)
    })?;
    let revision = snapshot
        .get(&header_keyspace, HOME_REVISION_KEY)
        .map_err(|source| {
            HomeOpenError::unreadable(
                configured_path,
                HomeUnreadableStage::DecodeHomeRevision,
                source,
            )
        })?
        .ok_or_else(|| {
            HomeOpenError::unreadable(
                configured_path,
                HomeUnreadableStage::MissingHomeRevision,
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "required complete-home revision record is missing",
                ),
            )
        })?;
    decode_home_revision(&revision).map_err(|source| {
        HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::DecodeHomeRevision,
            source,
        )
    })?;
    drop(snapshot);

    Ok(OpenedDatabase {
        database,
        control: HomeControl {
            header: header_keyspace,
            domains: domains_keyspace,
        },
        header,
    })
}
