use std::time::Duration;

use beryl_backend::{
    ThreadItem, ThreadTurnsListOptions, ThreadTurnsListResponse, TurnInfo, TurnStatus, UserInput,
    UserMessageItem,
};

mod shell {
    #![allow(dead_code)]

    #[path = "../../src/shell/composer_image_label_scan.rs"]
    pub(super) mod composer_image_label_scan;
    #[path = "../../src/shell/composer_image_labels.rs"]
    pub(super) mod composer_image_labels;
    #[path = "../../src/shell/transcript_history.rs"]
    pub(super) mod transcript_history;
}

use shell::composer_image_label_scan::{
    ComposerImageLabelScanError, scan_composer_image_labels,
    scan_composer_image_labels_with_page_limit,
};
use shell::transcript_history::{TranscriptHistoryBackend, initial_thread_history_page_options};

#[test]
fn scan_reads_all_pages_and_returns_label_observations() {
    let mut backend = FakeScanBackend::new(vec![
        Ok(ThreadTurnsListResponse {
            data: vec![image_turn("turn_3", "B")],
            next_cursor: Some("older".to_string()),
            backwards_cursor: None,
        }),
        Ok(ThreadTurnsListResponse {
            data: vec![image_turn("turn_2", "Z"), image_turn("turn_1", "A")],
            next_cursor: None,
            backwards_cursor: Some("newer".to_string()),
        }),
    ]);

    let result =
        scan_composer_image_labels(&mut backend, "thread_1", Duration::from_secs(5)).unwrap();

    assert_eq!(result.pages_scanned, 2);
    assert_eq!(backend.calls.len(), 2);
    assert_eq!(backend.calls[0].0, "thread_1");
    assert_eq!(backend.calls[0].1, initial_thread_history_page_options());
    assert_eq!(backend.calls[1].1.cursor.as_deref(), Some("older"));

    assert_eq!(result.observations.next_index(), 26);
}

#[test]
fn scan_reads_merged_generated_label_suffix_before_image() {
    let mut backend = FakeScanBackend::new(vec![Ok(ThreadTurnsListResponse {
        data: vec![merged_image_turn("turn_1", "Before paste: ", "C")],
        next_cursor: None,
        backwards_cursor: None,
    })]);

    let result =
        scan_composer_image_labels(&mut backend, "thread_1", Duration::from_secs(5)).unwrap();

    assert_eq!(result.pages_scanned, 1);
    assert_eq!(result.observations.next_index(), 3);
}

#[test]
fn scan_reads_delayed_generated_label_anchor_before_image() {
    let mut backend = FakeScanBackend::new(vec![Ok(ThreadTurnsListResponse {
        data: vec![delayed_image_turn(
            "turn_1",
            "Testing image paste: ",
            "B",
            "\ntext after marker",
        )],
        next_cursor: None,
        backwards_cursor: None,
    })]);

    let result =
        scan_composer_image_labels(&mut backend, "thread_1", Duration::from_secs(5)).unwrap();

    assert_eq!(result.pages_scanned, 1);
    assert_eq!(result.observations.next_index(), 2);
}

#[test]
fn scan_fails_when_page_limit_is_exceeded() {
    let mut backend = FakeScanBackend::new(vec![Ok(ThreadTurnsListResponse {
        data: vec![image_turn("turn_1", "A")],
        next_cursor: Some("older".to_string()),
        backwards_cursor: None,
    })]);

    let error = scan_composer_image_labels_with_page_limit(
        &mut backend,
        "thread_1",
        Duration::from_secs(5),
        1,
    )
    .unwrap_err();

    assert_eq!(
        error,
        ComposerImageLabelScanError::PageLimitExceeded { page_limit: 1 }
    );
}

struct FakeScanBackend {
    responses: Vec<Result<ThreadTurnsListResponse, String>>,
    calls: Vec<(String, ThreadTurnsListOptions)>,
}

impl FakeScanBackend {
    fn new(responses: Vec<Result<ThreadTurnsListResponse, String>>) -> Self {
        Self {
            responses,
            calls: Vec::new(),
        }
    }
}

impl TranscriptHistoryBackend for FakeScanBackend {
    type Error = String;

    fn list_thread_turns(
        &mut self,
        thread_id: &str,
        options: &ThreadTurnsListOptions,
        _: Duration,
    ) -> Result<ThreadTurnsListResponse, Self::Error> {
        self.calls.push((thread_id.to_string(), options.clone()));
        if self.responses.is_empty() {
            return Err("unexpected extra page request".to_string());
        }
        self.responses.remove(0)
    }
}

fn image_turn(id: &str, label: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        error: None,
        items: vec![ThreadItem::UserMessage(UserMessageItem {
            id: format!("{id}_user"),
            content: vec![
                UserInput::text(format!("Image {label}:")),
                UserInput::local_image(format!("/tmp/{label}.png")),
            ],
        })],
    }
}

fn merged_image_turn(id: &str, prefix: &str, label: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        error: None,
        items: vec![ThreadItem::UserMessage(UserMessageItem {
            id: format!("{id}_user"),
            content: vec![
                UserInput::text(format!("{prefix}Image {label}:")),
                UserInput::local_image(format!("/tmp/{label}.png")),
            ],
        })],
    }
}

fn delayed_image_turn(id: &str, prefix: &str, label: &str, suffix: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        error: None,
        items: vec![ThreadItem::UserMessage(UserMessageItem {
            id: format!("{id}_user"),
            content: vec![
                UserInput::text(format!("{prefix}Image {label}:{suffix}")),
                UserInput::local_image(format!("/tmp/{label}.png")),
            ],
        })],
    }
}
