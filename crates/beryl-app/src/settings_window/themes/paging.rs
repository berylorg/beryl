use std::time::Duration;

use gpui::Context;
use gpui_settings_window::{
    SettingsPageSplitDelivery, SettingsPageSplitFocusResolution, SettingsPageSplitItem,
    SettingsPageSplitPageFailure, SettingsPageSplitPageResult, SettingsPageSplitSelection,
    SettingsPageSplitSource, SettingsPageSplitSourceKey, SettingsPageSplitWork,
};

use super::*;
use crate::{
    settings_window::{controller::SettingsWindowController, fields::Route},
    theme_runtime::{ThemeSettingsFocus, ThemeSettingsResult, ThemeSettingsWork},
};

impl ThemesState {
    pub(in crate::settings_window) fn source_key(
        &self,
        window: u64,
        revision: u64,
    ) -> SettingsPageSplitSourceKey {
        SettingsPageSplitSourceKey::new(format!("installed-themes-{window}"), self.epoch, revision)
    }

    pub(in crate::settings_window) fn split_source(
        &self,
        window: u64,
        revision: u64,
    ) -> Option<SettingsPageSplitSource> {
        let catalog = self.catalog.as_ref()?;
        let mut source = SettingsPageSplitSource::new(
            self.source_key(window, revision),
            catalog.logical_item_count,
            32,
            16 * 1024,
        );
        if let Some((id, position)) = &self.selected_hint {
            source = source.with_selected(SettingsPageSplitSelection::new(id.clone(), *position));
        }
        Some(source)
    }
}

