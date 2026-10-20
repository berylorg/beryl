use super::*;
pub(super) fn classify_original(
    op: &SettingsOperation,
    retry: impl FnOnce(
        &ReconciliationHandle,
    ) -> Result<ReconciliationResolution, beryl_home_store::ReconciliationFailure>,
) -> Result<Option<bool>, SettingsServiceError> {
    let handle = {
        let state = op.state.lock().map_err(|_| SettingsServiceError::Custody)?;
        match &state.original {
            OriginalOutcome::Unexecuted => return Ok(None),
            OriginalOutcome::Direct(CommandOutcome::NotCommitted { .. }) => return Ok(Some(false)),
            OriginalOutcome::Direct(CommandOutcome::Committed { .. }) => return Ok(Some(true)),
            OriginalOutcome::Direct(CommandOutcome::Indeterminate { .. }) => {
                return Err(SettingsServiceError::Custody);
            }
            OriginalOutcome::Installed {
                handle, resolution, ..
            } => {
                if let Some(resolution) = resolution {
                    return classify_resolution(resolution);
                }
                handle.clone()
            }
        }
    };
    let result = retry(&handle)?;
    let mut state = op.state.lock().map_err(|_| SettingsServiceError::Custody)?;
    let OriginalOutcome::Installed { resolution, .. } = &mut state.original else {
        return Err(SettingsServiceError::Custody);
    };
    classify_resolution(resolution.get_or_insert(result))
}
fn classify_resolution(
    resolution: &ReconciliationResolution,
) -> Result<Option<bool>, SettingsServiceError> {
    match resolution {
        ReconciliationResolution::ExactOld => Ok(Some(false)),
        ReconciliationResolution::ExactNew { .. } => Ok(Some(true)),
        _ => Err(SettingsServiceError::Changed),
    }
}
pub(super) fn finish_observation(
    op: &SettingsOperation,
    observation: SettingsApplyObservation,
) -> Result<(), SettingsServiceError> {
    op.state
        .lock()
        .map_err(|_| SettingsServiceError::Custody)?
        .observation = Some(observation);
    Ok(())
}
pub(super) fn committed_observation(
    op: &SettingsOperation,
    baseline: Arc<SettingsBaseline>,
    theme: Option<ConfirmedSettingsTheme>,
) -> SettingsApplyObservation {
    SettingsApplyObservation::Committed {
        operation: op.id,
        binding: op.binding,
        action: op.action,
        baseline,
        theme,
    }
}
pub(super) fn not_committed_observation(op: &SettingsOperation) -> SettingsApplyObservation {
    SettingsApplyObservation::NotCommitted {
        operation: op.id,
        binding: op.binding,
        action: op.action,
        issue: SettingsIssue::CommandRefused,
    }
}
pub(super) fn unresolved_observation(
    op: &SettingsOperation,
    baseline: Arc<SettingsBaseline>,
) -> SettingsApplyObservation {
    SettingsApplyObservation::ScopedUnresolved {
        operation: op.id,
        binding: op.binding,
        action: op.action,
        baseline,
        issue: SettingsIssue::Unresolved,
    }
}
