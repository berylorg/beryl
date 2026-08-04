use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    time::Duration,
};

use super::ProjectionCandidateId;

const PAUSE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum CandidateReauthenticationPauseStage {
    AfterPreAuth,
    BeforeStableReadConfirmation,
    AfterStableRead,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum CandidateReauthenticationFactFault {
    RegistryConnectionIdentity,
    RegistryKey(beryl_model::CasThreadId),
    RegistrySyndicOwner(beryl_model::SyndicThreadId),
    RegistryLoadedGeneration(beryl_model::CasLoadedSessionGeneration),
    WitnessHomeId(beryl_model::BerylHomeId),
    WitnessHomeGeneration(beryl_home_store::HomeGeneration),
    WitnessSyndicOwner(beryl_model::SyndicThreadId),
    WitnessLoadedGeneration(beryl_model::CasLoadedSessionGeneration),
    GroupConnectionKey(beryl_model::CasThreadId),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct CandidateReauthenticationPauseKey {
    candidate_id: ProjectionCandidateId,
    stage: CandidateReauthenticationPauseStage,
}

struct PendingCandidateReauthenticationPause {
    token: u64,
    arrived: SyncSender<()>,
    release: Receiver<()>,
}

struct PendingCandidateSetSealPause {
    token: u64,
    arrived: SyncSender<()>,
    release: Receiver<()>,
}

#[derive(Clone)]
pub(super) struct CandidateReauthenticationPauseRegistry {
    next_token: Arc<AtomicU64>,
    pauses: Arc<
        Mutex<HashMap<CandidateReauthenticationPauseKey, PendingCandidateReauthenticationPause>>,
    >,
    seal_pause: Arc<Mutex<Option<PendingCandidateSetSealPause>>>,
    fact_faults: Arc<Mutex<HashMap<ProjectionCandidateId, CandidateReauthenticationFactFault>>>,
}

pub(in crate::cas_projection) struct CandidateReauthenticationPauseController {
    registry: CandidateReauthenticationPauseRegistry,
    key: CandidateReauthenticationPauseKey,
    token: u64,
    arrived: Receiver<()>,
    release: SyncSender<()>,
}

pub(in crate::cas_projection) struct CandidateSetSealPauseController {
    registry: CandidateReauthenticationPauseRegistry,
    token: u64,
    arrived: Receiver<()>,
    release: SyncSender<()>,
}

impl CandidateReauthenticationPauseRegistry {
    pub(super) fn new() -> Self {
        Self {
            next_token: Arc::new(AtomicU64::new(1)),
            pauses: Arc::new(Mutex::new(HashMap::new())),
            seal_pause: Arc::new(Mutex::new(None)),
            fact_faults: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(super) fn install_fact_fault(
        &self,
        candidate_id: ProjectionCandidateId,
        fault: CandidateReauthenticationFactFault,
    ) {
        let previous = self
            .fact_faults
            .lock()
            .expect("candidate-reauthentication fact-fault registry is usable")
            .insert(candidate_id, fault);
        assert!(
            previous.is_none(),
            "one candidate may own only one pending authentication-fact fault"
        );
    }

    pub(super) fn take_fact_fault(
        &self,
        candidate_id: ProjectionCandidateId,
    ) -> Option<CandidateReauthenticationFactFault> {
        self.fact_faults
            .lock()
            .expect("candidate-reauthentication fact-fault registry is usable")
            .remove(&candidate_id)
    }

    pub(super) fn install_seal_pause(&self) -> CandidateSetSealPauseController {
        let token = self.next_token.fetch_add(1, Ordering::Relaxed);
        let (arrived, observation) = sync_channel(1);
        let (release, continuation) = sync_channel(1);
        let mut pause = self
            .seal_pause
            .lock()
            .expect("candidate-set seal pause registry is usable");
        assert!(
            pause.is_none(),
            "only one candidate-set seal pause may be installed"
        );
        *pause = Some(PendingCandidateSetSealPause {
            token,
            arrived,
            release: continuation,
        });
        drop(pause);
        CandidateSetSealPauseController {
            registry: self.clone(),
            token,
            arrived: observation,
            release,
        }
    }

    pub(super) fn pause_seal_if_requested(&self) {
        let pending = self
            .seal_pause
            .lock()
            .expect("candidate-set seal pause registry is usable")
            .take();
        let Some(pending) = pending else {
            return;
        };
        pending
            .arrived
            .send(())
            .expect("candidate-set seal test still observes its pause");
        pending
            .release
            .recv_timeout(PAUSE_TIMEOUT)
            .expect("candidate-set seal test releases its pause");
    }

    pub(super) fn install(
        &self,
        candidate_id: ProjectionCandidateId,
        stage: CandidateReauthenticationPauseStage,
    ) -> CandidateReauthenticationPauseController {
        let key = CandidateReauthenticationPauseKey {
            candidate_id,
            stage,
        };
        let token = self.next_token.fetch_add(1, Ordering::Relaxed);
        let (arrived, observation) = sync_channel(1);
        let (release, continuation) = sync_channel(1);
        let pending = PendingCandidateReauthenticationPause {
            token,
            arrived,
            release: continuation,
        };
        let mut pauses = self
            .pauses
            .lock()
            .expect("candidate-reauthentication pause registry is usable");
        assert!(
            pauses.insert(key, pending).is_none(),
            "one candidate and reauthentication stage may own only one pause",
        );
        drop(pauses);
        CandidateReauthenticationPauseController {
            registry: self.clone(),
            key,
            token,
            arrived: observation,
            release,
        }
    }

    pub(super) fn pause_if_requested(
        &self,
        candidate_id: ProjectionCandidateId,
        stage: CandidateReauthenticationPauseStage,
    ) {
        let key = CandidateReauthenticationPauseKey {
            candidate_id,
            stage,
        };
        let pending = self
            .pauses
            .lock()
            .expect("candidate-reauthentication pause registry is usable")
            .remove(&key);
        let Some(pending) = pending else {
            return;
        };
        pending
            .arrived
            .send(())
            .expect("candidate-reauthentication test still observes its pause");
        pending
            .release
            .recv_timeout(PAUSE_TIMEOUT)
            .expect("candidate-reauthentication test releases its pause");
    }
}

impl CandidateReauthenticationPauseController {
    pub(in crate::cas_projection) fn wait_until_paused(&self, timeout: Duration) {
        self.arrived
            .recv_timeout(timeout)
            .expect("candidate reauthentication reached its requested pause");
    }

    pub(in crate::cas_projection) fn release(self) {
        self.release
            .send(())
            .expect("paused candidate reauthentication still awaits release");
    }
}

impl CandidateSetSealPauseController {
    pub(in crate::cas_projection) fn wait_until_paused(&self, timeout: Duration) {
        self.arrived
            .recv_timeout(timeout)
            .expect("candidate-set seal reached its requested pause");
    }

    pub(in crate::cas_projection) fn release(self) {
        self.release
            .send(())
            .expect("paused candidate-set seal still awaits release");
    }
}

impl Drop for CandidateReauthenticationPauseController {
    fn drop(&mut self) {
        let mut pauses = self
            .registry
            .pauses
            .lock()
            .expect("candidate-reauthentication pause registry is usable");
        if pauses
            .get(&self.key)
            .is_some_and(|pending| pending.token == self.token)
        {
            pauses.remove(&self.key);
        }
    }
}

impl Drop for CandidateSetSealPauseController {
    fn drop(&mut self) {
        let mut pause = self
            .registry
            .seal_pause
            .lock()
            .expect("candidate-set seal pause registry is usable");
        if pause
            .as_ref()
            .is_some_and(|pending| pending.token == self.token)
        {
            pause.take();
        }
        drop(pause);
        let _ = self.release.send(());
    }
}
