mod io;
mod snapshot;

use std::{fs, io as std_io, path::PathBuf};
use thiserror::Error;

use super::types::{
    BUILT_IN_INSTALLED_THEME_NAME, InstalledThemeId, InstalledThemeMetadata, unique_recovered_name,
    validate_theme_name,
};
use crate::appearance::theme::{
    ActiveThemeProjection, ThemeDefinition, ThemeResolutionError, ThemeResolver,
    ThemeValidationDiagnostics, built_in_theme_definition, built_in_theme_schema,
};
use snapshot::snapshot_from_loaded;

const THEME_REPOSITORY_DIR_NAME: &str = "themes";
const THEME_DOCUMENT_DIR_NAME: &str = "installed";
const THEME_REPOSITORY_MANIFEST_FILE_NAME: &str = "manifest.toml";
const THEME_REPOSITORY_SCHEMA_VERSION: i64 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeRepositoryStore {
    root_dir: PathBuf,
}

#[derive(Clone, Debug)]
pub struct ThemeRepositorySnapshot {
    active_theme_id: InstalledThemeId,
    themes: Vec<InstalledThemeMetadata>,
    active_definition: ThemeDefinition,
    active_projection: ActiveThemeProjection,
}

#[derive(Debug, Error)]
pub enum ThemeRepositoryError {
    #[error("failed to create theme repository directory {path}")]
    CreateDirectory {
        path: String,
        #[source]
        source: std_io::Error,
    },
    #[error("failed to read theme repository file {path}")]
    ReadFile {
        path: String,
        #[source]
        source: std_io::Error,
    },
    #[error("failed to write theme repository file {path}")]
    WriteFile {
        path: String,
        #[source]
        source: std_io::Error,
    },
    #[error("failed to serialize theme repository manifest")]
    SerializeManifest {
        #[source]
        source: toml::ser::Error,
    },
    #[error("theme repository manifest is invalid")]
    ParseManifest {
        #[source]
        source: toml::de::Error,
    },
    #[error("installed theme name is invalid")]
    InvalidThemeName,
    #[error("installed theme id is unknown")]
    UnknownTheme,
    #[error("installed theme name already exists")]
    DuplicateThemeName,
    #[error("the built-in fallback theme cannot be modified")]
    BuiltInThemeIsReadOnly,
    #[error("installed theme definition is invalid")]
    InvalidThemeDefinition {
        #[source]
        source: ThemeValidationDiagnostics,
    },
    #[error("installed theme could not be projected")]
    Projection {
        #[source]
        source: ThemeResolutionError,
    },
    #[error("theme document is invalid")]
    Document {
        #[from]
        source: super::document::ThemeDocumentError,
    },
}

#[derive(Clone, Debug)]
struct LoadedThemeRepository {
    active_theme_id: InstalledThemeId,
    themes: Vec<InstalledThemeRecord>,
}

#[derive(Clone, Debug)]
struct InstalledThemeRecord {
    id: InstalledThemeId,
    name: String,
    definition: ThemeDefinition,
}

impl ThemeRepositoryStore {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    pub fn repository_dir(&self) -> PathBuf {
        self.root_dir.join(THEME_REPOSITORY_DIR_NAME)
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.repository_dir()
            .join(THEME_REPOSITORY_MANIFEST_FILE_NAME)
    }

    pub fn theme_documents_dir(&self) -> PathBuf {
        self.repository_dir().join(THEME_DOCUMENT_DIR_NAME)
    }

    pub fn theme_document_path(&self, id: &InstalledThemeId) -> PathBuf {
        self.theme_documents_dir()
            .join(format!("{}.toml", id.as_str()))
    }

    pub fn load_or_default(&self) -> Result<ThemeRepositorySnapshot, ThemeRepositoryError> {
        self.load_repository().and_then(snapshot_from_loaded)
    }

    pub fn load_theme_definition(
        &self,
        id: &InstalledThemeId,
    ) -> Result<ThemeDefinition, ThemeRepositoryError> {
        if id_is_built_in(id) {
            return Ok(built_in_theme_definition());
        }
        let loaded = self.load_repository()?;
        loaded
            .themes
            .into_iter()
            .find(|theme| &theme.id == id)
            .map(|theme| theme.definition)
            .ok_or(ThemeRepositoryError::UnknownTheme)
    }

    pub fn activate_theme(
        &self,
        id: &InstalledThemeId,
    ) -> Result<ThemeRepositorySnapshot, ThemeRepositoryError> {
        let mut loaded = self.load_repository()?;
        if !id_is_built_in(id) && !loaded.themes.iter().any(|theme| &theme.id == id) {
            return Err(ThemeRepositoryError::UnknownTheme);
        }
        loaded.active_theme_id = id.clone();
        self.persist_repository(&loaded)?;
        snapshot_from_loaded(loaded)
    }

    pub fn install_theme(
        &self,
        name: impl AsRef<str>,
        definition: ThemeDefinition,
    ) -> Result<ThemeRepositorySnapshot, ThemeRepositoryError> {
        let mut loaded = self.load_repository()?;
        let name = validate_new_theme_name(name.as_ref(), &loaded.themes)?;
        validate_theme_definition(&definition)?;
        let id = InstalledThemeId::generated_from_name(&name, |candidate| {
            loaded
                .themes
                .iter()
                .any(|theme| theme.id.as_str() == candidate)
        });
        loaded.themes.push(InstalledThemeRecord {
            id,
            name,
            definition,
        });
        self.persist_repository(&loaded)?;
        snapshot_from_loaded(loaded)
    }

