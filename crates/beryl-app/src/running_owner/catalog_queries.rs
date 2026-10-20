use super::*;

impl RunningProcessOwner {
    pub(crate) fn activity_reader(&self) -> Option<crate::activity_service::ActivityWindowReader> {
        if self.exit_requested() || self.shutdown.is_some() || self.interrupted_exit.is_some() {
            return None;
        }
        match &self.process.services {
            Some(services) => services.activity_reader(),
            None => self.settings_flight_readers()?.activity.clone(),
        }
    }
    pub(crate) fn catalog_query_reader(
        &self,
    ) -> Option<crate::catalog_query::PublishedCatalogQueryReader> {
        if self.exit_requested() || self.shutdown.is_some() || self.interrupted_exit.is_some() {
            return None;
        }
        match &self.process.services {
            Some(services) => services.catalog_query_reader(),
            None => self.settings_flight_readers()?.catalog_query.clone(),
        }
    }
}
