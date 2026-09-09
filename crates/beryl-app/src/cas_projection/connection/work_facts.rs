use super::ProjectionConnection;
use crate::cas_projection::connection_work::{
    ConnectionWorkError, ConnectionWorkPageBuilder, ConnectionWorkStamp,
};

impl ProjectionConnection {
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
