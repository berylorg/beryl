use beryl_home_store::{CursorRange, CursorReadLimits, HomeStore};
use beryl_model::{CasThreadId, CasTurnId, ProviderObservationId};

use crate::{
    ProviderObservationBuildLifecycle, ProviderObservationBuildRecord,
    ProviderObservationChunkPayload, SyndicPointReadLimit, codec::*, domain::SyndicStorage,
};

use super::{
    CanonicalObservationState, ProviderObservationDigest, ProviderObservationStageBatchError,
    ProviderObservationStager, ProviderObservationValidatorState, replay_chunk,
};

/// Exact trailing CAS route for one provider observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderObservationRoute {
    thread_id: CasThreadId,
    turn_id: CasTurnId,
}

impl ProviderObservationRoute {
    #[must_use]
    pub const fn new(thread_id: CasThreadId, turn_id: CasTurnId) -> Self {
        Self { thread_id, turn_id }
    }

    #[must_use]
    pub const fn thread_id(&self) -> &CasThreadId {
        &self.thread_id
    }

    #[must_use]
    pub const fn turn_id(&self) -> &CasTurnId {
        &self.turn_id
    }
}

/// Compact immutable authority for one structurally sealed unpublished observation.
pub struct SealedProviderObservationHandle {
    build: ProviderObservationBuildRecord,
}

impl SealedProviderObservationHandle {
    pub(crate) fn from_build(build: &ProviderObservationBuildRecord) -> Self {
        debug_assert_eq!(build.lifecycle(), ProviderObservationBuildLifecycle::Sealed);
        Self {
            build: build.clone(),
        }
    }

    #[must_use]
    pub const fn identity(&self) -> ProviderObservationId {
        self.build.identity()
    }

    #[must_use]
    pub const fn begin(&self) -> super::ProviderObservationBegin {
        self.build.begin()
    }

    #[must_use]
    pub const fn digest(&self) -> ProviderObservationDigest {
        self.build.digest()
    }

    #[must_use]
    pub const fn canonical_bytes(&self) -> u64 {
        self.build.canonical_bytes()
    }

    /// Returns the sealed complete-history support evidence.
    #[must_use]
    pub const fn history_support(&self) -> crate::ProviderFrameHistorySupportV1 {
        self.build.history_support()
    }

    /// Compares canonical typed content independently of caller fragment segmentation.
    #[must_use]
    pub fn canonical_eq(&self, other: &Self) -> bool {
        self.begin() == other.begin()
            && self.digest() == other.digest()
            && self.canonical_bytes() == other.canonical_bytes()
    }

    /// Consumes this handle and binds it only when the trailing route is exact.
    pub fn bind(
        self,
        admitted: ProviderObservationRoute,
        trailing: ProviderObservationRoute,
    ) -> Result<BoundProviderObservation, ProviderObservationRouteMismatch> {
        if admitted != trailing {
            return Err(ProviderObservationRouteMismatch { admitted, trailing });
        }
        Ok(BoundProviderObservation {
            build: self.build,
            route: admitted,
        })
    }

    /// Explicitly abandons this unpublished handle without publishing or deleting anything.
    pub fn abandon(self) {}
}

/// Typed exact route mismatch. The sealed handle is consumed and abandoned.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("provider observation trailing route does not match its admitted lane")]
pub struct ProviderObservationRouteMismatch {
    admitted: ProviderObservationRoute,
    trailing: ProviderObservationRoute,
}

impl ProviderObservationRouteMismatch {
    #[must_use]
    pub const fn admitted(&self) -> &ProviderObservationRoute {
        &self.admitted
    }

    #[must_use]
    pub const fn trailing(&self) -> &ProviderObservationRoute {
        &self.trailing
    }
}

/// Non-cloneable exact-route authority for opening one bounded durable cursor.
pub struct BoundProviderObservation {
    build: ProviderObservationBuildRecord,
    route: ProviderObservationRoute,
}

impl BoundProviderObservation {
    #[must_use]
    pub const fn identity(&self) -> ProviderObservationId {
        self.build.identity()
    }

    #[must_use]
    pub const fn route(&self) -> &ProviderObservationRoute {
        &self.route
    }

    /// Explicitly abandons this route-bound unpublished handle without durable effects.
    pub fn abandon(self) {}

    pub(super) fn into_replay(self) -> ProviderObservationReplay {
        ProviderObservationReplay {
            build: self.build,
            route: self.route,
        }
    }
}

/// Private replay authority retained by the observation-frame compiler.
pub(super) struct ProviderObservationReplay {
    build: ProviderObservationBuildRecord,
    route: ProviderObservationRoute,
}

