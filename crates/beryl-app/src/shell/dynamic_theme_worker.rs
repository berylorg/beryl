use std::{
    sync::mpsc::{self, Receiver},
    thread,
};

use super::turn_worker::ShellDynamicToolRequest;
use crate::theme_dynamic_tools::ThemeSaveAsSource;

pub(super) struct DynamicThemeDurableTask {
    pub(super) request: ShellDynamicToolRequest,
    pub(super) receiver: Receiver<DynamicThemeDurableUpdate>,
}

#[derive(Clone)]
pub(super) enum DynamicThemeDurableOperation {
    Install {
        name: String,
        document: crate::ThemeDocument,
    },
    Update {
        theme_id: crate::InstalledThemeId,
        document: crate::ThemeDocument,
    },
    SaveAs {
        name: String,
        source: ThemeSaveAsSource,
    },
    Activate {
        theme_id: crate::InstalledThemeId,
    },
}

pub(super) struct DynamicThemeDurableUpdate {
    pub(super) operation: DynamicThemeDurableOperation,
    pub(super) result: Result<DynamicThemeDurableResult, (&'static str, String)>,
}

pub(super) struct DynamicThemeDurableResult {
    pub(super) snapshot: crate::ThemeRepositorySnapshot,
    pub(super) changed: bool,
}

pub(super) fn spawn_dynamic_theme_durable_worker(
    operation: DynamicThemeDurableOperation,
    store: crate::ThemeRepositoryStore,
) -> Receiver<DynamicThemeDurableUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = run_dynamic_theme_durable_operation(&operation, &store);
        let _ = sender.send(DynamicThemeDurableUpdate { operation, result });
    });
    receiver
}

fn theme_repository_tool_error(error: crate::ThemeRepositoryError) -> (&'static str, String) {
    let kind = match &error {
        crate::ThemeRepositoryError::InvalidThemeName => "invalid_theme_name",
        crate::ThemeRepositoryError::DuplicateThemeName => "duplicate_theme_name",
        crate::ThemeRepositoryError::UnknownTheme => "unknown_theme",
        crate::ThemeRepositoryError::BuiltInThemeIsReadOnly => "read_only_theme",
        crate::ThemeRepositoryError::InvalidThemeDefinition { .. }
        | crate::ThemeRepositoryError::Projection { .. }
        | crate::ThemeRepositoryError::Document { .. } => "invalid_theme_document",
        crate::ThemeRepositoryError::CreateDirectory { .. }
        | crate::ThemeRepositoryError::ReadFile { .. }
        | crate::ThemeRepositoryError::WriteFile { .. }
        | crate::ThemeRepositoryError::SerializeManifest { .. }
        | crate::ThemeRepositoryError::ParseManifest { .. } => "theme_repository_error",
    };
    (kind, error.to_string())
}

fn run_dynamic_theme_durable_operation(
    operation: &DynamicThemeDurableOperation,
    store: &crate::ThemeRepositoryStore,
) -> Result<DynamicThemeDurableResult, (&'static str, String)> {
    match operation {
        DynamicThemeDurableOperation::Install { name, document } => Ok(store
            .install_theme(name, document.definition().clone())
            .map(|snapshot| DynamicThemeDurableResult {
                snapshot,
                changed: true,
            })
            .map_err(theme_repository_tool_error)?),
        DynamicThemeDurableOperation::Update { theme_id, document } => {
            if theme_id.as_str() == crate::BUILT_IN_INSTALLED_THEME_ID {
                return Err(theme_repository_tool_error(
                    crate::ThemeRepositoryError::BuiltInThemeIsReadOnly,
                ));
            }
            let previous = store
                .load_theme_definition(theme_id)
                .map_err(theme_repository_tool_error)?;
            let definition = document.definition().clone();
            let changed = previous != definition;
            let snapshot = if changed {
                store
                    .update_theme(theme_id, definition)
                    .map_err(theme_repository_tool_error)?
            } else {
                store
                    .load_or_default()
                    .map_err(theme_repository_tool_error)?
            };
            Ok(DynamicThemeDurableResult { snapshot, changed })
        }
        DynamicThemeDurableOperation::SaveAs { name, source } => {
            let definition = match source {
                ThemeSaveAsSource::Document(document) => document.definition().clone(),
                ThemeSaveAsSource::ExistingTheme(theme_id) => store
                    .load_theme_definition(theme_id)
                    .map_err(theme_repository_tool_error)?,
            };
            Ok(store
                .save_as_theme(name, definition)
                .map(|snapshot| DynamicThemeDurableResult {
                    snapshot,
                    changed: true,
                })
                .map_err(theme_repository_tool_error)?)
        }
        DynamicThemeDurableOperation::Activate { theme_id } => {
            let loaded = store
                .load_or_default()
                .map_err(theme_repository_tool_error)?;
            let changed = loaded.active_theme_id() != theme_id;
            let snapshot = if changed {
                store
                    .activate_theme(theme_id)
                    .map_err(theme_repository_tool_error)?
            } else {
                loaded
            };
            Ok(DynamicThemeDurableResult { snapshot, changed })
        }
    }
}
