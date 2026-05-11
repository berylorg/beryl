use std::{
    env, fs,
    io::{self, Write},
    path::PathBuf,
};

use beryl_model::workspace::BerylWorkspaceId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const APP_ROOT_DIR_NAME: &str = ".beryl";
const STARTUP_METADATA_FILE_NAME: &str = "startup-state.json";

fn default_next_untitled_workspace_sequence() -> u64 {
    1
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupMetadata {
    #[serde(default)]
    recent_workspaces: Vec<BerylWorkspaceId>,
    #[serde(default)]
    last_opened_workspace: Option<BerylWorkspaceId>,
    #[serde(default = "default_next_untitled_workspace_sequence")]
    next_untitled_workspace_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupPersistence {
    root_dir: PathBuf,
}

#[derive(Debug, Error)]
pub enum StartupPersistenceError {
    #[error("could not determine the current user's home directory")]
    MissingHomeDirectory,
    #[error("failed to create Beryl state directory {path}")]
    CreateDirectory {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to read startup metadata from {path}")]
    ReadMetadata {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse startup metadata from {path}")]
    ParseMetadata {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to serialize startup metadata")]
    SerializeMetadata {
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to write startup metadata to {path}")]
    WriteMetadata {
        path: String,
        #[source]
        source: io::Error,
    },
}

impl StartupMetadata {
    pub fn recent_workspaces(&self) -> &[BerylWorkspaceId] {
        &self.recent_workspaces
    }

    pub fn last_opened_workspace(&self) -> Option<&BerylWorkspaceId> {
        self.last_opened_workspace.as_ref()
    }

    pub fn next_untitled_workspace_sequence(&self) -> u64 {
        self.next_untitled_workspace_sequence.max(1)
    }

    pub fn allocate_untitled_workspace_sequence(&mut self) -> u64 {
        let sequence = self.next_untitled_workspace_sequence();
        self.next_untitled_workspace_sequence = sequence.saturating_add(1);
        sequence
    }

    pub fn remember_workspace(&mut self, workspace: BerylWorkspaceId) {
        self.recent_workspaces
            .retain(|existing| existing != &workspace);
        self.recent_workspaces.insert(0, workspace.clone());
        self.last_opened_workspace = Some(workspace);
    }

    pub fn forget_workspace(&mut self, workspace: &BerylWorkspaceId) {
        self.recent_workspaces
            .retain(|existing| existing != workspace);
        if self.last_opened_workspace.as_ref() == Some(workspace) {
            self.last_opened_workspace = self.recent_workspaces.first().cloned();
        }
    }

    pub fn replace_workspace(&mut self, old: &BerylWorkspaceId, new: BerylWorkspaceId) {
        for workspace in &mut self.recent_workspaces {
            if workspace == old {
                *workspace = new.clone();
            }
        }

        let mut deduped = Vec::with_capacity(self.recent_workspaces.len());
        for workspace in self.recent_workspaces.drain(..) {
            if !deduped.iter().any(|existing| existing == &workspace) {
                deduped.push(workspace);
            }
        }
        self.recent_workspaces = deduped;

        if self.last_opened_workspace.as_ref() == Some(old) {
            self.last_opened_workspace = Some(new);
        }
    }
}

impl Default for StartupMetadata {
    fn default() -> Self {
        Self {
            recent_workspaces: Vec::new(),
            last_opened_workspace: None,
            next_untitled_workspace_sequence: default_next_untitled_workspace_sequence(),
        }
    }
}

impl StartupPersistence {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    pub fn from_environment() -> Result<Self, StartupPersistenceError> {
        let home = home_directory().ok_or(StartupPersistenceError::MissingHomeDirectory)?;
        Ok(Self::new(home.join(APP_ROOT_DIR_NAME)))
    }

    pub fn load(&self) -> Result<StartupMetadata, StartupPersistenceError> {
        self.ensure_root_dir()?;

        let path = self.startup_metadata_path();
        if !path.exists() {
            return Ok(StartupMetadata::default());
        }

        let contents =
            fs::read_to_string(&path).map_err(|source| StartupPersistenceError::ReadMetadata {
                path: path.display().to_string(),
                source,
            })?;

        serde_json::from_str(&contents).map_err(|source| StartupPersistenceError::ParseMetadata {
            path: path.display().to_string(),
            source,
        })
    }

    pub fn save(&self, metadata: &StartupMetadata) -> Result<(), StartupPersistenceError> {
        self.ensure_root_dir()?;

        let path = self.startup_metadata_path();
        let contents = serde_json::to_vec_pretty(metadata)
            .map_err(|source| StartupPersistenceError::SerializeMetadata { source })?;

        let mut temp_file = tempfile::NamedTempFile::new_in(&self.root_dir).map_err(|source| {
            StartupPersistenceError::WriteMetadata {
                path: path.display().to_string(),
                source,
            }
        })?;
        temp_file.write_all(&contents).map_err(|source| {
            StartupPersistenceError::WriteMetadata {
                path: temp_file.path().display().to_string(),
                source,
            }
        })?;
        temp_file.persist(&path).map_err(|error| {
            let tempfile::PersistError { error: source, .. } = error;
            StartupPersistenceError::WriteMetadata {
                path: path.display().to_string(),
                source,
            }
        })?;

        Ok(())
    }

    fn startup_metadata_path(&self) -> PathBuf {
        self.root_dir.join(STARTUP_METADATA_FILE_NAME)
    }

    fn ensure_root_dir(&self) -> Result<(), StartupPersistenceError> {
        fs::create_dir_all(&self.root_dir).map_err(|source| {
            StartupPersistenceError::CreateDirectory {
                path: self.root_dir.display().to_string(),
                source,
            }
        })
    }
}

fn home_directory() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}
