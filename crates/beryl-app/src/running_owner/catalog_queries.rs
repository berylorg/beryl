use super::*;

impl RunningProcessOwner {
    pub(crate) fn catalog_query_reader(
        &self,
    ) -> Option<crate::catalog_query::PublishedCatalogQueryReader> {
        if self.exit_requested() || self.shutdown.is_some() || self.interrupted_exit.is_some() {
            return None;
        }
        self.process.services.as_ref()?.catalog_query_reader()
    }
}
