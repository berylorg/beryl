use super::*;
use crate::composer_marker_seal::{
    DraftMarkerSealAdmission, DraftMarkerSealDriveOutcome, DraftMarkerSealFlight,
    DraftMarkerSealFlightRequest, DraftMarkerSealReleaseIntent, DraftMarkerSealReleaseOutcome,
};
use crate::main_window::{
    MainWindowFailedComposerRetirement, MainWindowFailedResidentCandidateSource,
};
use beryl_home_store::HomeRecoveryCandidate;
use gpui_text_input::RangeRestorationSeed;
use syndic_storage::{
    DraftEditorCandidatePublicationEvidenceV1 as Evidence, DraftMarkerSealOperationIdV1,
    DraftPieceOperationIdV1,
};

fn fresh_bytes<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes)
        .map_err(|_| "failed resident identity generation failed".to_owned())?;
    Ok(bytes)
}

fn release_saved_flight(
    markers: &mut crate::composer_marker_seal::DraftMarkerSealRetainedFlights,
    candidate: &mut HomeRecoveryCandidate,
    flight: &mut Option<DraftMarkerSealFlight>,
) -> Result<(), String> {
    let Some(captured) = *flight else {
        return Ok(());
    };
    match markers
        .release_candidate(
            candidate,
            captured,
            DraftMarkerSealReleaseIntent::ServiceDisposed,
        )
        .map_err(|e| e.to_string())?
    {
        DraftMarkerSealReleaseOutcome::Settled { .. }
        | DraftMarkerSealReleaseOutcome::ReleasedWithoutDurableSeal(_)
        | DraftMarkerSealReleaseOutcome::ReleasedAfterSeal(_)
        | DraftMarkerSealReleaseOutcome::ReleasedAfterOtherTerminal { .. }
        | DraftMarkerSealReleaseOutcome::AlreadyReleased => {
            *flight = None;
            Ok(())
        }
        other => Err(format!(
            "saved resident marker settlement retains custody: {other:?}"
        )),
    }
}

pub(super) struct FailedResidentSource {
    pub retired: Option<MainWindowFailedComposerRetirement>,
    pub seed: RangeRestorationSeed,
    pub window: beryl_model::WindowId,
    flight: Option<DraftMarkerSealFlight>,
    evidence: Option<Evidence>,
    operation: Option<DraftPieceOperationIdV1>,
}

impl FailedResidentSource {
    pub(crate) fn new(
        retired: MainWindowFailedComposerRetirement,
        seed: RangeRestorationSeed,
    ) -> Self {
        let window = retired.selection().window_id();
        Self {
            retired: Some(retired),
            seed,
            window,
            flight: None,
            evidence: None,
            operation: None,
        }
    }
}

impl ProcessServiceOwner {
    pub(crate) fn retain_cancelled_failed_residents(
        &mut self,
        graph: &mut super::recovery_graph::PreparedRecoveryServiceGraph,
    ) {
        assert!(graph.failed_residents.iter().all(|source| {
            !self
                .failed_residents
                .iter()
                .any(|retained| retained.window == source.window)
        }));
        self.failed_residents.append(&mut graph.failed_residents);
    }

    pub(crate) fn capture_failed_markers(&mut self) -> Result<(), String> {
        if self.failed_markers.is_some() {
            return Ok(());
        }
        let graph = self.graph.as_ref().ok_or("failed graph is unavailable")?;
        self.failed_markers = Some(
            graph
                .marker
                .as_ref()
                .ok_or("failed marker service is unavailable")?
                .capture_failed_home(graph.home())
                .map_err(|e| e.to_string())?,
        );
        Ok(())
    }

    pub(crate) fn failed_marker_custody(
        &self,
    ) -> Option<&crate::composer_marker_seal::DraftMarkerSealRetainedFlights> {
        self.failed_markers.as_ref()
    }

    pub(crate) fn retain_failed_resident(
        &mut self,
        retired: MainWindowFailedComposerRetirement,
        seed: RangeRestorationSeed,
    ) -> Result<(), MainWindowFailedComposerRetirement> {
        if self
            .failed_residents
            .iter()
            .any(|source| source.window == retired.selection().window_id())
        {
            return Err(retired);
        }
        self.failed_residents
            .push(FailedResidentSource::new(retired, seed));
        Ok(())
    }