impl SettingsWindowController {
    pub(in crate::settings_window) fn drive_theme_reads(&mut self, cx: &mut Context<Self>) {
        self.theme_pulse = None;
        if self.retired {
            return;
        }
        self.poll_theme_repository(cx);
        let tickets: Vec<_> = self.theme_state.reads.keys().copied().collect();
        let mut changed = false;
        for ticket in tickets {
            let result = match self.themes.poll(ticket) {
                Ok(None) => continue,
                result => result,
            };
            let Some(purpose) = self.theme_state.reads.remove(&ticket) else {
                continue;
            };
            match (purpose, result) {
                (
                    ReadPurpose::Catalog { binding },
                    Ok(Some(ThemeSettingsResult::Catalog(catalog))),
                ) => {
                    if binding != self.draft.as_ref().map(|draft| draft.binding())
                        || catalog.observation.home() != self.themes.home()
                    {
                        continue;
                    }
                    if self
                        .theme_state
                        .catalog
                        .as_ref()
                        .is_none_or(|prior| prior.observation != catalog.observation)
                    {
                        let Some(epoch) = self.theme_state.epoch.checked_add(1) else {
                            self.theme_state.read_failed = true;
                            continue;
                        };
                        self.theme_state.epoch = epoch;
                        self.theme_qualification = None;
                        self.theme_qualification_error = None;
                        self.theme_state.pages.clear();
                        self.theme_state.selected = None;
                    }
                    self.theme_state.catalog = Some(catalog);
                    self.theme_state.message = None;
                    if !self.theme_state.repository_unavailable {
                        if let (Some(editor), Some(binding), Some(catalog)) = (
                            &self.theme_state.editor,
                            self.draft.as_ref().map(|draft| draft.binding()),
                            self.theme_state.catalog.as_ref(),
                        ) {
                            let id = editor.baseline.identity().theme_id().clone();
                            if self
                                .theme_state
                                .selected_hint
                                .as_ref()
                                .is_none_or(|(selected, _)| selected != id.as_str())
                            {
                                if let Ok(ticket) =
                                    self.themes.submit(ThemeSettingsWork::PageRange {
                                        observation: catalog.observation.clone(),
                                        range: 0..catalog.logical_item_count.min(1),
                                        focus: Some(id.clone()),
                                    })
                                {
                                    self.theme_state
                                        .reads
                                        .insert(ticket, ReadPurpose::RefreshEditor { binding, id });
                                }
                            }
                        }
                    }
                    if let (Some((id, position)), Some(binding), Some(catalog)) = (
                        self.theme_state.selected_hint.clone(),
                        self.draft.as_ref().map(|draft| draft.binding()),
                        self.theme_state.catalog.as_ref(),
                    ) {
                        if let Ok(id) = beryl_state::InstalledThemeId::new(id) {
                            if let Ok(ticket) = self.themes.submit(ThemeSettingsWork::PageRange {
                                observation: catalog.observation.clone(),
                                range: position.min(catalog.logical_item_count)
                                    ..position.saturating_add(1).min(catalog.logical_item_count),
                                focus: Some(id.clone()),
                            }) {
                                self.theme_state
                                    .reads
                                    .insert(ticket, ReadPurpose::RefreshSelection { binding, id });
                            }
                        }
                    }
                    changed = true;
                }
                (
                    ReadPurpose::RefreshSelection { binding, id },
                    Ok(Some(ThemeSettingsResult::Page(page))),
                ) => {
                    if self
                        .draft
                        .as_ref()
                        .is_none_or(|draft| draft.binding() != binding)
                    {
                        self.theme_state.read_failed = true;
                        self.theme_state.message = Some(
                            "The Settings draft changed while the theme source was loading. Retry to load its current source.",
                        );
                        changed = true;
                        continue;
                    }
                    if self
                        .draft
                        .as_ref()
                        .is_none_or(|draft| draft.binding() != binding)
                        || self
                            .theme_state
                            .catalog
                            .as_ref()
                            .is_none_or(|catalog| catalog.observation != page.observation)
                    {
                        continue;
                    }
                    if let Some(selection) = page
                        .selections
                        .iter()
                        .find(|selection| selection.id() == &id)
                        .cloned()
                    {
                        self.theme_state.selected = Some(selection.clone());
                        if let Ok(ticket) = self.themes.submit(ThemeSettingsWork::LoadDocument {
                            observation: page.observation,
                            selection: selection.clone(),
                        }) {
                            self.theme_state
                                .reads
                                .insert(ticket, ReadPurpose::Document { binding, selection });
                        }
                    } else if let Some(ThemeSettingsFocus::Found { position }) = page.focus {
                        if page.range.start != position {
                            if let Ok(ticket) = self.themes.submit(ThemeSettingsWork::PageRange {
                                observation: page.observation,
                                range: position..position.saturating_add(1),
                                focus: Some(id.clone()),
                            }) {
                                self.theme_state
                                    .reads
                                    .insert(ticket, ReadPurpose::RefreshSelection { binding, id });
                            }
                        }
                    } else {
                        self.theme_state.message = Some(
                            "The bound installed theme was removed. Its editor is preserved read-only.",
                        );
                        self.theme_state.repository_unavailable = true;
                    }
                    changed = true;
                }
                (
                    ReadPurpose::RefreshEditor { binding, id },
                    Ok(Some(ThemeSettingsResult::Page(page))),
                ) => {
                    if self
                        .draft
                        .as_ref()
                        .is_none_or(|draft| draft.binding() != binding)
                    {
                        self.theme_state.read_failed = true;
                        self.theme_state.message = Some(
                            "The Settings draft changed while the editor source was loading. Retry to authenticate its current source.",
                        );
                        changed = true;
                        continue;
                    }
                    if self
                        .draft
                        .as_ref()
                        .is_none_or(|draft| draft.binding() != binding)
                        || self
                            .theme_state
                            .catalog
                            .as_ref()
                            .is_none_or(|catalog| catalog.observation != page.observation)
                        || self
                            .theme_state
                            .editor
                            .as_ref()
                            .is_none_or(|editor| editor.baseline.identity().theme_id() != &id)
                    {
                        continue;
                    }
                    if let Some(selection) = page
                        .selections
                        .iter()
                        .find(|selection| selection.id() == &id)
                        .cloned()
                    {
                        if let Ok(ticket) = self.themes.submit(ThemeSettingsWork::LoadDocument {
                            observation: page.observation,
                            selection: selection.clone(),
                        }) {
                            self.theme_state
                                .reads
                                .insert(ticket, ReadPurpose::EditorDocument { binding, selection });
                        }
                    } else if let Some(ThemeSettingsFocus::Found { position }) = page.focus {
                        if page.range.start != position {
                            if let Ok(ticket) = self.themes.submit(ThemeSettingsWork::PageRange {
                                observation: page.observation,
                                range: position..position.saturating_add(1),
                                focus: Some(id.clone()),
                            }) {
                                self.theme_state
                                    .reads
                                    .insert(ticket, ReadPurpose::RefreshEditor { binding, id });
                            }
                        }
                    } else {
                        if let Some(editor) = &mut self.theme_state.editor {
                            editor.issue = Some(
                                "The original installed document was removed. Its exact draft is preserved read-only.",
                            );
                        }
                        self.theme_state.repository_unavailable = true;
                    }
                    changed = true;
                }
                (
                    ReadPurpose::EditorDocument { binding, selection },
                    Ok(Some(ThemeSettingsResult::Document(document))),
                ) => {
                    if self
                        .draft
                        .as_ref()
                        .is_none_or(|draft| draft.binding() != binding)
                    {
                        self.theme_state.read_failed = true;
                        self.theme_state.message = Some(
                            "The Settings draft changed while the editor source was loading. Retry to authenticate its current source.",
                        );
                        changed = true;
                        continue;
                    }
                    if self
                        .draft
                        .as_ref()
                        .is_none_or(|draft| draft.binding() != binding)
                        || self.theme_state.catalog.as_ref().is_none_or(|catalog| {
                            catalog.observation.manifest() != selection.manifest()
                        })
                        || document.identity().manifest() != selection.manifest()
                        || document.identity().theme_id() != selection.id()
                    {
                        continue;
                    }
                    self.qualify_editor_document(&document);
                    self.theme_state.read_failed = false;
                    changed = true;
                }
                (ReadPurpose::Page { request }, Ok(Some(ThemeSettingsResult::Page(page)))) => {
                    let Some(catalog) = &self.theme_state.catalog else {
                        continue;
                    };
                    let revision = self
                        .draft
                        .as_ref()
                        .map(|draft| draft.binding().draft_revision)
                        .unwrap_or(0);
                    if page.observation != catalog.observation
                        || page.range != request.range()
                        || request.source_key()
                            != &self.theme_state.source_key(self.window_instance, revision)
                    {
                        continue;
                    }
                    let items = page
                        .selections
                        .iter()
                        .enumerate()
                        .map(|(offset, selection)| {
                            SettingsPageSplitItem::new(
                                page.range.start + offset,
                                selection.id().as_str(),
                                selection.summary().name().as_str(),
                            )
                            .with_subtext(selection.id().as_str())
                        })
                        .collect();
                    let mut result = SettingsPageSplitPageResult::ready(
                        request.clone(),
                        catalog.logical_item_count,
                        items,
                    );
                    if let Some(focus) = page.focus {
                        result = result.with_focus_resolution(match focus {
                            ThemeSettingsFocus::Found { position } => {
                                SettingsPageSplitFocusResolution::Found(position)
                            }
                            ThemeSettingsFocus::Removed => {
                                SettingsPageSplitFocusResolution::Removed
                            }
                        });
                    }
                    let Some(window) = &self.window else {
                        continue;
                    };
                    if matches!(
                        window.deliver_page_split_result(cx, result),
                        Ok(Ok(SettingsPageSplitDelivery::Ready))
                    ) {
                        self.theme_state
                            .pages
                            .insert(request.request_id(), page.selections);
                    }
                }
                (ReadPurpose::Page { request }, _) => {
                    if let (Some(window), Some(catalog)) = (&self.window, &self.theme_state.catalog)
                    {
                        let _ = window.deliver_page_split_result(cx, SettingsPageSplitPageResult::failed(request, catalog.logical_item_count, SettingsPageSplitPageFailure::Unavailable("The exact installed-theme page is unavailable. Retry the Themes page.".into())));
                    }
                }
                (
                    ReadPurpose::Document { binding, selection },
                    Ok(Some(ThemeSettingsResult::Document(document))),
                ) => {
                    if self
                        .draft
                        .as_ref()
                        .is_none_or(|draft| draft.binding() != binding)
                    {
                        if self.theme_state.selected.as_ref() == Some(&selection) {
                            self.theme_state.read_failed = true;
                            self.theme_state.message = Some(
                                "The Settings draft changed while the selected theme was loading. Retry to load its current source.",
                            );
                            changed = true;
                        }
                        continue;
                    }
                    if self
                        .draft
                        .as_ref()
                        .is_none_or(|draft| draft.binding() != binding)
                        || self.theme_state.selected.as_ref() != Some(&selection)
                        || document.identity().manifest() != selection.manifest()
                        || document.identity().theme_id() != selection.id()
                    {
                        continue;
                    }
                    self.qualify_editor_document(&document);
                    self.theme_state.document = Some(document);
                    self.theme_state.read_failed = false;
                    self.theme_state.message = None;
                    changed = true;
                }
                _ => {
                    self.theme_state.message =
                        Some("The theme source is unavailable. Retry to load a coherent source.");
                    self.theme_state.read_failed = true;
                    changed = true;
                }
            }
        }
        if changed {
            self.publish(cx);
        }
        self.drain_theme_page_work(cx);
        if matches!(self.route, Route::Themes | Route::ThemeEditor)
            && self.theme_state.catalog.is_none()
            && !self.theme_state.read_failed
            && !self
                .theme_state
                .reads
                .values()
                .any(|purpose| matches!(purpose, ReadPurpose::Catalog { .. }))
        {
            if let Ok(ticket) = self.themes.submit(ThemeSettingsWork::Observe) {
                self.theme_state.reads.insert(
                    ticket,
                    ReadPurpose::Catalog {
                        binding: self.draft.as_ref().map(|draft| draft.binding()),
                    },
                );
            }
        }
        if self
            .window
            .as_ref()
            .is_some_and(|window| window.is_visible(cx).unwrap_or(false))
            || !self.theme_state.reads.is_empty()
        {
            self.schedule_theme_pulse(cx);
        }
    }