    pub fn save_as_theme(
        &self,
        name: impl AsRef<str>,
        definition: ThemeDefinition,
    ) -> Result<ThemeRepositorySnapshot, ThemeRepositoryError> {
        let mut loaded = self.load_repository()?;
        let name = validate_new_theme_name(name.as_ref(), &loaded.themes)?;
        validate_theme_definition(&definition)?;
        let id = InstalledThemeId::generated_from_name(&name, |candidate| {
            loaded
                .themes
                .iter()
                .any(|theme| theme.id.as_str() == candidate)
        });
        loaded.active_theme_id = id.clone();
        loaded.themes.push(InstalledThemeRecord {
            id,
            name,
            definition,
        });
        self.persist_repository(&loaded)?;
        snapshot_from_loaded(loaded)
    }

    pub fn update_theme(
        &self,
        id: &InstalledThemeId,
        definition: ThemeDefinition,
    ) -> Result<ThemeRepositorySnapshot, ThemeRepositoryError> {
        if id_is_built_in(id) {
            return Err(ThemeRepositoryError::BuiltInThemeIsReadOnly);
        }
        validate_theme_definition(&definition)?;
        let mut loaded = self.load_repository()?;
        let Some(theme) = loaded.themes.iter_mut().find(|theme| &theme.id == id) else {
            return Err(ThemeRepositoryError::UnknownTheme);
        };
        theme.definition = definition;
        self.persist_repository(&loaded)?;
        snapshot_from_loaded(loaded)
    }

    pub fn rename_theme(
        &self,
        id: &InstalledThemeId,
        name: impl AsRef<str>,
    ) -> Result<ThemeRepositorySnapshot, ThemeRepositoryError> {
        if id_is_built_in(id) {
            return Err(ThemeRepositoryError::BuiltInThemeIsReadOnly);
        }
        let mut loaded = self.load_repository()?;
        let name =
            validate_theme_name(name.as_ref()).ok_or(ThemeRepositoryError::InvalidThemeName)?;
        if loaded
            .themes
            .iter()
            .any(|theme| theme.id != *id && theme.name.eq_ignore_ascii_case(&name))
        {
            return Err(ThemeRepositoryError::DuplicateThemeName);
        }
        let Some(theme) = loaded.themes.iter_mut().find(|theme| &theme.id == id) else {
            return Err(ThemeRepositoryError::UnknownTheme);
        };
        theme.name = name;
        self.persist_repository(&loaded)?;
        snapshot_from_loaded(loaded)
    }

    pub fn delete_theme(
        &self,
        id: &InstalledThemeId,
    ) -> Result<ThemeRepositorySnapshot, ThemeRepositoryError> {
        if id_is_built_in(id) {
            return Err(ThemeRepositoryError::BuiltInThemeIsReadOnly);
        }
        let mut loaded = self.load_repository()?;
        let before = loaded.themes.len();
        loaded.themes.retain(|theme| &theme.id != id);
        if loaded.themes.len() == before {
            return Err(ThemeRepositoryError::UnknownTheme);
        }
        if &loaded.active_theme_id == id {
            loaded.active_theme_id = loaded
                .themes
                .first()
                .map(|theme| theme.id.clone())
                .unwrap_or_else(InstalledThemeId::built_in);
        }
        self.persist_repository(&loaded)?;
        let path = self.theme_document_path(id);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(source) if source.kind() == std_io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(ThemeRepositoryError::WriteFile {
                    path: path.display().to_string(),
                    source,
                });
            }
        }
        snapshot_from_loaded(loaded)
    }
}

fn validate_theme_definition(definition: &ThemeDefinition) -> Result<(), ThemeRepositoryError> {
    ThemeResolver::new(built_in_theme_schema(), definition.clone())
        .map(|_| ())
        .map_err(|source| ThemeRepositoryError::InvalidThemeDefinition { source })
}

fn validate_new_theme_name(
    name: &str,
    themes: &[InstalledThemeRecord],
) -> Result<String, ThemeRepositoryError> {
    let name = validate_theme_name(name).ok_or(ThemeRepositoryError::InvalidThemeName)?;
    if themes
        .iter()
        .any(|theme| theme.name.eq_ignore_ascii_case(&name))
        || name.eq_ignore_ascii_case(BUILT_IN_INSTALLED_THEME_NAME)
    {
        return Err(ThemeRepositoryError::DuplicateThemeName);
    }
    Ok(name)
}

fn recover_duplicate_names(records: &mut [InstalledThemeRecord]) {
    let mut existing = Vec::new();
    for record in records {
        if let Some(name) = unique_recovered_name(&record.name, &mut existing) {
            record.name = name;
        }
    }
}

fn id_is_built_in(id: &InstalledThemeId) -> bool {
    id.as_str() == super::types::BUILT_IN_INSTALLED_THEME_ID
}
