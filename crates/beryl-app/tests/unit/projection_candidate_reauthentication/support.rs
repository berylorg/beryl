use beryl_model::{
    AssetReferenceSetDigest, AssetReferenceSetId, CasNativeTurnCount, SealedAssetReferenceSetProof,
    SyndicDraftId, SyndicItemId,
};
use beryl_state::{BeginAssetReferenceSet, SealAssetReferenceSet};
use syndic_storage::{
    CanonicalItemRecord, ComposerAtom, ComposerPayload, ContentAppend, ContentBuild, CreateThread,
    DraftPayloadUpdate, DraftPayloadUpdateDecision, HistorySummaryRecord, IdleSubmission,
    InputGateRecord, PreparedContent, PublishValidBinding, SelectedPathProof, SyndicTimestamp,
    test_faults::{FixtureBatch, FixtureDelete, FixtureRecord},
};

use crate::{
    conversation_tools::ConversationToolRegistry, input_admission::idle_submission_command,
};

struct Phase83Fixture {
    ledger: Option<AdoptedProjectionCandidateReauthenticationLedger>,
    home: Arc<HomeStore>,
    storage: SyndicStorage,
    syndic_thread_id: SyndicThreadId,
    faults: FaultController,
    connection: Arc<ProjectionConnection>,
    stable_identity: crate::cas_projection::connection::ProjectionConnectionIdentityObservation,
    registry_before:
        Vec<crate::cas_projection::connection::registry::LoadedRegistryRecoveryObservation>,
    home_revision: u64,
    recovered_generation: HomeGeneration,
    replacement_workers: usize,
    replacement_shutdowns: Arc<AtomicUsize>,
    server: Option<admission_server::NormalTerminalServer>,
    server_close_mode: Phase83ServerCloseMode,
    directory: Option<tempfile::TempDir>,
}

#[derive(Clone, Copy)]
enum Phase83ServerCloseMode {
    ControlledQuiet,
    ClientClose,
}

impl Phase83Fixture {
    fn new(seed: u8, candidate_count: usize, exact_witness: bool) -> Self {
        Self::new_with_server_close_mode(
            seed,
            candidate_count,
            exact_witness,
            Phase83ServerCloseMode::ControlledQuiet,
        )
    }

    fn new_for_explicit_terminal_disposition(
        seed: u8,
        candidate_count: usize,
        exact_witness: bool,
    ) -> Self {
        Self::new_with_server_close_mode(
            seed,
            candidate_count,
            exact_witness,
            Phase83ServerCloseMode::ClientClose,
        )
    }

    fn new_with_server_close_mode(
        seed: u8,
        candidate_count: usize,
        exact_witness: bool,
        server_close_mode: Phase83ServerCloseMode,
    ) -> Self {
        assert!(candidate_count <= 2);
        let (directory, faults, state, _shutdowns, service) = service_with_worker_capacity(8);
        let server = match server_close_mode {
            Phase83ServerCloseMode::ControlledQuiet => {
                admission_server::NormalTerminalServer::spawn_admission_only_controlled_close()
            }
            Phase83ServerCloseMode::ClientClose => {
                admission_server::NormalTerminalServer::spawn_admission_only()
            }
        };
        let runtime_id = RuntimeId::from_bytes([seed; 16]);
        let connector = ManagedBackendClientConnector::for_lifecycle_test(
            server.endpoint(),
            admission_server::AUTHORIZATION,
        );
        let session = service
            .admit_lifecycle_test_candidate(
                &connector,
                runtime_id,
                CasProcessGeneration::new(83_000 + u64::from(seed)).unwrap(),
                Path::new(r"C:\work\beryl"),
                Duration::from_secs(10),
            )
            .unwrap();
        server.wait_for_admission();
        let connection = Arc::clone(session.connection());
        let stable_identity = connection.identity_observation();
        let owner = SyndicThreadId::from_bytes([seed.wrapping_add(1); 16]);
        let cas_thread_id = CasThreadId::new(format!("phase-83-candidate-{seed}")).unwrap();
        let execution = phase79_execution_binding(runtime_id, seed.wrapping_add(2));
        let (binding_revision, lineage) = if candidate_count == 0 {
            (BindingRevision::new(1).unwrap(), phase79_lineage())
        } else {
            phase83_establish_pending_ordinary(
                service.home.as_deref().unwrap(),
                service.storage,
                state,
                owner,
                seed,
                execution.clone(),
                cas_thread_id.clone(),
            )
        };
        let witness_revision = if exact_witness {
            binding_revision
        } else {
            BindingRevision::new(binding_revision.get() + 1).unwrap()
        };
        let coordinator =
            CasProjectionCoordinator::for_healthy_home(service.home.as_deref().unwrap()).unwrap();
        let mut projections = Vec::new();
        if candidate_count > 0 {
            let lease = phase79_register_candidate_lease(
                &service,
                &connection,
                cas_thread_id.clone(),
                owner,
            );
            projections.push(LoadedCasProjection::new(
                &coordinator,
                owner,
                witness_revision,
                execution.clone(),
                cas_thread_id.clone(),
                lease,
                lineage,
            ));
        }
        if candidate_count > 1 {
            let sibling =
                phase79_acquire_candidate_sibling(&service, &connection, &cas_thread_id, owner);
            projections.push(LoadedCasProjection::new(
                &coordinator,
                owner,
                witness_revision,
                execution,
                cas_thread_id,
                sibling,
                lineage,
            ));
        }
        wait_until("the Phase 83 candidate holds to settle", || {
            service.worker_pool_diagnostics().active() == 2 + candidate_count
        });

        fail_home_through_live_command(&service, state, &faults);
        assert_eq!(
            service.persistent_failure_notification().notify(),
            PersistentFailureNotificationStatus::Joined
        );
        drop(projections);
        drop(session);
        wait_until("the Phase 83 failure cut to finish", || {
            let snapshot = service.persistent_failure_cut_snapshot();
            snapshot.state() == PersistentFailureCutState::Finished
                && snapshot.retained_projection_count() == candidate_count
        });
        let handoff = match service.close().unwrap() {
            ProjectionConnectionServiceCloseOutcome::PersistentFailure(handoff) => handoff,
            ProjectionConnectionServiceCloseOutcome::Closed => {
                panic!("Phase 83 candidates require retained recovery authority")
            }
        };
        let inventory = handoff.into_recovery_inventory().unwrap();
        let home = Arc::clone(inventory.retained_home());
        let config = inventory.retained_service_config();
        let quarantine = inventory.into_pending_projection_quarantine().unwrap();
        let recovery = home.recover_same_home().unwrap();
        let recovered_generation = recovery.generation();
        let storage = SyndicStorage::reacquire(&home).unwrap();
        let replacement_shutdowns = Arc::new(AtomicUsize::new(0));
        let replacement = UnpublishedProjectionConnectionService::from_recovered_home(
            Arc::clone(&home),
            config,
            Box::new(ShutdownProbe {
                count: Arc::clone(&replacement_shutdowns),
            }),
        )
        .unwrap();
        let adopted = quarantine.adopt_unpublished_service(replacement).unwrap();
        let replacement_workers = adopted.replacement_worker_diagnostics_for_test().active();
        let registry_before =
            crate::cas_projection::connection::registry::recovery_audit(&[stable_identity])
                .unwrap()
                .into_observations();
        let home_revision = home.home_revision().unwrap().get();
        let ledger = adopted.begin_candidate_reauthentication();
        Self {
            ledger: Some(ledger),
            home,
            storage,
            syndic_thread_id: owner,
            faults,
            connection,
            stable_identity,
            registry_before,
            home_revision,
            recovered_generation,
            replacement_workers,
            replacement_shutdowns,
            server: Some(server),
            server_close_mode,
            directory: Some(directory),
        }
    }

