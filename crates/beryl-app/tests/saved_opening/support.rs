use beryl_app::{
    composer_host::{
        ComposerHostBinding, ComposerHostFlushAdmission, ComposerHostFlushAdvance,
        ComposerHostFlushCapture, ComposerHostFlushPurpose, ComposerHostFlushState,
        ComposerHostFlushTicket, ComposerHostSubmissionAdvance, ComposerHostSubmissionRequest,
        ComposerHostSubmissionTicket, SyndicComposerHost,
    },
    composer_marker_seal::DraftMarkerSealService,
};
use beryl_home_store::{CommandCancellation, HomeStore};
use beryl_model::{SyndicDraftId, SyndicItemId, SyndicThreadId};
use beryl_state::{AssetState, BerylState};
use syndic_storage::{
    DraftComposerMaterializationOperationIdV1, SyndicCurrentDraft, SyndicPointReadLimit,
    SyndicStorage, SyndicTimestamp,
};

use super::{base, composer, publication};

pub struct Fixture {
    pub host: SyndicComposerHost,
    pub seals: DraftMarkerSealService,
    pub assets: AssetState,
    pub store: HomeStore,
    pub storage: SyndicStorage,
    pub thread: SyndicThreadId,
    pub faults: beryl_home_store::test_faults::FaultController,
    pub home: base::TestHome,
}

impl Fixture {
    pub fn new(name: &str, seed: u8) -> Self {
        let (home, mut store, storage, thread, faults) = base::fault_fixture(name, seed);
        let assets = BerylState::register(&mut store).unwrap().assets();
        let seals = publication::service(&store, storage.clone(), assets.clone(), 1, 1);
        let (host, _) = composer::activated(storage.clone(), &store, thread, seed + 2, seed + 3);
        Self {
            host,
            seals,
            assets,
            store,
            storage,
            thread,
            faults,
            home,
        }
    }

    pub fn reopened(name: &str, seed: u8, text: &str) -> Self {
        let mut fixture = Self::new(name, seed);
        if !text.is_empty() {
            let initial = fixture.binding();
            composer::commit_text(
                &mut fixture.host,
                &fixture.store,
                initial,
                1,
                0,
                0,
                text,
                text.len() as u64,
                1,
            );
            fixture.flush_submission(2);
        }
        let durable = fixture.current();
        let Self {
            host,
            seals,
            assets,
            store,
            storage: _,
            thread,
            faults,
            home,
        } = fixture;
        drop(host);
        drop(seals);
        drop(assets);
        drop(store);
        let (mut store, storage) = base::reopen(&home);
        let assets = BerylState::register(&mut store).unwrap().assets();
        let seals = publication::service(&store, storage.clone(), assets.clone(), 1, 1);
        let (host, _) = composer::activated(storage.clone(), &store, thread, seed + 5, seed + 6);
        let fixture = Self {
            host,
            seals,
            assets,
            store,
            storage,
            thread,
            faults,
            home,
        };
        assert_eq!(fixture.current(), durable);
        assert_eq!(fixture.binding().root(), durable.draft().piece_root());
        assert_ne!(fixture.binding().history(), durable.draft().history());
        assert_eq!(
            fixture.binding().candidate().candidate_generation() > 0,
            !text.is_empty()
        );
        fixture
    }

    pub fn binding(&self) -> ComposerHostBinding {
        self.host.binding().unwrap()
    }

    pub fn current(&self) -> SyndicCurrentDraft {
        self.storage
            .current_draft(&self.store, self.thread, point_limit())
            .unwrap()
            .unwrap()
    }

    pub fn begin(
        &mut self,
        purpose: ComposerHostFlushPurpose,
    ) -> (ComposerHostFlushTicket, ComposerHostFlushState) {
        match self.host.begin_flush(purpose).unwrap() {
            ComposerHostFlushAdmission::Started { ticket, state } => (ticket, state),
            other => panic!(" flush was not started: {other:?}"),
        }
    }

    pub fn capture(
        &mut self,
        ticket: ComposerHostFlushTicket,
        operation: u64,
    ) -> ComposerHostFlushCapture {
        self.host
            .capture_flush_publication(
                &self.store,
                ticket,
                self.assets.clone(),
                &self.seals,
                composer::operation_id(operation),
                None,
                SyndicTimestamp::from_unix_millis(operation),
                &CommandCancellation::new(),
            )
            .unwrap()
    }

    pub fn flush_submission(&mut self, operation: u64) {
        let (ticket, _) = self.begin(ComposerHostFlushPurpose::Submission);
        for _ in 0..64 {
            if self.host.flush_state(ticket).unwrap() == ComposerHostFlushState::CaptureRequired {
                match self.capture(ticket, operation) {
                    ComposerHostFlushCapture::Satisfied(ComposerHostFlushPurpose::Submission) => {
                        return;
                    }
                    ComposerHostFlushCapture::Captured(_) => {}
                    other => panic!(" flush capture failed: {other:?}"),
                }
            }
            match self.host.advance_flush(&self.store, ticket).unwrap() {
                ComposerHostFlushAdvance::Progress(_) => {}
                ComposerHostFlushAdvance::Satisfied(ComposerHostFlushPurpose::Submission) => return,
                other => panic!(" flush failed: {other:?}"),
            }
        }
        panic!(" submission flush exceeded bounded work");
    }

    pub fn advance_submission(
        &mut self,
        ticket: ComposerHostSubmissionTicket,
        operation: u64,
    ) -> ComposerHostSubmissionAdvance {
        self.host
            .advance_submission(
                &self.store,
                ticket,
                self.assets.clone(),
                &self.seals,
                composer::operation_id(operation),
                None,
                SyndicTimestamp::from_unix_millis(operation),
                &CommandCancellation::new(),
            )
            .unwrap()
    }
}

pub fn point_limit() -> SyndicPointReadLimit {
    SyndicPointReadLimit::new(65_536).unwrap()
}

pub fn submission_request(seed: u8) -> ComposerHostSubmissionRequest {
    ComposerHostSubmissionRequest::new(
        SyndicDraftId::from_bytes([seed; 16]),
        SyndicItemId::from_bytes([seed + 1; 16]),
        DraftComposerMaterializationOperationIdV1::from_bytes([seed + 2; 16]),
        composer::operation_id(seed as u64 + 3),
        SyndicTimestamp::from_unix_millis(500),
        beryl_app::cas_projection::ProjectionServiceConfig::try_new(
            1,
            4,
            beryl_home_store::MinimumTurnCaptureReserve::try_new(1).unwrap(),
        )
        .unwrap()
        .turn_start_admission_requirement(),
    )
}
