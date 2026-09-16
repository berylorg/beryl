use crate::{codec::*, domain::SyndicDomain, *};
use beryl_home_store::{CurrentDomainCommand, DomainMutation, DomainReader, MutationBuilder};
use beryl_model::SyndicThreadId;

pub enum RepairTargetReplacementForTest {
    Thread(ThreadRecord),
    Turn(TurnRecord),
    State(TurnStateRecord),
    CasThread(CasThreadIndexRecord),
    CasTurn(CasTurnIndexRecord),
}

pub fn inject_repair_target_replacement_for_test(
    store: &beryl_home_store::HomeStore,
    storage: &SyndicStorage,
    thread: SyndicThreadId,
    target: &RepairRequiredTarget,
    replacement: RepairTargetReplacementForTest,
) -> Result<(), String> {
    let command = storage.handle.current_command(ReplaceRepairFact {
        thread,
        target: target.clone(),
        replacement,
    });
    match store.execute_current(command) {
        beryl_home_store::CommandOutcome::Committed {
            later_failure: None,
            ..
        } => Ok(()),
        beryl_home_store::CommandOutcome::Committed {
            later_failure: Some(error),
            ..
        } => Err(format!("{error:?}")),
        beryl_home_store::CommandOutcome::NotCommitted { evidence } => Err(format!("{evidence:?}")),
        beryl_home_store::CommandOutcome::Indeterminate {
            failure,
            reconciliation,
        } => {
            reconciliation.install();
            Err(format!("{failure:?}"))
        }
    }
}

