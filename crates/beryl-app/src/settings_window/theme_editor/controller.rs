use std::num::NonZeroU64;

use beryl_state::{
    InstalledThemeId, SaveTheme, SaveThemeAs, SettingKey, ThemeDocumentDraft, ThemeDraftIdentity,
    ThemeDraftRevision, ThemeName, ThemeRepositoryCommand, ThemeRepositoryCommit,
};
use gpui::Context;

use super::*;
use crate::settings_window::{
    controller::SettingsWindowController, fields::Route, themes::commands::RepositoryKind,
};

impl SettingsWindowController {
    fn editor_is_active(&self) -> bool {
        self.theme_state
            .editor
            .as_ref()
            .zip(self.draft.as_ref())
            .is_some_and(|(editor, draft)| {
                draft.baseline().row(SettingKey::ActiveThemeId).value
                    == editor.baseline.identity().theme_id().as_str()
            })
    }
    fn next_editor_revision(&mut self) -> Option<u64> {
        self.editor_revision = self.editor_revision.checked_add(1)?;
        Some(self.editor_revision)
    }

    pub(in crate::settings_window) fn bound_theme_document_id(&self) -> Option<InstalledThemeId> {
        self.theme_state
            .editor
            .as_ref()
            .map(|editor| editor.baseline.identity().theme_id().clone())
    }

    pub(in crate::settings_window) fn open_theme_editor(&mut self, cx: &mut Context<Self>) {
        if self.exit_is_gated() || !self.home_available || !self.source_current {
            return;
        }
        let Some(document) = self.theme_state.document.clone() else {
            return;
        };
        if !self.draft.as_ref().is_some_and(|draft| {
            draft.baseline().row(SettingKey::ActiveThemeId).value
                == document.identity().theme_id().as_str()
        }) {
            return;
        }
        if self.theme_state.editor.as_ref().is_none_or(|editor| {
            editor.baseline.identity().theme_id() != document.identity().theme_id()
        }) {
            let Some(revision) = self.next_editor_revision() else {
                return;
            };
            self.theme_state.editor = Some(ThemeDocumentEditor::new(document, revision, revision));
        }
        self.change_route("theme-editor");
        self.sync_theme_navigator(cx);
        self.publish(cx);
    }

    pub(in crate::settings_window) fn sync_theme_navigator(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &self.theme_state.editor else {
            return;
        };
        let projection = CanonicalRoleSource::projection(
            editor.projection_revision,
            &editor.selected_role,
            editor.rows(None).len(),
        );
        let (navigator, generation, admission) = {
            let state = self.appearance.borrow();
            (
                state.navigator.clone(),
                state.generation.clone(),
                state.navigator_admission.clone(),
            )
        };
        if let Some(navigator) = navigator {
            if navigator
                .update(cx, |navigator, cx| {
                    navigator.sync(projection, generation, cx)
                })
                .is_err()
            {
                self.theme_state.message =
                    Some("The role navigator could not preserve its current identity.");
            }
            return;
        }
        match ThemeRoleNavigator::mount(projection, generation, admission, cx) {
            Ok(navigator) => {
                self.navigator_subscription =
                    Some(cx.subscribe(&navigator, |owner, source, event, cx| {
                        let ThemeRoleNavigatorEvent::RoleActivated { revision, role } = event;
                        if owner.route != Route::ThemeEditor
                            || owner.exit_is_gated()
                            || owner
                                .appearance
                                .borrow()
                                .navigator
                                .as_ref()
                                .is_none_or(|current| current.entity_id() != source.entity_id())
                        {
                            return;
                        }
                        let admitted = owner
                            .appearance
                            .borrow()
                            .navigator_admission
                            .borrow()
                            .as_ref()
                            .is_some_and(|admission| admission.is_open());
                        if !admitted
                            || owner
                                .theme_state
                                .editor
                                .as_ref()
                                .is_none_or(|editor| editor.projection_revision != *revision)
                        {
                            return;
                        }
                        let Some(next) = owner.next_editor_revision() else {
                            return;
                        };
                        if let Some(editor) = &mut owner.theme_state.editor {
                            editor.selected_role = role.clone();
                            editor.projection_revision = next;
                        }
                        owner.sync_theme_navigator(cx);
                        owner.publish(cx);
                    }));
                let mut state = self.appearance.borrow_mut();
                state.navigator = Some(navigator);
                state.mount_revision = state
                    .mount_revision
                    .checked_add(1)
                    .expect("bounded window navigator lifetime");
            }
            Err(_) => {
                self.theme_state.message = Some("The role navigator appearance is unavailable.")
            }
        }
    }

