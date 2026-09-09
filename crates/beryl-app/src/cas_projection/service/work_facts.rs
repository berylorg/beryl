use std::sync::Arc;

use super::ProjectionConnectionService;
use crate::cas_projection::{
    connection_work::{
        ConnectionWorkCursor, ConnectionWorkError, ConnectionWorkPage, ConnectionWorkPageBuilder,
        ConnectionWorkPageLimits, ConnectionWorkRevision, ConnectionWorkStamp,
    },
    service_registry::ConnectionRegistryGuard,
};

impl ProjectionConnectionService {
    fn check_work_open(&self) -> Result<(), ConnectionWorkError> {
        if self.settled || !self.command_authorizer.is_open() {
            Err(ConnectionWorkError::Closed)
        } else {
            Ok(())
        }
    }

    fn read_work_stamp(
        &self,
        connections: &ConnectionRegistryGuard<'_>,
    ) -> Result<ConnectionWorkStamp, ConnectionWorkError> {
        self.check_work_open()?;
        let mut stamp = ConnectionWorkStamp {
            membership: connections
                .revision()
                .ok_or(ConnectionWorkError::RevisionUnavailable)?,
            ..ConnectionWorkStamp::default()
        };
        for connection in connections.iter() {
            stamp.add(connection.work_stamp()?)?;
        }
        self.check_work_open()?;
        Ok(stamp)
    }

    fn check_work_revision_owner(
        &self,
        revision: &ConnectionWorkRevision,
    ) -> Result<(), ConnectionWorkError> {
        if !Arc::ptr_eq(&revision.owner, self.connections.work_owner())
            || revision.home_id != self.home_id
            || revision.home_generation != self.home_generation
            || revision.service_generation != self.service_generation
        {
            return Err(ConnectionWorkError::ForeignRevision);
        }
        Ok(())
    }

    pub fn connection_work_revision(&self) -> Result<ConnectionWorkRevision, ConnectionWorkError> {
        let connections = self
            .connections
            .lock()
            .map_err(|_| ConnectionWorkError::Poisoned)?;
        let before = self.read_work_stamp(&connections)?;
        if self.read_work_stamp(&connections)? != before {
            return Err(ConnectionWorkError::StaleRevision);
        }
        Ok(ConnectionWorkRevision {
            owner: Arc::clone(self.connections.work_owner()),
            home_id: self.home_id,
            home_generation: self.home_generation,
            service_generation: self.service_generation,
            stamp: before,
        })
    }

    pub fn validate_connection_work_revision(
        &self,
        revision: &ConnectionWorkRevision,
    ) -> Result<(), ConnectionWorkError> {
        self.check_work_revision_owner(revision)?;
        let connections = self
            .connections
            .lock()
            .map_err(|_| ConnectionWorkError::Poisoned)?;
        if self.read_work_stamp(&connections)? != revision.stamp {
            return Err(ConnectionWorkError::StaleRevision);
        }
        if self.read_work_stamp(&connections)? != revision.stamp {
            return Err(ConnectionWorkError::StaleRevision);
        }
        Ok(())
    }

    pub fn connection_work_page(
        &self,
        revision: &ConnectionWorkRevision,
        cursor: Option<&ConnectionWorkCursor>,
        limits: ConnectionWorkPageLimits,
    ) -> Result<ConnectionWorkPage, ConnectionWorkError> {
        self.check_work_revision_owner(revision)?;
        if cursor.is_some_and(|cursor| &cursor.revision != revision) {
            return Err(ConnectionWorkError::ForeignRevision);
        }
        let connections = self
            .connections
            .lock()
            .map_err(|_| ConnectionWorkError::Poisoned)?;
        if self.read_work_stamp(&connections)? != revision.stamp {
            return Err(ConnectionWorkError::StaleRevision);
        }
        let mut page = ConnectionWorkPageBuilder::new(cursor.map(|cursor| &cursor.after), limits);
        let mut after_connection = 0;
        while let Some(connection) = connections
            .iter()
            .filter(|connection| {
                connection.identity_observation().connection_generation() > after_connection
            })
            .min_by_key(|connection| connection.identity_observation().connection_generation())
        {
            after_connection = connection.identity_observation().connection_generation();
            if page
                .after
                .is_some_and(|after| after_connection < after.connection)
            {
                continue;
            }
            if !connection.collect_work_records(&mut page)? {
                break;
            }
        }
        if self.read_work_stamp(&connections)? != revision.stamp {
            return Err(ConnectionWorkError::StaleRevision);
        }
        Ok(page.finish(revision.clone()))
    }
}
