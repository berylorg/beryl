use super::frame::{ContextFrameSource, describe};
use crate::{
    CasRepresentedPrefixProof, ContextEnvelopeRecord, ContextSummary, ConversationParent,
    RecoveryProjectionError, SyndicPointReadLimit, SyndicStorage, ThreadRecord, TurnRecord,
};
use beryl_home_store::HomeStore;
use beryl_model::{DiscussionContextOwnerId, SyndicThreadId};
use sha2::{Digest, Sha256};

pub(crate) fn point() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(66_048).expect("context record point bound is nonzero")
}

impl SyndicStorage {
    pub(crate) fn context_record_for_execution(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
    ) -> Result<ContextEnvelopeRecord, RecoveryProjectionError> {
        self.context_record_for_execution_counted(store, thread_id)
            .map(|(record, _)| record)
    }
    pub(crate) fn context_record_for_execution_counted(
        &self,
        store: &HomeStore,
        thread_id: SyndicThreadId,
    ) -> Result<(ContextEnvelopeRecord, crate::read::ReadByteTotals), RecoveryProjectionError> {
        use crate::codec::*;
        let mut totals = crate::read::ReadByteTotals::default();
        let thread = counted::<ThreadsFamily>(self, store, thread_id, &mut totals)?;
        let Some(owner @ DiscussionContextOwnerId::SubmittedTurn(owner_turn)) =
            thread.context_owner_id()
        else {
            return Err(RecoveryProjectionError::UnsupportedHistory {
                reason: "discussion has no submitted context owner",
            });
        };
        let owner_record = counted::<TurnsFamily>(self, store, owner_turn, &mut totals)?;
        let record = counted::<ContextEnvelopesFamily>(
            self,
            store,
            ContextOwnerKey::from(owner),
            &mut totals,
        )?;
        let source = record.envelope().descriptor().source();
        if owner_record.origin_thread_id() != thread_id
            || owner_record.parent() != ConversationParent::Turn(source.turn_id())
            || thread.parent_thread_id() != Some(source.thread_id())
            || record.owner() != owner
        {
            return Err(RecoveryProjectionError::Invariant(
                "context immutable owner/source boundary disagrees",
            ));
        }
        counted::<TurnsFamily>(self, store, source.turn_id(), &mut totals)?;
        Ok((record, totals))
    }
    pub(crate) fn context_thread(
        &self,
        store: &HomeStore,
        thread: SyndicThreadId,
    ) -> Result<ThreadRecord, RecoveryProjectionError> {
        self.thread(store, thread, point())?
            .ok_or(RecoveryProjectionError::MissingHistory {
                record: "context thread",
            })
    }
    pub(crate) fn context_turn_at_cancellable(
        &self,
        store: &HomeStore,
        tail: &TurnRecord,
        depth: u64,
        revision: beryl_model::DomainRevision,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<TurnRecord, RecoveryProjectionError> {
        let mut reads = 0usize;
        crate::selected_path::lift_to_depth(
            tail.clone(),
            crate::TurnDepth::new(depth)
                .map_err(|_| RecoveryProjectionError::Invariant("invalid context turn depth"))?,
            |id| {
                if reads % 64 == 0 {
                    if cancelled() {
                        return Err(RecoveryProjectionError::Cancelled);
                    }
                    if self.revision(store)? != revision {
                        return Err(RecoveryProjectionError::ConcurrentChange);
                    }
                    if reads > 0 {
                        std::thread::yield_now();
                    }
                }
                reads += 1;
                self.turn(store, id, point())?
                    .ok_or(RecoveryProjectionError::MissingHistory {
                        record: "context path turn",
                    })
            },
            RecoveryProjectionError::Invariant,
        )
    }
    pub(crate) fn context_before_turn(
        &self,
        store: &HomeStore,
        turn: &TurnRecord,
    ) -> Result<Option<ContextFrameSource>, RecoveryProjectionError> {
        let origin = self.context_thread(store, turn.origin_thread_id())?;
        if origin.context_owner_id().is_none() {
            return Ok(None);
        }
        if let ConversationParent::Turn(parent) = turn.parent() {
            let previous = self.turn(store, parent, point())?.ok_or(
                RecoveryProjectionError::MissingHistory {
                    record: "context crossing parent",
                },
            )?;
            if previous.origin_thread_id() == turn.origin_thread_id() {
                return Ok(None);
            }
        }
        let record = self.context_record_for_execution(store, turn.origin_thread_id())?;
        if turn.parent()
            != ConversationParent::Turn(record.envelope().descriptor().source_turn_id())
        {
            return Err(RecoveryProjectionError::Invariant(
                "selected context crossing has another boundary",
            ));
        }
        describe(store, turn.origin_thread_id(), &record).map(Some)
    }
    pub(crate) fn context_after_tail(
        &self,
        store: &HomeStore,
        target: SyndicThreadId,
        tail: &TurnRecord,
    ) -> Result<Option<ContextFrameSource>, RecoveryProjectionError> {
        let thread = self.context_thread(store, target)?;
        if thread.context_owner_id().is_none() || tail.origin_thread_id() == target {
            return Ok(None);
        }
        let record = self.context_record_for_execution(store, target)?;
        if record.envelope().descriptor().source_turn_id() != tail.id() {
            return Err(RecoveryProjectionError::Invariant(
                "unrepresented target context has another boundary",
            ));
        }
        describe(store, target, &record).map(Some)
    }
    pub(crate) fn context_summary(
        &self,
        store: &HomeStore,
        target: SyndicThreadId,
        prefix: CasRepresentedPrefixProof,
        include_target: bool,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<ContextSummary, RecoveryProjectionError> {
        self.context_summary_limited(store, target, prefix, include_target, None, cancelled)
    }
    pub(crate) fn context_summary_limited(
        &self,
        store: &HomeStore,
        target: SyndicThreadId,
        prefix: CasRepresentedPrefixProof,
        include_target: bool,
        maximum_bytes: Option<u64>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<ContextSummary, RecoveryProjectionError> {
        let before = self.revision(store)?;
        let Some(tail_id) = prefix.tail() else {
            return Ok(ContextSummary::empty());
        };
        let tail =
            self.turn(store, tail_id, point())?
                .ok_or(RecoveryProjectionError::MissingHistory {
                    record: "context prefix tail",
                })?;
        if tail.chain_digest() != prefix.digest() {
            return Err(RecoveryProjectionError::StaleSelectedPath);
        }
        let mut summary = ContextSummary::empty();
        let append = |summary: ContextSummary,
                      frame: ContextFrameSource|
         -> Result<ContextSummary, RecoveryProjectionError> {
            let next = summary
                .append(frame.entry, frame.utf8_bytes)
                .map_err(|_| RecoveryProjectionError::Invariant("context summary overflow"))?;
            if let Some(maximum) = maximum_bytes
                && next.utf8_bytes() > maximum
            {
                return Err(RecoveryProjectionError::BudgetOverflow {
                    kind: crate::RecoveryBudgetKind::Utf8Bytes,
                    maximum,
                    actual: next.utf8_bytes(),
                });
            }
            Ok(next)
        };
        for depth in 1..=tail.depth().get() {
            if cancelled() {
                return Err(RecoveryProjectionError::Cancelled);
            }
            if self.revision(store)? != before {
                return Err(RecoveryProjectionError::ConcurrentChange);
            }
            let turn = self.context_turn_at_cancellable(store, &tail, depth, before, cancelled)?;
            if let Some(frame) = self.context_before_turn(store, &turn)? {
                summary = append(summary, frame)?;
            }
            if depth % 64 == 0 {
                std::thread::yield_now();
            }
        }
        if include_target && let Some(frame) = self.context_after_tail(store, target, &tail)? {
            summary = append(summary, frame)?;
        }
        if self.revision(store)? != before {
            return Err(RecoveryProjectionError::ConcurrentChange);
        }
        Ok(summary)
    }
}

fn counted<F: crate::codec::Family>(
    storage: &SyndicStorage,
    store: &HomeStore,
    key: F::Key,
    totals: &mut crate::read::ReadByteTotals,
) -> Result<F::Value, RecoveryProjectionError> {
    let page = storage.page::<F>(
        store,
        beryl_home_store::CursorRange::closed(key.clone(), key),
        beryl_home_store::CursorReadLimits::new(1, 66_048)
            .expect("context exact-point page bounds"),
    )?;
    totals.add(
        page.stored_bytes(),
        page.decoded_bytes(),
        "context point byte totals overflow",
    )?;
    page.into_records()
        .into_iter()
        .next()
        .ok_or(RecoveryProjectionError::MissingHistory { record: F::NAME })
}

pub(crate) fn projection_digest(
    home: beryl_model::BerylHomeId,
    thread: SyndicThreadId,
    prefix: CasRepresentedPrefixProof,
    summary: ContextSummary,
    execution: &beryl_model::ExecutionBinding,
    tool_profile: beryl_model::CasConversationToolProfile,
) -> [u8; 32] {
    let mut encoded = crate::codec::parts::Encoder::new();
    crate::codec::parts::enc_execution(&mut encoded, execution);
    let execution_digest = Sha256::digest(encoded.finish());
    let mut encoded = crate::codec::parts::Encoder::new();
    crate::codec::parts::enc_represented_prefix(&mut encoded, prefix);
    let mut hash = Sha256::new();
    hash.update(b"syndic/context-projection/v1\0");
    hash.update(home.as_bytes());
    hash.update(thread.as_bytes());
    hash.update(encoded.finish());
    summary.hash_into(&mut hash);
    hash.update(execution_digest);
    hash.update([tool_profile.version() as u8]);
    hash.update(tool_profile.digest());
    hash.finalize().into()
}
