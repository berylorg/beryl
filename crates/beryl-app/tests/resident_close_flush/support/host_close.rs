use super::super::composer;
use super::Host;
use beryl_app::composer_host::{
    ComposerHostFlushAdmission, ComposerHostFlushAdvance, ComposerHostFlushCapture,
    ComposerHostFlushPurpose, ComposerHostFlushState, ComposerHostFlushTicket,
    ComposerHostPublicationTicket,
};
use beryl_home_store::CommandCancellation;
use syndic_storage::SyndicTimestamp;

pub fn begin_close(fixture: &mut Host) -> ComposerHostFlushTicket {
    match fixture
        .host
        .begin_flush(ComposerHostFlushPurpose::WindowClose)
        .unwrap()
    {
        ComposerHostFlushAdmission::Started { ticket, .. } => ticket,
        other => panic!("expected new close attempt: {other:?}"),
    }
}

pub fn capture(
    fixture: &mut Host,
    close: ComposerHostFlushTicket,
    operation: u64,
) -> ComposerHostPublicationTicket {
    match fixture
        .host
        .capture_flush_publication(
            &fixture.store,
            close,
            fixture.assets.clone(),
            &fixture.seals,
            composer::operation_id(operation),
            None,
            SyndicTimestamp::from_unix_millis(operation),
            &CommandCancellation::new(),
        )
        .unwrap()
    {
        ComposerHostFlushCapture::Captured(ticket) => ticket,
        other => panic!("expected close publication for operation {operation}: {other:?}"),
    }
}

pub fn ready(fixture: &mut Host, close: ComposerHostFlushTicket, operation: u64) {
    assert_eq!(
        fixture.host.flush_state(close).unwrap(),
        ComposerHostFlushState::CaptureRequired
    );
    if fixture.host.is_dirty() {
        capture(fixture, close, operation);
        assert_eq!(
            fixture.host.advance_flush(&fixture.store, close).unwrap(),
            ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CaptureRequired)
        );
    }
    assert!(!fixture.host.is_dirty());
    let revision = fixture.store.home_revision().unwrap();
    let binding = fixture.host.binding();
    assert_eq!(fixture.host.publication_custody_count(), 0);
    assert_eq!(
        fixture
            .host
            .capture_flush_publication(
                &fixture.store,
                close,
                fixture.assets.clone(),
                &fixture.seals,
                composer::operation_id(operation),
                None,
                SyndicTimestamp::from_unix_millis(operation),
                &CommandCancellation::new(),
            )
            .unwrap(),
        ComposerHostFlushCapture::State(ComposerHostFlushState::CloseReady)
    );
    assert_eq!(fixture.store.home_revision().unwrap(), revision);
    assert_eq!(fixture.host.binding(), binding);
    assert_eq!(fixture.host.publication_custody_count(), 0);
    assert_eq!(
        fixture.host.advance_flush(&fixture.store, close).unwrap(),
        ComposerHostFlushAdvance::Progress(ComposerHostFlushState::CloseReady)
    );
    assert_eq!(
        fixture.host.flush_state(close).unwrap(),
        ComposerHostFlushState::CloseReady
    );
}
