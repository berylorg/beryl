use std::ops::Range;

use beryl_state::{InstalledThemeSelection, ThemeManifestCursor, ThemeObservedDocument};

use super::*;
use crate::theme_runtime::{ThemeSettingsCatalog, ThemeSettingsFocus, ThemeSettingsPage};

impl ThemeSettingsSource {
    pub(crate) fn observe_catalog(
        &self,
        store: &HomeStore,
    ) -> Result<ThemeSettingsCatalog, ThemeRuntimeFailureClass> {
        self.check(store)?;
        let observation = load::ThemeLoadAccess::from(store)
            .observe(&self.service, self.config)
            .map_err(|error| map_repository_load_failure(&error))?;
        let mut session = self
            .service
            .open_manifest(
                store,
                &observation,
                self.config.max_manifest_bytes,
                self.config.manifest_read,
            )
            .map_err(|error| map_repository_load_failure(&error))?;
        let mut cursor = ThemeManifestCursor::first(observation.manifest());
        let mut count = 0usize;
        loop {
            let page = session
                .read_page(cursor, self.config.page)
                .map_err(|error| map_repository_load_failure(&error))?;
            count = count
                .checked_add(page.records().len())
                .ok_or(ThemeRuntimeFailureClass::ManifestByteLimit)?;
            self.check(store)?;
            match page.next() {
                Some(next) => cursor = next,
                None => break,
            }
        }
        Ok(ThemeSettingsCatalog {
            observation,
            logical_item_count: count,
        })
    }

    pub(crate) fn catalog_range(
        &self,
        store: &HomeStore,
        observation: ThemeRepositoryObservation,
        range: Range<usize>,
        focus: Option<InstalledThemeId>,
    ) -> Result<ThemeSettingsPage, ThemeRuntimeFailureClass> {
        self.check(store)?;
        if range.start > range.end || range.len() > self.config.page.max_items() {
            return Err(ThemeRuntimeFailureClass::Identity);
        }
        let mut session = self
            .service
            .open_manifest(
                store,
                &observation,
                self.config.max_manifest_bytes,
                self.config.manifest_read,
            )
            .map_err(|error| map_repository_load_failure(&error))?;
        let mut cursor = ThemeManifestCursor::first(observation.manifest());
        let mut position = 0usize;
        let mut selections = Vec::with_capacity(range.len());
        let mut decoded_bytes = 0usize;
        let mut found = None;
        loop {
            let page = session
                .read_page(cursor, self.config.page)
                .map_err(|error| map_repository_load_failure(&error))?;
            for (index, record) in page.records().iter().enumerate() {
                if focus.as_ref() == Some(record.id()) {
                    found = Some(ThemeSettingsFocus::Found { position });
                }
                if range.contains(&position) {
                    decoded_bytes = decoded_bytes
                        .checked_add(record.id().as_str().len())
                        .and_then(|value| value.checked_add(record.name().as_str().len()))
                        .and_then(|value| value.checked_add(8))
                        .ok_or(ThemeRuntimeFailureClass::ManifestByteLimit)?;
                    if decoded_bytes > self.config.page.max_decoded_bytes() {
                        return Err(ThemeRuntimeFailureClass::ManifestByteLimit);
                    }
                    selections.push(page.selection(index).expect("enumerated record"));
                }
                position = position
                    .checked_add(1)
                    .ok_or(ThemeRuntimeFailureClass::ManifestByteLimit)?;
            }
            self.check(store)?;
            if position >= range.end && (focus.is_none() || found.is_some()) {
                break;
            }
            match page.next() {
                Some(next) => cursor = next,
                None => {
                    if position < range.end {
                        return Err(ThemeRuntimeFailureClass::Identity);
                    }
                    break;
                }
            }
        }
        let focus = focus.map(|_| found.unwrap_or(ThemeSettingsFocus::Removed));
        Ok(ThemeSettingsPage {
            observation,
            range,
            selections,
            focus,
        })
    }

    pub(crate) fn selected_document(
        &self,
        store: &HomeStore,
        observation: &ThemeRepositoryObservation,
        selection: &InstalledThemeSelection,
    ) -> Result<ThemeObservedDocument, ThemeRuntimeFailureClass> {
        self.check(store)?;
        let document = self
            .service
            .load_document(store, observation, selection, None)
            .map_err(|error| {
                map_live_failure(map_live_load_failure(&load::ActiveLoadError::Document(
                    error,
                )))
            })?;
        self.check(store)?;
        Ok(document)
    }

    pub(crate) fn prepare_current(
        &self,
        store: &HomeStore,
        settings: ThemeSettingsIdentity,
        active: Option<&InstalledThemeId>,
    ) -> Result<PreparedThemeAppearance, ThemeRuntimeFailureClass> {
        self.prepare_current_bound(store, settings, active, None)
    }
}
