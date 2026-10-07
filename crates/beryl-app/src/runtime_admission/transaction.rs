use super::*;
use beryl_state::{
    CatalogSourceRevisions, CreateRuntimeWithHomeRoot, PreparedWindowClaimReplacement,
    PublishCatalogClaim, RecordRevision, RootRegistration, RuntimeRegistration,
    WindowClaimReplacementPreparation,
};
use syndic_storage::CreateThread;

pub(super) struct PreparedAdmission {
    pub command: HomeCommand,
    pub admission: AdmissionFacts,
}

pub(super) fn prepare_runtime(
    service: &RuntimeAdmissionService,
    source: &SessionWindowRecord,
    admitted: validation::AdmittedRuntime,
    runtime_id: RuntimeId,
    root_id: RootId,
    cancellation: &CommandCancellation,
) -> Result<PreparedAdmission, AdmissionError> {
    let store = &service.store;
    let before = store.home_revision().map_err(preparation)?;
    let registry = service.state.runtime_roots();
    let registry_revision = registry.revision(store).map_err(preparation)?;
    let first = registry
        .list_runtimes(store, None, CursorReadLimits::new(1, 32_768).unwrap())
        .map_err(preparation)?
        .records()
        .is_empty();
    let session = service
        .state
        .session()
        .minimal_bootstrap(store)
        .map_err(preparation)?
        .ok_or_else(|| invalid("runtime admission requires an initialized session"))?;
    if session.header().exit_intent() != SessionExitIntent::Running {
        return Err(invalid("runtime admission requires a running session"));
    }
    if session
        .windows()
        .iter()
        .find(|window| window.window_id() == source.window_id())
        != Some(source)
    {
        return Err(invalid("runtime admission invoking window changed"));
    }
    if first
        && (session.windows().len() != 1
            || source.selected_thread().is_some()
            || source.remembered_target().is_some()
            || session.header().fallback().is_some())
    {
        return Err(invalid(
            "first runtime requires the sole exact threadless window",
        ));
    }
    if !first && source.selected_thread().is_none() {
        return Err(invalid(
            "a configured home cannot admit from a threadless window",
        ));
    }
    let now = timestamp()?;
    let available =
        beryl_state::AvailabilitySnapshot::observed(beryl_model::Availability::Available, now)
            .map_err(preparation)?;
    let runtime = RuntimeRegistration::new(
        runtime_id,
        admitted.canonical_executable().clone(),
        admitted.mode().clone(),
        admitted.launch_form(),
        admitted.runtime_native_executable().clone(),
        now,
        available,
    )
    .map_err(preparation)?;
    let root = RootRegistration::new(
        root_id,
        admitted.home_root().clone(),
        admitted.home_display_path().clone(),
        now,
        available,
    );
    let creation = CreateRuntimeWithHomeRoot::new(runtime, root).map_err(preparation)?;
    let registry_source = creation.initial_catalog_source();
    let mut command = HomeCommand::new(before).with_cancellation(cancellation.clone());
    command
        .add(registry.create_runtime_with_home_root(registry_revision, creation))
        .map_err(preparation)?;
    let onboarding = if first {
        let thread_id = SyndicThreadId::from_bytes(identity()?);
        let draft_id = SyndicDraftId::from_bytes(identity()?);
        let target = RememberedTarget::new(runtime_id, root_id);
        let execution =
            beryl_model::ExecutionBinding::new(runtime_id, root_id, admitted.home_root().clone());
        let thread = CreateThread::ordinary(
            thread_id,
            draft_id,
            execution,
            syndic_storage::SyndicTimestamp::from_unix_millis(now.get()),
            service.history_policy,
        );
        let replacement = match service
            .state
            .session()
            .prepare_window_claim_replacement(store, source.window_id(), None, target, thread_id)
            .map_err(preparation)?
        {
            WindowClaimReplacementPreparation::Prepared(prepared) => prepared,
            _ => return Err(invalid("first-runtime thread identity is already claimed")),
        };
        let summary = thread.initial_catalog_summary();
        let facts = crate::window_acquisition::project_unclaimed_facts(&summary, &registry_source)
            .map_err(preparation)?;
        command
            .add(
                service.state.catalog().publish_claim(
                    service
                        .state
                        .catalog()
                        .revision(store)
                        .map_err(preparation)?,
                    PublishCatalogClaim::initial(
                        thread_id,
                        CatalogSourceRevisions::new(
                            summary.revision(),
                            RecordRevision::INITIAL,
                            RecordRevision::INITIAL,
                            None,
                        ),
                        facts,
                        replacement.catalog_claim(),
                    ),
                ),
            )
            .map_err(preparation)?;
        command
            .add(
                service
                    .syndic
                    .create_thread(service.syndic.revision(store).map_err(preparation)?, thread),
            )
            .map_err(preparation)?;
        command
            .add(
                replacement
                    .contribution(&service.state.session(), store)
                    .map_err(preparation)?,
            )
            .map_err(preparation)?;
        Some(OnboardingFacts {
            thread_id,
            draft_id,
            replacement,
        })
    } else {
        None
    };
    if store.home_revision().map_err(preparation)? != before {
        return Err(invalid(
            "runtime admission sources changed during preparation",
        ));
    }
    Ok(PreparedAdmission {
        command,
        admission: AdmissionFacts {
            window_id: source.window_id(),
            runtime_id,
            root_id,
            onboarding,
        },
    })
}

