use super::*;

struct LocalRegistryDisposition {
    key: LoadedThreadKey,
    connection: super::super::registry::ConnectionGeneration,
    owner: SyndicThreadId,
    generation: CasLoadedSessionGeneration,
    token: LeaseToken,
}

/// Terminal owner of one exact local registry authority.
#[must_use = "the disposition owner must survive until its registry authority is settled"]
pub(in crate::cas_projection) struct LocalLoadedRegistryDispositionOwner {
    disposition: Option<LocalRegistryDisposition>,
}

/// Terminal owner selected directly from a loaded projection lease.
#[must_use = "the disposition owner retains local registry authority"]
pub(in crate::cas_projection) struct TerminalLoadedLeaseDispositionOwner {
    inner: LocalLoadedRegistryDispositionOwner,
}

impl LocalRegistryDisposition {
    fn settle(self) {
        let _ = registry::release_exact(
            &self.key,
            self.connection,
            self.owner,
            self.generation,
            self.token,
        );
    }
}

impl TerminalLoadedLeaseDispositionOwner {
    pub(in crate::cas_projection) fn dispose_local(self) {
        drop(self);
    }
}

impl Drop for LocalLoadedRegistryDispositionOwner {
    fn drop(&mut self) {
        if let Some(disposition) = self.disposition.take() {
            disposition.settle();
        }
    }
}

impl LoadedProjectionLease {
    fn terminal_registry_disposition(&self) -> LocalRegistryDisposition {
        LocalRegistryDisposition {
            key: self.key.clone(),
            connection: self.connection.authority.generation,
            owner: self.owner,
            generation: self.generation,
            token: self.token,
        }
    }

    pub(in crate::cas_projection) fn into_terminal_loaded_lease_disposition_owner(
        mut self,
    ) -> TerminalLoadedLeaseDispositionOwner {
        let disposition = self.terminal_registry_disposition();
        self.active = false;
        TerminalLoadedLeaseDispositionOwner {
            inner: LocalLoadedRegistryDispositionOwner {
                disposition: Some(disposition),
            },
        }
    }

    pub(in crate::cas_projection) fn into_local_registry_disposition_owner(
        mut self,
    ) -> LocalLoadedRegistryDispositionOwner {
        let disposition = self.terminal_registry_disposition();
        self.active = false;
        LocalLoadedRegistryDispositionOwner {
            disposition: Some(disposition),
        }
    }
}
