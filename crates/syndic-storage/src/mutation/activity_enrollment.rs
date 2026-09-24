use super::{point, required};
use crate::{codec::*, domain::SyndicDomain, read::access::ReadAccess, *};
use beryl_home_store::{
    CursorDirection, CursorRange, CursorReadLimits, DomainHandle, DomainMutation, DomainReader,
    HomeCandidateRecoveryAccess, HomeCommand, HomeStore, MutationBuilder,
    ReconciliationReservation,
};
use beryl_model::{BerylHomeId, DomainRevision, ExecutionBinding, HomeRevision, RuntimeId};
use std::sync::Arc;

mod outcome;
mod prepare;

pub const ACTIVITY_ENROLLMENT_CLEANUP_ROWS: usize = 32;
const PAGE_BYTES: usize = 65_536;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityPeriodToken {
    home: BerylHomeId,
    runtime: RuntimeId,
    period: ActivityWorkPeriod,
}

impl ActivityPeriodToken {
    pub fn work_period(&self) -> ActivityWorkPeriod {
        self.period
    }
    pub fn runtime_id(&self) -> RuntimeId {
        self.runtime
    }
}

pub struct ActivityEnrollmentRequest {
    source: ActivityQuerySource,
    execution: ExecutionBinding,
    head_revision: ActivityQueryRevision,
    token: Option<ActivityPeriodToken>,
}

impl ActivityEnrollmentRequest {
    pub fn first(
        source: ActivityQuerySource,
        execution: ExecutionBinding,
        head_revision: ActivityQueryRevision,
    ) -> Self {
        Self {
            source,
            execution,
            head_revision,
            token: None,
        }
    }
    pub fn reuse(
        source: ActivityQuerySource,
        execution: ExecutionBinding,
        head_revision: ActivityQueryRevision,
        token: &ActivityPeriodToken,
    ) -> Self {
        Self {
            source,
            execution,
            head_revision,
            token: Some(token.clone()),
        }
    }
}

pub enum ActivityEnrollmentPreparation {
    AlreadyEnrolled(ActivityPeriodToken),
    Prepared(PreparedActivityEnrollment),
}

pub struct PreparedActivityEnrollment {
    handle: DomainHandle<SyndicDomain>,
    domain_revision: DomainRevision,
    expected_home: HomeRevision,
    records: Arc<EnrollmentRecords>,
}

pub struct ActivityEnrollmentWitness(Arc<EnrollmentRecords>);

pub enum ActivityEnrollmentStatus {
    NotCommitted,
    Committed { token: Option<ActivityPeriodToken> },
    Conflict,
}

struct EnrollmentRecords {
    home: BerylHomeId,
    token: ActivityPeriodToken,
    old_head: ActivityQueryHeadRecord,
    new_head: ActivityQueryHeadRecord,
    source: Option<ActivityQuerySourceRecord>,
    deleted: Vec<(ActivityQueryEntryKey, ActivityQueryEntryRecord)>,
    thread: ThreadRecord,
    execution: ThreadExecutionRecord,
    turn: TurnRecord,
    state: TurnStateRecord,
    previous_state: Option<TurnStateRecord>,
    previous_member: Option<ActivityQuerySourceRecord>,
}

impl PreparedActivityEnrollment {
    pub fn home_id(&self) -> BerylHomeId {
        self.records.home
    }

    pub fn runtime_id(&self) -> RuntimeId {
        self.records.token.runtime
    }

    pub fn is_cleanup(&self) -> bool {
        self.records.source.is_none()
    }
    pub fn into_command(self) -> (HomeCommand, ActivityEnrollmentWitness) {
        let mut command = HomeCommand::new(self.expected_home);
        command
            .add(self.handle.contribution(
                self.domain_revision,
                EnrollmentMutation(Arc::clone(&self.records)),
            ))
            .expect("one enrollment participant");
        (command, ActivityEnrollmentWitness(self.records))
    }
}

struct EnrollmentMutation(Arc<EnrollmentRecords>);

