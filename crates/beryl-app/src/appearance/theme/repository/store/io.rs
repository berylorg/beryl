use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    io::Write,
    path::Path,
};

use serde::{Deserialize, Serialize};

use super::{
    InstalledThemeRecord, LoadedThemeRepository, THEME_REPOSITORY_SCHEMA_VERSION,
    ThemeRepositoryError, ThemeRepositoryStore, id_is_built_in, recover_duplicate_names,
};
use crate::appearance::theme::{
    ThemeDocument,
    repository::types::{InstalledThemeId, validate_theme_name},
};

#[derive(Debug, Default, Deserialize, Serialize)]
struct ManifestToml {
    schema: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active_theme_id: Option<String>,
    #[serde(default, rename = "theme")]
    themes: Vec<ManifestThemeToml>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ManifestThemeToml {
    id: String,
    name: String,
    file: String,
}

impl ThemeRepositoryStore {
    pub(super) fn load_repository(&self) -> Result<LoadedThemeRepository, ThemeRepositoryError> {
        let manifest = self.read_manifest()?;
        let mut records = self.read_theme_documents()?;
        let mut ordered = Vec::new();
        let mut retained_ids = BTreeSet::new();

        if let Some(manifest) = manifest.as_ref() {
            for entry in &manifest.themes {
                let Ok(id) = InstalledThemeId::new(entry.id.clone()) else {
                    continue;
                };
                let Some(mut record) = records.remove(&id) else {
                    continue;
                };
                if let Some(name) = validate_theme_name(&entry.name) {
                    record.name = name;
                }
                if retained_ids.insert(id.clone()) {
                    ordered.push(record);
                }
            }
        }

        for (_, record) in records {
            if retained_ids.insert(record.id.clone()) {
                ordered.push(record);
            }
        }
        recover_duplicate_names(&mut ordered);

        let active_theme_id = manifest
            .and_then(|manifest| manifest.active_theme_id)
            .and_then(|id| InstalledThemeId::new(id).ok())
            .filter(|id| id_is_built_in(id) || ordered.iter().any(|theme| &theme.id == id))
            .unwrap_or_else(InstalledThemeId::built_in);

        Ok(LoadedThemeRepository {
            active_theme_id,
            themes: ordered,
        })
    }

    fn read_manifest(&self) -> Result<Option<ManifestToml>, ThemeRepositoryError> {
        let path = self.manifest_path();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(ThemeRepositoryError::ReadFile {
                    path: path.display().to_string(),
                    source,
                });
            }
        };
        let Ok(manifest) = toml::from_str::<ManifestToml>(&text) else {
            return Ok(None);
        };
        if manifest.schema != THEME_REPOSITORY_SCHEMA_VERSION {
            return Ok(None);
        }
        Ok(Some(manifest))
    }

    fn read_theme_documents(
        &self,
    ) -> Result<BTreeMap<InstalledThemeId, InstalledThemeRecord>, ThemeRepositoryError> {
        let mut records = BTreeMap::new();
        let dir = self.theme_documents_dir();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(records),
            Err(source) => {
                return Err(ThemeRepositoryError::ReadFile {
                    path: dir.display().to_string(),
                    source,
                });
            }
        };
        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| ThemeRepositoryError::ReadFile {
                path: dir.display().to_string(),
                source,
            })?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) == Some("toml") {
                paths.push(path);
            }
        }
        paths.sort();

        for path in paths {
            let Some(record) = self.read_theme_document_record(&path)? else {
                continue;
            };
            if !id_is_built_in(&record.id) {
                records.entry(record.id.clone()).or_insert(record);
            }
        }
        Ok(records)
    }

    fn read_theme_document_record(
        &self,
        path: &Path,
    ) -> Result<Option<InstalledThemeRecord>, ThemeRepositoryError> {
        let text = fs::read_to_string(path).map_err(|source| ThemeRepositoryError::ReadFile {
            path: path.display().to_string(),
            source,
        })?;
        let Ok(document) = ThemeDocument::from_toml_str(&text) else {
            return Ok(None);
        };
        let id = match document.id().cloned().or_else(|| id_from_file_stem(path)) {
            Some(id) => id,
            None => return Ok(None),
        };
        let name = document
            .name()
            .and_then(validate_theme_name)
            .unwrap_or_else(|| id.as_str().to_string());
        Ok(Some(InstalledThemeRecord {
            id,
            name,
            definition: document.into_definition(),
        }))
    }

    pub(super) fn persist_repository(
        &self,
        loaded: &LoadedThemeRepository,
    ) -> Result<(), ThemeRepositoryError> {
        ensure_directory(&self.theme_documents_dir())?;
        for theme in &loaded.themes {
            self.write_theme_document(theme)?;
        }
        self.write_manifest(loaded)
    }

    fn write_theme_document(
        &self,
        theme: &InstalledThemeRecord,
    ) -> Result<(), ThemeRepositoryError> {
        let document = ThemeDocument::new(
            Some(theme.id.clone()),
            Some(theme.name.clone()),
            theme.definition.clone(),
        )?;
        let text = document.to_toml_string()?;
        write_file_atomically(
            &self.theme_documents_dir(),
            &self.theme_document_path(&theme.id),
            &text,
        )
    }

    fn write_manifest(&self, loaded: &LoadedThemeRepository) -> Result<(), ThemeRepositoryError> {
        ensure_directory(&self.repository_dir())?;
        let manifest = ManifestToml {
            schema: THEME_REPOSITORY_SCHEMA_VERSION,
            active_theme_id: Some(loaded.active_theme_id.as_str().to_string()),
            themes: loaded
                .themes
                .iter()
                .map(|theme| ManifestThemeToml {
                    id: theme.id.as_str().to_string(),
                    name: theme.name.clone(),
                    file: theme_file_name(&theme.id),
                })
                .collect(),
        };
        let text = toml::to_string_pretty(&manifest)
            .map_err(|source| ThemeRepositoryError::SerializeManifest { source })?;
        write_file_atomically(&self.repository_dir(), &self.manifest_path(), &text)
    }
}

fn ensure_directory(path: &Path) -> Result<(), ThemeRepositoryError> {
    fs::create_dir_all(path).map_err(|source| ThemeRepositoryError::CreateDirectory {
        path: path.display().to_string(),
        source,
    })
}

fn write_file_atomically(
    temp_dir: &Path,
    target: &Path,
    text: &str,
) -> Result<(), ThemeRepositoryError> {
    ensure_directory(temp_dir)?;
    let mut temp_file = tempfile::NamedTempFile::new_in(temp_dir).map_err(|source| {
        ThemeRepositoryError::WriteFile {
            path: target.display().to_string(),
            source,
        }
    })?;
    temp_file
        .write_all(text.as_bytes())
        .map_err(|source| ThemeRepositoryError::WriteFile {
            path: temp_file.path().display().to_string(),
            source,
        })?;
    temp_file
        .persist(target)
        .map_err(|error| ThemeRepositoryError::WriteFile {
            path: target.display().to_string(),
            source: error.error,
        })?;
    Ok(())
}

fn id_from_file_stem(path: &Path) -> Option<InstalledThemeId> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| InstalledThemeId::new(stem.to_string()).ok())
}

fn theme_file_name(id: &InstalledThemeId) -> String {
    format!("{}.toml", id.as_str())
}
