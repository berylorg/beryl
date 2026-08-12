use beryl_home_store::HomeStore;
use beryl_state::{
    PreparedThemeAppearance, ThemeLiveEditFailure, ThemeLiveEditOutcome, ThemeRepositoryCommit,
    ThemeRepositoryObservation, ThemeResolver,
};

use super::{
    DurablePublicationIdentity, DurablePublicationOutcome, RepositoryAppearanceResult,
    ThemeRuntime, ThemeRuntimeFailureClass, installed_identity, load_observed, load_prepared,
    map_live_failure, map_live_load_failure, map_publication_error, map_startup_failure,
};

impl ThemeRuntime {
    pub(super) fn apply_repository_commit(
        &mut self,
        store: &HomeStore,
        publication: &ThemeRepositoryCommit,
    ) -> RepositoryAppearanceResult {
        let active_affected = self.active.as_ref().is_some_and(|active| {
            publication
                .affected_documents()
                .iter()
                .any(|document| document.theme_id() == active)
        });
        if publication.manifest().is_some() {
            self.refresh_repository(store)
        } else if active_affected {
            self.reload_active_document(store)
        } else {
            RepositoryAppearanceResult::Unchanged
        }
    }

    pub(super) fn refresh_repository(&mut self, store: &HomeStore) -> RepositoryAppearanceResult {
        let service = self.service.as_ref().expect("running service");
        let candidate = match service.observe_repository(
            store,
            self.config.max_manifest_bytes,
            self.config.manifest_read,
            self.repository.as_ref(),
        ) {
            Ok(candidate) => candidate,
            Err(_) => return self.retain(ThemeRuntimeFailureClass::Repository),
        };
        let Some(active) = self.active.as_ref() else {
            self.repository = Some(candidate);
            return RepositoryAppearanceResult::Unchanged;
        };
        let loaded = load_prepared(
            service,
            store,
            &candidate,
            active,
            self.settings,
            self.last_observed_document.as_ref(),
            self.config,
            &mut self.pages_read,
        );
        let (prepared, observed) = match loaded {
            Ok(value) => value,
            Err(failure) => return self.retain(map_startup_failure(&failure)),
        };
        let old_manifest = self
            .repository
            .as_ref()
            .map(ThemeRepositoryObservation::manifest);
        if installed_identity(
            self.appearance
                .as_ref()
                .expect("running appearance")
                .durable_base()
                .prepared(),
        ) == installed_identity(&prepared)
        {
            self.repository = Some(candidate);
            self.last_observed_document = Some(observed);
            self.refresh_reread_needed = false;
            return RepositoryAppearanceResult::Unchanged;
        }
        let identity = match old_manifest {
            Some(old) if old != candidate.manifest() => {
                DurablePublicationIdentity::RepositoryRefresh(prepared.source().clone())
            }
            _ => DurablePublicationIdentity::ActiveDocument(
                installed_identity(&prepared)
                    .expect("installed candidate")
                    .clone(),
            ),
        };
        match self.publish_external(identity, prepared) {
            Ok(result) => {
                self.repository = Some(candidate);
                self.last_observed_document = Some(observed);
                self.refresh_reread_needed = false;
                result
            }
            Err(failure) => {
                self.refresh_reread_needed = true;
                self.retain(failure)
            }
        }
    }

