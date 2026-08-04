mod advance;
mod lifecycle;
mod materialize;
pub(crate) mod parser;
pub(crate) mod range;
mod start;

use beryl_home_store::{CurrentDomainCommand, MutationContribution};
use beryl_model::{DomainRevision, ProjectionRevision, SyndicItemId};

use crate::{ItemProjectionGeneration, SyndicStorage};

pub(super) use lifecycle::invalidate_item_projection;
pub(in crate::mutation) use lifecycle::validate_projection_source;
pub(crate) use materialize::materialize_output;

/// Starts one caller-named bounded projection generation for an exact canonical item revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StartItemProjectionBuild {
    item_id: SyndicItemId,
    expected_item_revision: ProjectionRevision,
    generation: ItemProjectionGeneration,
}

impl StartItemProjectionBuild {
    #[must_use]
    pub const fn new(
        item_id: SyndicItemId,
        expected_item_revision: ProjectionRevision,
        generation: ItemProjectionGeneration,
    ) -> Self {
        Self {
            item_id,
            expected_item_revision,
            generation,
        }
    }
}

/// Advances one exact incomplete item-projection generation by one bounded parser step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdvanceItemProjectionBuild {
    item_id: SyndicItemId,
    generation: ItemProjectionGeneration,
    expected_build_revision: ProjectionRevision,
}

impl AdvanceItemProjectionBuild {
    #[must_use]
    pub const fn new(
        item_id: SyndicItemId,
        generation: ItemProjectionGeneration,
        expected_build_revision: ProjectionRevision,
    ) -> Self {
        Self {
            item_id,
            generation,
            expected_build_revision,
        }
    }
}

impl SyndicStorage {
    #[must_use]
    pub fn current_start_item_projection_build(
        &self,
        request: StartItemProjectionBuild,
    ) -> CurrentDomainCommand {
        self.handle.current_command(StartBuildMutation { request })
    }

    #[must_use]
    pub fn start_item_projection_build(
        &self,
        expected_domain_revision: DomainRevision,
        request: StartItemProjectionBuild,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, StartBuildMutation { request })
    }

    #[must_use]
    pub fn advance_item_projection_build(
        &self,
        expected_domain_revision: DomainRevision,
        request: AdvanceItemProjectionBuild,
    ) -> MutationContribution {
        self.handle
            .contribution(expected_domain_revision, AdvanceBuildMutation { request })
    }

    #[must_use]
    pub fn current_advance_item_projection_build(
        &self,
        request: AdvanceItemProjectionBuild,
    ) -> CurrentDomainCommand {
        self.handle
            .current_command(AdvanceBuildMutation { request })
    }
}

struct StartBuildMutation {
    request: StartItemProjectionBuild,
}

struct AdvanceBuildMutation {
    request: AdvanceItemProjectionBuild,
}
