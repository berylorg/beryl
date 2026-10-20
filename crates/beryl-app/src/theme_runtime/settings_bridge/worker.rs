use std::panic::{AssertUnwindSafe, catch_unwind};

use super::*;

fn not_started(
    ticket: ThemeSettingsTicket,
    home: ThemeHomeIdentity,
    failure: ThemeRuntimeFailureClass,
) -> (
    Result<ThemeSettingsResult, ThemeSettingsBridgeError>,
    Option<ThemeSettingsCompletion>,
) {
    (
        Err(ThemeSettingsBridgeError::NotStarted(failure)),
        Some(ThemeSettingsCompletion::not_started(ticket, home, failure)),
    )
}

fn definite_refusal(
    error: &beryl_state::ThemeRepositoryExecutionError,
) -> Option<ThemeRuntimeFailureClass> {
    use beryl_state::{ThemeCommandFactError as Fact, ThemeRepositoryExecutionError as Error};
    match error {
        Error::ManifestDecode(beryl_state::ThemeManifestDecodeError::LimitExceeded(limit))
        | Error::ManifestEncode(beryl_state::ThemeManifestEncodeError::LimitExceeded(limit)) => {
            Some(match limit {
                beryl_state::ThemeManifestLimit::InstalledEntries => {
                    ThemeRuntimeFailureClass::InstalledEntryLimit
                }
                beryl_state::ThemeManifestLimit::EncodedBytes => {
                    ThemeRuntimeFailureClass::ManifestByteLimit
                }
                _ => ThemeRuntimeFailureClass::Repository,
            })
        }
        Error::InvalidLimits
        | Error::Document(_)
        | Error::ManifestDecode(_)
        | Error::ManifestEncode(_)
        | Error::Service(_)
        | Error::Identity(_)
        | Error::ReferenceSnapshotUnavailable => Some(ThemeRuntimeFailureClass::Repository),
        Error::CommandFact(
            Fact::ManifestMismatch
            | Fact::ExpectedRowMismatch
            | Fact::ThemeAlreadyInstalled
            | Fact::ThemeNotInstalled
            | Fact::NewOrderOutOfRange
            | Fact::DeleteReferenceChanged
            | Fact::ScopeGated
            | Fact::RetainedOperationCapacityExhausted
            | Fact::ReconciliationAlreadyInFlight
            | Fact::CollisionScopeClosed,
        ) => Some(ThemeRuntimeFailureClass::Repository),
        _ => None,
    }
}

pub(super) fn run(home: HomeServiceReference, source: ThemeSettingsSource, shared: Arc<Shared>) {
    let result = catch_unwind(AssertUnwindSafe(|| drive(&home, &source, &shared)));
    let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
    state.stopped = true;
    if result.is_err() {
        state.worker_failed = true;
        for slot in state.slots.values_mut() {
            slot.state = SlotState::Ready(Err(ThemeSettingsBridgeError::Worker));
        }
    }
    shared.changed.notify_all();
}

fn drive(home: &HomeServiceReference, source: &ThemeSettingsSource, shared: &Shared) {
    loop {
        let (request, work, selection, retained_target) = {
            let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if state.closing {
                    drop(state);
                    reconcile_retiring(home, source, shared);
                    return;
                }
                if let Some(request) = state.queue.pop_front() {
                    let Some(slot) = state.slots.get_mut(&request) else {
                        continue;
                    };
                    let SlotState::Queued(work) =
                        std::mem::replace(&mut slot.state, SlotState::Running)
                    else {
                        continue;
                    };
                    break (
                        request,
                        work,
                        state.selection.clone(),
                        state.unresolved_target.clone(),
                    );
                }
                state = shared
                    .changed
                    .wait(state)
                    .unwrap_or_else(|e| e.into_inner());
            }
        };
        let (result, completion) = execute(
            home,
            source,
            work,
            selection,
            ThemeSettingsTicket {
                broker: shared.id,
                request,
            },
            retained_target,
        );
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(completion) = completion {
            state.unresolved = completion.unresolved;
            state.unresolved_ticket = completion.unresolved.map(|_| completion.ticket);
            state.unresolved_target = completion
                .unresolved
                .and(completion.appearance_target.clone());
            state.completion = Some(completion);
        }
        if state.slots.get(&request).is_some_and(|slot| slot.cancelled) {
            state.slots.remove(&request);
        } else if let Some(slot) = state.slots.get_mut(&request) {
            slot.state = SlotState::Ready(result);
        }
        shared.changed.notify_all();
    }
}

