use std::{io, path::Path};

use beryl_model::BerylHomeId;
use fjall::{Database, PersistMode};
use thiserror::Error;

use super::{HomeControl, OpenedDatabase, StorageProfile};
use crate::{
    health::ClassifiedFjallError,
    layout::HomeLayout,
    metadata::{
        decode_home_revision, encode_home_revision, DOMAINS_KEYSPACE, HEADER_KEY, HEADER_KEYSPACE,
        HOME_REVISION_BYTES, HOME_REVISION_KEY, MAX_HOME_HEADER_BYTES,
    },
    HomeHeader, HomeOpenError, HomeOpenStage, HomeSchemaVersion, HomeUnreadableStage,
};

pub(super) fn create_fresh_database(
    configured_path: &Path,
    layout: &HomeLayout,
    schema: HomeSchemaVersion,
    storage_profile: StorageProfile,
) -> Result<OpenedDatabase, HomeOpenError> {
    let config = storage_profile
        .configuration(&layout.database_path)
        .map_err(|source| {
            HomeOpenError::open(
                configured_path,
                HomeOpenStage::CreateDatabase,
                ClassifiedFjallError::direct(source),
            )
        })?;
    let database = Database::create(config).map_err(|source| {
        HomeOpenError::open(
            configured_path,
            HomeOpenStage::CreateDatabase,
            ClassifiedFjallError::direct(source),
        )
    })?;
    let header_keyspace = database
        .create_keyspace(HEADER_KEYSPACE)
        .map_err(|source| {
            HomeOpenError::open(
                configured_path,
                HomeOpenStage::InitializeHeader,
                ClassifiedFjallError::direct(source),
            )
        })?;
    let domains_keyspace = database
        .create_keyspace(DOMAINS_KEYSPACE)
        .map_err(|source| {
            HomeOpenError::open(
                configured_path,
                HomeOpenStage::InitializeHeader,
                ClassifiedFjallError::direct(source),
            )
        })?;

    let mut identity = [0; 16];
    getrandom::fill(&mut identity).map_err(|source| {
        HomeOpenError::open(configured_path, HomeOpenStage::GenerateHomeIdentity, source)
    })?;
    let header = HomeHeader {
        schema,
        home_id: BerylHomeId::from_bytes(identity),
    };

    let encoded_header = header.encode();
    let encoded_revision =
        encode_home_revision(beryl_model::HomeRevision::new(1).expect("one is nonzero"));
    let key_bytes = HEADER_KEY
        .len()
        .checked_add(HOME_REVISION_KEY.len())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or_else(|| {
            HomeOpenError::open(
                configured_path,
                HomeOpenStage::InitializeHeader,
                io::Error::other("fixed control-key byte count overflowed"),
            )
        })?;
    let value_bytes = encoded_header
        .len()
        .checked_add(encoded_revision.len())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or_else(|| {
            HomeOpenError::open(
                configured_path,
                HomeOpenStage::InitializeHeader,
                io::Error::other("fixed control-value byte count overflowed"),
            )
        })?;
    let capacity = database
        .storage_policy()
        .batch_capacity(2, key_bytes, value_bytes)
        .map_err(|source| {
            HomeOpenError::open(
                configured_path,
                HomeOpenStage::InitializeHeader,
                ClassifiedFjallError::direct(source),
            )
        })?;
    let mut batch = database
        .batch(capacity, PersistMode::Buffer)
        .map_err(|source| {
            HomeOpenError::open(
                configured_path,
                HomeOpenStage::InitializeHeader,
                ClassifiedFjallError::direct(source),
            )
        })?;
    batch
        .insert(
            &header_keyspace,
            HEADER_KEY.to_vec().into_boxed_slice(),
            encoded_header.to_vec().into_boxed_slice(),
        )
        .map_err(|source| {
            HomeOpenError::open(
                configured_path,
                HomeOpenStage::InitializeHeader,
                ClassifiedFjallError::direct(source),
            )
        })?;
    batch
        .insert(
            &header_keyspace,
            HOME_REVISION_KEY.to_vec().into_boxed_slice(),
            encoded_revision.into_boxed_slice(),
        )
        .map_err(|source| {
            HomeOpenError::open(
                configured_path,
                HomeOpenStage::InitializeHeader,
                ClassifiedFjallError::direct(source),
            )
        })?;
    batch.commit().map_err(|source| {
        HomeOpenError::open(
            configured_path,
            HomeOpenStage::InitializeHeader,
            ClassifiedFjallError::direct(source),
        )
    })?;
    database.persist(PersistMode::SyncAll).map_err(|source| {
        HomeOpenError::open(
            configured_path,
            HomeOpenStage::InitializeHeader,
            ClassifiedFjallError::direct(source),
        )
    })?;

    let snapshot = database.snapshot().map_err(|source| {
        HomeOpenError::open(
            configured_path,
            HomeOpenStage::InitializeHeader,
            ClassifiedFjallError::direct(source),
        )
    })?;
    let persisted = bounded_point(
        &snapshot,
        &header_keyspace,
        HEADER_KEY,
        MAX_HOME_HEADER_BYTES,
    )
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
    let verified = HomeHeader::decode(persisted.value()).map_err(|source| {
        HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
    })?;
    if verified != header {
        return Err(HomeOpenError::open(
            configured_path,
            HomeOpenStage::InitializeHeader,
            io::Error::new(
                io::ErrorKind::InvalidData,
                "fresh home header changed before verification",
            ),
        ));
    }
    let revision = bounded_point(
        &snapshot,
        &header_keyspace,
        HOME_REVISION_KEY,
        HOME_REVISION_BYTES,
    )
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
    decode_home_revision(revision.value()).map_err(|source| {
        HomeOpenError::open(configured_path, HomeOpenStage::InitializeHeader, source)
    })?;
    drop(persisted);
    drop(revision);
    drop(snapshot);
    database.health().map_err(|source| {
        HomeOpenError::open(
            configured_path,
            HomeOpenStage::InitializeHeader,
            ClassifiedFjallError::direct(source),
        )
    })?;

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
    storage_profile: StorageProfile,
) -> Result<OpenedDatabase, HomeOpenError> {
    let config = storage_profile
        .configuration(&layout.database_path)
        .map_err(|source| {
            HomeOpenError::unreadable(
                configured_path,
                HomeUnreadableStage::RecoverDatabase,
                ClassifiedFjallError::direct(source),
            )
        })?;
    let database = Database::recover(config).map_err(|source| {
        HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::RecoverDatabase,
            ClassifiedFjallError::direct(source),
        )
    })?;
    if !database
        .keyspace_exists(HEADER_KEYSPACE)
        .map_err(|source| {
            HomeOpenError::unreadable(
                configured_path,
                HomeUnreadableStage::OpenHeaderKeyspace,
                ClassifiedFjallError::direct(source),
            )
        })?
    {
        return Err(HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::MissingHeaderKeyspace,
            io::Error::new(
                io::ErrorKind::InvalidData,
                "required Beryl-home header keyspace is missing",
            ),
        ));
    }
    if !database
        .keyspace_exists(DOMAINS_KEYSPACE)
        .map_err(|source| {
            HomeOpenError::unreadable(
                configured_path,
                HomeUnreadableStage::OpenDomainRegistryKeyspace,
                ClassifiedFjallError::direct(source),
            )
        })?
    {
        return Err(HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::MissingDomainRegistryKeyspace,
            io::Error::new(
                io::ErrorKind::InvalidData,
                "required Beryl-home domain registry keyspace is missing",
            ),
        ));
    }

    let header_keyspace = database.open_keyspace(HEADER_KEYSPACE).map_err(|source| {
        HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::OpenHeaderKeyspace,
            ClassifiedFjallError::direct(source),
        )
    })?;
    let domains_keyspace = database.open_keyspace(DOMAINS_KEYSPACE).map_err(|source| {
        HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::OpenDomainRegistryKeyspace,
            ClassifiedFjallError::direct(source),
        )
    })?;
    let snapshot = database.snapshot().map_err(|source| {
        HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::OpenHeaderKeyspace,
            ClassifiedFjallError::direct(source),
        )
    })?;
    let encoded = bounded_point(
        &snapshot,
        &header_keyspace,
        HEADER_KEY,
        MAX_HOME_HEADER_BYTES,
    )
    .map_err(|source| {
        let stage = match &source {
            ControlPointError::Oversized { .. } => HomeUnreadableStage::DecodeHeader,
            ControlPointError::Storage(_) => HomeUnreadableStage::OpenHeaderKeyspace,
        };
        HomeOpenError::unreadable(configured_path, stage, source)
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
    let header = HomeHeader::decode(encoded.value()).map_err(|source| {
        HomeOpenError::unreadable(configured_path, HomeUnreadableStage::DecodeHeader, source)
    })?;
    let revision = bounded_point(
        &snapshot,
        &header_keyspace,
        HOME_REVISION_KEY,
        HOME_REVISION_BYTES,
    )
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
    decode_home_revision(revision.value()).map_err(|source| {
        HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::DecodeHomeRevision,
            source,
        )
    })?;
    drop(encoded);
    drop(revision);
    drop(snapshot);
    database.health().map_err(|source| {
        HomeOpenError::unreadable(
            configured_path,
            HomeUnreadableStage::RecoverDatabase,
            ClassifiedFjallError::direct(source),
        )
    })?;

    Ok(OpenedDatabase {
        database,
        control: HomeControl {
            header: header_keyspace,
            domains: domains_keyspace,
        },
        header,
    })
}

#[derive(Debug, Error)]
enum ControlPointError {
    #[error(
        "control record has {actual} stored bytes, exceeding its metadata-first bound {maximum}"
    )]
    Oversized { maximum: usize, actual: usize },
    #[error(transparent)]
    Storage(#[from] ClassifiedFjallError),
}

impl From<fjall::Error> for ControlPointError {
    fn from(source: fjall::Error) -> Self {
        Self::Storage(ClassifiedFjallError::direct(source))
    }
}

fn bounded_point<'origin>(
    snapshot: &'origin fjall::Snapshot,
    keyspace: &'origin fjall::Keyspace,
    key: &[u8],
    maximum: usize,
) -> Result<Option<fjall::KvPair<'origin>>, ControlPointError> {
    let Some(point) = snapshot.point(keyspace, key)? else {
        return Ok(None);
    };
    let actual = usize::try_from(point.stored_value_len())
        .expect("u32 stored-value length fits usize on supported targets");
    if actual > maximum {
        return Err(ControlPointError::Oversized { maximum, actual });
    }
    point.acquire().map(Some).map_err(ControlPointError::from)
}
