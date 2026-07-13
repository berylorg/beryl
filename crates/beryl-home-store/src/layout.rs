use std::{fs, io, path::PathBuf};

use crate::HomeUnreadableStage;

pub(crate) const LOCK_FILE_NAME: &str = "home.lock";
pub(crate) const DATABASE_DIRECTORY_NAME: &str = "state";
const FJALL_VERSION_MARKER: &str = "version";

#[derive(Clone, Debug)]
pub(crate) struct HomeLayout {
    pub(crate) database_path: PathBuf,
    pub(crate) lock_path: PathBuf,
}

impl HomeLayout {
    pub(crate) fn at(canonical_home_path: &std::path::Path) -> Self {
        Self {
            database_path: canonical_home_path.join(DATABASE_DIRECTORY_NAME),
            lock_path: canonical_home_path.join(LOCK_FILE_NAME),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DatabaseDisposition {
    Fresh,
    Existing,
}

#[derive(Debug)]
pub(crate) enum LayoutAdmissionError {
    Collision(io::Error),
    Unreadable {
        stage: HomeUnreadableStage,
        source: io::Error,
    },
}

pub(crate) fn reject_database_as_home(home_path: &std::path::Path) -> io::Result<()> {
    let marker = home_path.join(FJALL_VERSION_MARKER);
    match fs::symlink_metadata(&marker) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "the configured Beryl-home path is itself a Fjall database directory",
        )),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(source),
    }
}

pub(crate) fn inspect_database(
    database_path: &std::path::Path,
) -> Result<DatabaseDisposition, LayoutAdmissionError> {
    let metadata = match fs::symlink_metadata(database_path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Ok(DatabaseDisposition::Fresh);
        }
        Err(source) => {
            return Err(LayoutAdmissionError::Unreadable {
                stage: HomeUnreadableStage::InspectDatabaseLayout,
                source,
            });
        }
    };

    if !metadata.is_dir() || is_reparse_point(&metadata) {
        return Err(LayoutAdmissionError::Collision(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "the reserved Beryl state path is not an ordinary local directory",
        )));
    }

    let marker = database_path.join(FJALL_VERSION_MARKER);
    match fs::symlink_metadata(&marker) {
        Ok(marker_metadata) if marker_metadata.is_file() && !is_reparse_point(&marker_metadata) => {
            return Ok(DatabaseDisposition::Existing);
        }
        Ok(_) => {
            return Err(LayoutAdmissionError::Unreadable {
                stage: HomeUnreadableStage::MissingDatabaseVersion,
                source: io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Fjall's version marker is not an ordinary file",
                ),
            });
        }
        Err(source) if source.kind() == io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(LayoutAdmissionError::Unreadable {
                stage: HomeUnreadableStage::InspectDatabaseLayout,
                source,
            });
        }
    }

    let mut entries =
        fs::read_dir(database_path).map_err(|source| LayoutAdmissionError::Unreadable {
            stage: HomeUnreadableStage::InspectDatabaseLayout,
            source,
        })?;
    match entries.next() {
        None => Ok(DatabaseDisposition::Fresh),
        Some(Ok(_)) => Err(LayoutAdmissionError::Unreadable {
            stage: HomeUnreadableStage::MissingDatabaseVersion,
            source: io::Error::new(
                io::ErrorKind::InvalidData,
                "nonempty Beryl state directory is missing Fjall's version marker",
            ),
        }),
        Some(Err(source)) => Err(LayoutAdmissionError::Unreadable {
            stage: HomeUnreadableStage::InspectDatabaseLayout,
            source,
        }),
    }
}

#[cfg(target_os = "windows")]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(target_os = "windows"))]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}
