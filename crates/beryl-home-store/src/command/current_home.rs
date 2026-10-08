use super::{CommandBuildError, CommandCancellation, CurrentDomainCommand};

pub struct CurrentHomeCommand {
    pub(crate) commands: Vec<CurrentDomainCommand>,
    pub(crate) cancellation: CommandCancellation,
}

impl std::fmt::Debug for CurrentHomeCommand {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CurrentHomeCommand")
            .field("participants", &self.commands)
            .finish_non_exhaustive()
    }
}

impl CurrentHomeCommand {
    #[must_use]
    pub fn new(primary: CurrentDomainCommand) -> Self {
        Self {
            commands: vec![primary],
            cancellation: CommandCancellation::new(),
        }
    }

    pub fn add(&mut self, command: CurrentDomainCommand) -> Result<&mut Self, CommandBuildError> {
        if self.commands.iter().any(|existing| {
            existing.plan.store == command.plan.store && existing.plan.slot == command.plan.slot
        }) {
            return Err(CommandBuildError::DuplicateDomain {
                domain: command.plan.domain,
            });
        }
        self.commands.push(command);
        Ok(self)
    }

    #[must_use]
    pub fn with_cancellation(mut self, cancellation: CommandCancellation) -> Self {
        self.cancellation = cancellation;
        self
    }

    pub(crate) fn cancellation_signals(&self) -> Vec<CommandCancellation> {
        std::iter::once(self.cancellation.clone())
            .chain(
                self.commands
                    .iter()
                    .map(|command| command.cancellation.clone()),
            )
            .collect()
    }
}
