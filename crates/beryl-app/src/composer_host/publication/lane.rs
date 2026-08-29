use super::*;

impl SyndicComposerHost {
    pub fn is_dirty(&self) -> bool {
        self.active.as_ref().is_some_and(|active| {
            active.published_candidate_generation < active.storage_candidate.candidate_generation()
                || active.published_pair
                    != DraftRootHistoryPairV1::new(
                        active.storage_candidate.root(),
                        active.storage_candidate.history(),
                    )
        })
    }

    pub fn publication_custody_count(&self) -> usize {
        usize::from(self.publication.lane.is_some())
    }

    #[cfg(feature = "test-faults")]
    pub fn test_publication_source_custody_count(&self) -> usize {
        match self.publication.lane.as_deref() {
            Some(ComposerHostPublicationLane::Publication(pending)) => {
                usize::from(pending.intent.source.is_some())
            }
            _ => 0,
        }
    }

    #[cfg(feature = "test-faults")]
    pub fn test_arm_publication_convergence_read_fault(
        &mut self,
        fault: impl FnOnce(&HomeStore, syndic_storage::SyndicStorage) + Send + 'static,
    ) {
        assert!(self.publication.convergence_read_fault.is_none());
        self.publication.convergence_read_fault = Some(Box::new(fault));
    }

    pub const fn publication_retained_draft_bytes(&self) -> usize {
        0
    }