    pub(super) fn reload_active_document(
        &mut self,
        store: &HomeStore,
    ) -> RepositoryAppearanceResult {
        let Some(repository) = self.repository.as_ref() else {
            return self.refresh_repository(store);
        };
        let Some(active) = self.active.as_ref() else {
            return RepositoryAppearanceResult::Unchanged;
        };
        let loaded = load_observed(
            self.service.as_ref().expect("running service"),
            store,
            repository,
            active,
            self.last_observed_document.as_ref(),
            self.config,
            &mut self.pages_read,
        );
        let prior = self
            .appearance
            .as_ref()
            .expect("running appearance")
            .durable_base()
            .prepared()
            .clone();
        let observed = match loaded {
            Ok(observed) => observed,
            Err(error) => {
                let identity = error.observed_identity().cloned();
                let failure = map_live_load_failure(&error);
                let _ = ThemeLiveEditOutcome::invalid(prior, identity, failure.clone());
                return self.retain(map_live_failure(failure));
            }
        };
        let appearance = match ThemeResolver::new(observed.document().definition()) {
            Ok(resolver) => resolver.resolve(),
            Err(_) => {
                let failure = ThemeLiveEditFailure::ResolutionFailed;
                let _ = ThemeLiveEditOutcome::invalid(
                    prior,
                    Some(observed.identity().clone()),
                    failure.clone(),
                );
                return self.retain(map_live_failure(failure));
            }
        };
        let outcome = match ThemeLiveEditOutcome::valid(
            prior.clone(),
            observed.identity().clone(),
            appearance,
        ) {
            Ok(value) => value,
            Err(_) => {
                let prepared = match PreparedThemeAppearance::installed(
                    self.settings,
                    active,
                    observed.identity().clone(),
                    ThemeResolver::new(observed.document().definition())
                        .expect("already resolved definition")
                        .resolve(),
                ) {
                    Ok(value) => value,
                    Err(_) => return self.retain(ThemeRuntimeFailureClass::Identity),
                };
                return self.publish_external_document(observed.identity().clone(), prepared);
            }
        };
        match outcome {
            ThemeLiveEditOutcome::Unchanged { .. } => {
                self.refresh_reread_needed = false;
                RepositoryAppearanceResult::Unchanged
            }
            ThemeLiveEditOutcome::Prepared { replacement } => {
                self.publish_external_document(observed.identity().clone(), replacement)
            }
            ThemeLiveEditOutcome::Retained { failure, .. } => {
                self.retain(map_live_failure(failure))
            }
        }
    }

    fn publish_external_document(
        &mut self,
        observed: beryl_state::ThemeDocumentIdentity,
        prepared: PreparedThemeAppearance,
    ) -> RepositoryAppearanceResult {
        match self.publish_external(
            DurablePublicationIdentity::ActiveDocument(observed.clone()),
            prepared,
        ) {
            Ok(result) => {
                self.last_observed_document = Some(observed);
                self.refresh_reread_needed = false;
                result
            }
            Err(failure) => {
                self.refresh_reread_needed = true;
                self.retain(failure)
            }
        }
    }

    fn publish_external(
        &mut self,
        identity: DurablePublicationIdentity,
        prepared: PreparedThemeAppearance,
    ) -> Result<RepositoryAppearanceResult, ThemeRuntimeFailureClass> {
        let appearance = self
            .appearance
            .as_mut()
            .ok_or(ThemeRuntimeFailureClass::Retired)?;
        let request = appearance
            .begin_durable_publication(identity)
            .map_err(map_publication_error)?;
        let outcome = appearance
            .publish_external_durable(request, prepared)
            .map_err(map_publication_error)?;
        self.last_failure = None;
        Ok(map_durable_outcome(outcome))
    }

    pub(super) fn publish(
        &mut self,
        identity: DurablePublicationIdentity,
        prepared: PreparedThemeAppearance,
    ) -> Result<RepositoryAppearanceResult, ThemeRuntimeFailureClass> {
        let appearance = self
            .appearance
            .as_mut()
            .ok_or(ThemeRuntimeFailureClass::Retired)?;
        let request = appearance
            .begin_durable_publication(identity)
            .map_err(map_publication_error)?;
        let outcome = appearance
            .publish_durable(request, prepared)
            .map_err(map_publication_error)?;
        self.last_failure = None;
        Ok(map_durable_outcome(outcome))
    }

    fn retain(&mut self, failure: ThemeRuntimeFailureClass) -> RepositoryAppearanceResult {
        self.last_failure = Some(failure);
        RepositoryAppearanceResult::Retained(failure)
    }
}

fn map_durable_outcome(outcome: DurablePublicationOutcome) -> RepositoryAppearanceResult {
    match outcome {
        DurablePublicationOutcome::Published(_) => RepositoryAppearanceResult::Published,
        DurablePublicationOutcome::HiddenBaseReplaced(_) => {
            RepositoryAppearanceResult::HiddenBaseReplaced
        }
    }
}
