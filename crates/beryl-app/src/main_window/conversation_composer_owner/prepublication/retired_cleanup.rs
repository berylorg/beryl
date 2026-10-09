use super::*;
use gpui_text_input::RangePrepublicationCleanupAcknowledgement;

struct DeliveredCleanupFlight {
    token: RangePrepublicationCleanupToken,
    key: FlightKey,
}

pub(crate) struct MainWindowRetiredPrepublicationCleanup {
    selection: MainWindowComposerSelectionIdentity,
    environment_id: u64,
    generation: RangePrepublicationSessionGeneration,
    cleanup: RangePrepublicationCleanupLedger,
    flights: Box<[Option<DeliveredCleanupFlight>]>,
    effects: VecDeque<RangePrepublicationCleanupEffect>,
    #[cfg(test)]
    accepted_page_releases: usize,
}

impl MainWindowNativeLineagePrepublicationSource {
    pub(in crate::main_window::conversation_composer_owner) fn qualify_cleanup_handoff(
        &self,
        expected: MainWindowComposerSelectionIdentity,
    ) -> Result<(), String> {
        let current = self.selection;
        let current_binding = current.binding();
        let expected_binding = expected.binding();
        if current.window_id() != expected.window_id()
            || current.claim() != expected.claim()
            || current_binding.home_id() != expected_binding.home_id()
            || current_binding.home_generation() != expected_binding.home_generation()
            || current_binding.host_generation() != expected_binding.host_generation()
            || current_binding.candidate().draft_id() != expected_binding.candidate().draft_id()
            || current_binding.candidate().session_id() != expected_binding.candidate().session_id()
            || current_binding.candidate().session_generation()
                != expected_binding.candidate().session_generation()
            || current_binding.presentation_generation()
                != expected_binding.presentation_generation()
            || self.environment_id == 0
            || self.generation.get() == 0
            || self.owner_active.load(Ordering::Acquire)
        {
            return Err("prepublication cleanup original owner changed or remains active".into());
        }
        let flights = self
            .flights
            .lock()
            .map_err(|_| "prepublication cleanup flight custody is poisoned")?;
        let effects = self
            .cleanup_effects
            .lock()
            .map_err(|_| "prepublication cleanup effect custody is poisoned")?;
        let ownership = self.cleanup.ownership();
        if ownership.slots != flights.len() || effects.len() > flights.len() {
            return Err("prepublication cleanup bounds changed".into());
        }
        let binding = current_binding.range_binding();
        for (index, flight) in flights.iter().enumerate() {
            let Some(flight) = flight else { continue };
            let exact = match flight.key {
                FlightKey::Validation(key) => key.generation == self.generation && key.request != 0,
                FlightKey::Page(generation, key) => {
                    generation == self.generation
                        && key.binding() == binding.binding()
                        && key.revision() == binding.revision()
                }
                FlightKey::ObjectPage(generation, key) => {
                    generation == self.generation
                        && key.binding() == binding.binding()
                        && key.revision() == binding.revision()
                        && key.presentation_generation().get()
                            == current_binding.presentation_generation().get()
                }
            };
            if !exact
                || !matches!(flight.state, FlightState::Delivered)
                || flight.token.id() == 0
                || flights[..index]
                    .iter()
                    .flatten()
                    .any(|previous| previous.token == flight.token || previous.key == flight.key)
            {
                return Err(
                    "prepublication cleanup has pending, undelivered or foreign flight custody"
                        .into(),
                );
            }
        }
        for effect in effects.iter() {
            if !effect_matches(
                *effect,
                self.environment_id,
                self.generation,
                |token, key| {
                    flights
                        .iter()
                        .flatten()
                        .any(|f| f.token == token && f.key == key)
                },
            ) {
                return Err("prepublication cleanup retained effect changed".into());
            }
        }
        Ok(())
    }

