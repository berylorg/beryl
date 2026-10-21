use super::CommandError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandParticipantKind {
    Mutation,
    Validation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandPairRequirement {
    protocol: u64,
    operation: u64,
    peer_domain: &'static str,
    peer_kind: CommandParticipantKind,
    correlation: [u8; 32],
}

impl CommandPairRequirement {
    pub const fn new<const PROTOCOL_ID: u64, const OPERATION_ID: u64>(
        peer_domain: &'static str,
        peer_kind: CommandParticipantKind,
        correlation: [u8; 32],
    ) -> Self {
        Self {
            protocol: PROTOCOL_ID,
            operation: OPERATION_ID,
            peer_domain,
            peer_kind,
            correlation,
        }
    }

    fn valid(self) -> bool {
        self.protocol != 0
            && self.operation != 0
            && !self.peer_domain.is_empty()
            && self.peer_domain.len() <= 255
            && self.peer_domain.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte)
            })
    }
}

pub(crate) fn validate_command_pair(
    participants: impl IntoIterator<
        Item = (
            &'static str,
            CommandParticipantKind,
            Option<CommandPairRequirement>,
        ),
    >,
) -> Result<(), CommandError> {
    let mut first_two = [None, None];
    let mut count = 0usize;
    let mut bound = false;
    for participant in participants {
        if count < first_two.len() {
            first_two[count] = Some(participant);
        }
        count = count.saturating_add(1);
        bound |= participant.2.is_some();
    }
    if !bound {
        return Ok(());
    }
    if count != 2 {
        return Err(CommandError::InvalidCommandPair);
    }
    let [
        Some((left_domain, left_kind, Some(left))),
        Some((right_domain, right_kind, Some(right))),
    ] = first_two
    else {
        return Err(CommandError::InvalidCommandPair);
    };
    if left_domain == right_domain
        || !left.valid()
        || !right.valid()
        || left.peer_domain != right_domain
        || right.peer_domain != left_domain
        || left.peer_kind != right_kind
        || right.peer_kind != left_kind
        || left.protocol != right.protocol
        || left.operation != right.operation
        || left.correlation != right.correlation
    {
        return Err(CommandError::InvalidCommandPair);
    }
    Ok(())
}
