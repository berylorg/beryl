use super::{ContextPreparationKind, frame, read};
use crate::{
    CasRepresentedPrefixProof, ContextProjectionBasis, NativeProjectionBasis, RecoveryBudgetKind,
    RecoveryItemCount, RecoveryProjection, RecoveryProjectionError, RecoveryProjectionVersion,
    RecoveryUtf8ByteCount, SelectedPathProof, SyndicStorage,
};
use beryl_home_store::HomeStore;
use beryl_model::{
    DomainRevision, RecoveryItemSequenceAccumulator, RecoveryItemSequenceRole, SyndicThreadId,
};

impl SyndicStorage {
    pub(crate) fn prepare_replay_context(
        &self,
        store: &HomeStore,
        thread: SyndicThreadId,
        selected: SelectedPathProof,
        prefix: CasRepresentedPrefixProof,
        revision: DomainRevision,
        tool: Option<beryl_model::CasConversationToolProfile>,
        maximum_bytes: u64,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<ContextProjectionBasis>, RecoveryProjectionError> {
        let presence =
            super::presence::ContextPresenceAccess::Live(self, store).classify(thread, prefix)?;
        if presence == super::presence::ContextPresence::Required && tool.is_none() {
            return Err(RecoveryProjectionError::MissingContextToolProfile);
        }
        let summary = self.context_summary_limited(
            store,
            thread,
            prefix,
            true,
            Some(maximum_bytes),
            cancelled,
        )?;
        if summary.count() == 0 {
            return Ok(None);
        }
        let execution = self.thread_execution(store, thread, read::point())?.ok_or(
            RecoveryProjectionError::MissingHistory {
                record: "context execution",
            },
        )?;
        let tool = tool.ok_or(RecoveryProjectionError::MissingContextToolProfile)?;
        let expected_gate = self
            .input_gate(store, thread, read::point())?
            .ok_or(RecoveryProjectionError::MissingHistory {
                record: "context gate",
            })?
            .revision();
        Ok(Some(ContextProjectionBasis {
            home: store.home_id(),
            home_generation: self.context_generation,
            thread,
            prefix,
            summary,
            projection_digest: read::projection_digest(
                store.home_id(),
                thread,
                prefix,
                summary,
                execution.execution(),
                tool,
            ),
            source_revision: revision,
            selected_path: selected,
            expected_gate,
            source_head_revision: None,
            kind: ContextPreparationKind::Replay,
        }))
    }

    pub fn prepare_native_context_seed(
        &self,
        store: &HomeStore,
        basis: NativeProjectionBasis,
        model_window: Option<u64>,
    ) -> Result<RecoveryProjection, RecoveryProjectionError> {
        self.prepare_native_context_seed_cancellable(store, basis, model_window, &|| false)
    }

    pub fn prepare_native_context_seed_cancellable(
        &self,
        store: &HomeStore,
        basis: NativeProjectionBasis,
        model_window: Option<u64>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<RecoveryProjection, RecoveryProjectionError> {
        if cancelled() {
            return Err(RecoveryProjectionError::Cancelled);
        }
        let context = basis
            .context()
            .ok_or(RecoveryProjectionError::UnsupportedHistory {
                reason: "native basis has no context",
            })?;
        if !context.needs_seed() || self.revision(store)? != context.source_revision() {
            return Err(RecoveryProjectionError::ConcurrentChange);
        }
        let tokens = model_window.ok_or(RecoveryProjectionError::MissingModelContextWindow)?;
        if tokens == 0 {
            return Err(RecoveryProjectionError::ZeroModelContextWindow);
        }
        let record = self.context_record_for_execution(store, basis.thread_id())?;
        let source = frame::describe(store, basis.thread_id(), &record)?;
        if !matches!(context.kind, ContextPreparationKind::Native { seed: Some(entry), .. } if entry == source.entry)
        {
            return Err(RecoveryProjectionError::Invariant(
                "seed envelope differs from prepared entry",
            ));
        }
        let maximum = 262_144.min(tokens / 2);
        if source.utf8_bytes > maximum {
            return Err(RecoveryProjectionError::BudgetOverflow {
                kind: RecoveryBudgetKind::Utf8Bytes,
                maximum,
                actual: source.utf8_bytes,
            });
        }
        let mut accumulator = RecoveryItemSequenceAccumulator::new(1, source.utf8_bytes);
        accumulator
            .begin_item(
                1,
                RecoveryItemSequenceRole::AssistantOutputText,
                source.utf8_bytes,
            )
            .map_err(|_| RecoveryProjectionError::Invariant("context seed sequence header"))?;
        let mut page = vec![0u8; 65_536].into_boxed_slice();
        let mut offset = 0;
        while offset < source.utf8_bytes {
            if cancelled() {
                return Err(RecoveryProjectionError::Cancelled);
            }
            let (len, _) = frame::read_page(self, store, source, offset, 65_536, &mut page)?;
            accumulator
                .update_text(&page[..len])
                .map_err(|_| RecoveryProjectionError::Invariant("context seed sequence text"))?;
            offset += len as u64;
        }
        accumulator
            .finish_item()
            .map_err(|_| RecoveryProjectionError::Invariant("context seed sequence end"))?;
        let sequence_digest = accumulator
            .finish()
            .map_err(|_| RecoveryProjectionError::Invariant("context seed sequence EOF"))?;
        if self.revision(store)? != context.source_revision() {
            return Err(RecoveryProjectionError::ConcurrentChange);
        }
        Ok(RecoveryProjection {
            context_classification: basis.context_classification,
            context: Some(context),
            seed_only: true,
            version: RecoveryProjectionVersion::ContextAwareV2,
            thread_id: basis.thread_id(),
            selected_path: basis.selected_path(),
            represented_prefix: basis.represented_prefix(),
            item_count: RecoveryItemCount::new(1).expect("one item"),
            utf8_bytes: RecoveryUtf8ByteCount::new(source.utf8_bytes).expect("bounded frame"),
            sequence_digest,
            source_revision: context.source_revision(),
        })
    }
}
