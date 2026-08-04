use crate::CurrentDomainCommand;

#[derive(Clone, Copy)]
pub(super) struct CommandFaultContext {
    #[cfg(feature = "test-faults")]
    pub(super) scope: Option<crate::fault::FaultScope>,
}

impl CommandFaultContext {
    pub(super) const fn unscoped() -> Self {
        Self {
            #[cfg(feature = "test-faults")]
            scope: None,
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
