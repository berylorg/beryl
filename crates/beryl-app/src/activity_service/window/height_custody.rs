use super::*;
use std::collections::BTreeMap;

type Outcome = Result<ActivityHeightCommit, ActivityHeightError>;

#[derive(Default)]
pub(crate) struct ActivityHeightCustody(Mutex<BTreeMap<WindowId, Arc<ActivityHeightFlight>>>);

pub(crate) struct ActivityHeightFlight {
    generation: beryl_home_store::HomeGenerationIdentity,
    result: Mutex<Option<Outcome>>,
}

impl ActivityHeightFlight {
    pub(crate) fn generation(&self) -> beryl_home_store::HomeGenerationIdentity {
        self.generation
    }
    fn pending(&self) -> bool {
        self.result
            .try_lock()
            .map(|result| result.is_none())
            .unwrap_or(true)
    }
    pub(crate) fn with_outcome<T>(&self, read: impl FnOnce(&Outcome) -> T) -> Option<T> {
        self.result.try_lock().ok()?.as_ref().map(read)
    }
    pub(crate) fn blocks_replacement(&self) -> bool {
        self.with_outcome(|outcome| {
            matches!(
                outcome,
                Ok(ActivityHeightCommit::Indeterminate { .. })
                    | Ok(ActivityHeightCommit::Committed {
                        local_finalization: Some(_),
                        ..
                    })
            )
        })
        .unwrap_or(true)
    }
}

impl ActivityWindowSelection {
    pub(crate) fn save_height(
        &self,
        height: beryl_state::ActivityPanelHeight,
    ) -> Result<Arc<ActivityHeightFlight>, ActivityHeightError> {
        self.save_height_from_input(height, None)
    }

    pub(crate) fn save_height_from_input(
        &self,
        height: beryl_state::ActivityPanelHeight,
        input: Option<&ActivityInteractionGate>,
    ) -> Result<Arc<ActivityHeightFlight>, ActivityHeightError> {
        let operation = self.begin()?;
        let generation = operation.resources().home.generation_identity()?;
        let flight = {
            let mut records = self
                .reader
                .heights
                .0
                .lock()
                .map_err(|_| ActivityReadError::Unavailable)?;
            records.retain(|window, flight| {
                (flight.generation == generation || flight.pending())
                    && (*window == self.window || flight.blocks_replacement())
            });
            if records
                .get(&self.window)
                .is_some_and(|flight| flight.blocks_replacement())
            {
                return Err(ActivityReadError::RequestPending.into());
            }
            if !records.contains_key(&self.window)
                && records.len() >= beryl_state::MAX_RESTORABLE_WINDOWS
            {
                return Err(ActivityReadError::RequestPending.into());
            }
            let flight = Arc::new(ActivityHeightFlight {
                generation,
                result: Mutex::new(None),
            });
            records.insert(self.window, flight.clone());
            flight
        };
        let outcome = self.commit_height_elected(height, input);
        *flight
            .result
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(outcome);
        Ok(flight)
    }
}

#[cfg(feature = "test-faults")]
impl ActivityWindowReader {
    pub(crate) fn height_flight(&self, window: WindowId) -> Option<Arc<ActivityHeightFlight>> {
        self.heights.0.lock().unwrap().get(&window).cloned()
    }
}
