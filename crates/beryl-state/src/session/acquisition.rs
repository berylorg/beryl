use beryl_home_store::{CursorDirection, CursorRange, CursorReadLimits, HomeStore, PointReadLimit};
use beryl_model::{SyndicThreadId, WindowId};

use super::{
    CLAIM_V1_BYTES, MAX_SESSION_CLAIMS, MinimalSessionBootstrap, SESSION_HEADER_V1_BYTES,
    SESSION_WINDOW_V1_BYTES, SessionDomain, SessionHeader, SessionReadError, SessionState,
    SessionWindowRecord, ThreadClaimRecord, bootstrap,
    codec::{
        ClaimByThreadCodec, ClaimByWindowCodec, HEADER_KEY, SessionHeaderCodec, SessionWindowCodec,
    },
};

const ACQUISITION_AUDIT_MAX_BYTES: usize = 1024 * 1024;

pub(crate) struct SessionAcquisitionSource {
    pub(crate) bootstrap: Option<MinimalSessionBootstrap>,
    pub(crate) window: Option<SessionWindowRecord>,
    pub(crate) claims_by_window: Vec<(WindowId, ThreadClaimRecord)>,
    pub(crate) claims_by_thread: Vec<(SyndicThreadId, ThreadClaimRecord)>,
    pub(crate) claims_bounded: bool,
}

pub(crate) struct SessionAbandonmentSource {
    pub(crate) header: Option<SessionHeader>,
    pub(crate) window: Option<SessionWindowRecord>,
    pub(crate) claim_by_window: Option<ThreadClaimRecord>,
    pub(crate) claim_by_thread: Option<ThreadClaimRecord>,
}

pub(crate) fn read(
    state: &SessionState,
    store: &HomeStore,
    window_id: WindowId,
) -> Result<SessionAcquisitionSource, SessionReadError> {
    let bootstrap = bootstrap::read(&state.handle, store)?;
    let window = store.read_point::<SessionDomain, SessionWindowCodec>(
        &state.handle,
        &window_id,
        point_limit(SESSION_WINDOW_V1_BYTES),
    )?;
    let by_window = store.read_cursor::<SessionDomain, ClaimByWindowCodec>(
        &state.handle,
        &CursorRange::closed(
            WindowId::from_bytes([0; 16]),
            WindowId::from_bytes([u8::MAX; 16]),
        ),
        CursorDirection::Forward,
        limits(),
    )?;
    let by_thread = store.read_cursor::<SessionDomain, ClaimByThreadCodec>(
        &state.handle,
        &CursorRange::closed(
            SyndicThreadId::from_bytes([0; 16]),
            SyndicThreadId::from_bytes([u8::MAX; 16]),
        ),
        CursorDirection::Forward,
        limits(),
    )?;
    let claims_bounded = !by_window.has_more()
        && !by_thread.has_more()
        && by_window.records().len() <= MAX_SESSION_CLAIMS
        && by_thread.records().len() <= MAX_SESSION_CLAIMS;
    Ok(SessionAcquisitionSource {
        bootstrap,
        window,
        claims_by_window: by_window
            .into_records()
            .into_iter()
            .map(|record| record.into_parts())
            .collect(),
        claims_by_thread: by_thread
            .into_records()
            .into_iter()
            .map(|record| record.into_parts())
            .collect(),
        claims_bounded,
    })
}

pub(crate) fn read_abandonment(
    state: &SessionState,
    store: &HomeStore,
    window_id: WindowId,
    thread_id: SyndicThreadId,
) -> Result<SessionAbandonmentSource, SessionReadError> {
    let header = read_header(state, store)?;
    let source = SessionAbandonmentSource {
        window: store.read_point::<SessionDomain, SessionWindowCodec>(
            &state.handle,
            &window_id,
            point_limit(SESSION_WINDOW_V1_BYTES),
        )?,
        claim_by_window: store.read_point::<SessionDomain, ClaimByWindowCodec>(
            &state.handle,
            &window_id,
            point_limit(CLAIM_V1_BYTES),
        )?,
        claim_by_thread: store.read_point::<SessionDomain, ClaimByThreadCodec>(
            &state.handle,
            &thread_id,
            point_limit(CLAIM_V1_BYTES),
        )?,
        header,
    };
    if read_header(state, store)? != source.header {
        return Err(SessionReadError::ConcurrentPublication);
    }
    Ok(source)
}

fn read_header(
    state: &SessionState,
    store: &HomeStore,
) -> Result<Option<SessionHeader>, SessionReadError> {
    store
        .read_point::<SessionDomain, SessionHeaderCodec>(
            &state.handle,
            &HEADER_KEY,
            point_limit(SESSION_HEADER_V1_BYTES),
        )
        .map_err(Into::into)
}

fn point_limit(payload: usize) -> PointReadLimit {
    PointReadLimit::new(payload + 4).expect("fixed session point limit is nonzero")
}

