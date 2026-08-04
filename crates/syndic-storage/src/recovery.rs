use beryl_home_store::HomeStore;
use beryl_model::DomainRevision;

use crate::{
    CasRepresentedPrefixProof, ConversationParent, ProjectionTextSource, RecoveryItemCount,
    RecoveryProjectionError, RecoveryProjectionVersion, RecoveryUtf8ByteCount,
    SyndicPointReadLimit, SyndicStorage, TurnKind, TurnLifecycle,
};

const POINT_READ_MAX_BYTES: usize = 16_384;
const INDEX_PAGE_MAX_ITEMS: usize = 256;
const INDEX_PAGE_MAX_BYTES: usize = 65_536;

mod cursor;
mod text;
mod traversal;
mod types;

pub use cursor::{RecoveryCursor, RecoveryCursorPage, RecoveryItemSequenceRole};
pub use text::RECOVERY_CURSOR_PAGE_MAX_UTF8_BYTES;
use traversal::{RecoveryTailEligibility, RecoveryTopology};
pub use types::{
    RecoveryAssembly, RecoveryProjection, RecoveryProjectionRequest, RecoveryProjectionScope,
};

#[derive(Clone, Copy)]
struct RecoveryItemDescriptor {
    role: RecoveryItemSequenceRole,
    source: ProjectionTextSource,
}

impl SyndicStorage {
    /// Preflights one exact recovery prefix without mutating Syndic state.
    ///
    /// The ready result contains only compact proof. Call [`Self::open_recovery_cursor`] to replay
    /// it through bounded UTF-8 pages under the same exact domain revision.
    pub fn prepare_recovery_projection(
        &self,
        store: &HomeStore,
        request: RecoveryProjectionRequest,
    ) -> Result<RecoveryAssembly, RecoveryProjectionError> {
        let source_revision = self.revision(store)?;
        let thread = self
            .thread(store, request.thread_id(), point_limit())?
            .ok_or(RecoveryProjectionError::StaleSelectedPath)?;
        let thread = thread;
        if thread.id() != request.thread_id()
            || thread.revision() != request.selected_path().thread_revision()
            || thread.committed_tail() != request.selected_path().tail()
            || thread.selected_path_digest() != request.selected_path().digest()
        {
            return Err(RecoveryProjectionError::StaleSelectedPath);
        }

        let Some(selected_tail_id) = request.selected_path().tail() else {
            return match request.scope() {
                RecoveryProjectionScope::CurrentSelectedPath => {
                    self.native_empty_recovery(store, request, source_revision)
                }
                RecoveryProjectionScope::PendingSelectedTurnParent => {
                    Err(RecoveryProjectionError::CurrentTailNotPendingOrdinaryUser)
                }
            };
        };
        let selected_tail = self.load_recovery_turn(store, selected_tail_id)?;
        let selected_tail_state = self
            .turn_state(store, selected_tail_id, point_limit())?
            .ok_or(RecoveryProjectionError::MissingHistory {
                record: "turn-state",
            })?;
        if selected_tail_state.turn_id() != selected_tail_id
            || selected_tail.chain_digest() != request.selected_path().digest()
        {
            return Err(RecoveryProjectionError::StaleSelectedPath);
        }

        let (prefix_tail, expected_depth, tail_eligibility) = match request.scope() {
            RecoveryProjectionScope::CurrentSelectedPath => (
                selected_tail_id,
                selected_tail.depth().get(),
                RecoveryTailEligibility::Strict,
            ),
            RecoveryProjectionScope::PendingSelectedTurnParent => {
                if selected_tail.kind() != TurnKind::OrdinaryUser
                    || selected_tail_state.lifecycle() != TurnLifecycle::Pending
                {
                    return Err(RecoveryProjectionError::CurrentTailNotPendingOrdinaryUser);
                }
                match selected_tail.parent() {
                    ConversationParent::Root => {
                        if selected_tail.depth().get() != 1 {
                            return Err(RecoveryProjectionError::Invariant(
                                "a root pending tail does not have depth one",
                            ));
                        }
                        return self.native_empty_recovery(store, request, source_revision);
                    }
                    ConversationParent::Turn(parent) => {
                        let depth = selected_tail.depth().get().checked_sub(1).ok_or(
                            RecoveryProjectionError::Invariant(
                                "a non-root pending tail has no parent depth",
                            ),
                        )?;
                        (
                            parent,
                            depth,
                            RecoveryTailEligibility::PendingParentAuthorityLost,
                        )
                    }
                }
            }
        };

        let model_tokens = request
            .model_context_window_tokens()
            .ok_or(RecoveryProjectionError::MissingModelContextWindow)?;
        if model_tokens == 0 {
            return Err(RecoveryProjectionError::ZeroModelContextWindow);
        }
        let utf8_limit = RecoveryUtf8ByteCount::MAX.min(model_tokens / 2);
        let topology =
            self.inspect_recovery_topology(store, prefix_tail, expected_depth, tail_eligibility)?;
        let utf8_bytes = self.recovery_utf8_total(store, &topology, utf8_limit)?;
        let sequence_digest = self.hash_recovery_sequence(store, &topology, utf8_bytes)?;
        let item_count = RecoveryItemCount::new(topology.item_count).map_err(|_| {
            RecoveryProjectionError::Invariant("accepted recovery item count became invalid")
        })?;
        let utf8_bytes = RecoveryUtf8ByteCount::new(utf8_bytes).map_err(|_| {
            RecoveryProjectionError::Invariant("accepted recovery byte count became invalid")
        })?;
        if self.revision(store)? != source_revision {
            return Err(RecoveryProjectionError::ConcurrentChange);
        }
        Ok(RecoveryAssembly::Ready(RecoveryProjection {
            version: RecoveryProjectionVersion::V1,
            thread_id: request.thread_id(),
            selected_path: request.selected_path(),
            represented_prefix: CasRepresentedPrefixProof::new(
                Some(prefix_tail),
                request.selected_path().thread_revision(),
                topology.digest,
            ),
            item_count,
            utf8_bytes,
            sequence_digest,
            source_revision,
        }))
    }