    pub(in crate::settings_window) fn deactivate_theme_editor(&mut self, cx: &mut Context<Self>) {
        let (navigator, admission) = {
            let state = self.appearance.borrow();
            (state.navigator.clone(), state.navigator_admission.clone())
        };
        *admission.borrow_mut() = None;
        if let (Some(window), Some(navigator)) = (&self.window, navigator) {
            let _ = window.window_handle().update(cx, |_, window, cx| {
                navigator.update(cx, |navigator, cx| navigator.deactivate(window, cx))
            });
        }
    }

    pub(in crate::settings_window) fn stage_theme_property(
        &mut self,
        id: &str,
        value: String,
        cx: &mut Context<Self>,
    ) {
        if self.route != Route::ThemeEditor
            || self.exit_is_gated()
            || !self.home_available
            || !self.source_current
            || self.theme_state.repository_unavailable
            || !self.theme_state.editor_source_is_current()
        {
            return;
        }
        if id == "theme.save-as-name" {
            if value.len() <= 128 {
                if let Some(editor) = &mut self.theme_state.editor {
                    editor.save_as_name = value;
                    editor.feedback = None;
                }
                self.publish(cx);
            }
            return;
        }
        let Some(revision) = self.next_editor_revision() else {
            return;
        };
        if self
            .theme_state
            .editor
            .as_mut()
            .is_some_and(|editor| editor.stage(id, value, revision))
        {
            self.schedule_theme_validation(cx);
            self.publish(cx);
        }
    }

    fn schedule_theme_validation(&mut self, cx: &mut Context<Self>) {
        if self.editor_worker.is_some() {
            return;
        }
        let Some(editor) = &self.theme_state.editor else {
            return;
        };
        let revision = editor.revision;
        let binding = editor.baseline.identity().clone();
        let document = editor.clean_document.clone();
        let changes = editor.changes.clone();
        let work = cx
            .background_executor()
            .spawn(async move { validate_changes(document, changes) });
        self.editor_worker = Some(cx.spawn(async move |owner, cx| {
            let result = work.await;
            let _ = owner.update(cx, |owner, cx| {
                owner.editor_worker = None;
                let Some(editor) = &mut owner.theme_state.editor else {
                    return;
                };
                if editor.baseline.identity() != &binding {
                    return;
                }
                if editor.revision != revision {
                    owner.schedule_theme_validation(cx);
                    return;
                }
                editor.validating = false;
                editor.validated_revision = revision;
                match result {
                    Ok(document) => {
                        editor.validated = Some(document);
                        editor.issue = None;
                    }
                    Err(issue) => {
                        editor.validated = None;
                        editor.issue = Some(issue);
                    }
                }
                owner.publish(cx);
            });
        }));
    }

