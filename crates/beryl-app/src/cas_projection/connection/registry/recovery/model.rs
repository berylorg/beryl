use beryl_model::{CasLoadedSessionGeneration, SyndicThreadId};

use super::super::{
    ConnectionGeneration, LeaseToken, LoadedThreadKey, ReacquisitionAnchorToken,
    ReacquisitionReservationToken,
};

/// Kind of one opaque loaded-registry token observed for recovery auditing.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) enum LoadedRegistryRecoveryTokenKind {
    ActiveLease,
    QuarantinedAnchor,
    ReacquisitionReservation,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum RecoveryToken {
    ActiveLease(LeaseToken),
    QuarantinedAnchor(ReacquisitionAnchorToken),
    ReacquisitionReservation(ReacquisitionReservationToken),
}

/// Comparable token identity that cannot authorize a loaded-registry operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) struct LoadedRegistryRecoveryToken(pub(super) RecoveryToken);

impl LoadedRegistryRecoveryToken {
    pub(in crate::cas_projection::connection) const fn active(token: LeaseToken) -> Self {
        Self(RecoveryToken::ActiveLease(token))
    }

    pub(in crate::cas_projection::connection) const fn quarantined(
        token: ReacquisitionAnchorToken,
    ) -> Self {
        Self(RecoveryToken::QuarantinedAnchor(token))
    }

    pub(in crate::cas_projection::connection) const fn reservation(
        token: ReacquisitionReservationToken,
    ) -> Self {
        Self(RecoveryToken::ReacquisitionReservation(token))
    }

    pub(in crate::cas_projection) const fn kind(self) -> LoadedRegistryRecoveryTokenKind {
        match self.0 {
            RecoveryToken::ActiveLease(_) => LoadedRegistryRecoveryTokenKind::ActiveLease,
            RecoveryToken::QuarantinedAnchor(_) => {
                LoadedRegistryRecoveryTokenKind::QuarantinedAnchor
            }
            RecoveryToken::ReacquisitionReservation(_) => {
                LoadedRegistryRecoveryTokenKind::ReacquisitionReservation
            }
        }
    }

    pub(super) fn sort_key(self) -> (u8, u64) {
        match self.0 {
            RecoveryToken::ActiveLease(LeaseToken(token)) => (0, token),
            RecoveryToken::QuarantinedAnchor(ReacquisitionAnchorToken(token)) => (1, token),
            RecoveryToken::ReacquisitionReservation(ReacquisitionReservationToken(token)) => {
                (2, token)
            }
        }
    }

    pub(super) fn active_raw(self) -> Option<LeaseToken> {
        match self.0 {
            RecoveryToken::ActiveLease(token) => Some(token),
            RecoveryToken::QuarantinedAnchor(_) | RecoveryToken::ReacquisitionReservation(_) => {
                None
            }
        }
    }

    pub(super) fn quarantined_raw(self) -> Option<ReacquisitionAnchorToken> {
        match self.0 {
            RecoveryToken::QuarantinedAnchor(token) => Some(token),
            RecoveryToken::ActiveLease(_) | RecoveryToken::ReacquisitionReservation(_) => None,
        }
    }

    pub(super) fn reservation_raw(self) -> Option<ReacquisitionReservationToken> {
        match self.0 {
            RecoveryToken::ReacquisitionReservation(token) => Some(token),
            RecoveryToken::ActiveLease(_) | RecoveryToken::QuarantinedAnchor(_) => None,
        }
    }
}

/// Exact non-authorizing registry authority observed at one instant.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) enum LoadedRegistryRecoveryAuthority {
    ActiveLease {
        token: LoadedRegistryRecoveryToken,
    },
    QuarantinedAnchor {
        token: LoadedRegistryRecoveryToken,
    },
    ReacquisitionReservation {
        anchor_connection: ConnectionGeneration,
        anchor_token: LoadedRegistryRecoveryToken,
        token: LoadedRegistryRecoveryToken,
    },
}

/// Discriminant for one observed loaded-registry authority.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) enum LoadedRegistryRecoveryAuthorityKind {
    ActiveLease,
    QuarantinedAnchor,
    ReacquisitionReservation,
}

impl LoadedRegistryRecoveryAuthority {
    pub(in crate::cas_projection) const fn kind(self) -> LoadedRegistryRecoveryAuthorityKind {
        match self {
            Self::ActiveLease { .. } => LoadedRegistryRecoveryAuthorityKind::ActiveLease,
            Self::QuarantinedAnchor { .. } => {
                LoadedRegistryRecoveryAuthorityKind::QuarantinedAnchor
            }
            Self::ReacquisitionReservation { .. } => {
                LoadedRegistryRecoveryAuthorityKind::ReacquisitionReservation
            }
        }
    }