    pub fn publication_unavailable(&self) -> Option<ComposerHostPublicationUnavailable> {
        match self.publication.lane.as_deref() {
            Some(ComposerHostPublicationLane::Publication(pending)) => match &pending.stage {
                PublicationStage::Terminal { reason, .. } => Some(*reason),
                _ => None,
            },
            Some(ComposerHostPublicationLane::Disposal(pending)) => pending.terminal,
            None => None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::composer_host) fn capture_lifecycle_publication(
        &mut self,
        store: &HomeStore,
        assets: AssetState,
        marker_seals: &DraftMarkerSealService,
        operation_id: DraftPieceOperationIdV1,
        marker_authority: Option<ComposerHostMarkerSealAuthority>,
        published_at: SyndicTimestamp,
        cancellation: &CommandCancellation,
    ) -> Result<ComposerHostPublicationCapture, ComposerHostError> {
        if self.publication.lane.is_some() {
            return Err(ComposerHostError::PublicationPending);
        }
        if self.live_operation_pending() {
            return Err(ComposerHostError::LifecycleBlocked);
        }
        let active = self.active.as_ref().ok_or(ComposerHostError::OldBinding)?;
        validate_store(active.binding, store)?;
        if active.unavailable || active.session_disposed {
            return Err(ComposerHostError::PublicationUnavailable);
        }
        if !self.is_dirty() {
            return Ok(ComposerHostPublicationCapture::CleanNoOp);
        }
        if cancellation.is_cancelled() {
            return Ok(ComposerHostPublicationCapture::CancelledBeforeAdmission);
        }

        authenticate_capture(self, store, active)?;
        let candidate = active.storage_candidate;
        let candidate_pair = DraftRootHistoryPairV1::new(candidate.root(), candidate.history());
        let source = self
            .storage
            .capture_draft_editor_candidate_publication_source(
                store,
                DraftEditorCandidatePublicationSourceCaptureRequestV1::new(
                    active.durable_selector,
                    candidate,
                    operation_id,
                    published_at,
                ),
            )?;
        let mut intent = PublicationIntent {
            binding: active.binding,
            selector: active.durable_selector,
            candidate,
            candidate_pair,
            marker_authority,
            source: Some(source),
            assets,
            cancellation: cancellation.clone(),
        };
        let lane_generation = next(self.publication.lane_generation)?;
        let ticket = ComposerHostPublicationTicket {
            host_generation: active.binding.host_generation(),
            lane_generation,
            candidate_generation: candidate.candidate_generation(),
        };

        let prior_root = active.published_pair.root();
        let changed = prior_root.marker_commitment() != candidate.root().marker_commitment();
        let stage = if changed {
            let authority = intent
                .marker_authority
                .ok_or(ComposerHostError::MarkerSealAuthorityRequired)?;
            let request = DraftMarkerSealFlightRequest::new(
                candidate,
                authority.operation_id,
                authority.staging,
            );
            match marker_seals.admit(store, request, cancellation)? {
                DraftMarkerSealAdmission::Admitted(flight)
                | DraftMarkerSealAdmission::Coalesced(flight) => PublicationStage::Sealing {
                    service: marker_seals.clone(),
                    flight,
                },
                DraftMarkerSealAdmission::CancelledBeforeAdmission => {
                    return Ok(ComposerHostPublicationCapture::CancelledBeforeAdmission);
                }
                DraftMarkerSealAdmission::Saturated => {
                    return Err(ComposerHostError::MarkerSealCapacity);
                }
                DraftMarkerSealAdmission::Conflict => {
                    return Err(ComposerHostError::MarkerSealIdentityCollision);
                }
            }
        } else {
            if intent.marker_authority.is_some() {
                return Err(ComposerHostError::UnexpectedMarkerSealAuthority);
            }
            let evidence = unchanged_evidence(store, &intent.assets, candidate_pair)?;
            PublicationStage::Ready(prepare_publication(
                store,
                &self.storage,
                &mut intent,
                evidence,
            )?)
        };

        self.publication.lane_generation = lane_generation;
        self.publication.lane = Some(Box::new(ComposerHostPublicationLane::Publication(
            PendingPublication {
                ticket,
                intent,
                stage,
            },
        )));
        Ok(ComposerHostPublicationCapture::Captured(ticket))
    }

    pub(in crate::composer_host) fn drive_publication_lane(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostPublicationTicket,
    ) -> Result<ComposerHostPublicationDrive, ComposerHostError> {
        let (service, flight, releasing) = {
            let pending = self.pending_publication(ticket)?;
            validate_store(pending.intent.binding, store)?;
            match &pending.stage {
                PublicationStage::Ready(_) => return Ok(ComposerHostPublicationDrive::Ready),
                PublicationStage::Sealed(evidence) => {
                    let evidence = *evidence;
                    self.install_ready_publication(store, ticket, evidence)?;
                    return Ok(ComposerHostPublicationDrive::Ready);
                }
                PublicationStage::Sealing { service, flight } => (service.clone(), *flight, false),
                PublicationStage::Releasing {
                    service, flight, ..
                } => (service.clone(), *flight, true),
                PublicationStage::Reconciling { .. } | PublicationStage::Terminal { .. } => {
                    return Err(ComposerHostError::PublicationPending);
                }
            }
        };
        if releasing {
            return self.continue_marker_release(store, ticket, service, flight);
        }

        match service.drive(store, flight) {
            Ok(DraftMarkerSealDriveOutcome::Progress) => Ok(ComposerHostPublicationDrive::Progress),
            Ok(DraftMarkerSealDriveOutcome::NotCommitted(stage)) => {
                Ok(ComposerHostPublicationDrive::NotCommitted(stage))
            }
            Ok(DraftMarkerSealDriveOutcome::ChangedNonempty { syndic, assets }) => {
                self.pending_publication_mut(ticket)?.stage = PublicationStage::Sealed(
                    DraftEditorCandidatePublicationEvidenceV1::ChangedNonempty {
                        seal_proof: syndic,
                        asset_proof: assets,
                    },
                );
                self.install_ready_publication(
                    store,
                    ticket,
                    DraftEditorCandidatePublicationEvidenceV1::ChangedNonempty {
                        seal_proof: syndic,
                        asset_proof: assets,
                    },
                )?;
                Ok(ComposerHostPublicationDrive::Ready)
            }
            Ok(DraftMarkerSealDriveOutcome::ChangedToEmpty { syndic }) => {
                self.pending_publication_mut(ticket)?.stage = PublicationStage::Sealed(
                    DraftEditorCandidatePublicationEvidenceV1::ChangedEmpty { seal_proof: syndic },
                );
                self.install_ready_publication(
                    store,
                    ticket,
                    DraftEditorCandidatePublicationEvidenceV1::ChangedEmpty { seal_proof: syndic },
                )?;
                Ok(ComposerHostPublicationDrive::Ready)
            }
            Ok(DraftMarkerSealDriveOutcome::TerminalSettlementPending(intent)) => {
                self.set_marker_release(ticket, service.clone(), flight, intent)?;
                self.continue_marker_release(store, ticket, service, flight)
            }
            Err(error) => {
                if matches!(
                    &error,
                    crate::composer_marker_seal::DraftMarkerSealServiceError::ReconciliationCollision
                ) {
                    self.make_publication_terminal(
                        ticket,
                        ComposerHostPublicationUnavailable::ReconciliationCollision,
                    )?;
                    return Err(error.into());
                }
                let intent = match error {
                    crate::composer_marker_seal::DraftMarkerSealServiceError::CandidateSessionDisposed => {
                        DraftMarkerSealReleaseIntent::SessionDisposed
                    }
                    crate::composer_marker_seal::DraftMarkerSealServiceError::ServiceDisposed
                    | crate::composer_marker_seal::DraftMarkerSealServiceError::ServiceDisposing => {
                        DraftMarkerSealReleaseIntent::ServiceDisposed
                    }
                    _ => DraftMarkerSealReleaseIntent::Failed(
                        DraftMarkerSealFailureReasonV1::Operational,
                    ),
                };
                self.set_marker_release(ticket, service, flight, intent)?;
                Err(error.into())
            }
        }
    }

    pub(in crate::composer_host) fn release_publication_lane(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostPublicationTicket,
        reason: ComposerHostPublicationReleaseReason,
    ) -> Result<ComposerHostPublicationReleaseCompletion, ComposerHostError> {
        let (service, flight, intent) = {
            let successor = self.active.as_ref().map(|active| active.storage_candidate);
            let intent = release_intent(reason, successor)?;
            let pending = self.pending_publication(ticket)?;
            validate_store(pending.intent.binding, store)?;
            match &pending.stage {
                PublicationStage::Sealing { service, flight } => (service.clone(), *flight, intent),
                PublicationStage::Releasing {
                    service,
                    flight,
                    intent: installed,
                } if *installed == intent => (service.clone(), *flight, intent),
                PublicationStage::Ready(_) => {
                    self.publication.lane = None;
                    return Ok(ComposerHostPublicationReleaseCompletion::Released);
                }
                PublicationStage::Sealed(_) => {
                    self.publication.lane = None;
                    return Ok(ComposerHostPublicationReleaseCompletion::Released);
                }
                PublicationStage::Releasing { .. }
                | PublicationStage::Reconciling { .. }
                | PublicationStage::Terminal { .. } => {
                    return Err(ComposerHostError::PublicationPending);
                }
            }
        };
        self.set_marker_release(ticket, service.clone(), flight, intent)?;
        match self.continue_marker_release(store, ticket, service, flight)? {
            ComposerHostPublicationDrive::ReleasePending => {
                Ok(ComposerHostPublicationReleaseCompletion::Pending)
            }
            _ => Ok(ComposerHostPublicationReleaseCompletion::Released),
        }
    }

    fn install_ready_publication(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostPublicationTicket,
        evidence: DraftEditorCandidatePublicationEvidenceV1,
    ) -> Result<(), ComposerHostError> {
        let (storage, publication) = (&self.storage, &mut self.publication);
        let pending = match publication.lane.as_deref_mut() {
            Some(ComposerHostPublicationLane::Publication(pending)) if pending.ticket == ticket => {
                pending
            }
            Some(_) => return Err(ComposerHostError::StalePublicationGeneration),
            None => return Err(ComposerHostError::PublicationNotPending),
        };
        let prepared = prepare_publication(store, storage, &mut pending.intent, evidence)?;
        pending.stage = PublicationStage::Ready(prepared);
        Ok(())
    }

    fn set_marker_release(
        &mut self,
        ticket: ComposerHostPublicationTicket,
        service: DraftMarkerSealService,
        flight: DraftMarkerSealFlight,
        intent: DraftMarkerSealReleaseIntent,
    ) -> Result<(), ComposerHostError> {
        self.pending_publication_mut(ticket)?.stage = PublicationStage::Releasing {
            service,
            flight,
            intent,
        };
        Ok(())
    }

    fn continue_marker_release(
        &mut self,
        store: &HomeStore,
        ticket: ComposerHostPublicationTicket,
        service: DraftMarkerSealService,
        flight: DraftMarkerSealFlight,
    ) -> Result<ComposerHostPublicationDrive, ComposerHostError> {
        let intent = match &self.pending_publication(ticket)?.stage {
            PublicationStage::Releasing { intent, .. } => *intent,
            _ => return Err(ComposerHostError::PublicationPending),
        };
        let outcome = match service.release(store, flight, intent) {
            Ok(outcome) => outcome,
            Err(
                error @ crate::composer_marker_seal::DraftMarkerSealServiceError::ReconciliationCollision,
            ) => {
                self.make_publication_terminal(
                    ticket,
                    ComposerHostPublicationUnavailable::ReconciliationCollision,
                )?;
                return Err(error.into());
            }
            Err(error) => return Err(error.into()),
        };
        match outcome {
            DraftMarkerSealReleaseOutcome::DeferredByActiveDrive(_)
            | DraftMarkerSealReleaseOutcome::NotCommitted(_) => {
                Ok(ComposerHostPublicationDrive::ReleasePending)
            }
            DraftMarkerSealReleaseOutcome::ConflictingIntent { .. } => {
                self.make_publication_terminal(
                    ticket,
                    ComposerHostPublicationUnavailable::IdentityCollision,
                )?;
                Err(ComposerHostError::MarkerSealIdentityCollision)
            }
            DraftMarkerSealReleaseOutcome::Settled { .. }
            | DraftMarkerSealReleaseOutcome::ReleasedWithoutDurableSeal(_)
            | DraftMarkerSealReleaseOutcome::ReleasedAfterSeal(_)
            | DraftMarkerSealReleaseOutcome::ReleasedAfterOtherTerminal { .. }
            | DraftMarkerSealReleaseOutcome::AlreadyReleased
            | DraftMarkerSealReleaseOutcome::HomeGenerationRetired => {
                self.publication.lane = None;
                Ok(ComposerHostPublicationDrive::Progress)
            }
        }
    }

    pub(super) fn pending_publication(
        &self,
        ticket: ComposerHostPublicationTicket,
    ) -> Result<&PendingPublication, ComposerHostError> {
        match self.publication.lane.as_deref() {
            Some(ComposerHostPublicationLane::Publication(pending))
                if pending.ticket == ticket
                    && ticket.host_generation == pending.intent.binding.host_generation() =>
            {
                Ok(pending)
            }
            Some(_) => Err(ComposerHostError::StalePublicationGeneration),
            None => Err(ComposerHostError::PublicationNotPending),
        }
    }

    pub(super) fn pending_publication_mut(
        &mut self,
        ticket: ComposerHostPublicationTicket,
    ) -> Result<&mut PendingPublication, ComposerHostError> {
        match self.publication.lane.as_deref_mut() {
            Some(ComposerHostPublicationLane::Publication(pending)) if pending.ticket == ticket => {
                Ok(pending)
            }
            Some(_) => Err(ComposerHostError::StalePublicationGeneration),
            None => Err(ComposerHostError::PublicationNotPending),
        }
    }

    pub(super) fn make_publication_terminal(
        &mut self,
        ticket: ComposerHostPublicationTicket,
        reason: ComposerHostPublicationUnavailable,
    ) -> Result<(), ComposerHostError> {
        let binding = {
            let pending = self.pending_publication_mut(ticket)?;
            let prepared = match &pending.stage {
                PublicationStage::Ready(prepared)
                | PublicationStage::Reconciling { prepared, .. } => Some(prepared.clone()),
                PublicationStage::Terminal { prepared, .. } => prepared.clone(),
                PublicationStage::Sealed(_)
                | PublicationStage::Sealing { .. }
                | PublicationStage::Releasing { .. } => None,
            };
            pending.intent.source = None;
            pending.stage = PublicationStage::Terminal { prepared, reason };
            pending.intent.binding
        };
        self.mark_active_session_unavailable(binding);
        Ok(())
    }
}
