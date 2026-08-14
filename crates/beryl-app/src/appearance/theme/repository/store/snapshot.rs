use super::{LoadedThemeRepository, ThemeRepositoryError, ThemeRepositorySnapshot, id_is_built_in};
use crate::appearance::theme::{
    ActiveThemeProjection, ThemeDefinition, ThemeResolver, built_in_theme_definition,
    built_in_theme_schema,
    repository::types::{BUILT_IN_INSTALLED_THEME_NAME, InstalledThemeId, InstalledThemeMetadata},
};

impl ThemeRepositorySnapshot {
    pub fn built_in() -> Self {
        Self::new(
            InstalledThemeId::built_in(),
            vec![InstalledThemeMetadata::new(
                InstalledThemeId::built_in(),
                BUILT_IN_INSTALLED_THEME_NAME,
                true,
                true,
            )],
            built_in_theme_definition(),
        )
        .expect("built-in theme repository snapshot must be valid")
    }

    fn new(
        active_theme_id: InstalledThemeId,
        themes: Vec<InstalledThemeMetadata>,
        active_definition: ThemeDefinition,
    ) -> Result<Self, ThemeRepositoryError> {
        let active_projection = projection_from_definition(&active_definition)?;
        Ok(Self {
            active_theme_id,
            themes,
            active_definition,
            active_projection,
        })
    }

    pub fn active_theme_id(&self) -> &InstalledThemeId {
        &self.active_theme_id
    }

    pub fn themes(&self) -> &[InstalledThemeMetadata] {
        &self.themes
    }

    pub fn active_definition(&self) -> &ThemeDefinition {
        &self.active_definition
    }

    pub fn active_projection(&self) -> &ActiveThemeProjection {
        &self.active_projection
    }
}

pub(super) fn snapshot_from_loaded(
    loaded: LoadedThemeRepository,
) -> Result<ThemeRepositorySnapshot, ThemeRepositoryError> {
    let active_definition = if id_is_built_in(&loaded.active_theme_id) {
        built_in_theme_definition()
    } else {
        loaded
            .themes
            .iter()
            .find(|theme| theme.id == loaded.active_theme_id)
            .map(|theme| theme.definition.clone())
            .unwrap_or_else(built_in_theme_definition)
    };

    let mut themes = Vec::with_capacity(loaded.themes.len().saturating_add(1));
    themes.push(InstalledThemeMetadata::new(
        InstalledThemeId::built_in(),
        BUILT_IN_INSTALLED_THEME_NAME,
        true,
        id_is_built_in(&loaded.active_theme_id),
    ));
    themes.extend(loaded.themes.iter().map(|theme| {
        InstalledThemeMetadata::new(
            theme.id.clone(),
            theme.name.clone(),
            false,
            theme.id == loaded.active_theme_id,
        )
    }));

    ThemeRepositorySnapshot::new(loaded.active_theme_id, themes, active_definition)
}

fn projection_from_definition(
    definition: &ThemeDefinition,
) -> Result<ActiveThemeProjection, ThemeRepositoryError> {
    let resolver = ThemeResolver::new(built_in_theme_schema(), definition.clone())
        .map_err(|source| ThemeRepositoryError::InvalidThemeDefinition { source })?;
    ActiveThemeProjection::from_built_in_resolver(resolver)
        .map_err(|source| ThemeRepositoryError::Projection { source })
}
