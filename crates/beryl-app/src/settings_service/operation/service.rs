use super::*;
impl SettingsService {
    pub fn qualify_active_theme(
        &self,
        draft: &SettingsDraft,
    ) -> Result<ConfirmedSettingsTheme, SettingsServiceError> {
        let baseline = self.snapshot()?;
        let binding = draft.binding();
        if baseline.identity != binding.source
            || binding.window_instance == 0
            || binding.draft_revision == 0
        {
            return Err(SettingsServiceError::Stale);
        }
        let key = SettingKey::ActiveThemeId;
        if !baseline.row(key).is_available() {
            return Err(SettingsServiceError::Unavailable(key));
        }
        let value = validate_value(key, draft.value(key))
            .map_err(|issue| SettingsServiceError::Invalid { key, issue })?;
        let active = InstalledThemeId::new(
            value
                .as_active_theme_id()
                .ok_or(SettingsServiceError::Theme)?,
        )
        .map_err(|_| SettingsServiceError::Theme)?;
        let target = baseline
            .identity
            .settings_revision
            .checked_next()
            .map_err(|_| SettingsServiceError::Stale)?;
        let revision =
            baseline
                .record_revision(key)
                .map_or(Ok(RecordRevision::INITIAL), |old| {
                    RecordRevision::new(
                        old.get()
                            .checked_add(1)
                            .ok_or(SettingsServiceError::Stale)?,
                    )
                    .map_err(|_| SettingsServiceError::Stale)
                })?;
        let resources = self
            .inner
            .resources
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?;
        let resources = resources.as_ref().ok_or(SettingsServiceError::Retired)?;
        if self.snapshot()?.identity() != binding.source {
            return Err(SettingsServiceError::Stale);
        }
        let source = resources
            .theme
            .as_ref()
            .ok_or(SettingsServiceError::Theme)?;
        let confirmed = source
            .prepare_selection(
                &resources.home,
                ThemeDraftIdentity::new(NonZeroU64::new(binding.window_instance).unwrap()),
                ThemeDraftRevision::new(NonZeroU64::new(binding.draft_revision).unwrap()),
                beryl_state::ThemeSettingsIdentity::new(source.home(), target, Some(revision)),
                Some(active),
            )
            .map_err(|_| SettingsServiceError::Theme)?;
        if self.snapshot()?.identity() != binding.source {
            return Err(SettingsServiceError::Stale);
        }
        Ok(confirmed)
    }