pub(super) fn prepare_root(
    service: &RuntimeAdmissionService,
    source: &SessionWindowRecord,
    runtime: &beryl_state::RuntimeRecord,
    root: validation::AdmittedRoot,
    root_id: RootId,
    cancellation: &CommandCancellation,
) -> Result<PreparedAdmission, AdmissionError> {
    let store = &service.store;
    let before = store.home_revision().map_err(preparation)?;
    let registry = service.state.runtime_roots();
    let revision = registry.revision(store).map_err(preparation)?;
    if registry
        .runtime(store, runtime.runtime_id())
        .map_err(preparation)?
        .as_ref()
        != Some(runtime)
    {
        return Err(invalid("root admission runtime changed"));
    }
    let session = service
        .state
        .session()
        .minimal_bootstrap(store)
        .map_err(preparation)?
        .ok_or_else(|| invalid("root admission requires an initialized session"))?;
    if session.header().exit_intent() != SessionExitIntent::Running {
        return Err(invalid("root admission requires a running session"));
    }
    if session
        .windows()
        .iter()
        .find(|window| window.window_id() == source.window_id())
        != Some(source)
        || source.selected_thread().is_none()
    {
        return Err(invalid("root admission invoking selection changed"));
    }
    let now = timestamp()?;
    let root = RootRegistration::new(
        root_id,
        root.runtime_native_path().clone(),
        root.display_path().clone(),
        now,
        beryl_state::AvailabilitySnapshot::observed(beryl_model::Availability::Available, now)
            .map_err(preparation)?,
    );
    let mut command = HomeCommand::new(before).with_cancellation(cancellation.clone());
    command
        .add(registry.add_root(
            revision,
            beryl_state::AddConfiguredRoot::new(runtime.runtime_id(), root),
        ))
        .map_err(preparation)?;
    if store.home_revision().map_err(preparation)? != before {
        return Err(invalid("root admission sources changed during preparation"));
    }
    Ok(PreparedAdmission {
        command,
        admission: AdmissionFacts {
            window_id: source.window_id(),
            runtime_id: runtime.runtime_id(),
            root_id,
            onboarding: None,
        },
    })
}

pub struct OnboardingFacts {
    thread_id: SyndicThreadId,
    draft_id: SyndicDraftId,
    replacement: PreparedWindowClaimReplacement,
}

impl OnboardingFacts {
    pub fn thread_id(&self) -> SyndicThreadId {
        self.thread_id
    }
    pub fn draft_id(&self) -> SyndicDraftId {
        self.draft_id
    }
    pub fn window(&self) -> &SessionWindowRecord {
        self.replacement.future_window()
    }
    pub fn claim(&self) -> beryl_state::WindowClaimSelection {
        self.replacement.future_selection()
    }
}

fn timestamp() -> Result<beryl_state::UnixMillis, AdmissionError> {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(preparation)?
        .as_millis();
    Ok(beryl_state::UnixMillis::new(
        millis.try_into().map_err(preparation)?,
    ))
}

pub(super) fn identity() -> Result<[u8; 16], AdmissionError> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(preparation)?;
    Ok(bytes)
}
