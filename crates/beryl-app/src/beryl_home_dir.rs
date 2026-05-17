use std::{
    env,
    path::{Component, Path, PathBuf},
};

use thiserror::Error;

use crate::{
    AppearanceSettingsStore, BerylWorkspacePersistence, GuiPreferencesStore, StartupPersistence,
    ThemeRepositoryStore,
};

const DEFAULT_BERYL_HOME_DIR_NAME: &str = ".beryl";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BerylHomeDir {
    root_dir: PathBuf,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum BerylHomeDirError {
    #[error("could not determine the current user's home directory")]
    MissingHomeDirectory,
    #[error(
        "failed to determine current directory while resolving Beryl home directory: {message}"
    )]
    CurrentDirectory { message: String },
}

impl BerylHomeDir {
    pub fn from_environment() -> Result<Self, BerylHomeDirError> {
        let home = env::var_os("USERPROFILE")
            .or_else(|| env::var_os("HOME"))
            .map(PathBuf::from)
            .ok_or(BerylHomeDirError::MissingHomeDirectory)?;
        Self::from_user_home_directory(home)
    }

    pub fn from_user_home_directory(
        home_dir: impl Into<PathBuf>,
    ) -> Result<Self, BerylHomeDirError> {
        absolute_lexical_path(home_dir.into().join(DEFAULT_BERYL_HOME_DIR_NAME))
            .map(|root_dir| Self { root_dir })
    }

    pub fn from_explicit_path(path: impl Into<PathBuf>) -> Result<Self, BerylHomeDirError> {
        absolute_lexical_path(path.into()).map(|root_dir| Self { root_dir })
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn startup_persistence(&self) -> StartupPersistence {
        StartupPersistence::new(self.root_dir.clone())
    }

    pub fn workspace_persistence(&self) -> BerylWorkspacePersistence {
        BerylWorkspacePersistence::new(self.root_dir.clone())
    }

    pub fn gui_preferences_store(&self) -> GuiPreferencesStore {
        GuiPreferencesStore::new(self.root_dir.clone())
    }

    pub fn appearance_settings_store(&self) -> AppearanceSettingsStore {
        AppearanceSettingsStore::new(self.root_dir.clone())
    }

    pub fn theme_repository_store(&self) -> ThemeRepositoryStore {
        ThemeRepositoryStore::new(self.root_dir.clone())
    }
}

fn absolute_lexical_path(path: PathBuf) -> Result<PathBuf, BerylHomeDirError> {
    let absolute = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .map_err(|source| BerylHomeDirError::CurrentDirectory {
                message: source.to_string(),
            })?
            .join(path)
    };

    Ok(normalize_lexically(absolute))
}

fn normalize_lexically(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !is_at_filesystem_root(&normalized) && !normalized.pop() {
                    normalized.push(component.as_os_str());
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }

    normalized
}

fn is_at_filesystem_root(path: &Path) -> bool {
    path.components()
        .next_back()
        .is_some_and(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
}
