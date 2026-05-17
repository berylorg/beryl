use std::{
    collections::{HashMap, VecDeque},
    sync::mpsc::{self, Receiver},
    thread,
};

use thiserror::Error;

use crate::{
    ActiveThemeProjection, InstalledThemeId, ThemeDefinition, ThemeDocument, ThemeDocumentError,
    ThemeRepositoryError, ThemeRepositorySnapshot, ThemeRepositoryStore, ThemeResolutionError,
    ThemeResolver, ThemeValidationDiagnostics, built_in_theme_schema,
};

pub(super) const BERYL_THEME_LANGUAGE_LABEL: &str = "beryl-theme";
const MAX_PANEL_FEEDBACK_ENTRIES: usize = 128;
const MAX_PANEL_FEEDBACK_BYTES: usize = 240;

#[derive(Clone, Debug)]
pub(super) struct ValidatedThemeCandidate {
    definition: ThemeDefinition,
    install_name: Option<String>,
    preview_projection: ActiveThemeProjection,
}

#[derive(Debug, Error)]
pub(super) enum ThemeCandidateValidationError {
    #[error("theme document is invalid: {source}")]
    Document {
        #[from]
        source: ThemeDocumentError,
    },
    #[error("candidate theme id `{id}` is already installed")]
    DuplicateEmbeddedId { id: InstalledThemeId },
    #[error("theme definition is invalid: {source}")]
    InvalidDefinition {
        #[source]
        source: ThemeValidationDiagnostics,
    },
    #[error("theme candidate could not be projected: {source}")]
    Projection {
        #[source]
        source: ThemeResolutionError,
    },
    #[error("theme candidate must include a top-level name before installation")]
    MissingInstallName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ThemeCandidateFeedbackKind {
    Info,
    Success,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ThemeCandidatePanelFeedback {
    kind: ThemeCandidateFeedbackKind,
    message: String,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ThemeCandidatePanelSnapshot {
    active_preview_panel_id: Option<String>,
    feedback: HashMap<String, ThemeCandidatePanelFeedback>,
}

#[derive(Default)]
pub(super) struct ThemeCandidateState {
    active_preview: Option<ActiveThemeCandidatePreview>,
    feedback: HashMap<String, ThemeCandidatePanelFeedback>,
    feedback_order: VecDeque<String>,
}

#[derive(Clone)]
struct ActiveThemeCandidatePreview {
    panel_id: String,
    selected_thread_id: Option<String>,
    restore_projection: ActiveThemeProjection,
}

pub(super) struct ThemeCandidateInstallUpdate {
    pub(super) panel_id: String,
    pub(super) name: String,
    pub(super) result: Result<ThemeRepositorySnapshot, String>,
}

impl ValidatedThemeCandidate {
    pub(super) fn definition(&self) -> &ThemeDefinition {
        &self.definition
    }

    pub(super) fn install_name(&self) -> Result<&str, ThemeCandidateValidationError> {
        self.install_name
            .as_deref()
            .ok_or(ThemeCandidateValidationError::MissingInstallName)
    }

    pub(super) fn preview_projection(&self) -> &ActiveThemeProjection {
        &self.preview_projection
    }
}

impl ThemeCandidatePanelFeedback {
    pub(super) fn info(message: impl AsRef<str>) -> Self {
        Self::new(ThemeCandidateFeedbackKind::Info, message)
    }

    pub(super) fn success(message: impl AsRef<str>) -> Self {
        Self::new(ThemeCandidateFeedbackKind::Success, message)
    }

    pub(super) fn error(message: impl AsRef<str>) -> Self {
        Self::new(ThemeCandidateFeedbackKind::Error, message)
    }

    fn new(kind: ThemeCandidateFeedbackKind, message: impl AsRef<str>) -> Self {
        Self {
            kind,
            message: bounded_feedback(message.as_ref()),
        }
    }

    pub(super) fn kind(&self) -> ThemeCandidateFeedbackKind {
        self.kind
    }

    pub(super) fn message(&self) -> &str {
        self.message.as_str()
    }
}

impl ThemeCandidatePanelSnapshot {
    pub(super) fn active_preview_panel_id(&self) -> Option<&str> {
        self.active_preview_panel_id.as_deref()
    }

    pub(super) fn feedback(&self, panel_id: &str) -> Option<&ThemeCandidatePanelFeedback> {
        self.feedback.get(panel_id)
    }
}

impl ThemeCandidateState {
    pub(super) fn snapshot(&self) -> ThemeCandidatePanelSnapshot {
        ThemeCandidatePanelSnapshot {
            active_preview_panel_id: self
                .active_preview
                .as_ref()
                .map(|preview| preview.panel_id.clone()),
            feedback: self.feedback.clone(),
        }
    }

    pub(super) fn restore_projection_for_new_preview(
        &self,
        current_projection: ActiveThemeProjection,
    ) -> ActiveThemeProjection {
        self.active_preview
            .as_ref()
            .map(|preview| preview.restore_projection.clone())
            .unwrap_or(current_projection)
    }

    pub(super) fn start_preview(
        &mut self,
        panel_id: String,
        selected_thread_id: Option<String>,
        restore_projection: ActiveThemeProjection,
    ) {
        self.active_preview = Some(ActiveThemeCandidatePreview {
            panel_id: panel_id.clone(),
            selected_thread_id,
            restore_projection,
        });
        self.set_feedback(
            panel_id,
            ThemeCandidatePanelFeedback::success("Preview active"),
        );
    }

    pub(super) fn stop_preview_for_panel(
        &mut self,
        panel_id: &str,
    ) -> Option<ActiveThemeProjection> {
        if self
            .active_preview
            .as_ref()
            .is_some_and(|preview| preview.panel_id == panel_id)
        {
            return self.stop_active_preview();
        }
        None
    }

    pub(super) fn stop_active_preview(&mut self) -> Option<ActiveThemeProjection> {
        self.active_preview
            .take()
            .map(|preview| preview.restore_projection)
    }

    pub(super) fn clear_after_durable_theme_change(&mut self) {
        self.active_preview = None;
    }

    pub(super) fn restore_if_thread_changed(
        &mut self,
        selected_thread_id: Option<&str>,
    ) -> Option<ActiveThemeProjection> {
        if self
            .active_preview
            .as_ref()
            .is_some_and(|preview| preview.selected_thread_id.as_deref() != selected_thread_id)
        {
            return self.stop_active_preview();
        }
        None
    }

    pub(super) fn set_feedback(&mut self, panel_id: String, feedback: ThemeCandidatePanelFeedback) {
        if !self.feedback.contains_key(&panel_id) {
            self.feedback_order.push_back(panel_id.clone());
        }
        self.feedback.insert(panel_id, feedback);
        while self.feedback_order.len() > MAX_PANEL_FEEDBACK_ENTRIES {
            if let Some(evicted) = self.feedback_order.pop_front() {
                self.feedback.remove(&evicted);
            }
        }
    }
}

pub(super) fn is_theme_candidate_language(language: Option<&str>) -> bool {
    language
        .and_then(|label| label.split_whitespace().next())
        .map(|label| label.trim_matches(['`', '\'', '"']))
        .is_some_and(|label| label.eq_ignore_ascii_case(BERYL_THEME_LANGUAGE_LABEL))
}

pub(super) fn validate_theme_candidate(
    source: &str,
    repository: &ThemeRepositorySnapshot,
) -> Result<ValidatedThemeCandidate, ThemeCandidateValidationError> {
    let document = ThemeDocument::from_toml_str(source)?;
    if let Some(id) = document.id()
        && repository.themes().iter().any(|theme| theme.id() == id)
    {
        return Err(ThemeCandidateValidationError::DuplicateEmbeddedId { id: id.clone() });
    }

    let install_name = document.name().map(str::to_string);
    let definition = document.into_definition();
    let resolver = ThemeResolver::new(built_in_theme_schema(), definition.clone())
        .map_err(|source| ThemeCandidateValidationError::InvalidDefinition { source })?;
    let preview_projection = ActiveThemeProjection::from_built_in_resolver(resolver)
        .map_err(|source| ThemeCandidateValidationError::Projection { source })?;

    Ok(ValidatedThemeCandidate {
        definition,
        install_name,
        preview_projection,
    })
}

pub(super) fn spawn_theme_candidate_install_worker(
    panel_id: String,
    name: String,
    definition: ThemeDefinition,
    store: ThemeRepositoryStore,
) -> Receiver<ThemeCandidateInstallUpdate> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = store
            .install_theme(&name, definition)
            .map_err(theme_repository_error_message);
        let _ = sender.send(ThemeCandidateInstallUpdate {
            panel_id,
            name,
            result,
        });
    });
    receiver
}

fn theme_repository_error_message(error: ThemeRepositoryError) -> String {
    error.to_string()
}

fn bounded_feedback(message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.len() <= MAX_PANEL_FEEDBACK_BYTES {
        return trimmed.to_string();
    }

    let mut output = String::new();
    for ch in trimmed.chars() {
        if output.len().saturating_add(ch.len_utf8()).saturating_add(3) > MAX_PANEL_FEEDBACK_BYTES {
            output.push_str("...");
            return output;
        }
        output.push(ch);
    }
    output
}
