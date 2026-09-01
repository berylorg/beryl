use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use gpui_text_input::{
    ObjectRequestKey, PageRequestKey, RangePrepublicationCleanupEffect,
    RangePrepublicationCleanupLedger, RangePrepublicationCleanupToken, RangePrepublicationEffect,
    RangePrepublicationSessionGeneration, RangePrepublicationValidationKey,
    RangePrepublicationValidationResponse,
};

use super::{MainWindowComposerDispatchOutcome, MainWindowComposerSelectionIdentity};

pub(in crate::main_window) enum MainWindowNativeLineagePrepublicationResult {
    Validation(Result<RangePrepublicationValidationResponse, String>),
    Request(Result<MainWindowComposerDispatchOutcome, String>),
}

pub(in crate::main_window) enum MainWindowNativeLineagePrepublicationWork {
    Validation {
        token: RangePrepublicationCleanupToken,
        request: gpui_text_input::RangePrepublicationValidationRequest,
    },
    Page {
        token: RangePrepublicationCleanupToken,
        generation: RangePrepublicationSessionGeneration,
        request: gpui_text_input::PageRequest,
    },
    ObjectPage {
        token: RangePrepublicationCleanupToken,
        generation: RangePrepublicationSessionGeneration,
        request: gpui_text_input::ObjectRequest,
    },
}

pub(in crate::main_window) enum MainWindowNativeLineageCleanupAdvance {
    Pending,
    Acknowledge,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum FlightKey {
    Validation(RangePrepublicationValidationKey),
    Page(RangePrepublicationSessionGeneration, PageRequestKey),
    ObjectPage(RangePrepublicationSessionGeneration, ObjectRequestKey),
}

enum FlightState {
    Pending {
        cancel_requested: bool,
        release_requested: bool,
    },
    Terminal(Option<MainWindowNativeLineagePrepublicationResult>),
    Delivered,
    CancelledTerminal,
    ReleasedTerminal,
}

struct FlightSlot {
    token: RangePrepublicationCleanupToken,
    key: FlightKey,
    state: FlightState,
}

#[cfg(feature = "test-faults")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MainWindowNativeLineagePrepublicationDiagnostics {
    pub sources: usize,
    pub owner_active_sources: usize,
    pub validation_flights: usize,
    pub page_flights: usize,
    pub object_page_flights: usize,
    pub pending_flights: usize,
    pub terminal_flights: usize,
    pub delivered_flights: usize,
    pub cleanup_active: usize,
    pub cleanup_ready: usize,
    pub cleanup_awaiting_acknowledgement: usize,
}

pub(in crate::main_window) struct MainWindowNativeLineagePrepublicationSource {
    selection: MainWindowComposerSelectionIdentity,
    environment_id: u64,
    generation: RangePrepublicationSessionGeneration,
    cleanup: RangePrepublicationCleanupLedger,
    owner_active: AtomicBool,
    flights: Mutex<Box<[Option<FlightSlot>]>>,
    cleanup_effects: Mutex<VecDeque<RangePrepublicationCleanupEffect>>,
}

impl MainWindowNativeLineagePrepublicationSource {
    pub(in crate::main_window) fn new(
        selection: MainWindowComposerSelectionIdentity,
        environment_id: u64,
        generation: RangePrepublicationSessionGeneration,
        cleanup: RangePrepublicationCleanupLedger,
    ) -> Arc<Self> {
        let slots = cleanup.ownership().slots;
        Arc::new(Self {
            selection,
            environment_id,
            generation,
            cleanup,
            owner_active: AtomicBool::new(true),
            flights: Mutex::new(
                std::iter::repeat_with(|| None)
                    .take(slots)
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ),
            cleanup_effects: Mutex::new(VecDeque::with_capacity(slots)),
        })
    }

    pub(in crate::main_window) const fn selection(&self) -> MainWindowComposerSelectionIdentity {
        self.selection
    }

    pub(in crate::main_window) fn release_owner(&self) {
        self.owner_active.store(false, Ordering::Release);
    }

