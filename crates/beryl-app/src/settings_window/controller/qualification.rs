use super::*;

impl SettingsWindowController {
    pub(super) fn theme_is_qualified(&self) -> bool {
        let Some(draft) = &self.draft else {
            return true;
        };
        !draft.is_modified(SettingKey::ActiveThemeId)
            || self
                .theme_qualification
                .as_ref()
                .is_some_and(|(binding, proof)| {
                    *binding == draft.binding()
                        && proof.active().is_some_and(|active| {
                            active.as_str() == draft.value(SettingKey::ActiveThemeId)
                        })
                        && proof.prepared().home() == self.themes.home()
                        && self.theme_state.catalog.as_ref().is_none_or(|catalog| {
                            match proof.prepared().source() {
                                beryl_state::ThemeAppearanceSource::Installed(document) => {
                                    document.manifest() == catalog.observation.manifest()
                                }
                                _ => false,
                            }
                        })
                })
    }

    pub(super) fn theme_qualification_feedback(&self) -> Option<&'static str> {
        if self.theme_is_qualified() {
            return None;
        }
        if self
            .draft
            .as_ref()
            .is_some_and(|draft| self.theme_qualification_error == Some(draft.binding()))
        {
            Some(
                "This exact theme selection could not be completely prepared. Retry or choose another installed theme.",
            )
        } else {
            Some("Validating this exact theme selection before Apply or OK.")
        }
    }

    pub(super) fn drive_theme_qualification(&mut self, cx: &mut Context<Self>) {
        if self.theme_qualification_worker.is_some()
            || self.retired
            || self.exit_is_gated()
            || !self.home_available
            || !self.source_current
        {
            return;
        }
        let Some(draft) = &self.draft else { return };
        if !draft.is_modified(SettingKey::ActiveThemeId) {
            self.theme_qualification = None;
            self.theme_qualification_error = None;
            return;
        }
        if self.theme_is_qualified()
            || self.theme_qualification_error == Some(draft.binding())
            || !draft.can_apply()
        {
            return;
        }
        let binding = draft.binding();
        let draft = draft.clone();
        let service = self.service.clone();
        let work = cx
            .background_executor()
            .spawn(async move { service.qualify_active_theme(&draft) });
        self.theme_qualification_worker = Some(cx.spawn(async move |owner, cx| {
            let result = work.await;
            let _ = owner.update(cx, |owner, cx| {
                owner.theme_qualification_worker = None;
                if owner
                    .draft
                    .as_ref()
                    .is_some_and(|draft| draft.binding() == binding)
                    && owner.home_available
                    && owner.source_current
                    && !owner.retired
                {
                    match result {
                        Ok(proof) => {
                            owner.theme_qualification = Some((binding, proof));
                            owner.theme_qualification_error = None;
                        }
                        Err(_) => {
                            owner.theme_qualification = None;
                            owner.theme_qualification_error = Some(binding);
                        }
                    }
                }
                owner.publish(cx);
            });
        }));
    }

    pub(in crate::settings_window) fn retry_theme_qualification(&mut self, cx: &mut Context<Self>) {
        if self.exit_is_gated() || !self.source_current || !self.home_available {
            return;
        }
        self.theme_qualification = None;
        self.theme_qualification_error = None;
        self.publish(cx);
    }
}
