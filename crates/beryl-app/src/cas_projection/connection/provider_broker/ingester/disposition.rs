use super::*;

impl ProviderBrokerStopped {
    pub(in crate::cas_projection::connection) const fn receipt(
        &self,
    ) -> ProviderBrokerTerminalReceipt {
        self.receipt
    }

    pub(in crate::cas_projection::connection) fn into_worker(
        self,
    ) -> Option<ProjectionWorkerPermit> {
        self.worker
    }

    pub(in crate::cas_projection::connection) fn retains_worker(&self) -> bool {
        self.worker.is_some()
    }

    pub(in crate::cas_projection::connection) fn validate_receipt(
        &self,
        service_generation: crate::cas_projection::ProjectionServiceGeneration,
        home_generation: HomeGeneration,
    ) -> Result<(), ProviderBrokerIngesterJoinError> {
        self.receipt
            .validate_exact(service_generation, home_generation)
    }

    pub(in crate::cas_projection::connection) fn into_adoption(
        mut self,
        cut: PersistentFailureCutIdentity,
    ) -> Result<ProviderBrokerAdoptionStopped, Self> {
        if self.worker_disposition != Some(ProviderBrokerWorkerDisposition::RetainForAdoption(cut))
            || !self
                .receipt
                .is_exact(cut.service_generation, cut.home_generation)
        {
            return Err(self);
        }
        let Some(worker) = self.worker.take() else {
            return Err(self);
        };
        Ok(ProviderBrokerAdoptionStopped {
            worker,
            receipt: self.receipt,
            cut,
        })
    }
}

impl ProviderBrokerAdoptionStopped {
    pub(in crate::cas_projection::connection) const fn receipt(
        &self,
    ) -> ProviderBrokerTerminalReceipt {
        self.receipt
    }

    pub(in crate::cas_projection::connection) const fn cut_identity(
        &self,
    ) -> PersistentFailureCutIdentity {
        self.cut
    }

    pub(in crate::cas_projection::connection) fn into_worker(self) -> ProjectionWorkerPermit {
        self.worker
    }
}

impl ProviderBrokerTerminalReceipt {
    pub(in crate::cas_projection::connection) fn validate_exact(
        self,
        service_generation: crate::cas_projection::ProjectionServiceGeneration,
        home_generation: HomeGeneration,
    ) -> Result<(), ProviderBrokerIngesterJoinError> {
        if !self.clean {
            return Err(ProviderBrokerIngesterJoinError::WorkerFailed);
        }
        if self.service_generation != service_generation || self.home_generation != home_generation
        {
            return Err(ProviderBrokerIngesterJoinError::EpochMismatch);
        }
        Ok(())
    }

    pub(in crate::cas_projection::connection) fn is_exact(
        self,
        service_generation: crate::cas_projection::ProjectionServiceGeneration,
        home_generation: HomeGeneration,
    ) -> bool {
        self.validate_exact(service_generation, home_generation)
            .is_ok()
    }
}