    pub(in crate::settings_window) fn schedule_theme_pulse(&mut self, cx: &mut Context<Self>) {
        if self.theme_pulse.is_some() || self.retired {
            return;
        }
        self.theme_pulse = Some(cx.spawn(async move |owner, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(24))
                .await;
            let _ = owner.update(cx, |owner, cx| owner.drive_theme_reads(cx));
        }));
    }

    pub(in crate::settings_window) fn retire_theme_reads(&mut self) {
        for ticket in self.theme_state.reads.keys().copied() {
            let _ = self.themes.cancel(ticket);
        }
        self.theme_state.reads.clear();
        self.theme_state.pages.clear();
    }

    fn drain_theme_page_work(&mut self, cx: &mut Context<Self>) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let receiver = window.page_split_work_receiver();
        while let Some(work) = receiver.take_work() {
            match work {
                SettingsPageSplitWork::Page(request) => {
                    let Some(catalog) = &self.theme_state.catalog else {
                        continue;
                    };
                    let focus = request.focus_probe().and_then(|probe| {
                        beryl_state::InstalledThemeId::new(probe.item_id().as_str()).ok()
                    });
                    match self.themes.submit(ThemeSettingsWork::PageRange {
                        observation: catalog.observation.clone(),
                        range: request.range(),
                        focus,
                    }) {
                        Ok(ticket) => {
                            self.theme_state
                                .reads
                                .insert(ticket, ReadPurpose::Page { request });
                        }
                        Err(_) => {
                            let _ = window.deliver_page_split_result(
                                cx,
                                SettingsPageSplitPageResult::failed(
                                    request,
                                    catalog.logical_item_count,
                                    SettingsPageSplitPageFailure::Unavailable(
                                        "Installed-theme paging capacity is unavailable.".into(),
                                    ),
                                ),
                            );
                        }
                    }
                }
                SettingsPageSplitWork::Cancel(request)
                | SettingsPageSplitWork::Release(request) => {
                    self.theme_state.pages.remove(&request.request_id());
                    let ticket = self.theme_state.reads.iter().find_map(|(ticket, purpose)| matches!(purpose, ReadPurpose::Page { request: original } if original == &request).then_some(*ticket));
                    if let Some(ticket) = ticket {
                        let _ = self.themes.cancel(ticket);
                        self.theme_state.reads.remove(&ticket);
                    }
                }
            }
        }
    }
}
