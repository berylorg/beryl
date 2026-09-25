use super::*;
use beryl_model::{SessionRevision, SyndicDraftId, WindowId};
use beryl_state::{SessionState, SessionWindowRecord, ThreadClaimRecord, ThreadClaimState};
use std::sync::{
    Weak,
    atomic::{AtomicBool, Ordering},
};
use syndic_storage::{DraftPieceTextDemandV1, SyndicPointReadLimit};

struct RestoreAttemptIdentity {
    live: AtomicBool,
}

#[cfg(feature = "test-faults")]
pub struct RestoredWindowServiceTestLifetime(Arc<()>);

pub struct RestoredWindowPreparationAttempt {
    identity: Arc<RestoreAttemptIdentity>,
    service: Weak<()>,
    store: Arc<HomeServiceReference>,
    session: SessionState,
    storage: SyndicStorage,
    generation: HomeGeneration,
}

struct RestoredWindowSource {
    attempt: Arc<RestoreAttemptIdentity>,
    service: Weak<()>,
    store: Arc<HomeServiceReference>,
    session: SessionState,
    storage: SyndicStorage,
    generation: HomeGeneration,
    session_revision: SessionRevision,
    window: SessionWindowRecord,
    claim: ThreadClaimRecord,
    draft_id: SyndicDraftId,
}

impl Drop for RestoredWindowPreparationAttempt {
    fn drop(&mut self) {
        self.identity.live.store(false, Ordering::Release);
    }
}

impl RestoredWindowPreparationAttempt {
    pub(crate) fn validate_lifetime(&self) -> Result<(), String> {
        if !self.identity.live.load(Ordering::Acquire)
            || self.service.upgrade().is_none()
            || self.store.health().generation() != Some(self.generation)
        {
            return Err("restore attempt or service generation is retired".to_owned());
        }
        Ok(())
    }

    #[cfg(feature = "test-faults")]
    pub fn new_for_test(
        store: Arc<HomeStore>,
        session: SessionState,
        storage: SyndicStorage,
    ) -> Result<(Self, RestoredWindowServiceTestLifetime), String> {
        let lifetime = Arc::new(());
        let attempt = Self::new(
            Arc::new(store.service_reference()),
            session,
            storage,
            Arc::downgrade(&lifetime),
        )?;
        Ok((attempt, RestoredWindowServiceTestLifetime(lifetime)))
    }
}

impl RestoredWindowPreparationAttempt {
    pub fn begin(
        &self,
        revision: SessionRevision,
        window_id: WindowId,
        request: ComposerHostActivationRequest,
        retirement_operation: DraftPieceOperationIdV1,
        marker_authority: MainWindowComposerMarkerMetadataAuthority,
    ) -> Result<RestoredWindowComposer, String> {
        let source = self.capture(revision, window_id)?;
        let claim = source.window.selected_thread().unwrap();
        if request.thread_id() != claim.thread_id() {
            return Err("restore editor request selects a different thread".to_owned());
        }
        SyndicComposerHost::validate_initial_request(&request)
            .map_err(|error| error.to_string())?;
        let candidate = InitialComposerCandidate::new(
            self.store.clone(),
            self.storage.clone(),
            self.generation,
            claim,
            request,
            retirement_operation,
            marker_authority,
        );
        Ok(RestoredWindowComposer { source, candidate })
    }

    pub(crate) fn new(
        store: Arc<HomeServiceReference>,
        session: SessionState,
        storage: SyndicStorage,
        service: Weak<()>,
    ) -> Result<Self, String> {
        session
            .revision(&store)
            .map_err(|error| error.to_string())?;
        storage
            .revision(&store)
            .map_err(|error| error.to_string())?;
        let generation = store
            .health()
            .generation()
            .ok_or_else(|| "restoration home is unavailable".to_owned())?;
        Ok(Self {
            identity: Arc::new(RestoreAttemptIdentity {
                live: AtomicBool::new(true),
            }),
            service,
            store,
            session,
            storage,
            generation,
        })
    }

    fn capture(
        &self,
        revision: SessionRevision,
        window_id: WindowId,
    ) -> Result<RestoredWindowSource, String> {
        self.validate_lifetime()?;
        let before = self
            .store
            .home_revision()
            .map_err(|error| error.to_string())?;
        let bootstrap = self
            .session
            .minimal_bootstrap(&self.store)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "restore session is missing".to_owned())?;
        if bootstrap.header().revision() != revision {
            return Err("restore session revision changed".to_owned());
        }
        let window = bootstrap
            .windows()
            .iter()
            .find(|window| window.window_id() == window_id)
            .ok_or_else(|| "restore member is missing".to_owned())?
            .clone();
        let selection = window
            .selected_thread()
            .ok_or_else(|| "runtime-backed restore member has no selected thread".to_owned())?;
        let claim = self
            .session
            .thread_claim_catalog_source(&self.store, selection.thread_id())
            .map_err(|error| error.to_string())?
            .claim()
            .ok_or_else(|| "restore claim is missing".to_owned())?;
        let draft_id = self
            .storage
            .current_draft_piece_text_demand(
                &self.store,
                selection.thread_id(),
                DraftPieceTextDemandV1::Validate(0),
                4,
            )
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "restore draft is missing".to_owned())?
            .selector()
            .draft_id();
        let source = RestoredWindowSource {
            attempt: self.identity.clone(),
            service: self.service.clone(),
            store: self.store.clone(),
            session: self.session.clone(),
            storage: self.storage.clone(),
            generation: self.generation,
            session_revision: revision,
            window,
            claim,
            draft_id,
        };
        source.validate()?;
        if self
            .store
            .home_revision()
            .map_err(|error| error.to_string())?
            != before
        {
            return Err("restore source changed during discovery".to_owned());
        }
        Ok(source)
    }
}

