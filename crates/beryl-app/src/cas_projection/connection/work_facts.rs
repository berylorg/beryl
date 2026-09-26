use super::{
    ProjectionConnection, authority::ConnectionAuthorityWorkFact, registry::ConnectionGeneration,
};
use crate::cas_projection::ProjectionCoordinatorError;
use crate::cas_projection::connection_work::{
    ConnectionWorkError, ConnectionWorkPageBuilder, ConnectionWorkStamp,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cas_projection) struct ConnectionCustodyWorkFact {
    authority: ConnectionAuthorityWorkFact,
    detached: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::cas_projection) struct ConnectionCustodyWorkStamp {
    session_owners: u64,
    promotion_owners: u64,
    cleanup_owners: u64,
    promotion_serials: u64,
    cleanup_serials: u64,
    retired: u64,
    retirement_complete: u64,
    detached: u64,
    unfinished_cleanup: u64,
}

impl ConnectionCustodyWorkStamp {
    pub(in crate::cas_projection) fn requires_cleanup(self) -> bool {
        self.unfinished_cleanup != 0
    }

    pub(in crate::cas_projection) fn add(
        &mut self,
        fact: ConnectionCustodyWorkFact,
    ) -> Result<(), ProjectionCoordinatorError> {
        let unavailable = || ProjectionCoordinatorError::RegistryWorkRevisionUnavailable {
            registry: crate::cas_projection::ProjectionRegistryKind::ProjectionConnection,
        };
        let add = |target: &mut u64, value: u64| -> Result<(), ProjectionCoordinatorError> {
            *target = target.checked_add(value).ok_or_else(unavailable)?;
            Ok(())
        };
        add(
            &mut self.session_owners,
            u64::from(fact.authority.session_owner_live),
        )?;
        add(
            &mut self.promotion_owners,
            u64::from(fact.authority.promotion.is_some()),
        )?;
        add(
            &mut self.cleanup_owners,
            u64::try_from(fact.authority.cleanup_owners).map_err(|_| unavailable())?,
        )?;
        add(
            &mut self.promotion_serials,
            fact.authority.next_promotion_id,
        )?;
        add(&mut self.cleanup_serials, fact.authority.next_cleanup_id)?;
        add(&mut self.retired, u64::from(fact.authority.retired))?;
        add(
            &mut self.retirement_complete,
            u64::from(fact.authority.retirement_complete),
        )?;
        add(&mut self.detached, u64::from(fact.detached))?;
        add(
            &mut self.unfinished_cleanup,
            u64::from(fact.requires_cleanup()),
        )
    }
}

impl ConnectionCustodyWorkFact {
    pub(in crate::cas_projection) const fn generation(self) -> ConnectionGeneration {
        self.authority.generation
    }

    pub(in crate::cas_projection) fn requires_cleanup(self) -> bool {
        self.authority.requires_cleanup(self.detached)
    }
}

impl ProjectionConnection {
    pub(in crate::cas_projection) fn try_custody_work_fact(
        &self,
    ) -> Result<ConnectionCustodyWorkFact, crate::cas_projection::runtime_work::RuntimeWorkError>
    {
        Ok(ConnectionCustodyWorkFact {
            authority: self.authority.try_work_fact()?,
            detached: self.try_with_work_attachment(|attachment| Ok(attachment.is_none()))?,
        })
    }

    pub(in crate::cas_projection) fn try_work_stamp(
        &self,
    ) -> Result<ConnectionWorkStamp, crate::cas_projection::runtime_work::RuntimeWorkError> {
        let mut stamp = self.try_with_work_attachment(|attachment| match attachment {
            Some(attachment) => attachment.router.try_work_stamp(),
            None => Ok(ConnectionWorkStamp {
                detached: 1,
                ..ConnectionWorkStamp::default()
            }),
        })?;
        stamp.retired = u64::from(self.is_retired());
        Ok(stamp)
    }

    pub(in crate::cas_projection) fn custody_work_fact(
        &self,
    ) -> Result<ConnectionCustodyWorkFact, ProjectionCoordinatorError> {
        Ok(ConnectionCustodyWorkFact {
            authority: self.authority.work_fact()?,
            detached: self.work_attachment()?.is_none(),
        })
    }

    pub(in crate::cas_projection) fn work_stamp(
        &self,
    ) -> Result<ConnectionWorkStamp, ConnectionWorkError> {
        let attachment = self
            .work_attachment()
            .map_err(|_| ConnectionWorkError::SourceUnavailable)?;
        let mut stamp = match attachment {
            Some(attachment) => attachment.router.work_stamp()?,
            None => ConnectionWorkStamp {
                detached: 1,
                ..ConnectionWorkStamp::default()
            },
        };
        stamp.retired = u64::from(self.is_retired());
        Ok(stamp)
    }

    pub(in crate::cas_projection) fn collect_work_records(
        &self,
        page: &mut ConnectionWorkPageBuilder<'_>,
    ) -> Result<bool, ConnectionWorkError> {
        match self
            .work_attachment()
            .map_err(|_| ConnectionWorkError::SourceUnavailable)?
        {
            Some(attachment) => attachment
                .router
                .collect_work_records(self.is_retired(), page),
            None => Ok(true),
        }
    }
}
