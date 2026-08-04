use beryl_backend::{
    ThreadInjectionRole, ThreadInjectionSourceError, ThreadInjectionSourceIdentity,
    ThreadInjectionSourcePage, ThreadInjectionSourceRevision,
};
use beryl_model::{BerylHomeId, RecoveryItemSequenceRole, SyndicTurnId};
use sha2::{Digest, Sha256};
use syndic_storage::{
    RecoveryCursorPage, RecoveryProjection, RecoveryProjectionError, RecoveryProjectionVersion,
};

pub(super) fn map_recovery_page(
    page: RecoveryCursorPage,
    source_identity: ThreadInjectionSourceIdentity,
    source_revision: ThreadInjectionSourceRevision,
    max_utf8_bytes: usize,
) -> Result<ThreadInjectionSourcePage, ThreadInjectionSourceError> {
    if max_utf8_bytes == 0 {
        return Err(ThreadInjectionSourceError::ZeroPageRequest);
    }
    if page.text().len() > max_utf8_bytes {
        return Err(ThreadInjectionSourceError::PageTooLarge {
            maximum: max_utf8_bytes,
            actual: page.text().len(),
        });
    }
    let ordinal = page.sequence_ordinal();
    let role = match page.role() {
        RecoveryItemSequenceRole::UserInputText => ThreadInjectionRole::UserInputText,
        RecoveryItemSequenceRole::AssistantOutputText => ThreadInjectionRole::AssistantOutputText,
    };
    let declared_utf8_bytes = page.declared_item_utf8_bytes();
    let offset = page.item_offset();
    let item_terminal = page.item_terminal();
    let sequence_terminal = page.sequence_terminal();
    let page_lease = page.into_page_lease();
    ThreadInjectionSourcePage::new(
        source_identity,
        source_revision,
        ordinal,
        role,
        declared_utf8_bytes,
        offset,
        page_lease,
        item_terminal,
        sequence_terminal,
    )
}

pub(super) fn map_recovery_cursor_error(
    error: RecoveryProjectionError,
) -> ThreadInjectionSourceError {
    match error {
        RecoveryProjectionError::ConcurrentChange => ThreadInjectionSourceError::RevisionDrift,
        RecoveryProjectionError::Read(_) => ThreadInjectionSourceError::ReadFailed,
        RecoveryProjectionError::MissingModelContextWindow
        | RecoveryProjectionError::ZeroModelContextWindow
        | RecoveryProjectionError::StaleSelectedPath
        | RecoveryProjectionError::CurrentTailNotPendingOrdinaryUser
        | RecoveryProjectionError::MissingHistory { .. }
        | RecoveryProjectionError::IncompleteHistory { .. }
        | RecoveryProjectionError::UnsupportedHistory { .. }
        | RecoveryProjectionError::MediaHistory { .. }
        | RecoveryProjectionError::EmptyHistoryItem
        | RecoveryProjectionError::BudgetOverflow { .. }
        | RecoveryProjectionError::InvalidCursorPageLimit { .. }
        | RecoveryProjectionError::CursorPageLimitTooSmall { .. }
        | RecoveryProjectionError::CursorTerminal
        | RecoveryProjectionError::CursorMismatch { .. }
        | RecoveryProjectionError::Invariant(_) => ThreadInjectionSourceError::InvalidSource,
    }
}

pub(super) fn recovery_source_identity(
    home_id: BerylHomeId,
    projection: RecoveryProjection,
) -> ThreadInjectionSourceIdentity {
    let mut digest = Sha256::new();
    digest.update(b"beryl-recovery-source-v1\0");
    digest.update([match projection.version() {
        RecoveryProjectionVersion::V1 => 1,
    }]);
    digest.update(home_id.as_bytes());
    digest.update(projection.thread_id().as_bytes());

    let selected_path = projection.selected_path();
    update_optional_turn(&mut digest, selected_path.tail());
    digest.update(selected_path.thread_revision().get().to_be_bytes());
    digest.update(selected_path.digest().as_bytes());

    let represented_prefix = projection.represented_prefix();
    update_optional_turn(&mut digest, represented_prefix.tail());
    digest.update(
        represented_prefix
            .source_thread_revision()
            .get()
            .to_be_bytes(),
    );
    digest.update(represented_prefix.digest().as_bytes());
    digest.update(projection.source_revision().get().to_be_bytes());
    digest.update(u64::from(projection.item_count().get()).to_be_bytes());
    digest.update(projection.utf8_bytes().get().to_be_bytes());
    digest.update(projection.sequence_digest().as_bytes());
    ThreadInjectionSourceIdentity::new(digest.finalize().into())
}

fn update_optional_turn(digest: &mut Sha256, turn: Option<SyndicTurnId>) {
    match turn {
        Some(turn) => {
            digest.update([1]);
            digest.update(turn.as_bytes());
        }
        None => digest.update([0]),
    }
}