pub struct RestoredWindowComposer {
    source: RestoredWindowSource,
    candidate: InitialComposerCandidate,
}

pub struct RestoredWindowComposerFailure {
    pub custody: RestoredWindowComposer,
    pub error: String,
}

pub enum RestoredWindowComposerRetirement {
    Retired,
    Pending(RestoredWindowComposerFailure),
}

pub struct RestoredWindowComposerPrepared {
    prepared: MainWindowConversationComposerPreparedSelection,
    custody: RestoredWindowComposer,
}

impl RestoredWindowComposer {
    pub fn window_id(&self) -> WindowId {
        self.source.window.window_id()
    }

    pub fn advance(
        &mut self,
        attempt: &RestoredWindowPreparationAttempt,
        cancellation: &CommandCancellation,
    ) -> Result<MainWindowInitialComposerProgress, String> {
        if cancellation.is_cancelled() {
            return Err("restored editor activation cancelled".to_owned());
        }
        self.validate_attempt(attempt)?;
        let source = &self.source;
        self.candidate.advance(
            cancellation,
            source.claim.thread_id(),
            source.draft_id,
            &|| source.validate(),
        )
    }

    fn validate_attempt(&self, attempt: &RestoredWindowPreparationAttempt) -> Result<(), String> {
        if !Arc::ptr_eq(&self.source.attempt, &attempt.identity)
            || !Arc::ptr_eq(&self.source.store, &attempt.store)
        {
            return Err("restored editor belongs to another startup attempt".to_owned());
        }
        self.source.validate()
    }

    pub fn prepare(
        mut self,
        attempt: &RestoredWindowPreparationAttempt,
        configurator: &mut impl FnMut(
            super::super::MainWindowComposerSelectionIdentity,
        ) -> Result<
            super::super::MainWindowConversationComposerConfig,
            String,
        >,
    ) -> Result<RestoredWindowComposerPrepared, RestoredWindowComposerFailure> {
        let result = self.validate_attempt(attempt).and_then(|()| {
            let source = &self.source;
            self.candidate
                .prepare_selection(source.window.window_id(), configurator, &|| {
                    source.validate()
                })
        });
        match result {
            Ok(prepared) => Ok(RestoredWindowComposerPrepared {
                prepared,
                custody: self,
            }),
            Err(error) => Err(RestoredWindowComposerFailure {
                custody: self,
                error,
            }),
        }
    }

    pub fn retire(mut self, cancellation: CommandCancellation) -> RestoredWindowComposerRetirement {
        self.candidate.retirement_started = true;
        match self.candidate.drive_retirement(cancellation) {
            Ok(true) => RestoredWindowComposerRetirement::Retired,
            result => RestoredWindowComposerRetirement::Pending(RestoredWindowComposerFailure {
                custody: self,
                error: result
                    .err()
                    .unwrap_or_else(|| "restored editor retirement remains pending".to_owned()),
            }),
        }
    }
}

impl RestoredWindowComposerPrepared {
    pub fn selection_identity(&self) -> super::super::MainWindowComposerSelectionIdentity {
        self.prepared.selection_identity()
    }

    pub fn retire(self, cancellation: CommandCancellation) -> RestoredWindowComposerRetirement {
        drop(self.prepared);
        self.custody.retire(cancellation)
    }
}

impl RestoredWindowSource {
    fn validate(&self) -> Result<(), String> {
        if !self.attempt.live.load(Ordering::Acquire) || self.service.upgrade().is_none() {
            return Err("restore attempt or service generation is retired".to_owned());
        }
        if self.store.health().generation() != Some(self.generation) {
            return Err("restore home generation changed".to_owned());
        }
        let before = self
            .store
            .home_revision()
            .map_err(|error| error.to_string())?;
        let bootstrap = self
            .session
            .minimal_bootstrap(&self.store)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "restore session is missing".to_owned())?;
        if bootstrap.header().revision() != self.session_revision
            || !bootstrap
                .windows()
                .iter()
                .any(|window| window == &self.window)
        {
            return Err("restore member or session revision changed".to_owned());
        }
        let selection = self
            .window
            .selected_thread()
            .ok_or_else(|| "restore selection is missing".to_owned())?;
        let claim = self
            .session
            .thread_claim_catalog_source(&self.store, selection.thread_id())
            .map_err(|error| error.to_string())?
            .claim();
        if claim != Some(self.claim)
            || self.claim.window_id() != self.window.window_id()
            || self.claim.generation() != selection.generation()
            || self.claim.revision() != selection.revision()
            || self.claim.state() != ThreadClaimState::Restoring
        {
            return Err("restore paired claim changed".to_owned());
        }
        let target = self
            .window
            .remembered_target()
            .ok_or_else(|| "restore runtime/root is missing".to_owned())?;
        let execution = self
            .storage
            .thread_execution(
                &self.store,
                selection.thread_id(),
                SyndicPointReadLimit::new(65_536).unwrap(),
            )
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "restore execution binding is missing".to_owned())?;
        if execution.execution().runtime_id() != target.runtime_id()
            || execution.execution().root_id() != target.root_id()
        {
            return Err("restore runtime/root differs from selected thread".to_owned());
        }
        let draft = self
            .storage
            .current_draft_piece_text_demand(
                &self.store,
                selection.thread_id(),
                DraftPieceTextDemandV1::Validate(0),
                4,
            )
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "restore draft is missing".to_owned())?;
        if draft.selector().draft_id() != self.draft_id
            || self
                .store
                .home_revision()
                .map_err(|error| error.to_string())?
                != before
        {
            return Err("restore draft or coherent source revision changed".to_owned());
        }
        Ok(())
    }
}
