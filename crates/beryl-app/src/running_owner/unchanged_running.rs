use beryl_home_store::HomeRecoveryCandidate;
use beryl_model::{BerylHomeId, WindowId, WindowPlacement};
use beryl_state::{
    MinimalSessionBootstrap, RecordRevision, SessionExitIntent, SessionState, ThreadClaimState,
    WindowClaimSelection,
};

#[derive(Debug)]
pub(crate) struct RunningWindowFacts {
    pub(crate) window: WindowId,
    pub(crate) revision: RecordRevision,
    pub(crate) placement: WindowPlacement,
    pub(crate) selection: Option<WindowClaimSelection>,
}

#[derive(Debug)]
pub(crate) struct UnchangedRunning {
    first: Option<crate::runtime_admission::recovery::FirstConversationAdmissionRecovery>,
    home: BerylHomeId,
    generation: beryl_home_store::HomeGeneration,
    path: std::path::PathBuf,
    members: Vec<RunningWindowFacts>,
    thread_creations: Vec<WindowId>,
    pinned: Option<MinimalSessionBootstrap>,
}

impl UnchangedRunning {
    pub(crate) fn new(
        home: BerylHomeId,
        generation: beryl_home_store::HomeGeneration,
        path: std::path::PathBuf,
        members: Vec<RunningWindowFacts>,
    ) -> Self {
        Self {
            first: None,
            home,
            generation,
            path,
            members,
            thread_creations: Vec::new(),
            pinned: None,
        }
    }

    pub(crate) fn with_first(
        mut self,
        first: Option<crate::runtime_admission::recovery::FirstConversationAdmissionRecovery>,
    ) -> Self {
        self.first = first;
        self
    }

    pub(crate) fn with_thread_creations(mut self, windows: Vec<WindowId>) -> Self {
        assert!(
            windows
                .iter()
                .all(|window| self.members.iter().any(|member| member.window == *window))
        );
        self.thread_creations = windows;
        self
    }

    pub(crate) fn accept_thread_creation_candidate(
        &mut self,
        window: WindowId,
        committed: Option<&crate::same_window_thread_acquisition::SameWindowThreadCommit>,
        candidate: &mut HomeRecoveryCandidate,
        state: &beryl_state::BerylState,
    ) -> Result<(), String> {
        if !self.thread_creations.contains(&window) {
            return Err(
                "original thread creation window is not captured by the Running set".into(),
            );
        }
        let member = self
            .members
            .iter_mut()
            .find(|member| member.window == window)
            .ok_or("original thread creation window is missing")?;
        if let Some(committed) = committed {
            let access = candidate
                .recovery_access()
                .map_err(|error| error.to_string())?;
            committed
                .validate_candidate(&access, state)
                .map_err(|error| error.to_string())?;
            if committed.window.window_id() != window
                || committed.window.placement() != &member.placement
            {
                return Err("original thread creation changed its preserved native window".into());
            }
            member.revision = committed.window.revision();
            member.selection = Some(committed.selection);
        }
        self.thread_creations.retain(|captured| *captured != window);
        self.pinned = None;
        Ok(())
    }

    pub(crate) fn first_conversation_facts(
        &self,
    ) -> Option<&crate::runtime_admission::recovery::FirstConversationFacts> {
        self.first
            .as_ref()
            .filter(|first| first.committed())
            .map(|first| first.facts())
    }

    pub(crate) fn has_first_admission(&self) -> bool {
        self.first.is_some()
    }

    pub(crate) fn captured_generation(&self) -> beryl_home_store::HomeGeneration {
        self.generation
    }

    fn read(
        &self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
    ) -> Result<MinimalSessionBootstrap, String> {
        let access = candidate.recovery_access().map_err(|e| e.to_string())?;
        if access.home_id() != self.home
            || access.canonical_path() != self.path
            || self.members.is_empty()
            || self.members.len() > beryl_state::MAX_RESTORABLE_WINDOWS
        {
            return Err("unchanged Running capture belongs to another home or window set".into());
        }
        let revision = access.home_revision().map_err(|e| e.to_string())?;
        let snapshot = session
            .minimal_bootstrap_candidate(&access)
            .map_err(|e| e.to_string())?
            .ok_or("captured Running session is missing")?;
        if snapshot.header().exit_intent() != SessionExitIntent::Running
            || snapshot.windows().len() != self.members.len()
        {
            return Err("captured Running membership changed".into());
        }
        for captured in &self.members {
            let window = snapshot
                .windows()
                .iter()
                .find(|w| w.window_id() == captured.window)
                .ok_or("captured Running window is missing")?;
            let deferred = self.thread_creations.contains(&captured.window);
            if window.placement() != &captured.placement
                || !deferred
                    && (window.revision() != captured.revision
                        || window.selected_thread() != captured.selection)
            {
                return Err(
                    "captured Running window revision, placement or selection changed".into(),
                );
            }
            let source = session
                .window_claim_catalog_source_candidate(&access, captured.window)
                .map_err(|e| e.to_string())?;
            let exact = match (
                source.claim(),
                if deferred {
                    window.selected_thread()
                } else {
                    captured.selection
                },
            ) {
                (None, None) => self.members.len() == 1 && snapshot.header().fallback().is_none(),
                (Some(claim), Some(selection)) => {
                    claim.thread_id() == selection.thread_id()
                        && claim.generation() == selection.generation()
                        && claim.revision() == selection.revision()
                        && claim.state() == ThreadClaimState::Active
                }
                _ => false,
            };
            if !exact {
                return Err("captured Running paired Active claim changed".into());
            }
        }
        if self
            .pinned
            .as_ref()
            .is_some_and(|pinned| pinned != &snapshot)
        {
            return Err("pinned unchanged Running session changed".into());
        }
        if access.home_revision().map_err(|e| e.to_string())? != revision {
            return Err("unchanged Running candidate source changed during validation".into());
        }
        Ok(snapshot)
    }

    pub(crate) fn converge(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
    ) -> Result<(), String> {
        if let Some(first) = self.first.as_mut() {
            let access = candidate.recovery_access().map_err(|e| e.to_string())?;
            if first.settle(&access)? {
                let exact = first.facts().window();
                if self.members.len() != 1 {
                    return Err(
                        "original first conversation requires its sole preserved window".into(),
                    );
                }
                let member = self
                    .members
                    .iter_mut()
                    .find(|w| w.window == exact.window_id())
                    .ok_or("original first conversation window is absent from preserved set")?;
                if member.placement != *exact.placement() {
                    return Err("original first conversation preserved set changed".into());
                }
                member.revision = exact.revision();
                member.selection = exact.selected_thread();
            }
        }
        let snapshot = self.read(candidate, session)?;
        if self.thread_creations.is_empty() {
            self.pinned = Some(snapshot);
        }
        Ok(())
    }

    pub(crate) fn revalidate(
        &self,
        candidate: &mut HomeRecoveryCandidate,
        session: &SessionState,
    ) -> Result<(), String> {
        if self.pinned.is_none() || !self.thread_creations.is_empty() {
            return Err("unchanged Running session has not been authenticated".into());
        }
        self.read(candidate, session).map(|_| ())
    }
}