struct ReplaceRepairFact {
    thread: SyndicThreadId,
    target: RepairRequiredTarget,
    replacement: RepairTargetReplacementForTest,
}
impl DomainMutation<SyndicDomain> for ReplaceRepairFact {
    type Error = SyndicMutationError;
    type Prepared = Self;
    fn prepare(self, _: &DomainReader<'_, SyndicDomain>) -> Result<Self, Self::Error> {
        Ok(self)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match self.replacement {
            RepairTargetReplacementForTest::Thread(_) => {
                reservation.reserve_records::<ThreadsCodec>(1)?
            }
            RepairTargetReplacementForTest::Turn(_) => {
                reservation.reserve_records::<TurnsCodec>(1)?
            }
            RepairTargetReplacementForTest::State(_) => {
                reservation.reserve_records::<TurnStatesCodec>(1)?
            }
            RepairTargetReplacementForTest::CasThread(_) => {
                reservation.reserve_records::<CasThreadIndexCodec>(1)?
            }
            RepairTargetReplacementForTest::CasTurn(_) => {
                reservation.reserve_records::<CasTurnIndexCodec>(1)?
            }
        }
        Ok(())
    }
    fn contribute(
        request: Self,
        builder: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match request.replacement {
            RepairTargetReplacementForTest::Thread(value) => {
                builder.put::<ThreadsCodec>(&request.thread, &value)?
            }
            RepairTargetReplacementForTest::Turn(value) => {
                builder.put::<TurnsCodec>(&request.target.turn_id(), &value)?
            }
            RepairTargetReplacementForTest::State(value) => {
                builder.put::<TurnStatesCodec>(&request.target.turn_id(), &value)?
            }
            RepairTargetReplacementForTest::CasThread(value) => builder
                .put::<CasThreadIndexCodec>(
                    &CasThreadKey::Record(request.target.source().thread_id().clone()),
                    &value,
                )?,
            RepairTargetReplacementForTest::CasTurn(value) => builder.put::<CasTurnIndexCodec>(
                &CasTurnKey::Record(
                    request.target.source().thread_id().clone(),
                    request.target.source().turn_id().clone(),
                ),
                &value,
            )?,
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
pub enum RepairTargetFactForTest {
    Thread,
    Turn,
    State,
    CasThread,
    CasTurn,
    Terminal,
    Issue,
    Observation,
}

impl SyndicStorage {
    pub fn current_probe_repair_target_for_test(
        &self,
        thread: SyndicThreadId,
        target: RepairRequiredTarget,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(RepairTargetProbe { thread, target })
    }

    pub fn current_remove_repair_fact_for_test(
        &self,
        thread: SyndicThreadId,
        target: RepairRequiredTarget,
        fact: RepairTargetFactForTest,
    ) -> CurrentDomainCommand {
        self.handle.current_command(RemoveRepairFact {
            thread,
            target,
            fact,
        })
    }
}

struct RepairTargetProbe {
    thread: SyndicThreadId,
    target: RepairRequiredTarget,
}
impl DomainMutation<SyndicDomain> for RepairTargetProbe {
    type Error = SyndicMutationError;
    type Prepared = ThreadRecord;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        if !crate::record::repair::retained_repair_target_matches(
            reader,
            self.thread,
            &self.target,
        )? {
            return Err(SyndicMutationError::InputGateStateConflict);
        }
        crate::mutation::required::<ThreadsFamily>(reader, &self.thread)
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        reservation.reserve_records::<ThreadsCodec>(1)?;
        Ok(())
    }
    fn contribute(
        prepared: Self::Prepared,
        builder: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        builder.put::<ThreadsCodec>(&prepared.id(), &prepared)?;
        Ok(())
    }
}

struct RemoveRepairFact {
    thread: SyndicThreadId,
    target: RepairRequiredTarget,
    fact: RepairTargetFactForTest,
}
impl DomainMutation<SyndicDomain> for RemoveRepairFact {
    type Error = SyndicMutationError;
    type Prepared = (Self, Option<beryl_model::ProviderObservationId>);
    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        let observation = if matches!(self.fact, RepairTargetFactForTest::Observation) {
            let event = crate::mutation::required::<SourceEventsFamily>(
                reader,
                &TurnEventKey {
                    owner: self.target.turn_id(),
                    ordinal: self.target.gap().issue().unwrap().sequence(),
                },
            )?;
            let SourceEventPayload::ProviderObservationIssue(issue) = event.payload() else {
                unreachable!()
            };
            Some(issue.observation().identity())
        } else {
            None
        };
        Ok((self, observation))
    }
    fn reserve_reconciliation(
        &self,
        reservation: &mut beryl_home_store::ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match self.fact {
            RepairTargetFactForTest::Thread => reservation.reserve_records::<ThreadsCodec>(1)?,
            RepairTargetFactForTest::Turn => reservation.reserve_records::<TurnsCodec>(1)?,
            RepairTargetFactForTest::State => reservation.reserve_records::<TurnStatesCodec>(1)?,
            RepairTargetFactForTest::CasThread => {
                reservation.reserve_records::<CasThreadIndexCodec>(1)?
            }
            RepairTargetFactForTest::CasTurn => {
                reservation.reserve_records::<CasTurnIndexCodec>(1)?
            }
            RepairTargetFactForTest::Terminal | RepairTargetFactForTest::Issue => {
                reservation.reserve_records::<SourceEventsCodec>(1)?
            }
            RepairTargetFactForTest::Observation => {
                reservation.reserve_records::<ProviderObservationBuildsCodec>(1)?
            }
        }
        Ok(())
    }
    fn contribute(
        (request, observation): Self::Prepared,
        builder: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        let target = request.target;
        match request.fact {
            RepairTargetFactForTest::Thread => builder.delete::<ThreadsCodec>(&request.thread)?,
            RepairTargetFactForTest::Turn => builder.delete::<TurnsCodec>(&target.turn_id())?,
            RepairTargetFactForTest::State => {
                builder.delete::<TurnStatesCodec>(&target.turn_id())?
            }
            RepairTargetFactForTest::CasThread => builder.delete::<CasThreadIndexCodec>(
                &CasThreadKey::Record(target.source().thread_id().clone()),
            )?,
            RepairTargetFactForTest::CasTurn => {
                builder.delete::<CasTurnIndexCodec>(&CasTurnKey::Record(
                    target.source().thread_id().clone(),
                    target.source().turn_id().clone(),
                ))?
            }
            RepairTargetFactForTest::Terminal | RepairTargetFactForTest::Issue => {
                builder.delete::<SourceEventsCodec>(&TurnEventKey {
                    owner: target.turn_id(),
                    ordinal: if matches!(request.fact, RepairTargetFactForTest::Terminal) {
                        target.gap().terminal().sequence()
                    } else {
                        target.gap().issue().unwrap().sequence()
                    },
                })?
            }
            RepairTargetFactForTest::Observation => {
                builder.delete::<ProviderObservationBuildsCodec>(&observation.unwrap())?
            }
        }
        Ok(())
    }
}
