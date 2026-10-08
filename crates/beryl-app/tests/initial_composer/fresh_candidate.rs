use super::*;
use beryl_app::composer_host::{
    ComposerHostActivationRequest, ComposerHostInitialDemand, ComposerHostRequestId,
    ComposerHostRequestPurpose,
};
use beryl_home_store::{HomeHealthState, HomeRecoveryCandidate};
use beryl_state::SessionWindowRecord;
use std::num::NonZeroU64;
use syndic_storage::{
    DraftPieceMarkerDemandV1, DraftPieceMarkerDirectionV1, DraftPieceMarkerScopeV1,
    DraftPieceOperationIdV1, DraftPieceTextDemandV1,
};

struct FreshFixture {
    _directory: tempfile::TempDir,
    candidate: HomeRecoveryCandidate,
    state: BerylState,
    storage: SyndicStorage,
    window: SessionWindowRecord,
    thread: SyndicThreadId,
    draft: SyndicDraftId,
}

impl FreshFixture {
    fn new(seed: u8) -> Self {
        let fixture = Fixture::new(seed);
        let acquisition = fixture.acquire(seed.wrapping_add(1));
        let thread = acquisition.thread_id();
        let draft = acquisition.draft_id();
        let window = fixture
            .state
            .session()
            .minimal_bootstrap(&fixture.store)
            .unwrap()
            .unwrap()
            .windows()
            .iter()
            .find(|window| window.window_id() == acquisition.window_id())
            .unwrap()
            .clone();
        drop(acquisition);
        fixture.faults.fail_next(FaultPoint::BeforeReadConfirmation);
        assert!(fixture.store.home_revision().is_err());
        let Fixture {
            directory,
            store,
            state,
            storage,
            service,
            ..
        } = fixture;
        drop((state, storage, service));
        let mut candidate = Arc::try_unwrap(store)
            .ok()
            .unwrap()
            .recover_same_home()
            .unwrap();
        let state = BerylState::reacquire_candidate(&mut candidate).unwrap();
        let storage = SyndicStorage::reacquire_candidate(&candidate).unwrap();
        Self {
            _directory: directory,
            candidate,
            state,
            storage,
            window,
            thread,
            draft,
        }
    }

    fn owner(&mut self, seed: u8, bad_content: bool) -> MainWindowFreshComposerPreparation {
        MainWindowFreshComposerPreparation::new(
            &mut self.candidate,
            &self.state,
            self.storage.clone(),
            self.window.clone(),
            self.thread,
            self.draft,
            request(self.thread, seed, bad_content),
            operation(seed.wrapping_add(2)),
            MainWindowComposerMarkerMetadataAuthority::new(self.state.assets()),
        )
        .unwrap()
    }

    fn settle(&mut self, cleanup: &mut MainWindowInitialComposerRecoveryCleanup) {
        assert_eq!(
            cleanup
                .settle(
                    &self.storage,
                    &mut self.candidate.recovery_access().unwrap(),
                    CommandCancellation::new()
                )
                .unwrap(),
            MainWindowInitialComposerRecoveryProgress::Complete
        );
        let opening = cleanup.test_original_opening().unwrap();
        let home_id = self.candidate.home_id();
        assert!(matches!(
            self.storage
                .qualify_fresh_draft_editor_candidate_session_open_candidate(
                    &self.candidate.recovery_access().unwrap(),
                    home_id,
                    operation(opening.request().session_id().as_bytes()[0].wrapping_add(2)),
                    opening
                )
                .unwrap(),
            DraftEditorCandidateSessionReadOutcomeV1::Disposed(_)
        ));
    }
}

fn operation(seed: u8) -> DraftPieceOperationIdV1 {
    DraftPieceOperationIdV1::from_bytes([seed; 16])
}

fn request(thread: SyndicThreadId, seed: u8, bad: bool) -> ComposerHostActivationRequest {
    ComposerHostActivationRequest::new(
        thread,
        DraftEditorCandidateSessionIdV1::from_bytes([seed; 16]),
        operation(seed.wrapping_add(1)),
        NonZeroU64::new(1).unwrap(),
        None,
        vec![
            ComposerHostInitialDemand::Text {
                request_id: ComposerHostRequestId::new(NonZeroU64::new(1).unwrap()),
                purpose: ComposerHostRequestPurpose::Geometry,
                demand: DraftPieceTextDemandV1::Forward(if bad { u64::MAX } else { 0 }),
                max_bytes: 256,
            },
            ComposerHostInitialDemand::Markers {
                request_id: ComposerHostRequestId::new(NonZeroU64::new(2).unwrap()),
                purpose: ComposerHostRequestPurpose::Geometry,
                demand: DraftPieceMarkerDemandV1::new(
                    DraftPieceMarkerScopeV1::InclusiveRange { start: 0, end: 0 },
                    DraftPieceMarkerDirectionV1::Forward,
                    None,
                    48,
                    65_536,
                ),
            },
        ]
        .into_boxed_slice(),
    )
}