    pub(in crate::cas_projection) const fn token(self) -> LoadedRegistryRecoveryToken {
        match self {
            Self::ActiveLease { token }
            | Self::QuarantinedAnchor { token }
            | Self::ReacquisitionReservation { token, .. } => token,
        }
    }

    pub(in crate::cas_projection) const fn anchor_token(
        self,
    ) -> Option<LoadedRegistryRecoveryToken> {
        match self {
            Self::QuarantinedAnchor { token } => Some(token),
            Self::ReacquisitionReservation { anchor_token, .. } => Some(anchor_token),
            Self::ActiveLease { .. } => None,
        }
    }

    pub(in crate::cas_projection) const fn anchor_connection(self) -> Option<ConnectionGeneration> {
        match self {
            Self::ReacquisitionReservation {
                anchor_connection, ..
            } => Some(anchor_connection),
            Self::ActiveLease { .. } | Self::QuarantinedAnchor { .. } => None,
        }
    }
}

/// One exact loaded-thread authority observation captured under the registry lock.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::cas_projection) struct LoadedRegistryRecoveryObservation {
    pub(super) key: LoadedThreadKey,
    pub(super) connection: ConnectionGeneration,
    pub(super) owner: SyndicThreadId,
    pub(super) generation: CasLoadedSessionGeneration,
    pub(super) authority: LoadedRegistryRecoveryAuthority,
}

impl LoadedRegistryRecoveryObservation {
    pub(in crate::cas_projection::connection) fn active(
        key: LoadedThreadKey,
        connection: ConnectionGeneration,
        owner: SyndicThreadId,
        generation: CasLoadedSessionGeneration,
        token: LeaseToken,
    ) -> Self {
        Self {
            key,
            connection,
            owner,
            generation,
            authority: LoadedRegistryRecoveryAuthority::ActiveLease {
                token: LoadedRegistryRecoveryToken::active(token),
            },
        }
    }

    pub(in crate::cas_projection::connection) fn quarantined(
        key: LoadedThreadKey,
        connection: ConnectionGeneration,
        owner: SyndicThreadId,
        generation: CasLoadedSessionGeneration,
        token: ReacquisitionAnchorToken,
    ) -> Self {
        Self {
            key,
            connection,
            owner,
            generation,
            authority: LoadedRegistryRecoveryAuthority::QuarantinedAnchor {
                token: LoadedRegistryRecoveryToken::quarantined(token),
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::cas_projection::connection) fn reacquisition_reservation(
        key: LoadedThreadKey,
        anchor_connection: ConnectionGeneration,
        replacement_connection: ConnectionGeneration,
        owner: SyndicThreadId,
        anchor_generation: CasLoadedSessionGeneration,
        anchor_token: ReacquisitionAnchorToken,
        token: ReacquisitionReservationToken,
    ) -> Self {
        Self {
            key,
            connection: replacement_connection,
            owner,
            generation: anchor_generation,
            authority: LoadedRegistryRecoveryAuthority::ReacquisitionReservation {
                anchor_connection,
                anchor_token: LoadedRegistryRecoveryToken::quarantined(anchor_token),
                token: LoadedRegistryRecoveryToken::reservation(token),
            },
        }
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn replace_cas_thread_id_for_test(
        &mut self,
        cas_thread_id: beryl_model::CasThreadId,
    ) {
        self.key.cas_thread_id = cas_thread_id;
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn replace_owner_for_test(&mut self, owner: SyndicThreadId) {
        self.owner = owner;
    }

    #[cfg(test)]
    pub(in crate::cas_projection) fn replace_loaded_generation_for_test(
        &mut self,
        generation: CasLoadedSessionGeneration,
    ) {
        self.generation = generation;
    }

    pub(in crate::cas_projection) fn key(&self) -> &LoadedThreadKey {
        &self.key
    }

    pub(in crate::cas_projection) const fn connection_generation(&self) -> ConnectionGeneration {
        self.connection
    }

    pub(in crate::cas_projection) const fn owner(&self) -> SyndicThreadId {
        self.owner
    }

    pub(in crate::cas_projection) const fn loaded_generation(&self) -> CasLoadedSessionGeneration {
        self.generation
    }

    pub(in crate::cas_projection) const fn authority(&self) -> LoadedRegistryRecoveryAuthority {
        self.authority
    }

    pub(in crate::cas_projection) fn involves_connection(
        &self,
        connection: ConnectionGeneration,
    ) -> bool {
        self.connection == connection
            || matches!(
                self.authority,
                LoadedRegistryRecoveryAuthority::ReacquisitionReservation {
                    anchor_connection,
                    ..
                } if anchor_connection == connection
            )
    }

    pub(super) fn sort_key(&self) -> (u8, u64) {
        self.authority.token().sort_key()
    }
}
