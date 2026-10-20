use super::*;
impl SettingsProcessOwner {
    pub fn new(home: BerylHomeId) -> Self {
        Self {
            facts: Arc::new(Mutex::new(ProcessFacts {
                home,
                excluded: 0,
                active: None,
                latest: None,
                unresolved: Vec::new(),
                last_operation: 0,
                allocated_operation: 0,
            })),
        }
    }
    pub(in crate::settings_service) fn check_home(
        &self,
        home: BerylHomeId,
    ) -> Result<(), SettingsServiceError> {
        if self
            .facts
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?
            .home
            != home
        {
            return Err(SettingsServiceError::ForeignHome);
        }
        Ok(())
    }
    pub fn excluded_keys(&self) -> Result<Vec<SettingKey>, SettingsServiceError> {
        let excluded = self.excluded()?;
        Ok(SETTINGS_KEYS
            .into_iter()
            .filter(|key| excluded & mask(*key) != 0)
            .collect())
    }
    pub(in crate::settings_service) fn excluded(&self) -> Result<u8, SettingsServiceError> {
        Ok(self
            .facts
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?
            .excluded)
    }
    pub fn pending_operation(&self) -> Option<u64> {
        self.facts
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .active
            .as_ref()
            .map(|op| op.id)
    }
    pub fn next_operation_id(&self) -> Result<u64, SettingsServiceError> {
        let mut facts = self
            .facts
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?;
        facts.allocated_operation = facts
            .allocated_operation
            .checked_add(1)
            .ok_or(SettingsServiceError::Stale)?;
        Ok(facts.allocated_operation)
    }
    pub(super) fn operation(
        &self,
        id: u64,
    ) -> Result<Arc<SettingsOperation>, SettingsServiceError> {
        let facts = self
            .facts
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?;
        facts
            .active
            .iter()
            .chain(facts.latest.iter())
            .chain(facts.unresolved.iter())
            .find(|op| op.id == id)
            .cloned()
            .ok_or(SettingsServiceError::Stale)
    }
    pub fn observation(
        &self,
        id: u64,
    ) -> Result<Option<SettingsApplyObservation>, SettingsServiceError> {
        Ok(self
            .operation(id)?
            .state
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?
            .observation
            .clone())
    }
    pub(super) fn release(
        &self,
        op: &Arc<SettingsOperation>,
        unresolved: bool,
    ) -> Result<(), SettingsServiceError> {
        let mut facts = self
            .facts
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?;
        if !facts
            .active
            .as_ref()
            .is_some_and(|active| Arc::ptr_eq(active, op))
        {
            return Ok(());
        }
        facts.active = None;
        if unresolved {
            if facts.excluded & op.modified != op.modified {
                facts.excluded |= op.modified;
                facts.unresolved.push(Arc::clone(op));
            }
        }
        facts.latest = Some(Arc::clone(op));
        Ok(())
    }
    pub(in crate::settings_service) fn retire_preparation(
        &self,
    ) -> Result<(), SettingsServiceError> {
        let active = self
            .facts
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?
            .active
            .clone();
        if let Some(op) = active {
            let mut state = op.state.lock().map_err(|_| SettingsServiceError::Custody)?;
            state.theme = None;
            if matches!(state.original, OriginalOutcome::Unexecuted) {
                state.observation = Some(not_committed_observation(&op));
                drop(state);
                self.release(&op, false)?;
            }
        }
        Ok(())
    }
    pub fn reconcile_candidate_original(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        settings: &SettingsState,
    ) -> Result<(), SettingsServiceError> {
        self.check_home(access.home_id())?;
        let unresolved = self
            .facts
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?
            .unresolved
            .clone();
        for original in unresolved {
            let _ = classify_original(&original, |handle| access.retry_reconciliation(handle));
        }
        let active = self
            .facts
            .lock()
            .map_err(|_| SettingsServiceError::Custody)?
            .active
            .clone();
        let Some(op) = active else {
            return Ok(());
        };
        let classification = classify_original(&op, |handle| access.retry_reconciliation(handle));
        let committed = match classification {
            Ok(Some(value)) => Some(value),
            Ok(None) => return Err(SettingsServiceError::Busy),
            Err(_) => None,
        };
        let (revision, records) = read_candidate(access, settings)?;
        let expected = if committed == Some(true) {
            &op.after
        } else {
            &op.before.records
        };
        let exact = committed.is_some()
            && SETTINGS_KEYS
                .into_iter()
                .filter(|key| op.modified & mask(*key) != 0)
                .all(|key| records[index(key)] == expected[index(key)]);
        let mut identity = op.before.identity;
        identity.home_generation = access.generation();
        identity.settings_revision = revision;
        identity.publication = identity
            .publication
            .checked_add(1)
            .ok_or(SettingsServiceError::Stale)?;
        let excluded = self.excluded()? | if exact { 0 } else { op.modified };
        let baseline = Arc::new(build_baseline(identity, records, excluded)?);
        finish_observation(
            &op,
            if !exact {
                unresolved_observation(&op, baseline)
            } else if committed == Some(true) {
                committed_observation(&op, baseline, None)
            } else {
                not_committed_observation(&op)
            },
        )?;
        self.release(&op, !exact)
    }
}