    pub(crate) fn prepare_failed_residents(
        &mut self,
        candidate: &mut HomeRecoveryCandidate,
        original: &crate::running_owner::RunningShutdownSession,
        at: SyndicTimestamp,
        cancellation: &CommandCancellation,
    ) -> Result<(), String> {
        let Some(markers) = self.failed_markers.as_mut() else {
            return Ok(());
        };
        let storage = SyndicStorage::reacquire_candidate(candidate).map_err(|e| e.to_string())?;
        let state = BerylState::reacquire_candidate(candidate).map_err(|e| e.to_string())?;
        if !markers.is_bound_to(candidate) {
            markers
                .bind_candidate(candidate, storage.clone(), state.assets())
                .map_err(|e| e.to_string())?;
        }
        let restored = match original {
            crate::running_owner::RunningShutdownSession::RemovedWindow(close) => {
                close.restored_evidence()
            }
            _ => None,
        };
        for source in &mut self.failed_residents {
            if cancellation.is_cancelled() {
                return Err("failed resident qualification was cancelled".into());
            }
            let retired = source
                .retired
                .as_mut()
                .ok_or("failed resident retirement is unavailable")?;
            let restored = restored.filter(|evidence| {
                evidence.window().window_id() == source.window
                    && evidence.window().selected_thread() == Some(retired.selection().claim())
            });
            retired.qualify_saved(candidate, &storage, &state, restored)?;
        }
        for flight in markers.captured_flights().collect::<Vec<_>>() {
            if cancellation.is_cancelled() {
                return Err("original marker settlement was cancelled".into());
            }
            match markers
                .release_candidate(
                    candidate,
                    flight,
                    DraftMarkerSealReleaseIntent::ServiceDisposed,
                )
                .map_err(|e| e.to_string())?
            {
                DraftMarkerSealReleaseOutcome::Settled { .. }
                | DraftMarkerSealReleaseOutcome::ReleasedWithoutDurableSeal(_)
                | DraftMarkerSealReleaseOutcome::ReleasedAfterSeal(_)
                | DraftMarkerSealReleaseOutcome::ReleasedAfterOtherTerminal { .. }
                | DraftMarkerSealReleaseOutcome::AlreadyReleased => {}
                other => {
                    return Err(format!(
                        "original marker settlement retains custody: {other:?}"
                    ));
                }
            }
        }
        if self
            .failed_residents
            .iter()
            .all(|source| source.flight.is_none())
            && !markers.is_settled()
        {
            return Err("original marker ledger retains unresolved authority".into());
        }
        for source in &mut self.failed_residents {
            if cancellation.is_cancelled() {
                return Err("failed resident preparation was cancelled".into());
            }
            let retired = source
                .retired
                .as_mut()
                .ok_or("failed resident source is unavailable")?;
            let restored = restored.filter(|evidence| {
                evidence.window().window_id() == source.window
                    && evidence.window().selected_thread() == Some(retired.selection().claim())
            });
            if retired.qualify_saved(candidate, &storage, &state, restored)? {
                release_saved_flight(markers, candidate, &mut source.flight)?;
                continue;
            }
            let checkpoint =
                retired.qualified_publication_checkpoint(candidate, &storage, &state, restored)?;
            let prior = retired.prior_selector();
            if source.evidence.is_none() {
                if prior.root().marker_commitment() == checkpoint.root().marker_commitment() {
                    source.evidence = Some(
                        if checkpoint.root().marker_commitment().marker_count() == 0 {
                            Evidence::UnchangedEmpty
                        } else {
                            let access = candidate.recovery_access().map_err(|e| e.to_string())?;
                            let head = state
                                .assets()
                                .owner_head_candidate(
                                    &access,
                                    beryl_state::AssetOwner::CurrentDraft(checkpoint.draft_id()),
                                )
                                .map_err(|e| e.to_string())?
                                .ok_or("failed resident previous asset head is unavailable")?;
                            Evidence::UnchangedNonempty {
                                asset_proof: head.set(),
                            }
                        },
                    );
                } else {
                    if source.flight.is_none() {
                        let secret = fresh_bytes()?;
                        let request = DraftMarkerSealFlightRequest::new(
                            checkpoint,
                            DraftMarkerSealOperationIdV1::from_bytes(fresh_bytes()?),
                            beryl_state::AssetReferenceSetStagingAuthority::new(
                                beryl_model::AssetReferenceSetId::from_bytes(fresh_bytes()?),
                                secret,
                            ),
                        );
                        source.flight = Some(
                            match markers
                                .admit_candidate(candidate, request, cancellation)
                                .map_err(|e| e.to_string())?
                            {
                                DraftMarkerSealAdmission::Admitted(flight)
                                | DraftMarkerSealAdmission::Coalesced(flight) => flight,
                                other => {
                                    return Err(format!(
                                        "failed resident marker admission refused: {other:?}"
                                    ));
                                }
                            },
                        );
                    }
                    loop {
                        if cancellation.is_cancelled() {
                            return Err("failed resident marker preparation was cancelled".into());
                        }
                        match markers
                            .drive_candidate(candidate, source.flight.unwrap())
                            .map_err(|e| e.to_string())?
                        {
                            DraftMarkerSealDriveOutcome::Progress => {}
                            DraftMarkerSealDriveOutcome::ChangedNonempty { syndic, assets } => {
                                source.evidence = Some(Evidence::ChangedNonempty {
                                    seal_proof: syndic,
                                    asset_proof: assets,
                                });
                                break;
                            }
                            DraftMarkerSealDriveOutcome::ChangedToEmpty { syndic } => {
                                source.evidence =
                                    Some(Evidence::ChangedEmpty { seal_proof: syndic });
                                break;
                            }
                            other => {
                                return Err(format!(
                                    "failed resident marker preparation unsettled: {other:?}"
                                ));
                            }
                        }
                    }
                }
            }
            if source.operation.is_none() {
                source.operation = Some(DraftPieceOperationIdV1::from_bytes(fresh_bytes()?));
            }
            retired.publish_candidate(
                candidate,
                &storage,
                &state,
                restored,
                source.operation.unwrap(),
                at,
                source.evidence.unwrap(),
                cancellation.clone(),
            )?;
            if !retired.qualify_saved(candidate, &storage, &state, restored)? {
                return Err("failed resident publication is not saved".into());
            }
            release_saved_flight(markers, candidate, &mut source.flight)?;
        }
        if !markers.is_settled() {
            return Err("failed marker ledger retains original authority".into());
        }
        self.failed_markers.take();
        Ok(())
    }
}

