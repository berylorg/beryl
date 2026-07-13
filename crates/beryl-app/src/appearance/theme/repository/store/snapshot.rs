use super::{LoadedThemeRepository, ThemeRepositoryError, ThemeRepositorySnapshot};
use crate::appearance::theme::repository::types::{
    BUILT_IN_INSTALLED_THEME_NAME, InstalledThemeId, InstalledThemeMetadata,
};

impl ThemeRepositorySnapshot {
    pub fn built_in() -> Self {
        Self::new(vec![InstalledThemeMetadata::new(
            InstalledThemeId::built_in(),
            BUILT_IN_INSTALLED_THEME_NAME,
            true,
        )])
    }

    fn new(themes: Vec<InstalledThemeMetadata>) -> Self {
        Self { themes }
    }

    pub fn themes(&self) -> &[InstalledThemeMetadata] {
        &self.themes
    }
}

pub(super) fn snapshot_from_loaded(
    loaded: LoadedThemeRepository,
) -> Result<ThemeRepositorySnapshot, ThemeRepositoryError> {
    let mut themes = Vec::with_capacity(loaded.themes.len().saturating_add(1));
    themes.push(InstalledThemeMetadata::new(
        InstalledThemeId::built_in(),
        BUILT_IN_INSTALLED_THEME_NAME,
        true,
    ));
    themes.extend(
        loaded
            .themes
            .iter()
            .map(|theme| InstalledThemeMetadata::new(theme.id.clone(), theme.name.clone(), false)),
    );

    Ok(ThemeRepositorySnapshot::new(themes))
}