    pub(in crate::main_window::conversation_composer_owner) fn take_qualified_cleanup(
        &mut self,
    ) -> MainWindowRetiredPrepublicationCleanup {
        let flights = self.flights.get_mut().expect("qualified flight custody");
        let flights = std::mem::take(flights)
            .into_vec()
            .into_iter()
            .map(|flight| {
                flight.map(|flight| DeliveredCleanupFlight {
                    token: flight.token,
                    key: flight.key,
                })
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        MainWindowRetiredPrepublicationCleanup {
            selection: self.selection,
            environment_id: self.environment_id,
            generation: self.generation,
            cleanup: self.cleanup.clone(),
            flights,
            effects: std::mem::take(
                self.cleanup_effects
                    .get_mut()
                    .expect("qualified cleanup effects"),
            ),
            #[cfg(test)]
            accepted_page_releases: 0,
        }
    }
}

impl MainWindowRetiredPrepublicationCleanup {
    #[cfg(test)]
    pub(crate) fn test_page_release_evidence(
        &self,
    ) -> (MainWindowComposerSelectionIdentity, u64, u64, usize) {
        (
            self.selection,
            self.environment_id,
            self.generation.get(),
            self.accepted_page_releases,
        )
    }

    #[cfg(test)]
    pub(crate) fn accepted_page_release_acknowledgements(&self) -> usize {
        self.accepted_page_releases
    }

    pub(crate) fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }

    pub(crate) fn advance(&mut self, limit: usize) -> Result<bool, String> {
        if self.effects.is_empty() {
            self.effects
                .extend(self.cleanup.service(limit.min(self.flights.len())).effects);
        }
        for _ in 0..limit.min(self.effects.len()) {
            let effect = self.effects.pop_front().unwrap();
            if !effect_matches(
                effect,
                self.environment_id,
                self.generation,
                |token, key| {
                    self.flights
                        .iter()
                        .flatten()
                        .any(|f| f.token == token && f.key == key)
                },
            ) {
                self.effects.push_front(effect);
                return Err(
                    "retired prepublication cleanup effect does not belong to original custody"
                        .into(),
                );
            }
            if self.cleanup.acknowledge(effect.token())
                != RangePrepublicationCleanupAcknowledgement::Accepted
            {
                self.effects.push_front(effect);
                return Err("retired prepublication cleanup acknowledgement changed".into());
            }
            #[cfg(test)]
            if matches!(effect, RangePrepublicationCleanupEffect::ReleasePage { .. }) {
                self.accepted_page_releases += 1;
            }
            if !matches!(
                effect,
                RangePrepublicationCleanupEffect::CancelPage { .. }
                    | RangePrepublicationCleanupEffect::CancelObjectPage { .. }
            ) {
                for slot in &mut self.flights {
                    if slot
                        .as_ref()
                        .is_some_and(|flight| flight.token == effect.token())
                    {
                        *slot = None;
                    }
                }
            }
        }
        let ownership = self.cleanup.ownership();
        if ownership.active != 0
            || ownership.ready != 0
            || ownership.awaiting_acknowledgement != 0
            || !self.effects.is_empty()
        {
            return Ok(false);
        }
        for slot in &mut self.flights {
            if slot
                .as_ref()
                .is_some_and(|flight| matches!(flight.key, FlightKey::Validation(_)))
            {
                *slot = None;
            }
        }
        Ok(self.flights.iter().all(Option::is_none))
    }
}

fn effect_matches(
    effect: RangePrepublicationCleanupEffect,
    environment_id: u64,
    generation: RangePrepublicationSessionGeneration,
    matches: impl FnOnce(RangePrepublicationCleanupToken, FlightKey) -> bool,
) -> bool {
    match effect {
        RangePrepublicationCleanupEffect::CancelValidation { token, key }
        | RangePrepublicationCleanupEffect::ReleaseValidation { token, key } => {
            key.generation == generation && matches(token, FlightKey::Validation(key))
        }
        RangePrepublicationCleanupEffect::CancelPage {
            token,
            generation: actual,
            key,
        }
        | RangePrepublicationCleanupEffect::ReleasePage {
            token,
            generation: actual,
            key,
        } => actual == generation && matches(token, FlightKey::Page(actual, key)),
        RangePrepublicationCleanupEffect::CancelObjectPage {
            token,
            generation: actual,
            key,
        }
        | RangePrepublicationCleanupEffect::ReleaseObjectPage {
            token,
            generation: actual,
            key,
        } => actual == generation && matches(token, FlightKey::ObjectPage(actual, key)),
        RangePrepublicationCleanupEffect::ReleaseCandidate {
            generation: actual,
            environment_id: actual_environment,
            ..
        } => actual == generation && actual_environment == environment_id,
    }
}
