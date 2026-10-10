use beryl_home_store::{
    CurrentDomainCommand, DomainMutation, DomainReader, FrozenHomeRead, HomeStore, MutationBuilder,
    ReconciliationReservation,
};

use super::{
    Build, BuildKey, Command, Lifecycle, ParsePhase, Pass,
    access::IndexedResourceError,
    advance::{AdvanceCode, PreparedAdvance},
    codec::BuildsFamily,
    construction::{AdmitBuild, ConstructionError},
    frozen::FrozenRecords,
    query::QueryRecords,
    table_advance::AdvanceTable,
    table_publication::PreparedTableAdvance,
    transition::{ControlTransition, PreparedTransition},
};
use crate::{SyndicStorage, domain::SyndicDomain};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildStatus {
    Building,
    Completed,
    Cancelled,
    Superseded,
    Refused,
}

pub struct BuildState {
    build: Build,
}

impl BuildState {
    pub fn key(&self) -> BuildKey {
        self.build.key
    }

    pub fn revision(&self) -> u64 {
        self.build.revision
    }

    pub fn status(&self) -> BuildStatus {
        match self.build.lifecycle {
            Lifecycle::Building => BuildStatus::Building,
            Lifecycle::Completed => BuildStatus::Completed,
            Lifecycle::Cancelled => BuildStatus::Cancelled,
            Lifecycle::Superseded => BuildStatus::Superseded,
            Lifecycle::Refused => BuildStatus::Refused,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopBuild {
    Cancel,
    Supersede,
    Refuse,
}

enum StepMutation {
    Code(AdvanceCode),
    Table(AdvanceTable),
    Control(ControlTransition),
}

enum PreparedStep {
    Code(PreparedAdvance),
    Table(PreparedTableAdvance),
    Control(PreparedTransition),
}

impl DomainMutation<SyndicDomain> for StepMutation {
    type Error = ConstructionError;
    type Prepared = PreparedStep;

    fn prepare(
        self,
        reader: &DomainReader<'_, SyndicDomain>,
    ) -> Result<Self::Prepared, Self::Error> {
        match self {
            Self::Code(command) => command.prepare(reader).map(PreparedStep::Code),
            Self::Table(command) => command.prepare(reader).map(PreparedStep::Table),
            Self::Control(command) => command.prepare(reader).map(PreparedStep::Control),
        }
    }

    fn reserve_reconciliation(
        &self,
        reservation: &mut ReconciliationReservation<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match self {
            Self::Code(command) => command.reserve_reconciliation(reservation),
            Self::Table(command) => command.reserve_reconciliation(reservation),
            Self::Control(command) => command.reserve_reconciliation(reservation),
        }
    }

    fn contribute(
        prepared: Self::Prepared,
        mutations: &mut MutationBuilder<'_, SyndicDomain>,
    ) -> Result<(), Self::Error> {
        match prepared {
            PreparedStep::Code(prepared) => AdvanceCode::contribute(prepared, mutations),
            PreparedStep::Table(prepared) => AdvanceTable::contribute(prepared, mutations),
            PreparedStep::Control(prepared) => ControlTransition::contribute(prepared, mutations),
        }
    }
}

impl SyndicStorage {
    pub fn read_frozen_indexed_resource_build(
        &self,
        store: &HomeStore,
        frozen: &FrozenHomeRead,
        key: BuildKey,
    ) -> Result<Option<BuildState>, IndexedResourceError> {
        let records = FrozenRecords {
            store,
            handle: &self.handle,
            read: frozen,
        };
        let build = records.point::<BuildsFamily>(&key, 24_576)?;
        if build.as_ref().is_some_and(|build| build.key != key) {
            return Err(IndexedResourceError::Corrupt);
        }
        Ok(build.map(|build| BuildState { build }))
    }

    pub fn current_start_indexed_resource_build(&self, key: BuildKey) -> CurrentDomainCommand {
        self.handle.current_command(AdmitBuild { key })
    }

    pub fn current_advance_indexed_resource_build(
        &self,
        state: BuildState,
        source_bytes: usize,
        descriptors: usize,
    ) -> Result<CurrentDomainCommand, IndexedResourceError> {
        if source_bytes > 65_536 || !(1..=256).contains(&descriptors) {
            return Err(IndexedResourceError::Limit);
        }
        let before = state.build;
        if before.lifecycle != Lifecycle::Building {
            return Err(IndexedResourceError::Collision);
        }
        let mutation = if before.checkpoint.pass == Pass::Sealed {
            StepMutation::Control(ControlTransition {
                before,
                command: Command::Complete,
            })
        } else if before.checkpoint.phase == ParsePhase::Eof {
            StepMutation::Control(ControlTransition {
                before,
                command: Command::PassChange,
            })
        } else if before.checkpoint.pass == Pass::CodeLines {
            StepMutation::Code(AdvanceCode {
                before,
                source_bytes,
                descriptors,
            })
        } else {
            StepMutation::Table(AdvanceTable {
                before,
                source_bytes,
                descriptors,
            })
        };
        Ok(self.handle.current_command(mutation))
    }

    pub fn current_stop_indexed_resource_build(
        &self,
        state: BuildState,
        stop: StopBuild,
    ) -> Result<CurrentDomainCommand, IndexedResourceError> {
        if state.build.lifecycle != Lifecycle::Building {
            return Err(IndexedResourceError::Collision);
        }
        let command = match stop {
            StopBuild::Cancel => Command::Cancel,
            StopBuild::Supersede => Command::Supersede,
            StopBuild::Refuse => Command::Refuse,
        };
        Ok(self.handle.current_command(ControlTransition {
            before: state.build,
            command,
        }))
    }
}
