use super::*;

impl ProcessServiceOwner {
    pub(crate) fn catalog_query_reader(
        &self,
    ) -> Option<crate::catalog_query::PublishedCatalogQueryReader> {
        Some(self.graph()?.catalog_query_reader())
    }

    pub(super) fn begin_catalog_query_close(&mut self, service: Option<CatalogQueryService>) {
        if let Some(service) = service {
            assert!(self.closing_catalog_query.is_none());
            self.closing_catalog_query = Some(service);
        }
        if let Some(service) = &mut self.closing_catalog_query {
            if let Err(error) = service.stop_and_join() {
                if self.closing_catalog_query_error.is_none() {
                    self.closing_catalog_query_error = Some(error);
                }
            }
            assert!(service.work_drained());
        }
    }

    pub(super) fn finish_catalog_query_close(&mut self) -> Result<(), AppServiceCloseError> {
        self.begin_catalog_query_close(None);
        if self
            .closing_catalog_query
            .as_ref()
            .is_some_and(|service| !service.reads_drained())
        {
            return Err(AppServiceCloseError::CatalogQueryPending);
        }
        drop(self.closing_catalog_query.take());
        Ok(())
    }
}
