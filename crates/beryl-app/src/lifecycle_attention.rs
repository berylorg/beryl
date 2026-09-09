use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicBool, Ordering},
};

use beryl_model::{BerylHomeId, SyndicThreadId, SyndicTurnId};

use crate::{LifecycleYieldOutcome, notice_limits::NOTICE_RECORD_CAPACITY};

struct Owner {
    closed: AtomicBool,
}

struct Attempt {
    owner: Arc<Owner>,
    offered: AtomicBool,
    home_id: BerylHomeId,
    thread_id: SyndicThreadId,
    turn_id: SyndicTurnId,
    outcome: LifecycleYieldOutcome,
}

#[derive(Clone)]
pub struct LifecycleAttentionAttempt(Arc<Attempt>);

impl std::fmt::Debug for LifecycleAttentionAttempt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LifecycleAttentionAttempt(opaque)")
    }
}

#[derive(Clone, Debug)]
pub struct LifecycleAttentionToken(LifecycleAttentionAttempt);

impl PartialEq for LifecycleAttentionToken {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0.0, &other.0.0)
    }
}

impl Eq for LifecycleAttentionToken {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleAttentionKind {
    ReviewReady,
    OperatorAttention,
    PlanComplete,
    ContinuationFailed,
}

#[derive(Clone, Debug)]
pub struct LifecycleAttentionRecord {
    token: LifecycleAttentionToken,
    kind: LifecycleAttentionKind,
    report_count: u64,
}

impl LifecycleAttentionRecord {
    pub fn token(&self) -> &LifecycleAttentionToken {
        &self.token
    }

    pub fn home_id(&self) -> BerylHomeId {
        self.token.0.0.home_id
    }

    pub fn thread_id(&self) -> SyndicThreadId {
        self.token.0.0.thread_id
    }

    pub fn turn_id(&self) -> SyndicTurnId {
        self.token.0.0.turn_id
    }

    pub fn outcome(&self) -> LifecycleYieldOutcome {
        self.token.0.0.outcome
    }

    pub fn kind(&self) -> LifecycleAttentionKind {
        self.kind
    }

    pub fn report_count(&self) -> u64 {
        self.report_count
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LifecycleAttentionDiagnostics {
    pub omitted: u64,
    pub repeated: u64,
    pub rejected: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleAttentionRejection {
    Closed,
    ForeignAttempt,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LifecycleAttentionAdmission {
    Admitted(LifecycleAttentionToken),
    Updated(LifecycleAttentionToken),
    AlreadyReported,
    Omitted,
    NotRequested,
    Rejected(LifecycleAttentionRejection),
}

struct State {
    records: Vec<LifecycleAttentionRecord>,
    diagnostics: LifecycleAttentionDiagnostics,
}

pub struct ProcessLifecycleAttentionPool {
    owner: Arc<Owner>,
    state: Mutex<State>,
}

impl ProcessLifecycleAttentionPool {
    pub fn new() -> Self {
        Self {
            owner: Arc::new(Owner {
                closed: AtomicBool::new(false),
            }),
            state: Mutex::new(State {
                records: Vec::with_capacity(NOTICE_RECORD_CAPACITY),
                diagnostics: LifecycleAttentionDiagnostics::default(),
            }),
        }
    }

    pub fn track_accepted_yield(
        &self,
        home_id: BerylHomeId,
        thread_id: SyndicThreadId,
        turn_id: SyndicTurnId,
        outcome: LifecycleYieldOutcome,
    ) -> Option<LifecycleAttentionAttempt> {
        let _state = self.lock();
        if self.owner.closed.load(Ordering::Acquire) {
            return None;
        }
        Some(LifecycleAttentionAttempt(Arc::new(Attempt {
            owner: self.owner.clone(),
            offered: AtomicBool::new(false),
            home_id,
            thread_id,
            turn_id,
            outcome,
        })))
    }

    pub fn report_terminal(
        &self,
        attempt: &LifecycleAttentionAttempt,
    ) -> LifecycleAttentionAdmission {
        let kind = match attempt.0.outcome {
            LifecycleYieldOutcome::PhaseNeedsReview => Some(LifecycleAttentionKind::ReviewReady),
            LifecycleYieldOutcome::BlockedNeedsOperator => {
                Some(LifecycleAttentionKind::OperatorAttention)
            }
            LifecycleYieldOutcome::PlanComplete => Some(LifecycleAttentionKind::PlanComplete),
            LifecycleYieldOutcome::PhaseContinue => None,
        };
        self.report(attempt, kind)
    }

    pub fn report_continuation_failure(
        &self,
        attempt: &LifecycleAttentionAttempt,
    ) -> LifecycleAttentionAdmission {
        self.report(
            attempt,
            (attempt.0.outcome == LifecycleYieldOutcome::PhaseContinue)
                .then_some(LifecycleAttentionKind::ContinuationFailed),
        )
    }

    pub fn snapshot(&self) -> Vec<LifecycleAttentionRecord> {
        self.lock().records.clone()
    }

    pub fn diagnostics(&self) -> LifecycleAttentionDiagnostics {
        self.lock().diagnostics
    }

    pub fn acknowledge(&self, token: &LifecycleAttentionToken) -> bool {
        let mut state = self.lock();
        if !Arc::ptr_eq(&self.owner, &token.0.0.owner) || self.owner.closed.load(Ordering::Acquire)
        {
            return false;
        }
        let Some(index) = state
            .records
            .iter()
            .position(|record| &record.token == token)
        else {
            return false;
        };
        state.records.remove(index);
        true
    }

    pub fn close(&self) {
        let mut state = self.lock();
        self.owner.closed.store(true, Ordering::Release);
        state.records.clear();
    }

    fn report(
        &self,
        attempt: &LifecycleAttentionAttempt,
        kind: Option<LifecycleAttentionKind>,
    ) -> LifecycleAttentionAdmission {
        let mut state = self.lock();
        let rejection = if !Arc::ptr_eq(&self.owner, &attempt.0.owner) {
            Some(LifecycleAttentionRejection::ForeignAttempt)
        } else if self.owner.closed.load(Ordering::Acquire) {
            Some(LifecycleAttentionRejection::Closed)
        } else {
            None
        };
        if let Some(reason) = rejection {
            state.diagnostics.rejected = state.diagnostics.rejected.saturating_add(1);
            return LifecycleAttentionAdmission::Rejected(reason);
        }
        let Some(kind) = kind else {
            return LifecycleAttentionAdmission::NotRequested;
        };
        if attempt.0.offered.swap(true, Ordering::AcqRel) {
            state.diagnostics.repeated = state.diagnostics.repeated.saturating_add(1);
            if let Some(record) = state
                .records
                .iter_mut()
                .find(|record| Arc::ptr_eq(&record.token.0.0, &attempt.0))
            {
                record.report_count = record.report_count.saturating_add(1);
                return LifecycleAttentionAdmission::Updated(record.token.clone());
            }
            return LifecycleAttentionAdmission::AlreadyReported;
        }
        if state.records.len() == NOTICE_RECORD_CAPACITY {
            state.diagnostics.omitted = state.diagnostics.omitted.saturating_add(1);
            return LifecycleAttentionAdmission::Omitted;
        }
        let token = LifecycleAttentionToken(attempt.clone());
        state.records.push(LifecycleAttentionRecord {
            token: token.clone(),
            kind,
            report_count: 1,
        });
        LifecycleAttentionAdmission::Admitted(token)
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

impl Default for ProcessLifecycleAttentionPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ProcessLifecycleAttentionPool {
    fn drop(&mut self) {
        self.close();
    }
}