impl ProviderObservationReplay {
    pub(super) const fn build(&self) -> &ProviderObservationBuildRecord {
        &self.build
    }

    pub(super) const fn route(&self) -> &ProviderObservationRoute {
        &self.route
    }

    pub(super) fn open(
        &self,
        storage: &SyndicStorage,
        store: &HomeStore,
        limit: SyndicPointReadLimit,
    ) -> Result<ProviderObservationCursor, ProviderObservationCursorError> {
        let current = storage
            .provider_observation_build(store, self.build.identity(), limit)?
            .ok_or(ProviderObservationCursorError::BuildMissing)?;
        if current != self.build || current.lifecycle() != ProviderObservationBuildLifecycle::Sealed
        {
            return Err(ProviderObservationCursorError::BuildChanged);
        }
        Ok(ProviderObservationCursor {
            build: self.build.clone(),
            route: self.route.clone(),
            next_ordinal: 1,
            terminal: false,
        })
    }
}

/// One bounded typed observation page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderObservationPage {
    ordinal: u64,
    payload: ProviderObservationChunkPayload,
    stored_bytes: usize,
    decoded_bytes: usize,
}

impl ProviderObservationPage {
    #[must_use]
    pub const fn ordinal(&self) -> u64 {
        self.ordinal
    }

    #[must_use]
    pub const fn payload(&self) -> &ProviderObservationChunkPayload {
        &self.payload
    }

    #[must_use]
    pub const fn stored_bytes(&self) -> usize {
        self.stored_bytes
    }

    #[must_use]
    pub const fn decoded_bytes(&self) -> usize {
        self.decoded_bytes
    }

    #[must_use]
    pub fn into_payload(self) -> ProviderObservationChunkPayload {
        self.payload
    }
}

/// Non-cloneable sequential authority over one exact sealed observation.
pub struct ProviderObservationCursor {
    build: ProviderObservationBuildRecord,
    route: ProviderObservationRoute,
    next_ordinal: u64,
    terminal: bool,
}

impl ProviderObservationCursor {
    #[must_use]
    pub const fn identity(&self) -> ProviderObservationId {
        self.build.identity()
    }

    #[must_use]
    pub const fn route(&self) -> &ProviderObservationRoute {
        &self.route
    }

    /// Explicitly stops reading without changing unpublished durable state.
    pub fn abandon(self) {}
}