    pub(in crate::settings_window) fn save_theme_document(
        &mut self,
        save_as: bool,
        cx: &mut Context<Self>,
    ) {
        if !matches!(self.route, Route::Themes | Route::ThemeEditor)
            || (self.route == Route::Themes
                && (!self.editor_is_active()
                    || self
                        .theme_state
                        .editor
                        .as_ref()
                        .zip(self.theme_state.selected.as_ref())
                        .is_none_or(|(editor, selection)| {
                            editor.baseline.identity().theme_id() != selection.id()
                        })))
            || self.exit_is_gated()
            || !self.home_available
            || !self.source_current
            || self.theme_state.repository_unavailable
            || !self.theme_state.editor_source_is_current()
        {
            return;
        }
        let Some(editor) = &self.theme_state.editor else {
            return;
        };
        if !editor.can_save() {
            return;
        }
        let Ok(draft) = ThemeDocumentDraft::new(
            ThemeDraftIdentity::new(
                NonZeroU64::new(self.window_instance).expect("window identity"),
            ),
            ThemeDraftRevision::new(NonZeroU64::new(editor.revision).expect("draft revision")),
            editor.baseline.identity().clone(),
            editor.validated.clone().expect("validated current draft"),
        ) else {
            return;
        };
        let command = if save_as {
            let Ok(name) = ThemeName::new(&editor.save_as_name) else {
                return;
            };
            let mut bytes = [0u8; 16];
            if getrandom::fill(&mut bytes).is_err() {
                self.theme_state.message = Some("A fresh theme identity could not be allocated.");
                self.publish(cx);
                return;
            }
            let mut id = String::from("theme-");
            for byte in bytes {
                use std::fmt::Write as _;
                let _ = write!(id, "{byte:02x}");
            }
            let Ok(id) = InstalledThemeId::new(id) else {
                return;
            };
            let Ok(command) =
                SaveThemeAs::new(editor.baseline.identity().manifest(), draft, id, name)
            else {
                return;
            };
            ThemeRepositoryCommand::SaveAs(command)
        } else {
            ThemeRepositoryCommand::Save(SaveTheme::new(draft))
        };
        self.start_repository(
            command,
            if save_as {
                RepositoryKind::SaveAs
            } else {
                RepositoryKind::Save
            },
            cx,
        );
    }

    pub(in crate::settings_window) fn finish_theme_save(
        &mut self,
        kind: RepositoryKind,
        publication: &ThemeRepositoryCommit,
    ) {
        let Some(editor) = &mut self.theme_state.editor else {
            return;
        };
        if matches!(kind, RepositoryKind::Save) {
            editor.saved_identity = publication
                .affected_documents()
                .iter()
                .find(|identity| identity.theme_id() == editor.baseline.identity().theme_id())
                .cloned();
            if editor.saved_identity.is_some() {
                editor.clean_document = editor
                    .validated
                    .clone()
                    .expect("the original saved document was validated before admission");
                editor.changes.clear();
                editor.feedback = None;
                editor.issue = None;
            } else {
                editor.issue = Some("The saved document's exact new source is unavailable.");
                self.theme_state.repository_unavailable = true;
            }
        }
    }

    pub(in crate::settings_window) fn qualify_editor_document(
        &mut self,
        document: &ThemeObservedDocument,
    ) {
        let Some(editor) = &mut self.theme_state.editor else {
            return;
        };
        if editor.baseline.identity().theme_id() != document.identity().theme_id() {
            return;
        }
        if editor.saved_identity.as_ref().is_some_and(|saved| {
            saved.manifest() == document.identity().manifest()
                && saved.theme_id() == document.identity().theme_id()
                && saved.digest() == document.identity().digest()
                && saved.byte_length() == document.identity().byte_length()
        }) {
            editor.baseline = document.clone();
            editor.clean_document = document.document().clone();
            editor.changes.clear();
            editor.validated = Some(document.document().clone());
            editor.validated_revision = editor.revision;
            editor.saved_identity = None;
            editor.issue = None;
        } else if editor.baseline.identity().digest() == document.identity().digest() {
            editor.baseline = document.clone();
        } else {
            editor.issue = Some(
                "The bound theme document changed. Discard this editor before loading its new source.",
            );
            self.theme_state.repository_unavailable = true;
        }
    }
}
