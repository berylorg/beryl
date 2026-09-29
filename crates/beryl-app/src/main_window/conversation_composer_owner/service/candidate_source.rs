use super::*;
use crate::main_window::composer_slot::{dispatch::translate, native_lineage::position};
use crate::main_window::{
    MainWindowComposerRetiredClose, MainWindowConversationComposerCloseTicket,
};
use beryl_home_store::{HomeCandidateRecoveryAccess, HomeRecoveryCandidate};
use gpui_text_input::{ObjectPage, ObjectRequest, PageRequest, RangePage, RangeRestorationSeed};

pub struct MainWindowComposerCandidateSource {
    service: MainWindowConversationComposerService,
    storage: syndic_storage::SyndicStorage,
    state: beryl_state::BerylState,
    predecessor: MainWindowConversationComposerCloseTicket,
    close: MainWindowConversationComposerCloseTicket,
    selection: MainWindowComposerSelectionIdentity,
    selector: syndic_storage::DraftEditorCurrentSelectorV1,
    seed: RangeRestorationSeed,
}

impl MainWindowComposerCandidateSource {
    pub fn new(
        candidate: &mut HomeRecoveryCandidate,
        retired: MainWindowComposerRetiredClose,
        storage: syndic_storage::SyndicStorage,
        state: &beryl_state::BerylState,
        seed: RangeRestorationSeed,
    ) -> Result<Self, (MainWindowComposerRetiredClose, String)> {
        let binding = retired.host().binding();
        if seed.binding != binding.range_binding()
            || seed.history != Some(binding.range_history_frontier())
            || seed.selection.head != seed.caret
        {
            return Err((
                retired,
                "resident recovery seed does not match its retired source".into(),
            ));
        }
        let validated = (|| {
            let access = candidate.recovery_access().map_err(|e| e.to_string())?;
            if !retired
                .host()
                .saved_checkpoint_matches_candidate(&access, &storage)
                .map_err(|e| e.to_string())?
            {
                return Err("resident recovery checkpoint changed".into());
            }
            validate_positions(&access, &storage, binding, seed)
        })();
        if let Err(error) = validated {
            return Err((retired, error));
        }
        let predecessor = retired.close_ticket();
        let selector = retired.host().selector();
        let (service, close) = MainWindowConversationComposerService::rebind_candidate(
            candidate,
            retired,
            storage.clone(),
            state,
        )?;
        let selection = service
            .selected_identity()
            .expect("reconstructed service has selected host");
        let seed = RangeRestorationSeed {
            binding: selection.binding().range_binding(),
            history: Some(selection.binding().range_history_frontier()),
            ..seed
        };
        Ok(Self {
            service,
            storage,
            state: state.clone(),
            predecessor,
            close,
            selection,
            selector,
            seed,
        })
    }

    pub fn seed(&self) -> RangeRestorationSeed {
        self.seed
    }

    pub fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }

    pub fn predecessor(&self) -> MainWindowConversationComposerCloseTicket {
        self.predecessor
    }

    pub fn close_ticket(&self) -> MainWindowConversationComposerCloseTicket {
        self.close
    }

    pub fn validate(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        request: RangePrepublicationValidationRequest,
    ) -> Result<RangePrepublicationValidationResponse, String> {
        if request.binding != self.seed.binding || request.history != self.seed.history {
            return Err("resident recovery validation request is stale".into());
        }
        self.validate_source(access)?;
        validate_positions(access, &self.storage, self.selection.binding(), self.seed)?;
        Ok(RangePrepublicationValidationResponse {
            key: request.key,
            binding: request.binding,
            history: request.history,
            current: true,
        })
    }

    pub fn text_page(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        request: PageRequest,
    ) -> Result<RangePage, String> {
        self.validate_source(access)?;
        translate::candidate_text_page(&self.storage, access, self.selection.binding(), request)
            .map_err(|e| e.to_string())
    }

    pub fn object_page(
        &self,
        access: &HomeCandidateRecoveryAccess<'_>,
        request: ObjectRequest,
    ) -> Result<ObjectPage, String> {
        self.validate_source(access)?;
        translate::candidate_object_page(&self.storage, access, self.selection.binding(), request)
            .map_err(|e| e.to_string())
    }

    fn validate_source(&self, access: &HomeCandidateRecoveryAccess<'_>) -> Result<(), String> {
        let binding = self.selection.binding();
        if access.home_id() != binding.home_id()
            || access.generation() != binding.home_generation()
            || !self.service.window_close_is_current(self.close)
            || self.service.selected_identity() != Some(self.selection)
        {
            return Err("resident recovery candidate source is stale".into());
        }
        self.selection
            .validate_candidate_claim(access, &self.state)
            .map_err(|e| e.to_string())?;
        if !self
            .storage
            .draft_editor_candidate_is_saved_candidate(access, binding.candidate(), self.selector)
            .map_err(|e| e.to_string())?
        {
            return Err("resident recovery checkpoint changed".into());
        }
        Ok(())
    }

    pub(in crate::main_window) fn into_service(self) -> MainWindowConversationComposerService {
        self.service
    }
}

fn validate_positions(
    access: &HomeCandidateRecoveryAccess<'_>,
    storage: &syndic_storage::SyndicStorage,
    binding: crate::composer_host::ComposerHostBinding,
    seed: RangeRestorationSeed,
) -> Result<(), String> {
    let restoration = syndic_storage::DraftPieceRestorationV1::new(
        binding.root(),
        binding.history(),
        position(seed.caret).map_err(|e| e.to_string())?,
        position(seed.selection.anchor).map_err(|e| e.to_string())?,
        position(seed.scroll.position).map_err(|e| e.to_string())?,
    );
    storage
        .validate_draft_piece_restoration_candidate(access, restoration)
        .map(|_| ())
        .map_err(|e| e.to_string())
}