impl super::recovery_graph::PreparedRecoveryServiceGraph {
    pub(crate) fn return_failed_resident_source(
        &mut self,
        retired: MainWindowFailedComposerRetirement,
    ) {
        let source = self
            .failed_residents
            .iter_mut()
            .find(|source| source.window == retired.selection().window_id())
            .expect("cancelled failed resident retains its exact graph source");
        assert!(source.retired.is_none());
        source.retired = Some(retired);
    }

    pub(crate) fn failed_resident_source(
        &mut self,
        window: beryl_model::WindowId,
    ) -> Result<MainWindowFailedResidentCandidateSource, (MainWindowFailedComposerRetirement, String)>
    {
        let source = self
            .failed_residents
            .iter_mut()
            .find(|source| source.window == window)
            .expect("captured failed resident has exact graph source");
        let retired = source
            .retired
            .take()
            .expect("captured failed resident owns one reconstruction");
        let Some((candidate, _)) = self
            .services
            .as_mut()
            .unwrap()
            .cas
            .as_mut()
            .unwrap()
            .app_preparation_parts()
        else {
            return Err((retired, "failed resident candidate is unavailable".into()));
        };
        let restored = self.recovered_window.as_ref().filter(|evidence| {
            evidence.window().window_id() == source.window
                && evidence.window().selected_thread() == Some(retired.selection().claim())
        });
        MainWindowFailedResidentCandidateSource::new(
            candidate,
            retired,
            self.syndic.clone(),
            &self.state,
            source.seed,
            restored,
        )
    }

    pub(crate) fn failed_resident_read(
        &mut self,
        source: &MainWindowFailedResidentCandidateSource,
        effect: &gpui_text_input::RangePrepublicationEffect,
    ) -> Result<crate::main_window::MainWindowComposerCandidateRead, String> {
        let (candidate, _) = self
            .services
            .as_mut()
            .unwrap()
            .cas
            .as_mut()
            .unwrap()
            .app_preparation_parts()
            .ok_or("failed resident candidate is unavailable")?;
        MainWindowFailedResidentCandidateSource::read(candidate, source, effect)
    }
}
