use super::*;

impl ProcessServiceOwner {
    pub(crate) fn retained_close(&self) -> Option<&beryl_home_store::HomeCloseError> {
        self.failed_close.as_ref()
    }

    pub(super) fn dispose_initial_failure(
        &mut self,
        failure: preparation::PreparedAppServiceFailure,
    ) -> AppServiceOpenFailure {
        assert!(
            self.graph.is_none() && self.failed_close.is_none(),
            "only the admitted initial attempt can dispose its preparation"
        );
        let preparation::PreparedAppServiceFailure { error, prepared } = failure;
        self.retain_initial_close(prepared.dispose());
        AppServiceOpenFailure {
            error,
            rejected_candidate: None,
        }
    }

    pub(super) fn retain_initial_close(
        &mut self,
        result: Result<(), beryl_home_store::HomeCloseError>,
    ) {
        if let Err(error) = result {
            assert!(
                self.failed_close.is_none(),
                "initial disposal cannot replace retained close custody"
            );
            self.failed_close = Some(error);
        }
    }

    #[cfg(feature = "test-faults")]
    pub(crate) fn test_before_initial_publication(
        &mut self,
        hook: impl FnOnce(&mut HomeOpenPublication, &BerylState, &SyndicStorage) + Send + 'static,
    ) {
        self.before_initial_publication = Some(Box::new(hook));
    }

    #[cfg(feature = "test-faults")]
    pub(crate) fn test_cancel_initial_worker_release(&mut self) {
        self.cancel_initial_worker_release = true;
    }
}

impl PublishedAppServices {
    pub(super) fn dispose_unstarted(mut self) -> Result<(), beryl_home_store::HomeCloseError> {
        self.join_components();
        self.home
            .take()
            .expect("unstarted published graph retains its home")
            .close()
    }
}
