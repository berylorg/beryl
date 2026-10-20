use std::{num::NonZeroU64, sync::Arc};

use beryl_state::{
    DeleteTheme, InstalledThemeId, RenameTheme, SettingKey, ThemeName, ThemeReconciliation,
    ThemeReferenceSnapshot, ThemeReferenceSnapshotProvider, ThemeReferenceSnapshotUnavailable,
    ThemeRepositoryCommand, ThemeRepositoryCommit, ThemeRepositoryOperationOutcome,
    ThemeSettingsIdentity,
};
use gpui::{ClipboardItem, Context};

use super::*;
use crate::{
    settings_service::{PublishedSettingsReader, SettingsDraftValue},
    settings_window::{controller::SettingsWindowController, fields::Route},
    theme_runtime::{
        ThemeRuntimeFailureClass, ThemeSettingsBridgeError, ThemeSettingsResult, ThemeSettingsWork,
    },
};

fn refusal_message(class: ThemeRuntimeFailureClass) -> &'static str {
    match class {
        ThemeRuntimeFailureClass::InstalledEntryLimit => {
            "The installed-theme collection is at capacity. The operation was refused before changing your draft or repository."
        }
        ThemeRuntimeFailureClass::ManifestByteLimit => {
            "The theme collection would exceed its saved byte limit. The operation was refused before changing your draft or repository."
        }
        ThemeRuntimeFailureClass::Retired => {
            "The original theme source retired before this operation began. Your exact draft is preserved."
        }
        _ => {
            "The theme operation was refused before it began. Your exact draft and the installed collection are preserved."
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::settings_window) enum RepositoryKind {
    Rename,
    Delete,
    Save,
    SaveAs,
}

pub(super) struct PendingRepository {
    pub(super) ticket: Option<ThemeSettingsTicket>,
    pub(super) original_ticket: ThemeSettingsTicket,
    pub(super) kind: RepositoryKind,
    pub(super) operation: Option<NonZeroU64>,
    pub(super) binding: SettingsDraftBinding,
}

struct WindowReferences {
    reader: PublishedSettingsReader,
    manifest: beryl_state::ThemeManifestIdentity,
    staged: Option<InstalledThemeId>,
    document: Option<InstalledThemeId>,
}

impl ThemeReferenceSnapshotProvider for WindowReferences {
    fn current_theme_references(
        &self,
    ) -> Result<ThemeReferenceSnapshot, ThemeReferenceSnapshotUnavailable> {
        let baseline = self
            .reader
            .snapshot()
            .map_err(|_| ThemeReferenceSnapshotUnavailable)?;
        let active = baseline
            .applied()
            .active_theme()
            .map_err(|_| ThemeReferenceSnapshotUnavailable)?
            .cloned();
        ThemeReferenceSnapshot::new(
            self.manifest,
            ThemeSettingsIdentity::new(
                self.manifest.home(),
                baseline.identity().settings_revision,
                baseline.record_revision(SettingKey::ActiveThemeId),
            ),
            active,
            self.staged.clone(),
            self.document.iter().cloned().collect(),
            Vec::new(),
        )
        .map_err(|_| ThemeReferenceSnapshotUnavailable)
    }
}

impl ThemesState {
    pub(in crate::settings_window) fn is_pending(&self) -> bool {
        self.repository.is_some()
    }
}

impl SettingsWindowController {
    pub fn observe_theme_retirement(
        &mut self,
        retirement: &crate::theme_runtime::ThemeSettingsRetirement,
        cx: &mut Context<Self>,
    ) {
        self.deactivate_theme_editor(cx);
        self.retire_theme_reads();
        let completion = retirement
            .completion
            .as_ref()
            .filter(|completion| completion.home == self.themes.home());
        if self.repository_appearance_pending
            && completion.is_some_and(|completion| {
                Some(completion.ticket) == self.repository_appearance_ticket
            })
        {
            self.repository_appearance_pending = false;
            self.repository_appearance_ticket = None;
            self.appearance_busy = false;
        }
        let Some(pending) = self.theme_state.repository.take() else {
            self.publish(cx);
            return;
        };
        let completion = retirement.completion.as_ref().filter(|completion| {
            completion.home == self.themes.home()
                && (pending.ticket == Some(completion.ticket)
                    || pending.original_ticket == completion.ticket)
        });
        let binding_matches = self
            .draft
            .as_ref()
            .is_some_and(|draft| draft.binding() == pending.binding);
        if binding_matches {
            if let Some(completion) = completion {
                if let Some(class) = completion.not_started {
                    self.theme_state.message = Some(refusal_message(class));
                    self.publish(cx);
                    return;
                }
                if let Some(outcome) = &completion.repository {
                    match outcome.as_ref() {
                        ThemeRepositoryOperationOutcome::Committed { publication, .. } => {
                            self.finish_theme_save(pending.kind, publication);
                            self.theme_state.message =
                                Some("The original theme operation committed before recovery.");
                            self.publish(cx);
                            return;
                        }
                        ThemeRepositoryOperationOutcome::NotCommitted { .. } => {
                            self.theme_state.message = Some(
                                "The original theme operation did not commit. Your exact draft is preserved.",
                            );
                            self.publish(cx);
                            return;
                        }
                        ThemeRepositoryOperationOutcome::Indeterminate(_) => {}
                    }
                }
                if let Some(outcome) = &completion.reconciliation {
                    match outcome.as_ref() {
                        ThemeReconciliation::ExactNew(publication) => {
                            self.finish_theme_save(pending.kind, publication);
                            self.theme_state.message =
                                Some("The original theme operation committed before recovery.");
                            self.publish(cx);
                            return;
                        }
                        ThemeReconciliation::ExactOld => {
                            self.theme_state.message = Some(
                                "The original theme operation did not commit. Your exact draft is preserved.",
                            );
                            self.publish(cx);
                            return;
                        }
                        ThemeReconciliation::Collision => {}
                    }
                }
            }
        }
        self.theme_state.repository_unavailable = true;
        self.theme_state.message = Some(
            "The original theme outcome is unavailable. Its exact draft is preserved read-only.",
        );
        self.publish(cx);
    }

    pub(in crate::settings_window) fn select_installed_theme(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.exit_is_gated() || self.route != Route::Themes {
            return;
        }
        let selection = self
            .theme_state
            .pages
            .values()
            .flatten()
            .find(|selection| selection.id().as_str() == id)
            .cloned();
        let (Some(selection), Some(binding), Some(catalog)) = (
            selection,
            self.draft.as_ref().map(|draft| draft.binding()),
            self.theme_state.catalog.as_ref(),
        ) else {
            return;
        };
        if selection.manifest() != catalog.observation.manifest() {
            return;
        }
        self.theme_state.selected_hint =
            Some((id.to_owned(), selection.summary().order() as usize));
        self.theme_state.rename_name = selection.summary().name().as_str().to_owned();
        self.theme_state.selected = Some(selection.clone());
        self.theme_state.document = None;
        self.theme_state.message = Some("Loading the exact selected theme document.");
        match self.themes.submit(ThemeSettingsWork::LoadDocument {
            observation: catalog.observation.clone(),
            selection: selection.clone(),
        }) {
            Ok(ticket) => {
                self.theme_state
                    .reads
                    .insert(ticket, ReadPurpose::Document { binding, selection });
            }
            Err(_) => {
                self.theme_state.message =
                    Some("The selected theme document is unavailable. Retry the Themes page.")
            }
        }
        self.publish(cx);
        self.schedule_theme_pulse(cx);
    }

    pub(in crate::settings_window) fn handle_theme_field(
        &mut self,
        id: &str,
        value: String,
        cx: &mut Context<Self>,
    ) {
        if self.exit_is_gated() || !self.home_available || !self.source_current {
            return;
        }
        if id == "theme.rename-name" && value.len() <= 128 && self.theme_state.selected.is_some() {
            self.theme_state.rename_name = value;
            self.publish(cx);
        } else {
            self.stage_theme_property(id, value, cx);
        }
    }

    pub(in crate::settings_window) fn theme_action(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.exit_is_gated() {
            return;
        }
        if !self.home_available && id != "theme.copy-id" {
            return;
        }
        if id == "theme.validate-selection" {
            self.retry_theme_qualification(cx);
            return;
        }
        if id == "theme.retry" {
            self.retire_theme_reads();
            self.theme_state.catalog = None;
            self.theme_state.read_failed = false;
            self.theme_state.message = Some("Loading installed themes.");
            self.drive_theme_reads(cx);
            self.publish(cx);
            return;
        }
        let Some(selection) = self.theme_state.selected.clone() else {
            return;
        };
        match id {
            "theme.copy-id" => cx.write_to_clipboard(ClipboardItem::new_string(
                selection.id().as_str().to_owned(),
            )),
            "theme.activate" => {
                if self.theme_state.document.as_ref().is_none_or(|document| {
                    document.identity().manifest() != selection.manifest()
                        || document.identity().theme_id() != selection.id()
                }) {
                    return;
                }
                if self.source_current {
                    if let Some(draft) = &mut self.draft {
                        let _ = draft.set(SettingsDraftValue::ActiveThemeId(
                            selection.id().as_str().to_owned(),
                        ));
                    }
                }
            }
            "theme.edit" => {
                self.open_theme_editor(cx);
                return;
            }
            "theme.rename" if !self.theme_state.repository_unavailable => {
                let Ok(name) = ThemeName::new(&self.theme_state.rename_name) else {
                    return;
                };
                self.start_repository(
                    ThemeRepositoryCommand::Rename(RenameTheme::new(
                        selection.manifest(),
                        selection.summary().clone(),
                        name,
                    )),
                    RepositoryKind::Rename,
                    cx,
                );
            }
            "theme.delete" if !self.theme_state.repository_unavailable => {
                let Some(document) = self.theme_state.document.as_ref() else {
                    return;
                };
                let references = self.theme_references(selection.manifest());
                let Ok(snapshot) = references.current_theme_references() else {
                    return;
                };
                let Ok(command) = DeleteTheme::new(
                    selection.manifest(),
                    selection.summary().clone(),
                    document.identity().clone(),
                    snapshot,
                ) else {
                    return;
                };
                self.start_repository(
                    ThemeRepositoryCommand::Delete(command),
                    RepositoryKind::Delete,
                    cx,
                );
            }
            "theme.save" => {
                self.save_theme_document(false, cx);
                return;
            }
            "theme.save-as" => {
                self.save_theme_document(true, cx);
                return;
            }
            _ => return,
        }
        self.publish(cx);
    }

    fn theme_references(&self, manifest: beryl_state::ThemeManifestIdentity) -> WindowReferences {
        WindowReferences {
            reader: self.service.reader(),
            manifest,
            staged: self.draft.as_ref().and_then(|draft| {
                InstalledThemeId::new(draft.value(SettingKey::ActiveThemeId)).ok()
            }),
            document: self.bound_theme_document_id(),
        }
    }

    pub(in crate::settings_window) fn start_repository(
        &mut self,
        command: ThemeRepositoryCommand,
        kind: RepositoryKind,
        cx: &mut Context<Self>,
    ) {
        if self.exit_is_gated() || self.theme_state.repository_unavailable {
            return;
        }
        let (Some(binding), Some(catalog)) = (
            self.draft.as_ref().map(|draft| draft.binding()),
            self.theme_state.catalog.as_ref(),
        ) else {
            return;
        };
        let references = Arc::new(self.theme_references(catalog.observation.manifest()));
        match self.themes.submit(ThemeSettingsWork::Execute {
            observation: catalog.observation.clone(),
            command,
            references,
        }) {
            Ok(ticket) => {
                self.theme_state.repository = Some(PendingRepository {
                    ticket: Some(ticket),
                    original_ticket: ticket,
                    kind,
                    operation: None,
                    binding,
                });
                self.theme_state.message = Some("The original theme operation is settling.");
            }
            Err(_) => {
                self.theme_state.message =
                    Some("The theme operation was not admitted. Your exact draft is preserved.")
            }
        }
        self.publish(cx);
        self.schedule_theme_pulse(cx);
    }

    pub(in crate::settings_window) fn poll_theme_repository(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = &mut self.theme_state.repository else {
            return;
        };
        if pending.ticket.is_none() {
            if let Some(operation) = pending.operation {
                pending.ticket = self
                    .themes
                    .submit(ThemeSettingsWork::Reconcile { operation })
                    .ok();
            }
        }
        let Some(ticket) = pending.ticket else {
            return;
        };
        let result = match self.themes.poll(ticket) {
            Ok(None) => return,
            result => result,
        };
        let mut pending = self
            .theme_state
            .repository
            .take()
            .expect("original repository ticket");
        if self
            .draft
            .as_ref()
            .is_none_or(|draft| draft.binding() != pending.binding)
        {
            self.theme_state.repository_unavailable = true;
            self.theme_state.message = Some(
                "The original theme result belongs to a retired draft. Its source is unavailable.",
            );
            return;
        }
        match result {
            Ok(Some(ThemeSettingsResult::Repository(outcome))) => match outcome.as_ref() {
                ThemeRepositoryOperationOutcome::Committed { publication, .. } => {
                    self.repository_appearance_ticket = Some(ticket);
                    self.repository_committed(pending.kind, publication, cx)
                }
                ThemeRepositoryOperationOutcome::NotCommitted { .. } => {
                    self.theme_state.message = Some(
                        "The theme operation was not committed. Your exact draft is preserved.",
                    )
                }
                ThemeRepositoryOperationOutcome::Indeterminate(original) => {
                    pending.operation = Some(original.operation());
                    pending.ticket = None;
                    self.theme_state.repository = Some(pending);
                }
            },
            Ok(Some(ThemeSettingsResult::Reconciled(outcome))) => match outcome.as_ref() {
                ThemeReconciliation::ExactNew(publication) => {
                    self.repository_appearance_ticket = Some(ticket);
                    self.repository_committed(pending.kind, publication, cx)
                }
                ThemeReconciliation::ExactOld => {
                    self.theme_state.message = Some(
                        "The original theme operation was not committed. Your exact draft is preserved.",
                    )
                }
                ThemeReconciliation::Collision => {
                    self.theme_state.repository_unavailable = true;
                    self.theme_state.message = Some(
                        "The original theme operation remains unresolved. Its affected source is unavailable.",
                    );
                }
            },
            Err(ThemeSettingsBridgeError::NotStarted(class)) => {
                self.theme_state.message = Some(refusal_message(class));
            }
            _ => {
                self.theme_state.repository_unavailable = true;
                self.theme_state.message =
                    Some("The theme source is unavailable. The exact document draft is preserved.");
            }
        }
        self.publish(cx);
    }

    pub(in crate::settings_window) fn repository_committed(
        &mut self,
        kind: RepositoryKind,
        publication: &ThemeRepositoryCommit,
        cx: &mut Context<Self>,
    ) {
        self.appearance_busy = true;
        self.repository_appearance_pending = true;
        self.retire_theme_reads();
        self.finish_theme_save(kind, publication);
        if matches!(kind, RepositoryKind::Delete) {
            self.theme_state.selected = None;
            self.theme_state.selected_hint = None;
            self.theme_state.document = None;
        }
        self.theme_state.message = None;
        self.theme_state.catalog = None;
        self.theme_state.read_failed = false;
        self.drive_theme_reads(cx);
    }
}
