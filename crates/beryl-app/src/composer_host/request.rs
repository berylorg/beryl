use beryl_home_store::{HomeHealthState, HomeStore};
use syndic_storage::{DraftEditorCandidateSessionReadOutcomeV1, DraftPieceRangeSourceErrorV1};

use super::*;

impl SyndicComposerHost {
    pub fn begin_request(
        &mut self,
        key: ComposerHostRequestKey,
        kind: ComposerHostRequestKind,
    ) -> Result<ComposerHostPendingRequest, ComposerHostError> {
        let active = self.validate_key(key)?;
        validate_target(active, key, &kind)?;
        if key.request_id().get() <= self.last_request_id {
            return Err(ComposerHostError::StaleRequestIdentity);
        }
        if self.pending.len() >= COMPOSER_HOST_MAX_PENDING_REQUESTS {
            return Err(ComposerHostError::PendingRequestLimit);
        }
        let pending = ComposerHostPendingRequest::new(key, kind);
        self.last_request_id = key.request_id().get();
        if self
            .pending
            .insert(key.request_id().get(), pending.clone())
            .is_some()
        {
            return Err(ComposerHostError::StaleRequestIdentity);
        }
        Ok(pending)
    }

    pub fn cancel_request(&mut self, key: ComposerHostRequestKey) -> bool {
        let id = key.request_id().get();
        if !self
            .pending
            .get(&id)
            .is_some_and(|pending| pending.key() == key)
        {
            return false;
        }
        self.pending.remove(&id).is_some()
    }

    pub fn execute_pending(
        &self,
        store: &HomeStore,
        pending: ComposerHostPendingRequest,
    ) -> ComposerHostExecution {
        let result = self.execute_pending_inner(store, &pending);
        ComposerHostExecution { pending, result }
    }

    pub fn complete_request(
        &mut self,
        execution: ComposerHostExecution,
    ) -> Result<ComposerHostResponse, ComposerHostError> {
        let id = execution.pending.key().request_id().get();
        let Some(admitted) = self.pending.get(&id) else {
            return Err(ComposerHostError::RequestNotPending);
        };
        if admitted != &execution.pending {
            return Err(ComposerHostError::RequestMismatch);
        }
        self.validate_key(execution.pending.key())?;
        self.pending.remove(&id);
        let value = execution.result?;
        Ok(ComposerHostResponse::new(execution.pending.key(), value))
    }

    fn execute_pending_inner(
        &self,
        store: &HomeStore,
        pending: &ComposerHostPendingRequest,
    ) -> Result<ComposerHostResponseValue, ComposerHostError> {
        let admitted = self
            .pending
            .get(&pending.key().request_id().get())
            .ok_or(ComposerHostError::RequestNotPending)?;
        if admitted != pending {
            return Err(ComposerHostError::RequestMismatch);
        }
        let active = self.validate_key(pending.key())?;
        validate_store(active.binding, store)?;
        match pending.kind() {
            ComposerHostRequestKind::Text {
                target,
                demand,
                max_bytes,
            } => match target {
                ComposerHostReadTarget::Historical(root) => {
                    Ok(ComposerHostResponseValue::HistoricalText(
                        self.storage
                            .draft_piece_text_demand(store, *root, *demand, *max_bytes)?,
                    ))
                }
                ComposerHostReadTarget::Current(thread_id) => {
                    Ok(ComposerHostResponseValue::CurrentText(
                        self.storage.current_draft_piece_text_demand(
                            store, *thread_id, *demand, *max_bytes,
                        )?,
                    ))
                }
                ComposerHostReadTarget::Candidate => Ok(ComposerHostResponseValue::CandidateText(
                    self.storage.candidate_draft_piece_text_demand(
                        store,
                        active.binding.candidate(),
                        *demand,
                        *max_bytes,
                    )?,
                )),
            },
            ComposerHostRequestKind::Markers { target, demand } => match target {
                ComposerHostReadTarget::Historical(root) => {
                    Ok(ComposerHostResponseValue::HistoricalMarkers(
                        self.storage
                            .draft_piece_marker_demand(store, *root, demand.clone())?,
                    ))
                }
                ComposerHostReadTarget::Current(thread_id) => {
                    Ok(ComposerHostResponseValue::CurrentMarkers(
                        self.storage.current_draft_piece_marker_demand(
                            store,
                            *thread_id,
                            demand.clone(),
                        )?,
                    ))
                }
                ComposerHostReadTarget::Candidate => {
                    Ok(ComposerHostResponseValue::CandidateMarkers(
                        self.storage.candidate_draft_piece_marker_demand(
                            store,
                            active.binding.candidate(),
                            demand.clone(),
                        )?,
                    ))
                }
            },
            ComposerHostRequestKind::MarkerProof {
                target,
                request,
                retained_byte_ceiling,
            } => match target {
                ComposerHostReadTarget::Historical(root) => {
                    Ok(ComposerHostResponseValue::HistoricalMarkerProof(
                        self.storage.draft_piece_marker_edge_proof(
                            store,
                            *root,
                            *request,
                            *retained_byte_ceiling,
                        )?,
                    ))
                }
                ComposerHostReadTarget::Current(thread_id) => {
                    Ok(ComposerHostResponseValue::CurrentMarkerProof(
                        self.storage.current_draft_piece_marker_edge_proof(
                            store,
                            *thread_id,
                            *request,
                            *retained_byte_ceiling,
                        )?,
                    ))
                }
                ComposerHostReadTarget::Candidate => {
                    Ok(ComposerHostResponseValue::CandidateMarkerProof(
                        self.storage.candidate_draft_piece_marker_edge_proof(
                            store,
                            active.binding.candidate(),
                            *request,
                            *retained_byte_ceiling,
                        )?,
                    ))
                }
            },
            ComposerHostRequestKind::Restoration { target, seed } => {
                self.execute_restoration(store, active, *target, seed)
            }
        }
    }