#[test]
fn reopening_candidate_constructs_exact_editor_without_resident_or_restoration_seed() {
    let mut fixture = FreshFixture::new(201);
    let mut owner = fixture.owner(211, false);
    assert_eq!(
        owner
            .advance(&mut fixture.candidate, &CommandCancellation::new())
            .unwrap(),
        MainWindowInitialComposerProgress::Activated
    );
    let service = owner.service().unwrap();
    let binding = service.selected_identity().unwrap().binding();
    assert_eq!(binding.home_generation(), fixture.candidate.generation());
    assert_eq!(binding.candidate().draft_id(), fixture.draft);
    assert_eq!(
        binding.candidate().session_id(),
        DraftEditorCandidateSessionIdV1::from_bytes([211; 16])
    );
    drop(service);
    let prepared = owner.prepare(&mut config).unwrap();
    assert_eq!(prepared.test_seed_count(), 2);
    assert_eq!(prepared.test_seed_text_bytes(), 0);
    assert_eq!(
        fixture.candidate.service_reference().health().state(),
        HomeHealthState::Reopening
    );
    drop(prepared);
    let mut cleanup = owner
        .capture_cleanup()
        .unwrap_or_else(|(_, error)| panic!("{error}"));
    fixture.settle(&mut cleanup);
}

#[test]
fn prepared_gui_reference_blocks_runtime_retirement_until_released() {
    let mut fixture = FreshFixture::new(202);
    let mut owner = fixture.owner(212, false);
    owner
        .advance(&mut fixture.candidate, &CommandCancellation::new())
        .unwrap();
    let prepared = owner.prepare(&mut config).unwrap();
    let (owner, error) = owner
        .capture_cleanup()
        .err()
        .expect("retained prepared service blocks cleanup");
    assert!(error.contains("resources remain retained"));
    drop(prepared);
    let mut cleanup = owner
        .capture_cleanup()
        .unwrap_or_else(|(_, error)| panic!("{error}"));
    fixture.settle(&mut cleanup);
}

#[test]
fn content_and_config_failure_retain_opening_and_join_before_next_attempt() {
    for content_failure in [false, true] {
        let mut fixture = FreshFixture::new(203);
        let mut owner = fixture.owner(213, content_failure);
        let activated = owner.advance(&mut fixture.candidate, &CommandCancellation::new());
        if content_failure {
            assert!(activated.is_err());
        } else {
            assert_eq!(
                activated.unwrap(),
                MainWindowInitialComposerProgress::Activated
            );
            assert!(
                owner
                    .prepare(&mut |_| Err("injected config failure".into()))
                    .is_err()
            );
        }
        let mut cleanup = owner
            .capture_cleanup()
            .unwrap_or_else(|(_, error)| panic!("{error}"));
        fixture.settle(&mut cleanup);
        let mut successor = fixture.owner(214, false);
        assert_eq!(
            successor
                .advance(&mut fixture.candidate, &CommandCancellation::new())
                .unwrap(),
            MainWindowInitialComposerProgress::Activated
        );
        let mut next_cleanup = successor
            .capture_cleanup()
            .unwrap_or_else(|(_, error)| panic!("{error}"));
        fixture.settle(&mut next_cleanup);
    }
}

#[test]
fn cancelled_unopened_attempt_retains_no_session_and_can_complete_cleanup() {
    let mut fixture = FreshFixture::new(204);
    let mut owner = fixture.owner(215, false);
    let cancellation = CommandCancellation::new();
    cancellation.cancel();
    assert!(
        owner
            .advance(&mut fixture.candidate, &cancellation)
            .is_err()
    );
    let mut cleanup = owner
        .capture_cleanup()
        .unwrap_or_else(|(_, error)| panic!("{error}"));
    assert!(cleanup.test_original_opening().is_none());
    assert_eq!(
        cleanup
            .settle(
                &fixture.storage,
                &mut fixture.candidate.recovery_access().unwrap(),
                CommandCancellation::new()
            )
            .unwrap(),
        MainWindowInitialComposerRecoveryProgress::Complete
    );
}

#[test]
fn foreign_candidate_cannot_advance_or_validate_existing_fresh_preparation() {
    let mut fixture = FreshFixture::new(205);
    let mut foreign = FreshFixture::new(206);
    let mut owner = fixture.owner(216, false);
    assert!(
        owner
            .advance(&mut foreign.candidate, &CommandCancellation::new())
            .is_err()
    );
    assert!(owner.validate(&mut foreign.candidate).is_err());
    owner
        .advance(&mut fixture.candidate, &CommandCancellation::new())
        .unwrap();
    let mut cleanup = owner
        .capture_cleanup()
        .unwrap_or_else(|(_, error)| panic!("{error}"));
    fixture.settle(&mut cleanup);
}