fn execute(
    home: &HomeServiceReference,
    source: &ThemeSettingsSource,
    work: ThemeSettingsWork,
    selection: Option<(ThemeSettingsIdentity, Option<InstalledThemeId>)>,
    ticket: ThemeSettingsTicket,
    retained_target: Option<InstalledThemeId>,
) -> (
    Result<ThemeSettingsResult, ThemeSettingsBridgeError>,
    Option<ThemeSettingsCompletion>,
) {
    let result = match work {
        ThemeSettingsWork::Observe => source
            .observe_catalog(home)
            .map(ThemeSettingsResult::Catalog),
        ThemeSettingsWork::PageRange {
            observation,
            range,
            focus,
        } => source
            .catalog_range(home, observation, range, focus)
            .map(ThemeSettingsResult::Page),
        ThemeSettingsWork::LoadDocument {
            observation,
            selection,
        } => source
            .selected_document(home, &observation, &selection)
            .map(ThemeSettingsResult::Document),
        ThemeSettingsWork::Execute {
            observation,
            command,
            references,
        } => {
            if let Err(error) = source.check(home) {
                return not_started(ticket, source.home(), error);
            }
            let appearance_target = match &command {
                ThemeRepositoryCommand::Save(save) => {
                    Some(save.draft().binding().theme_id().clone())
                }
                ThemeRepositoryCommand::Update(update) => Some(update.target().theme_id().clone()),
                _ => None,
            };
            match source.service().execute_command(
                home,
                &observation,
                &command,
                source.max_manifest_bytes(),
                references.as_ref(),
            ) {
                Ok(outcome) => {
                    let original = Arc::new(outcome);
                    let (publication, unresolved) = match original.as_ref() {
                        ThemeRepositoryOperationOutcome::Committed { publication, .. } => {
                            (Some(publication.clone()), None)
                        }
                        ThemeRepositoryOperationOutcome::Indeterminate(operation) => {
                            (None, Some(operation.operation()))
                        }
                        ThemeRepositoryOperationOutcome::NotCommitted { .. } => (None, None),
                    };
                    let completion = complete(
                        ticket,
                        appearance_target,
                        home,
                        source,
                        publication,
                        unresolved,
                        selection,
                        Some(Arc::clone(&original)),
                        None,
                    );
                    return (
                        Ok(ThemeSettingsResult::Repository(original)),
                        Some(completion),
                    );
                }
                Err(error) => {
                    if let Some(failure) = definite_refusal(&error) {
                        return not_started(ticket, source.home(), failure);
                    }
                    Err(ThemeRuntimeFailureClass::Repository)
                }
            }
        }
        ThemeSettingsWork::Reconcile { operation } => {
            if let Err(error) = source.check(home) {
                return (Err(ThemeSettingsBridgeError::Theme(error)), None);
            }
            match source
                .service()
                .reconcile_operation(home, operation, source.max_manifest_bytes())
            {
                Ok(outcome) => {
                    let original = Arc::new(outcome);
                    let publication = match original.as_ref() {
                        ThemeReconciliation::ExactNew(publication) => Some(publication.clone()),
                        _ => None,
                    };
                    let unresolved = matches!(original.as_ref(), ThemeReconciliation::Collision)
                        .then_some(operation);
                    let completion = complete(
                        ticket,
                        retained_target,
                        home,
                        source,
                        publication,
                        unresolved,
                        selection,
                        None,
                        Some(Arc::clone(&original)),
                    );
                    return (
                        Ok(ThemeSettingsResult::Reconciled(original)),
                        Some(completion),
                    );
                }
                Err(_) => Err(ThemeRuntimeFailureClass::Reconciliation),
            }
        }
    };
    (result.map_err(ThemeSettingsBridgeError::Theme), None)
}

fn complete(
    ticket: ThemeSettingsTicket,
    appearance_target: Option<InstalledThemeId>,
    home: &HomeServiceReference,
    source: &ThemeSettingsSource,
    publication: Option<ThemeRepositoryCommit>,
    unresolved: Option<NonZeroU64>,
    selection: Option<(ThemeSettingsIdentity, Option<InstalledThemeId>)>,
    repository: Option<Arc<ThemeRepositoryOperationOutcome>>,
    reconciliation: Option<Arc<ThemeReconciliation>>,
) -> ThemeSettingsCompletion {
    let mut prepared = None;
    let mut preparation_failure = None;
    if publication
        .as_ref()
        .is_some_and(|publication| publication.manifest().is_none())
    {
        if let Some((settings, active)) = selection {
            if appearance_target.as_ref() == active.as_ref()
                && active.as_ref().is_some_and(|active| {
                    publication
                        .as_ref()
                        .unwrap()
                        .affected_documents()
                        .iter()
                        .any(|document| document.theme_id() == active)
                })
            {
                match source.prepare_current(home, settings, active.as_ref()) {
                    Ok(value) => prepared = Some(value),
                    Err(error) => preparation_failure = Some(error),
                }
            }
        }
    }
    ThemeSettingsCompletion {
        ticket,
        not_started: None,
        appearance_target,
        home: source.home(),
        publication,
        unresolved,
        prepared,
        repository,
        reconciliation,
        preparation_failure,
    }
}

fn reconcile_retiring(home: &HomeServiceReference, source: &ThemeSettingsSource, shared: &Shared) {
    let operation = shared
        .state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .unresolved;
    let Some(operation) = operation else {
        return;
    };
    if source.check(home).is_err() {
        return;
    }
    let Ok(outcome) =
        source
            .service()
            .reconcile_operation(home, operation, source.max_manifest_bytes())
    else {
        return;
    };
    let original = Arc::new(outcome);
    let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
    if !matches!(original.as_ref(), ThemeReconciliation::Collision) {
        state.unresolved = None;
    }
    if let Some(completion) = &mut state.completion {
        completion.reconciliation = Some(Arc::clone(&original));
        completion.unresolved =
            matches!(original.as_ref(), ThemeReconciliation::Collision).then_some(operation);
        if let ThemeReconciliation::ExactNew(publication) = original.as_ref() {
            completion.publication = Some(publication.clone());
        }
    } else {
        state.completion = Some(ThemeSettingsCompletion {
            ticket: state
                .unresolved_ticket
                .expect("retained original operation ticket"),
            not_started: None,
            appearance_target: state.unresolved_target.clone(),
            home: source.home(),
            publication: match original.as_ref() {
                ThemeReconciliation::ExactNew(publication) => Some(publication.clone()),
                _ => None,
            },
            unresolved: state.unresolved,
            prepared: None,
            repository: None,
            reconciliation: Some(original),
            preparation_failure: None,
        });
    }
}