impl SyndicStorage {
    /// Point-reads one unpublished build for exact callback reconciliation.
    pub fn provider_observation_build(
        &self,
        store: &HomeStore,
        identity: ProviderObservationId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ProviderObservationBuildRecord>, crate::SyndicReadError> {
        self.point::<ProviderObservationBuildsFamily>(store, identity, limit)
    }

    /// Replays a building observation with fixed resident state and resumes its next frontier.
    pub fn resume_provider_observation(
        &self,
        store: &HomeStore,
        identity: ProviderObservationId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ProviderObservationStager>, ProviderObservationCursorError> {
        let Some(stored) = self.provider_observation_build(store, identity, limit)? else {
            return Ok(None);
        };
        let build = stored.clone();
        if build.lifecycle() != ProviderObservationBuildLifecycle::Building {
            return Err(ProviderObservationCursorError::BuildNotBuilding);
        }
        let (validator, canonical) = self.replay_observation(store, &build, limit)?;
        ProviderObservationStager::from_replayed(build, validator, canonical)
            .map(Some)
            .map_err(Into::into)
    }

    /// Reopens compact authority for one exact immutable sealed observation.
    pub fn reopen_provider_observation(
        &self,
        store: &HomeStore,
        identity: ProviderObservationId,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<SealedProviderObservationHandle>, ProviderObservationCursorError> {
        let Some(stored) = self.provider_observation_build(store, identity, limit)? else {
            return Ok(None);
        };
        if stored.lifecycle() != ProviderObservationBuildLifecycle::Sealed {
            return Err(ProviderObservationCursorError::BuildNotSealed);
        }
        Ok(Some(SealedProviderObservationHandle::from_build(&stored)))
    }

    /// Consumes route-bound authority after reconfirming the exact durable sealed build.
    pub fn open_provider_observation_cursor(
        &self,
        store: &HomeStore,
        bound: BoundProviderObservation,
        limit: SyndicPointReadLimit,
    ) -> Result<ProviderObservationCursor, ProviderObservationCursorError> {
        let current = self
            .provider_observation_build(store, bound.build.identity(), limit)?
            .ok_or(ProviderObservationCursorError::BuildMissing)?;
        if current != bound.build
            || current.lifecycle() != ProviderObservationBuildLifecycle::Sealed
        {
            return Err(ProviderObservationCursorError::BuildChanged);
        }
        Ok(ProviderObservationCursor {
            build: bound.build,
            route: bound.route,
            next_ordinal: 1,
            terminal: false,
        })
    }

    /// Returns one typed bounded page, then one exact EOF. Later reads reject.
    pub fn read_provider_observation_cursor_page(
        &self,
        store: &HomeStore,
        cursor: &mut ProviderObservationCursor,
        limit: SyndicPointReadLimit,
    ) -> Result<Option<ProviderObservationPage>, ProviderObservationCursorError> {
        if cursor.terminal {
            return Err(ProviderObservationCursorError::CursorTerminal);
        }
        let current = self
            .provider_observation_build(store, cursor.build.identity(), limit)?
            .ok_or(ProviderObservationCursorError::BuildMissing)?;
        if current != cursor.build {
            return Err(ProviderObservationCursorError::BuildChanged);
        }
        if cursor.next_ordinal > cursor.build.chunk_count() {
            cursor.terminal = true;
            return Ok(None);
        }
        let ordinal = cursor.next_ordinal;
        let key = ProviderObservationChunkKey::new(cursor.build.identity(), ordinal);
        let page = self.page::<ProviderObservationChunksFamily>(
            store,
            CursorRange::closed(key.clone(), key),
            CursorReadLimits::new(1, limit.max_bytes()).expect("observation page bound is nonzero"),
        )?;
        let stored_bytes = page.stored_bytes();
        let decoded_bytes = page.decoded_bytes();
        let stored = page
            .into_records()
            .into_iter()
            .next()
            .ok_or(ProviderObservationCursorError::ChunkMissing { ordinal })?;
        if stored.identity() != cursor.build.identity() || stored.ordinal() != ordinal {
            return Err(ProviderObservationCursorError::ChunkMismatch { ordinal });
        }
        cursor.next_ordinal = ordinal
            .checked_add(1)
            .ok_or(ProviderObservationCursorError::FrontierOverflow)?;
        Ok(Some(ProviderObservationPage {
            ordinal,
            payload: stored.payload().clone(),
            stored_bytes,
            decoded_bytes,
        }))
    }

    fn replay_observation(
        &self,
        store: &HomeStore,
        build: &ProviderObservationBuildRecord,
        limit: SyndicPointReadLimit,
    ) -> Result<
        (ProviderObservationValidatorState, CanonicalObservationState),
        ProviderObservationCursorError,
    > {
        let mut validator = ProviderObservationValidatorState::initial();
        let mut canonical = CanonicalObservationState::initial(build.begin());
        for ordinal in 1..=build.chunk_count() {
            let stored = self
                .point::<ProviderObservationChunksFamily>(
                    store,
                    ProviderObservationChunkKey::new(build.identity(), ordinal),
                    limit,
                )?
                .ok_or(ProviderObservationCursorError::ChunkMissing { ordinal })?;
            if stored.identity() != build.identity() || stored.ordinal() != ordinal {
                return Err(ProviderObservationCursorError::ChunkMismatch { ordinal });
            }
            replay_chunk(build.begin(), &mut validator, &mut canonical, &stored)?;
        }
        if build.validator() != &validator
            || build.canonical_bytes() != canonical.canonical_bytes()
            || build.digest() != canonical.digest()
        {
            return Err(ProviderObservationCursorError::ReplayMismatch);
        }
        if build.lifecycle() == ProviderObservationBuildLifecycle::Sealed {
            validator
                .finish(build.begin())
                .map_err(|_| ProviderObservationCursorError::ReplayMismatch)?;
        }
        Ok((validator, canonical))
    }
}

/// Typed bounded observation read or replay failure.
#[derive(Debug, thiserror::Error)]
pub enum ProviderObservationCursorError {
    #[error(transparent)]
    Read(#[from] crate::SyndicReadError),
    #[error(transparent)]
    Stage(#[from] ProviderObservationStageBatchError),
    #[error("provider-observation build is missing")]
    BuildMissing,
    #[error("provider-observation build is not resumable")]
    BuildNotBuilding,
    #[error("provider-observation build is not sealed")]
    BuildNotSealed,
    #[error("provider-observation build changed while cursor authority was held")]
    BuildChanged,
    #[error("provider-observation chunk {ordinal} is missing")]
    ChunkMissing { ordinal: u64 },
    #[error("provider-observation chunk {ordinal} disagrees with its key")]
    ChunkMismatch { ordinal: u64 },
    #[error("provider-observation durable replay disagrees with its compact frontier")]
    ReplayMismatch,
    #[error("provider-observation cursor already returned EOF")]
    CursorTerminal,
    #[error("provider-observation cursor frontier overflowed")]
    FrontierOverflow,
}