    pub(in crate::main_window) fn begin(
        &self,
        effect: RangePrepublicationEffect,
    ) -> Result<MainWindowNativeLineagePrepublicationWork, String> {
        let (token, key, work) = match effect {
            RangePrepublicationEffect::ValidateOwner(request) => (
                request.cleanup,
                FlightKey::Validation(request.key),
                MainWindowNativeLineagePrepublicationWork::Validation {
                    token: request.cleanup,
                    request,
                },
            ),
            RangePrepublicationEffect::Page {
                cleanup,
                generation,
                request,
            } => (
                cleanup,
                FlightKey::Page(generation, request.key()),
                MainWindowNativeLineagePrepublicationWork::Page {
                    token: cleanup,
                    generation,
                    request,
                },
            ),
            RangePrepublicationEffect::ObjectPage {
                cleanup,
                generation,
                request,
            } => (
                cleanup,
                FlightKey::ObjectPage(generation, request.key()),
                MainWindowNativeLineagePrepublicationWork::ObjectPage {
                    token: cleanup,
                    generation,
                    request,
                },
            ),
        };
        let mut flights = self
            .flights
            .lock()
            .map_err(|_| "composer prepublication source lock failed".to_owned())?;
        if flights.iter().flatten().any(|flight| flight.token == token) {
            return Err("composer prepublication cleanup token was reused".to_owned());
        }
        let slot = flights
            .iter_mut()
            .find(|slot| slot.is_none())
            .ok_or_else(|| "composer prepublication source flight capacity is full".to_owned())?;
        *slot = Some(FlightSlot {
            token,
            key,
            state: FlightState::Pending {
                cancel_requested: false,
                release_requested: false,
            },
        });
        Ok(work)
    }

    pub(in crate::main_window) fn finish(
        &self,
        token: RangePrepublicationCleanupToken,
        result: MainWindowNativeLineagePrepublicationResult,
    ) {
        let Ok(mut flights) = self.flights.lock() else {
            return;
        };
        let Some(flight) = flights
            .iter_mut()
            .flatten()
            .find(|flight| flight.token == token)
        else {
            return;
        };
        let FlightState::Pending {
            cancel_requested,
            release_requested,
        } = flight.state
        else {
            return;
        };
        flight.state = if release_requested {
            drop(result);
            FlightState::ReleasedTerminal
        } else if cancel_requested {
            drop(result);
            FlightState::CancelledTerminal
        } else {
            FlightState::Terminal(Some(result))
        };
    }

    pub(in crate::main_window) fn take(
        &self,
        token: RangePrepublicationCleanupToken,
    ) -> Option<MainWindowNativeLineagePrepublicationResult> {
        let mut flights = self.flights.lock().ok()?;
        let flight = flights
            .iter_mut()
            .flatten()
            .find(|flight| flight.token == token)?;
        let FlightState::Terminal(result) = &mut flight.state else {
            return None;
        };
        let result = result.take()?;
        flight.state = FlightState::Delivered;
        Some(result)
    }

    pub(in crate::main_window) fn cleanup_advance(
        &self,
        effect: RangePrepublicationCleanupEffect,
    ) -> MainWindowNativeLineageCleanupAdvance {
        if let RangePrepublicationCleanupEffect::ReleaseCandidate {
            generation,
            environment_id,
            ..
        } = effect
        {
            return if generation == self.generation && environment_id == self.environment_id {
                MainWindowNativeLineageCleanupAdvance::Acknowledge
            } else {
                MainWindowNativeLineageCleanupAdvance::Pending
            };
        }
        let token = effect.token();
        let expected = match effect {
            RangePrepublicationCleanupEffect::CancelValidation { key, .. }
            | RangePrepublicationCleanupEffect::ReleaseValidation { key, .. } => {
                FlightKey::Validation(key)
            }
            RangePrepublicationCleanupEffect::CancelPage {
                generation, key, ..
            }
            | RangePrepublicationCleanupEffect::ReleasePage {
                generation, key, ..
            } => FlightKey::Page(generation, key),
            RangePrepublicationCleanupEffect::CancelObjectPage {
                generation, key, ..
            }
            | RangePrepublicationCleanupEffect::ReleaseObjectPage {
                generation, key, ..
            } => FlightKey::ObjectPage(generation, key),
            RangePrepublicationCleanupEffect::ReleaseCandidate { .. } => unreachable!(),
        };
        let cancel = matches!(
            effect,
            RangePrepublicationCleanupEffect::CancelValidation { .. }
                | RangePrepublicationCleanupEffect::CancelPage { .. }
                | RangePrepublicationCleanupEffect::CancelObjectPage { .. }
        );
        let validation_cancel = cancel && matches!(expected, FlightKey::Validation(_));
        let Ok(mut flights) = self.flights.lock() else {
            return MainWindowNativeLineageCleanupAdvance::Pending;
        };
        let Some(index) = flights.iter().position(|slot| {
            slot.as_ref()
                .is_some_and(|flight| flight.token == token && flight.key == expected)
        }) else {
            return MainWindowNativeLineageCleanupAdvance::Pending;
        };
        let flight = flights[index].as_mut().expect("matched flight exists");
        match &mut flight.state {
            FlightState::Pending {
                cancel_requested,
                release_requested,
            } => {
                if cancel {
                    *cancel_requested = true;
                } else {
                    *release_requested = true;
                }
                MainWindowNativeLineageCleanupAdvance::Pending
            }
            FlightState::Terminal(result) => {
                drop(result.take());
                if validation_cancel {
                    flights[index] = None;
                } else if cancel {
                    flight.state = FlightState::CancelledTerminal;
                } else {
                    flights[index] = None;
                }
                MainWindowNativeLineageCleanupAdvance::Acknowledge
            }
            FlightState::Delivered => {
                if validation_cancel {
                    flights[index] = None;
                } else if cancel {
                    flight.state = FlightState::CancelledTerminal;
                } else {
                    flights[index] = None;
                }
                MainWindowNativeLineageCleanupAdvance::Acknowledge
            }
            FlightState::CancelledTerminal => {
                if validation_cancel || !cancel {
                    flights[index] = None;
                }
                MainWindowNativeLineageCleanupAdvance::Acknowledge
            }
            FlightState::ReleasedTerminal => {
                flights[index] = None;
                MainWindowNativeLineageCleanupAdvance::Acknowledge
            }
        }
    }

