use beryl_backend::StreamedTextSourceId;

use super::{
    AcceptedInputReplaySource, ProjectionCancellationToken,
    fixture::Fixture,
};

pub(super) fn drain_text(
    source: &mut AcceptedInputReplaySource,
    fixture: &Fixture,
    source_id: StreamedTextSourceId,
    mut start: u64,
    maximum: usize,
) -> String {
    let cancellation = ProjectionCancellationToken::new();
    let mut text = String::new();
    loop {
        let page = source
            .read_text_page(
                &fixture.store,
                &cancellation,
                source_id,
                start,
                maximum,
            )
            .unwrap();
        assert!(!page.text().is_empty());
        assert!(page.text().len() <= maximum);
        assert_eq!(page.start(), start);
        text.push_str(page.text());
        let Some(next) = page.next_offset() else {
            break;
        };
        assert_eq!(next, start + u64::try_from(page.text().len()).unwrap());
        start = next;
    }
    text
}