    pub fn next_operation_id(&self) -> Result<u64, SettingsServiceError> {
        if self.inner.retired.load(Ordering::Acquire) {
            return Err(SettingsServiceError::Retired);
        }
        self.inner.owner.next_operation_id()
    }
    pub fn prepare_theme(
        &self,
        prepared: &mut PreparedSettingsApply,
    ) -> Result<(), SettingsServiceError> {
        if !prepared.service.ptr_eq(&Arc::downgrade(&self.inner))
            || self.inner.retired.load(Ordering::Acquire)
        {
            return Err(SettingsServiceError::Retired);
        }
        let Some(request) = prepared.theme_request().cloned() else {
            return Ok(());
        };
        let resources = self
            .inner
            .resources
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?;
        let resources = resources.as_ref().ok_or(SettingsServiceError::Retired)?;
        if self
            .inner
            .baseline
            .read()
            .map_err(|_| SettingsServiceError::Custody)?
            .identity
            != request.binding.source
        {
            return Err(SettingsServiceError::Stale);
        }
        let source = resources
            .theme
            .as_ref()
            .ok_or(SettingsServiceError::Theme)?;
        let intended = beryl_state::ThemeSettingsIdentity::new(
            source.home(),
            request.intended_domain_revision,
            Some(request.intended_active_record_revision),
        );
        let confirmed = source
            .prepare_selection(
                &resources.home,
                request.draft(),
                request.revision(),
                intended,
                Some(request.active.clone()),
            )
            .map_err(|_| SettingsServiceError::Theme)?;
        prepared.set_theme_preparation(SettingsThemePreparation::new(request, confirmed)?)
    }
    pub fn prepare_apply(
        &self,
        draft: &SettingsDraft,
        action: SettingsApplyAction,
        operation: u64,
    ) -> Result<PreparedSettingsApply, SettingsServiceError> {
        let baseline = self.snapshot()?;
        if baseline.identity != draft.binding.source
            || operation == 0
            || draft.binding.window_instance == 0
            || draft.binding.draft_revision == 0
        {
            return Err(SettingsServiceError::Stale);
        }
        let keys = draft.modified();
        if keys.is_empty() {
            return Err(SettingsServiceError::Clean);
        }
        let target = baseline
            .identity
            .settings_revision
            .checked_next()
            .map_err(|_| SettingsServiceError::Stale)?;
        let mut after = baseline.records.clone();
        let mut modified = 0;
        for key in keys {
            if !baseline.row(key).is_available() {
                return Err(SettingsServiceError::Unavailable(key));
            }
            let value = validate_value(key, draft.value(key))
                .map_err(|issue| SettingsServiceError::Invalid { key, issue })?;
            let revision = match baseline.record_revision(key) {
                None => RecordRevision::INITIAL,
                Some(old) => RecordRevision::new(
                    old.get()
                        .checked_add(1)
                        .ok_or(SettingsServiceError::Stale)?,
                )
                .map_err(|_| SettingsServiceError::Stale)?,
            };
            after[index(key)] = Some(SettingsRecordFacts {
                revision,
                schema: key.schema_version(),
                value,
            });
            modified |= mask(key);
        }
        let theme_request = if modified & mask(SettingKey::ActiveThemeId) != 0 {
            let record = after[0].as_ref().ok_or(SettingsServiceError::Theme)?;
            Some(SettingsThemeRequest {
                binding: draft.binding,
                operation,
                intended_domain_revision: target,
                intended_active_record_revision: record.revision,
                active: InstalledThemeId::new(
                    record
                        .value
                        .as_active_theme_id()
                        .ok_or(SettingsServiceError::Theme)?,
                )
                .map_err(|_| SettingsServiceError::Theme)?,
            })
        } else {
            None
        };
        let op = Arc::new(SettingsOperation {
            id: operation,
            binding: draft.binding,
            action,
            before: baseline,
            after,
            modified,
            target,
            theme_request,
            state: Mutex::new(OperationState {
                original: OriginalOutcome::Unexecuted,
                observation: None,
                theme: None,
            }),
        });
        let resources_guard = self
            .inner
            .resources
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?;
        if self.inner.retired.load(Ordering::Acquire) || resources_guard.is_none() {
            return Err(SettingsServiceError::Retired);
        }
        resources_guard
            .as_ref()
            .ok_or(SettingsServiceError::Retired)?
            .home
            .generation_identity()?;
        let mut facts = self
            .inner
            .owner
            .facts
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?;
        if facts.active.is_some() {
            return Err(SettingsServiceError::Busy);
        }
        if operation <= facts.last_operation {
            return Err(SettingsServiceError::Stale);
        }
        if facts.excluded & modified != 0 {
            return Err(SettingsServiceError::Unavailable(
                SETTINGS_KEYS
                    .into_iter()
                    .find(|key| facts.excluded & modified & mask(*key) != 0)
                    .expect("nonempty excluded set"),
            ));
        }
        if facts.latest.as_ref().is_some_and(|old| old.id == operation)
            || facts.unresolved.iter().any(|old| old.id == operation)
        {
            return Err(SettingsServiceError::Stale);
        }
        facts.active = Some(Arc::clone(&op));
        facts.last_operation = operation;
        facts.allocated_operation = facts.allocated_operation.max(operation);
        drop(facts);
        Ok(PreparedSettingsApply {
            service: Arc::downgrade(&self.inner),
            operation: op,
            consumed: false,
        })
    }
    pub fn execute(
        &self,
        mut prepared: PreparedSettingsApply,
        cancellation: &CommandCancellation,
    ) -> Result<SettingsApplyObservation, SettingsServiceError> {
        if !prepared.service.ptr_eq(&Arc::downgrade(&self.inner))
            || self.inner.retired.load(Ordering::Acquire)
        {
            return Err(SettingsServiceError::Retired);
        }
        let resources = self
            .inner
            .resources
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?;
        let resources = resources.as_ref().ok_or(SettingsServiceError::Retired)?;
        let op = &prepared.operation;
        if self
            .inner
            .baseline
            .read()
            .map_err(|_| SettingsServiceError::Custody)?
            .identity
            != op.binding.source
        {
            return Err(SettingsServiceError::Stale);
        }
        if op.theme_request.is_some()
            && op
                .state
                .lock()
                .map_err(|_| SettingsServiceError::Custody)?
                .theme
                .is_none()
        {
            return Err(SettingsServiceError::Theme);
        }
        let updates = SETTINGS_KEYS
            .into_iter()
            .filter(|key| op.modified & mask(*key) != 0)
            .map(|key| {
                let expected = op
                    .before
                    .record_revision(key)
                    .map(ExpectedSettingRevision::Exact)
                    .unwrap_or(ExpectedSettingRevision::Absent);
                SettingUpdate::new(
                    key,
                    expected,
                    op.after[index(key)]
                        .as_ref()
                        .expect("validated modified value")
                        .value
                        .clone(),
                )
            })
            .collect();
        let apply = ApplySettings::new(updates).map_err(|_| SettingsServiceError::Stale)?;
        let mut command = HomeCommand::new(resources.home.home_revision()?)
            .with_cancellation(cancellation.clone());
        command.add(
            resources
                .settings
                .apply(op.before.identity.settings_revision, apply),
        )?;
        prepared.consumed = true;
        let outcome = resources.home.execute(command);
        {
            let retained = match outcome {
                CommandOutcome::Indeterminate {
                    failure,
                    reconciliation,
                } => OriginalOutcome::Installed {
                    failure,
                    handle: reconciliation.install_and_handle(),
                    resolution: None,
                },
                original => OriginalOutcome::Direct(original),
            };
            op.state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .original = retained;
        }
        self.observe_with_resources(op, resources, false)
    }
    pub fn observation(
        &self,
        operation: u64,
    ) -> Result<Option<SettingsApplyObservation>, SettingsServiceError> {
        self.inner.owner.observation(operation)
    }
    pub fn reconcile_original(
        &self,
        operation: u64,
    ) -> Result<SettingsApplyObservation, SettingsServiceError> {
        if self.inner.retired.load(Ordering::Acquire) {
            return Err(SettingsServiceError::Retired);
        }
        let op = self.inner.owner.operation(operation)?;
        if let Some(observation) = op
            .state
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?
            .observation
            .clone()
        {
            if !matches!(observation, SettingsApplyObservation::Reconciling { .. }) {
                return Ok(observation);
            }
        }
        let resources = self
            .inner
            .resources
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?;
        self.observe_with_resources(
            &op,
            resources.as_ref().ok_or(SettingsServiceError::Retired)?,
            true,
        )
    }
    fn observe_with_resources(
        &self,
        op: &Arc<SettingsOperation>,
        resources: &Resources,
        reconcile: bool,
    ) -> Result<SettingsApplyObservation, SettingsServiceError> {
        let (classification, receipt) = {
            let state = op.state.lock().map_err(|_| SettingsServiceError::Custody)?;
            match &state.original {
                OriginalOutcome::Direct(CommandOutcome::NotCommitted { .. }) => (Some(false), None),
                OriginalOutcome::Direct(CommandOutcome::Committed { receipt, .. }) => {
                    (None, Some(receipt.clone()))
                }
                _ => (None, None),
            }
        };
        let classification = classification.or_else(|| {
            receipt.as_ref().and_then(|receipt| {
                (resources
                    .settings
                    .committed_revision(&resources.home, receipt)
                    .ok()
                    .flatten()
                    == Some(op.target))
                .then_some(true)
            })
        });
        let classification = if classification.is_none() && reconcile {
            match classify_original(op, |handle| resources.home.retry_reconciliation(handle)) {
                Ok(value) => value,
                Err(error) => {
                    if resources.home.health().state() != HomeHealthState::Healthy {
                        return Err(error);
                    }
                    return self.exclude(op, resources);
                }
            }
        } else {
            classification
        };
        let Some(committed) = classification else {
            let observation = SettingsApplyObservation::Reconciling {
                operation: op.id,
                binding: op.binding,
                action: op.action,
            };
            finish_observation(op, observation.clone())?;
            return Ok(observation);
        };
        let observation = if committed {
            let (revision, records) = read_store(&resources.home, &resources.settings)?;
            if !SETTINGS_KEYS
                .into_iter()
                .filter(|key| op.modified & mask(*key) != 0)
                .all(|key| records[index(key)] == op.after[index(key)])
            {
                return self.exclude(op, resources);
            }
            let mut identity = op.before.identity;
            identity.settings_revision = revision;
            identity.publication = identity
                .publication
                .checked_add(1)
                .ok_or(SettingsServiceError::Stale)?;
            let mut baseline = build_baseline(identity, records, self.inner.owner.excluded()?)?;
            baseline.applied.home_binding = op.before.applied.home_binding;
            let baseline = Arc::new(baseline);
            *self
                .inner
                .baseline
                .write()
                .map_err(|_| SettingsServiceError::Custody)? = Arc::clone(&baseline);
            let theme = op
                .state
                .lock()
                .map_err(|_| SettingsServiceError::Custody)?
                .theme
                .take()
                .map(|value| value.confirmed);
            committed_observation(op, baseline, theme)
        } else {
            not_committed_observation(op)
        };
        finish_observation(op, observation.clone())?;
        self.inner.owner.release(op, false)?;
        Ok(observation)
    }
    fn exclude(
        &self,
        op: &Arc<SettingsOperation>,
        resources: &Resources,
    ) -> Result<SettingsApplyObservation, SettingsServiceError> {
        self.inner.owner.release(op, true)?;
        let current = self
            .inner
            .baseline
            .read()
            .map_err(|_| SettingsServiceError::Custody)?
            .clone();
        let mut identity = current.identity;
        if let Ok(revision) = resources.settings.revision(&resources.home) {
            identity.settings_revision = revision;
        }
        identity.publication = identity
            .publication
            .checked_add(1)
            .ok_or(SettingsServiceError::Stale)?;
        let mut baseline = build_baseline(
            identity,
            current.records.clone(),
            self.inner.owner.excluded()?,
        )?;
        baseline.applied.home_binding = current.applied.home_binding;
        let baseline = Arc::new(baseline);
        *self
            .inner
            .baseline
            .write()
            .map_err(|_| SettingsServiceError::Custody)? = Arc::clone(&baseline);
        let observation = unresolved_observation(op, baseline);
        finish_observation(op, observation.clone())?;
        Ok(observation)
    }
}
