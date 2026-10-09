use super::*;

pub struct MainWindowFailedResidentAdoption {
    pub(super) ticket: MainWindowFailedResidentTicket,
    pub(super) protection: RangeResidentProtection,
    pub(super) retained: MainWindowFailedComposerRetirement,
    pub(super) capture: Box<MainWindowFailedResidentCapture>,
}

impl MainWindowFailedResidentAdoption {
    pub fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.ticket.selection
    }
    pub fn protection(&self) -> RangeResidentProtection {
        self.protection
    }
    pub fn original_known_commit(&self) -> Option<bool> {
        self.retained.original_known_commit()
    }
    pub fn recovery_known_commit(&self) -> Option<bool> {
        self.retained.recovery_known_commit()
    }
}

pub(crate) struct MainWindowFailedResidentAdoptionReturn {
    pub(in crate::main_window) retained: Box<MainWindowFailedComposerRetirement>,
    pub(in crate::main_window) capture: Box<MainWindowFailedResidentCapture>,
    pub(in crate::main_window) service: Arc<MainWindowConversationComposerService>,
    pub(in crate::main_window) selection: MainWindowComposerSelectionIdentity,
}

pub(in crate::main_window) struct MainWindowFailedResidentAdoptionReturnAuthority {
    service: Arc<MainWindowConversationComposerService>,
    protection: RangeResidentProtection,
}

impl MainWindowConversationComposer {
    pub(in crate::main_window) fn authenticate_adoption_return(
        &self,
        adoption: &MainWindowFailedResidentAdoption,
        close: crate::main_window::MainWindowConversationComposerCloseTicket,
        cx: &App,
    ) -> Result<Option<MainWindowFailedResidentAdoptionReturnAuthority>, String> {
        adoption
            .retained
            .qualify_adopted_return(adoption.selection())?;
        let current_protection = self
            .unpublished_recovery_protection
            .filter(|(current, protection)| {
                *current == close && protection.seed() == adoption.protection.seed()
            })
            .map(|(_, protection)| protection);
        if !self.recovery_binding_current(close)
            || self.selection != adoption.selection()
            || adoption.capture.ticket.owner != adoption.ticket.owner
            || adoption.capture.ticket.generation != adoption.ticket.generation
            || adoption.capture.selection != adoption.retained.selection()
            || self
                .failed_resident
                .as_ref()
                .is_none_or(|fence| fence.ticket != adoption.ticket || !fence.capture_taken)
            || current_protection.is_none_or(|protection| {
                !self
                    .input
                    .read(cx)
                    .resident_protection_is_current(protection)
            })
            || self.bound_service()?.selected_identity() != Some(self.selection)
        {
            #[cfg(test)]
            {
                let checks = [
                    ("recovery_binding", self.recovery_binding_current(close)),
                    (
                        "phase",
                        matches!(
                            self.phase,
                            MainWindowConversationComposerPhase::RecoveryFenced
                        ),
                    ),
                    ("window_close", self.window_close == Some(close)),
                    ("no_recovery_snapshot", self.recovery_snapshot.is_none()),
                    ("selection", self.selection == adoption.selection()),
                    (
                        "capture_owner",
                        adoption.capture.ticket.owner == adoption.ticket.owner,
                    ),
                    (
                        "capture_generation",
                        adoption.capture.ticket.generation == adoption.ticket.generation,
                    ),
                    (
                        "original_selection",
                        adoption.capture.selection == adoption.retained.selection(),
                    ),
                    (
                        "failed_fence",
                        self.failed_resident.as_ref().is_some_and(|fence| {
                            fence.ticket == adoption.ticket && fence.capture_taken
                        }),
                    ),
                    (
                        "current_protection",
                        current_protection.is_some_and(|protection| {
                            self.input
                                .read(cx)
                                .resident_protection_is_current(protection)
                        }),
                    ),
                    (
                        "current_protection_seed",
                        self.unpublished_recovery_protection.is_some_and(
                            |(current, protection)| {
                                current == close && protection.seed() == adoption.protection.seed()
                            },
                        ),
                    ),
                    (
                        "service_selection",
                        self.bound_service()?.selected_identity() == Some(self.selection),
                    ),
                ];
                let failed = checks
                    .iter()
                    .filter_map(|(name, accepted)| (!accepted).then_some(*name))
                    .collect::<Vec<_>>();
                return Err(format!(
                    "adopted resident original return identity changed: failed={failed:?}"
                ));
            }
            #[cfg(not(test))]
            return Err("adopted resident original return identity changed".into());
        }
        if !self.failed_editor_drained(cx) {
            return Ok(None);
        }
        Ok(Some(MainWindowFailedResidentAdoptionReturnAuthority {
            service: self.bound_service()?.clone(),
            protection: current_protection.unwrap(),
        }))
    }
}

impl MainWindowFailedResidentAdoption {
    pub(in crate::main_window) fn into_return(
        self: Box<Self>,
        authority: MainWindowFailedResidentAdoptionReturnAuthority,
    ) -> Box<MainWindowFailedResidentAdoptionReturn> {
        let Self {
            ticket,
            protection: _,
            retained,
            mut capture,
        } = *self;
        capture.current_ticket = Some(ticket);
        capture.protection = authority.protection;
        Box::new(MainWindowFailedResidentAdoptionReturn {
            retained: Box::new(retained),
            capture,
            service: authority.service,
            selection: ticket.selection,
        })
    }
}

impl MainWindowFailedResidentAdoptionReturn {
    pub(crate) fn window_id(&self) -> beryl_model::WindowId {
        self.selection.window_id()
    }

    pub(crate) fn original_restoration(&self) -> RangeRestorationSeed {
        self.capture.restoration()
    }

    pub(crate) fn authenticate_source(
        &self,
        candidate: &mut HomeRecoveryCandidate,
        storage: &syndic_storage::SyndicStorage,
        state: &beryl_state::BerylState,
    ) -> Result<
        (
            beryl_state::SessionWindowRecord,
            syndic_storage::DraftEditorCurrentSelectorV1,
        ),
        String,
    > {
        MainWindowFailedResidentCandidateSource::authenticate_adopted_return(
            candidate,
            &self.retained,
            &self.service,
            self.selection,
            storage,
            state,
            self.capture.restoration(),
        )
    }

    #[inline(never)]
    pub(crate) fn into_source(
        self: Box<Self>,
        storage: syndic_storage::SyndicStorage,
        state: beryl_state::BerylState,
        window: beryl_state::SessionWindowRecord,
        selector: syndic_storage::DraftEditorCurrentSelectorV1,
    ) -> (
        Box<MainWindowFailedResidentCandidateSource>,
        Box<MainWindowFailedResidentCapture>,
    ) {
        let Self {
            retained,
            capture,
            service,
            selection,
        } = *self;
        let source = MainWindowFailedResidentCandidateSource::from_adopted(
            retained,
            service,
            selection,
            storage,
            state,
            capture.restoration(),
            window,
            selector,
        );
        (source, capture)
    }
}
