use crate::{CurrentDomainCommand, HomeCommand};

#[derive(Clone, Copy)]
pub(super) struct CommandFaultContext {
    #[cfg(feature = "test-faults")]
    pub(super) scope: Option<crate::fault::FaultScope>,
}

impl CommandFaultContext {
    pub(super) const fn ordinary(command: &HomeCommand) -> Self {
        #[cfg(not(feature = "test-faults"))]
        let _ = command;
        Self {
            #[cfg(feature = "test-faults")]
            scope: command.test_fault_scope,
        }
    }

    #[cfg(feature = "test-faults")]
    pub(super) const fn current(command: &CurrentDomainCommand) -> Self {
        Self {
            scope: Some(command.fault_scope),
        }
    }

    #[cfg(not(feature = "test-faults"))]
    pub(super) const fn current(_command: &CurrentDomainCommand) -> Self {
        Self {}
    }
}
