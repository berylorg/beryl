use beryl_home_store::{CommandCancellation, HomeGeneration, HomeServiceReference};
use beryl_model::{BerylHomeId, WindowId, WindowPlacement};
use beryl_state::{BerylState, MinimalSessionBootstrap, RememberedTarget};

mod command;
mod probe;

use command::{CommandFlight, ExpectedSuccessor};
use probe::DiscoverySnapshot;

pub(super) enum DiscoveredRestoreSet {
    Restored(MinimalSessionBootstrap),
    Threadless(MinimalSessionBootstrap),
    Replacement {
        session: MinimalSessionBootstrap,
        target: RememberedTarget,
    },
}

pub(super) struct RestoreSetDiscovery {
    initial_window: WindowId,
    initial_placement: WindowPlacement,
    identity: Option<(BerylHomeId, HomeGeneration)>,
    commands: Vec<CommandFlight>,
    delivered: bool,
    stopped: bool,
    #[cfg(feature = "test-faults")]
    before_command: Option<Box<dyn FnOnce() + Send>>,
}

impl RestoreSetDiscovery {
    pub(super) fn new(initial_window: WindowId, initial_placement: WindowPlacement) -> Self {
        Self {
            initial_window,
            initial_placement,
            identity: None,
            commands: Vec::with_capacity(2),
            delivered: false,
            stopped: false,
            #[cfg(feature = "test-faults")]
            before_command: None,
        }
    }

    #[cfg(feature = "test-faults")]
    pub(super) fn test_arm_before_command(&mut self, hook: impl FnOnce() + Send + 'static) {
        self.before_command = Some(Box::new(hook));
    }

    pub(super) fn advance(
        &mut self,
        store: &HomeServiceReference,
        state: &BerylState,
        cancellation: &CommandCancellation,
    ) -> Result<Option<DiscoveredRestoreSet>, String> {
        self.validate_identity(store)?;
        if self.delivered || self.stopped {
            return Err("restore discovery has already completed or stopped".to_owned());
        }
        if cancellation.is_cancelled() {
            if !self.settle(store)? {
                return Ok(None);
            }
            self.stopped = true;
            return Err("restore discovery cancelled".to_owned());
        }
        if !self.settle(store)? {
            return Ok(None);
        }
        if self
            .commands
            .last()
            .is_some_and(|command| !command.committed())
        {
            self.stopped = true;
            return Err("restore discovery command did not commit".to_owned());
        }
        let snapshot = DiscoverySnapshot::read(store, state)?;
        if let Some(previous) = self.commands.last() {
            previous.expected.validate(&snapshot)?;
        }
        if self.commands.is_empty()
            || snapshot
                .session
                .as_ref()
                .is_none_or(|s| s.windows().is_empty())
                && snapshot.no_runtimes
        {
            let expected = if self.commands.is_empty() && snapshot.session.is_some() {
                ExpectedSuccessor::Restore(snapshot)
            } else {
                if !snapshot.no_runtimes {
                    return Err("runtime registry has no session fallback".to_owned());
                }
                ExpectedSuccessor::Initialize {
                    before: snapshot,
                    window: self.initial_window,
                    placement: self.initial_placement.clone(),
                }
            };
            let command = expected.command(state, cancellation)?;
            #[cfg(feature = "test-faults")]
            if let Some(hook) = self.before_command.take() {
                hook();
            }
            self.commands
                .push(CommandFlight::execute(store, command, expected));
            return Ok(None);
        }
        let session = snapshot
            .session
            .ok_or_else(|| "restored session is missing".to_owned())?;
        let result = if snapshot.no_runtimes {
            DiscoveredRestoreSet::Threadless(session)
        } else if session.windows().is_empty() {
            let target = session
                .header()
                .fallback()
                .ok_or_else(|| "empty session has no runtime fallback".to_owned())?;
            DiscoveredRestoreSet::Replacement { session, target }
        } else {
            DiscoveredRestoreSet::Restored(session)
        };
        self.delivered = true;
        Ok(Some(result))
    }

    pub(super) fn settle(&mut self, store: &HomeServiceReference) -> Result<bool, String> {
        for command in &mut self.commands {
            if !command.settle(store)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn retained_command(&self) -> Option<super::RestoreSetRetainedCommand<'_>> {
        self.commands
            .iter()
            .find_map(CommandFlight::retained_command)
    }

    fn validate_identity(&mut self, store: &HomeServiceReference) -> Result<(), String> {
        let identity = (
            store.home_id(),
            store
                .health()
                .generation()
                .ok_or_else(|| "restore home generation is retired".to_owned())?,
        );
        if self.identity.is_some_and(|original| original != identity) {
            return Err("restore discovery belongs to another home generation".to_owned());
        }
        self.identity = Some(identity);
        Ok(())
    }
}