    pub(in crate::main_window) fn drained(&self) -> bool {
        if self.owner_active.load(Ordering::Acquire) {
            return false;
        }
        if !self
            .flights
            .lock()
            .map(|flights| flights.iter().all(Option::is_none))
            .unwrap_or(false)
        {
            return false;
        }
        let ownership = self.cleanup.ownership();
        ownership.active == 0 && ownership.ready == 0 && ownership.awaiting_acknowledgement == 0
    }

    pub(in crate::main_window) fn drive_cleanup(&self, limit: usize) {
        let Ok(mut effects) = self.cleanup_effects.lock() else {
            return;
        };
        if effects.is_empty() {
            effects.extend(self.cleanup.service(limit).effects);
        }
        let count = effects.len();
        for _ in 0..count {
            let Some(effect) = effects.pop_front() else {
                break;
            };
            let token = effect.token();
            match self.cleanup_advance(effect) {
                MainWindowNativeLineageCleanupAdvance::Pending => effects.push_back(effect),
                MainWindowNativeLineageCleanupAdvance::Acknowledge => {
                    let _ = self.cleanup.acknowledge(token);
                }
            }
        }
        drop(effects);
        self.clear_settled_terminal_flights();
    }

    fn clear_settled_terminal_flights(&self) {
        let ownership = self.cleanup.ownership();
        if ownership.active != 0 || ownership.ready != 0 || ownership.awaiting_acknowledgement != 0
        {
            return;
        }
        if let Ok(mut flights) = self.flights.lock() {
            for slot in flights.iter_mut() {
                if slot.as_ref().is_some_and(|flight| {
                    (matches!(flight.key, FlightKey::Validation(_))
                        && matches!(flight.state, FlightState::Delivered))
                        || matches!(flight.state, FlightState::CancelledTerminal)
                }) {
                    *slot = None;
                }
            }
        }
    }

    #[cfg(feature = "test-faults")]
    pub(in crate::main_window) fn diagnostics(
        &self,
    ) -> MainWindowNativeLineagePrepublicationDiagnostics {
        let ownership = self.cleanup.ownership();
        let mut diagnostics = MainWindowNativeLineagePrepublicationDiagnostics {
            sources: 1,
            owner_active_sources: usize::from(self.owner_active.load(Ordering::Acquire)),
            cleanup_active: ownership.active,
            cleanup_ready: ownership.ready,
            cleanup_awaiting_acknowledgement: ownership.awaiting_acknowledgement,
            ..Default::default()
        };
        if let Ok(flights) = self.flights.lock() {
            for flight in flights.iter().flatten() {
                match flight.key {
                    FlightKey::Validation(_) => diagnostics.validation_flights += 1,
                    FlightKey::Page(_, _) => diagnostics.page_flights += 1,
                    FlightKey::ObjectPage(_, _) => diagnostics.object_page_flights += 1,
                }
                match flight.state {
                    FlightState::Pending { .. } => diagnostics.pending_flights += 1,
                    FlightState::Terminal(_)
                    | FlightState::CancelledTerminal
                    | FlightState::ReleasedTerminal => diagnostics.terminal_flights += 1,
                    FlightState::Delivered => diagnostics.delivered_flights += 1,
                }
            }
        }
        diagnostics
    }
}