    fn execute_restoration(
        &self,
        store: &HomeStore,
        active: &ActiveComposerHost,
        target: ComposerHostReadTarget,
        seed: &ComposerHostRestorationSeed,
    ) -> Result<ComposerHostResponseValue, ComposerHostError> {
        if seed.logical_extent() != seed.root().summary().logical_extent() {
            return Err(ComposerHostError::RestorationBindingMismatch);
        }
        match target {
            ComposerHostReadTarget::Historical(root) => {
                if root != seed.root() {
                    return Err(ComposerHostError::RestorationBindingMismatch);
                }
                self.storage
                    .validate_draft_piece_restoration(store, seed.restoration())?;
            }
            ComposerHostReadTarget::Current(thread_id) => {
                let restored = self
                    .storage
                    .validate_current_draft_piece_restoration(
                        store,
                        thread_id,
                        seed.restoration().caret(),
                        seed.restoration().selection(),
                        seed.restoration().scroll(),
                        seed.restoration().undo_frontier(),
                    )?
                    .ok_or(ComposerHostError::MissingCurrentDraft)?;
                if restored.root() != seed.root() {
                    return Err(ComposerHostError::RestorationBindingMismatch);
                }
            }
            ComposerHostReadTarget::Candidate => {
                if seed.root() != active.binding.root()
                    || seed.logical_extent() != active.binding.logical_extent()
                {
                    return Err(ComposerHostError::RestorationBindingMismatch);
                }
                let before = candidate_head(&self.storage, store, active.binding.candidate())?;
                self.storage
                    .validate_draft_piece_restoration(store, seed.restoration())?;
                let after = candidate_head(&self.storage, store, active.binding.candidate())?;
                if before != after {
                    return Err(ComposerHostError::Range(
                        DraftPieceRangeSourceErrorV1::ConcurrentChange,
                    ));
                }
            }
        }
        Ok(ComposerHostResponseValue::Restoration(seed.clone()))
    }

    fn validate_key(
        &self,
        key: ComposerHostRequestKey,
    ) -> Result<&ActiveComposerHost, ComposerHostError> {
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        if key.binding() != active.binding {
            return Err(ComposerHostError::OldBinding);
        }
        Ok(active)
    }
}

fn validate_target(
    active: &ActiveComposerHost,
    key: ComposerHostRequestKey,
    kind: &ComposerHostRequestKind,
) -> Result<(), ComposerHostError> {
    let target = match kind {
        ComposerHostRequestKind::Text { target, .. }
        | ComposerHostRequestKind::Markers { target, .. }
        | ComposerHostRequestKind::MarkerProof { target, .. }
        | ComposerHostRequestKind::Restoration { target, .. } => *target,
    };
    match target {
        ComposerHostReadTarget::Historical(root)
            if root.key().draft_id() != key.binding().candidate().draft_id() =>
        {
            Err(ComposerHostError::OldBinding)
        }
        ComposerHostReadTarget::Current(thread_id) if thread_id != active.thread_id => {
            Err(ComposerHostError::OldBinding)
        }
        _ => Ok(()),
    }
}

fn validate_store(
    binding: ComposerHostBinding,
    store: &HomeStore,
) -> Result<(), ComposerHostError> {
    if store.home_id() != binding.home_id() {
        return Err(ComposerHostError::ForeignHome {
            expected: binding.home_id(),
            actual: store.home_id(),
        });
    }
    let health = store.health();
    if health.state() != HomeHealthState::Healthy {
        return Err(ComposerHostError::HomeUnavailable(health.state()));
    }
    if health.generation() != Some(binding.home_generation()) {
        return Err(ComposerHostError::HomeGenerationChanged {
            expected: binding.home_generation(),
            actual: health.generation(),
        });
    }
    Ok(())
}

fn candidate_head(
    storage: &syndic_storage::SyndicStorage,
    store: &HomeStore,
    expected: syndic_storage::DraftEditorCandidateActivationBindingV1,
) -> Result<syndic_storage::DraftEditorCandidateSessionV1, ComposerHostError> {
    let head = match storage.draft_editor_candidate_session(
        store,
        expected.draft_id(),
        expected.session_id(),
    )? {
        DraftEditorCandidateSessionReadOutcomeV1::Active(head) => head,
        DraftEditorCandidateSessionReadOutcomeV1::Disposed(head) => {
            return Err(ComposerHostError::Range(
                DraftPieceRangeSourceErrorV1::Disposed(head),
            ));
        }
        DraftEditorCandidateSessionReadOutcomeV1::Absent => {
            return Err(ComposerHostError::Range(
                DraftPieceRangeSourceErrorV1::Absent,
            ));
        }
        DraftEditorCandidateSessionReadOutcomeV1::ConcurrentChange => {
            return Err(ComposerHostError::Range(
                DraftPieceRangeSourceErrorV1::ConcurrentChange,
            ));
        }
        DraftEditorCandidateSessionReadOutcomeV1::InvariantFailure => {
            return Err(ComposerHostError::Range(
                DraftPieceRangeSourceErrorV1::Invariant,
            ));
        }
    };
    if head.session_generation() != expected.session_generation() {
        return Err(ComposerHostError::Range(
            DraftPieceRangeSourceErrorV1::StaleSession,
        ));
    }
    if head.newest_candidate_generation() != expected.candidate_generation()
        || head.newest_root() != expected.root()
        || head.logical_extent() != expected.logical_extent()
    {
        return Err(ComposerHostError::Range(
            DraftPieceRangeSourceErrorV1::StaleCandidate,
        ));
    }
    Ok(head)
}