impl DomainMutation<SyndicDomain> for EnrollmentMutation {
    type Error = SyndicMutationError;
    type Prepared = Arc<EnrollmentRecords>;
    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let r = &self.0;
        if required::<ActivityQueryHeadsFamily>(reader, &r.thread.id())? != r.old_head
            || required::<ThreadsFamily>(reader, &r.thread.id())? != r.thread
            || required::<ThreadExecutionsFamily>(reader, &r.thread.id())? != r.execution
            || required::<TurnsFamily>(reader, &r.turn.id())? != r.turn
            || required::<TurnStatesFamily>(reader, &r.state.turn_id())? != r.state
        {
            return Err(SyndicMutationError::ActivityQueryConflict);
        }
        if let Some(state) = &r.previous_state {
            if required::<TurnStatesFamily>(reader, &state.turn_id())? != *state {
                return Err(SyndicMutationError::ActivityQueryConflict);
            }
        }
        if let Some(member) = &r.previous_member {
            if required::<ActivityQuerySourcesFamily>(reader, &source_key(member))? != *member {
                return Err(SyndicMutationError::ActivityQueryConflict);
            }
        }
        if let Some(source) = &r.source {
            if point::<ActivityQuerySourcesFamily>(reader, &source_key(source))?.is_some() {
                return Err(SyndicMutationError::IdentityCollision);
            }
            if r.new_head.work_period() == r.old_head.work_period() {
                let page = reader.cursor::<ActivityQueryEntriesCodec>(
                    &running_range(&r.old_head),
                    CursorDirection::Forward,
                    page_limits(1),
                )?;
                if page
                    .records()
                    .iter()
                    .any(|record| record.key().order.running())
                {
                    return Err(SyndicMutationError::ActivityQueryConflict);
                }
            } else {
                let entries = reader.cursor::<ActivityQueryEntriesCodec>(
                    &period_entries(r.thread.id(), r.new_head.work_period()),
                    CursorDirection::Forward,
                    page_limits(1),
                )?;
                let sources = reader.cursor::<ActivityQuerySourcesCodec>(
                    &period_sources(r.thread.id(), r.new_head.work_period()),
                    CursorDirection::Forward,
                    page_limits(1),
                )?;
                if !entries.records().is_empty() || !sources.records().is_empty() {
                    return Err(SyndicMutationError::IdentityCollision);
                }
            }
        }
        for (key, value) in &r.deleted {
            if required::<ActivityQueryEntriesFamily>(reader, key)? != *value {
                return Err(SyndicMutationError::ActivityQueryConflict);
            }
        }
        Ok(self.0)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ActivityQueryHeadsCodec>(1)?;
        if self.0.source.is_some() {
            reservation.reserve_records::<ActivityQuerySourcesCodec>(1)?;
        }
        if !self.0.deleted.is_empty() {
            reservation.reserve_records::<ActivityQueryEntriesCodec>(self.0.deleted.len())?;
        }
        Ok(())
    }
    fn contribute(
        r: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        mutations.put::<ActivityQueryHeadsCodec>(&r.thread.id(), &r.new_head)?;
        if let Some(source) = &r.source {
            mutations.put::<ActivityQuerySourcesCodec>(&source_key(source), source)?;
        }
        for (key, _) in &r.deleted {
            mutations.delete::<ActivityQueryEntriesCodec>(key)?;
        }
        Ok(())
    }
}

fn source_key(source: &ActivityQuerySourceRecord) -> ActivityQuerySourceKey {
    ActivityQuerySourceKey {
        thread: source.thread_id(),
        work_period: source.work_period(),
        source_thread: source.source().thread_id(),
        source_turn: source.source().turn_id(),
    }
}
fn running_range(head: &ActivityQueryHeadRecord) -> CursorRange<ActivityQueryEntryKey> {
    CursorRange::closed(
        ActivityQueryEntryKey::first_for_period(head.thread_id(), head.work_period()),
        ActivityQueryEntryKey::first_completed_for_period(head.thread_id(), head.work_period()),
    )
}
fn period_entries(
    thread: beryl_model::SyndicThreadId,
    period: ActivityWorkPeriod,
) -> CursorRange<ActivityQueryEntryKey> {
    CursorRange::closed(
        ActivityQueryEntryKey::first_for_period(thread, period),
        ActivityQueryEntryKey::last_for_period(thread, period),
    )
}
fn period_sources(
    thread: beryl_model::SyndicThreadId,
    period: ActivityWorkPeriod,
) -> CursorRange<ActivityQuerySourceKey> {
    CursorRange::closed(
        ActivityQuerySourceKey::first_for_period(thread, period),
        ActivityQuerySourceKey::last_for_period(thread, period),
    )
}
fn page_limits(rows: usize) -> CursorReadLimits {
    CursorReadLimits::new(rows, PAGE_BYTES).expect("fixed enrollment bounds")
}
fn limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(PAGE_BYTES).expect("fixed record bound")
}
fn invalid() -> SyndicReadError {
    SyndicReadError::Invariant("activity enrollment source or period disagrees")
}