fn limits() -> CursorReadLimits {
    CursorReadLimits::new(MAX_SESSION_CLAIMS + 1, ACQUISITION_AUDIT_MAX_BYTES)
        .expect("session acquisition audit limits are nonzero")
}

#[cfg(any(test, feature = "test-faults"))]
mod test_support {
    use beryl_home_store::{
        DomainMutation, DomainReader, MutationBuilder, MutationContribution, PointReadLimit,
        ReconciliationReservation,
    };
    use beryl_model::{DomainRevision, SyndicThreadId, WindowId};

    use crate::SessionMutationError;
    use crate::session::{
        CLAIM_V1_BYTES, ClaimByThreadCodec, ClaimByWindowCodec, SessionDomain, SessionState,
    };

    #[derive(Clone, Copy)]
    enum ClaimCopy {
        ByThread,
        ByWindow,
    }

    struct DeleteClaimCopy {
        window_id: WindowId,
        thread_id: SyndicThreadId,
        copy: ClaimCopy,
    }

    struct PreparedDeleteClaimCopy {
        window_id: WindowId,
        thread_id: SyndicThreadId,
        copy: ClaimCopy,
    }

    impl DomainMutation<SessionDomain> for DeleteClaimCopy {
        type Error = SessionMutationError;
        type Prepared = PreparedDeleteClaimCopy;

        fn prepare(
            self,
            reader: &DomainReader<'_, SessionDomain>,
        ) -> Result<Self::Prepared, Self::Error> {
            let by_window = reader.point::<ClaimByWindowCodec>(
                &self.window_id,
                PointReadLimit::new(CLAIM_V1_BYTES + 4)
                    .expect("fixed session claim limit is nonzero"),
            )?;
            let by_thread = reader.point::<ClaimByThreadCodec>(
                &self.thread_id,
                PointReadLimit::new(CLAIM_V1_BYTES + 4)
                    .expect("fixed session claim limit is nonzero"),
            )?;
            let Some(claim) = by_window else {
                return Err(SessionMutationError::ClaimMissing {
                    window_id: self.window_id,
                });
            };
            if by_thread != Some(claim)
                || claim.window_id() != self.window_id
                || claim.thread_id() != self.thread_id
            {
                return Err(SessionMutationError::ClaimCopiesDisagree {
                    window_id: self.window_id,
                });
            }
            Ok(PreparedDeleteClaimCopy {
                window_id: self.window_id,
                thread_id: self.thread_id,
                copy: self.copy,
            })
        }

        fn reserve_reconciliation(
            &self,
            reservation: &mut ReconciliationReservation<'_, SessionDomain>,
        ) -> Result<(), Self::Error> {
            match self.copy {
                ClaimCopy::ByThread => {
                    reservation.reserve_records::<ClaimByThreadCodec>(1)?;
                }
                ClaimCopy::ByWindow => {
                    reservation.reserve_records::<ClaimByWindowCodec>(1)?;
                }
            }
            Ok(())
        }

        fn contribute(
            prepared: Self::Prepared,
            mutations: &mut MutationBuilder<'_, SessionDomain>,
        ) -> Result<(), Self::Error> {
            match prepared.copy {
                ClaimCopy::ByThread => {
                    mutations.delete::<ClaimByThreadCodec>(&prepared.thread_id)?;
                }
                ClaimCopy::ByWindow => {
                    mutations.delete::<ClaimByWindowCodec>(&prepared.window_id)?;
                }
            }
            Ok(())
        }
    }

    impl SessionState {
        #[cfg(all(test, not(feature = "test-faults")))]
        pub(crate) fn delete_thread_claim_for_test(
            &self,
            expected_revision: DomainRevision,
            window_id: WindowId,
            thread_id: SyndicThreadId,
        ) -> MutationContribution {
            self.handle.contribution(
                expected_revision,
                DeleteClaimCopy {
                    window_id,
                    thread_id,
                    copy: ClaimCopy::ByThread,
                },
            )
        }

        #[cfg(feature = "test-faults")]
        #[doc(hidden)]
        #[must_use]
        pub fn delete_thread_claim_copy_for_test(
            &self,
            expected_revision: DomainRevision,
            window_id: WindowId,
            thread_id: SyndicThreadId,
        ) -> MutationContribution {
            self.handle.contribution(
                expected_revision,
                DeleteClaimCopy {
                    window_id,
                    thread_id,
                    copy: ClaimCopy::ByThread,
                },
            )
        }

        #[cfg(feature = "test-faults")]
        #[doc(hidden)]
        #[must_use]
        pub fn delete_window_claim_copy_for_test(
            &self,
            expected_revision: DomainRevision,
            window_id: WindowId,
            thread_id: SyndicThreadId,
        ) -> MutationContribution {
            self.handle.contribution(
                expected_revision,
                DeleteClaimCopy {
                    window_id,
                    thread_id,
                    copy: ClaimCopy::ByWindow,
                },
            )
        }
    }
}