    fn ledger(&self) -> &AdoptedProjectionCandidateReauthenticationLedger {
        self.ledger.as_ref().unwrap()
    }

    fn ledger_mut(&mut self) -> &mut AdoptedProjectionCandidateReauthenticationLedger {
        self.ledger.as_mut().unwrap()
    }

    fn take_ledger(&mut self) -> AdoptedProjectionCandidateReauthenticationLedger {
        self.ledger.take().unwrap()
    }

    fn candidate_ids(&self) -> Vec<ProjectionCandidateId> {
        self.ledger()
            .candidates()
            .map(|entry| entry.candidate_id())
            .collect()
    }

    fn registry_now(
        &self,
    ) -> Vec<crate::cas_projection::connection::registry::LoadedRegistryRecoveryObservation> {
        crate::cas_projection::connection::registry::recovery_audit(&[self.stable_identity])
            .unwrap()
            .into_observations()
    }

    fn pending_item(&self) -> CanonicalItemRecord {
        let thread = self
            .storage
            .thread(&self.home, self.syndic_thread_id, phase83_point_limit())
            .unwrap()
            .unwrap();
        let turn_id = thread
            .committed_tail()
            .expect("the fixture has one pending turn");
        let items = self
            .storage
            .turn_items(
                &self.home,
                turn_id,
                None,
                beryl_home_store::CursorReadLimits::new(2, 4 * 1024).unwrap(),
            )
            .unwrap();
        assert_eq!(items.records().len(), 1);
        self.storage
            .canonical_item(
                &self.home,
                items.records()[0].item_id(),
                phase83_point_limit(),
            )
            .unwrap()
            .unwrap()
    }

    fn apply_fixture_batch(&self, batch: FixtureBatch) {
        phase83_execute(
            &self.home,
            self.storage
                .fixture_contribution(self.storage.revision(&self.home).unwrap(), batch),
        );
    }

    fn seal_empty_asset_reference_set(
        &self,
        source: beryl_model::SealedContentMarkerSummary,
        seed: u8,
    ) -> SealedAssetReferenceSetProof {
        let state = BerylState::reacquire(&self.home).unwrap();
        let assets = state.assets();
        let begin =
            BeginAssetReferenceSet::new(AssetReferenceSetId::from_bytes([seed; 16]), source);
        let staging = begin.staging_authority();
        phase83_execute(
            &self.home,
            assets.begin_reference_set(assets.revision(&self.home).unwrap(), begin),
        );
        let build = assets
            .staged_reference_set_manifest(&self.home, staging)
            .unwrap()
            .build_proof();
        let proof = build.sealed_proof().unwrap();
        phase83_execute(
            &self.home,
            assets.seal_reference_set(
                assets.revision(&self.home).unwrap(),
                SealAssetReferenceSet::new(build, source),
            ),
        );
        proof
    }

    fn close(mut self) {
        drop(self.ledger.take());
        drop(self.connection);
        let server = self.server.take().unwrap();
        if matches!(
            self.server_close_mode,
            Phase83ServerCloseMode::ControlledQuiet
        ) {
            server.assert_quiet_and_close();
        }
        server.join();
        drop(self.home);
        drop(self.directory.take());
    }
}