    fn native_empty_recovery(
        &self,
        store: &HomeStore,
        request: RecoveryProjectionRequest,
        source_revision: DomainRevision,
    ) -> Result<RecoveryAssembly, RecoveryProjectionError> {
        if self.revision(store)? != source_revision {
            return Err(RecoveryProjectionError::ConcurrentChange);
        }
        Ok(RecoveryAssembly::NativeEmptyPrefix {
            thread_id: request.thread_id(),
            selected_path: request.selected_path(),
            source_revision,
        })
    }

    fn ensure_recovery_proof_bound(
        &self,
        store: &HomeStore,
        proof: RecoveryProjection,
    ) -> Result<RecoveryTailEligibility, RecoveryProjectionError> {
        if self.revision(store)? != proof.source_revision() {
            return Err(RecoveryProjectionError::ConcurrentChange);
        }
        if proof.version() != RecoveryProjectionVersion::V1
            || proof.represented_prefix().tail().is_none()
            || proof.represented_prefix().source_thread_revision()
                != proof.selected_path().thread_revision()
        {
            return Err(RecoveryProjectionError::CursorMismatch {
                reason: "recovery proof header is structurally invalid",
            });
        }
        let thread = self
            .thread(store, proof.thread_id(), point_limit())?
            .ok_or(RecoveryProjectionError::CursorMismatch {
                reason: "recovery proof thread is missing",
            })?;
        if thread.id() != proof.thread_id()
            || thread.revision() != proof.selected_path().thread_revision()
            || thread.committed_tail() != proof.selected_path().tail()
            || thread.selected_path_digest() != proof.selected_path().digest()
        {
            return Err(RecoveryProjectionError::CursorMismatch {
                reason: "recovery proof no longer names the exact selected path",
            });
        }
        let tail_eligibility = self.recovery_tail_eligibility(store, proof)?;
        let prefix_tail = proof
            .represented_prefix()
            .tail()
            .expect("the proof header checked a nonempty represented prefix");
        let tail = self.load_recovery_turn(store, prefix_tail)?;
        if tail.chain_digest() != proof.represented_prefix().digest() {
            return Err(RecoveryProjectionError::CursorMismatch {
                reason: "recovery proof represented-prefix digest disagrees with its tail",
            });
        }
        if self.revision(store)? != proof.source_revision() {
            return Err(RecoveryProjectionError::ConcurrentChange);
        }
        Ok(tail_eligibility)
    }

    fn recovery_tail_eligibility(
        &self,
        store: &HomeStore,
        proof: RecoveryProjection,
    ) -> Result<RecoveryTailEligibility, RecoveryProjectionError> {
        let prefix_tail = proof
            .represented_prefix()
            .tail()
            .expect("the proof header checked a nonempty represented prefix");
        let selected_tail =
            proof
                .selected_path()
                .tail()
                .ok_or(RecoveryProjectionError::CursorMismatch {
                    reason: "recovery proof has no selected tail",
                })?;
        if selected_tail == prefix_tail {
            return Ok(RecoveryTailEligibility::Strict);
        }
        let pending = self.load_recovery_turn(store, selected_tail)?;
        let state = self
            .turn_state(store, selected_tail, point_limit())?
            .ok_or(RecoveryProjectionError::MissingHistory {
                record: "pending recovery turn-state",
            })?;
        if pending.kind() != TurnKind::OrdinaryUser
            || state.turn_id() != selected_tail
            || state.lifecycle() != TurnLifecycle::Pending
            || pending.parent().turn() != Some(prefix_tail)
        {
            return Err(RecoveryProjectionError::CursorMismatch {
                reason: "recovery proof no longer names its current path or exact pending successor",
            });
        }
        Ok(RecoveryTailEligibility::PendingParentAuthorityLost)
    }
}

fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(POINT_READ_MAX_BYTES).expect("recovery point-read bound is nonzero")
}
