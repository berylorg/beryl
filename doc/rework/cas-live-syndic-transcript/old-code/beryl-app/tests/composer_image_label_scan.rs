#![allow(dead_code, unused_imports)]

use std::time::Duration;

use beryl_backend::{
    ImageGenerationItem, ThreadItem, ThreadTurnsListOptions, ThreadTurnsListResponse, TurnInfo,
    TurnStatus, UserInput, UserMessageItem,
};

mod shell {
    #![allow(dead_code)]

    #[path = "../../src/shell/composer_image_label_scan.rs"]
    pub(super) mod composer_image_label_scan;
    #[path = "../../src/shell/composer_image_labels.rs"]
    pub(super) mod composer_image_labels;
}

use shell::composer_image_label_scan::{
    ComposerImageLabelFrontierValidationError, ComposerImageLabelFrontierValidationOutcome,
    ComposerImageLabelHistoryBackend, ComposerImageLabelScanError, ComposerImageLabelScanPlan,
    composer_image_label_frontier_validation_page_options, composer_image_label_scan_page_options,
    scan_composer_image_labels, scan_composer_image_labels_for_plan,
    scan_composer_image_labels_with_page_limit, validate_composer_image_label_frontier,
    validate_composer_image_label_frontier_with_page_limit,
};
use shell::composer_image_labels::ComposerImageLabelHistoryFrontier;

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
    assert_eq!(
        backend.calls[0].1,
        composer_image_label_scan_page_options(None)
    );
    assert_eq!(
        backend.calls[1].1,
        composer_image_label_scan_page_options(Some("older"))
    );

    assert_eq!(result.observations.next_index(), 26);
    assert_eq!(
        result.frontier,
        frontier_desc(&[
            image_turn("turn_3", "B"),
            image_turn("turn_2", "Z"),
            image_turn("turn_1", "A"),
        ])
    );
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

#[test]
fn suffix_scan_reads_only_turns_before_cached_boundary() {
    let current_frontier = frontier_desc(&[
        image_turn("turn_4", "D"),
        image_turn("turn_3", "C"),
        image_turn("turn_2", "B"),
        image_turn("turn_1", "A"),
    ]);
    let mut backend = FakeScanBackend::new(vec![Ok(ThreadTurnsListResponse {
        data: vec![
            image_turn("turn_4", "D"),
            image_turn("turn_3", "C"),
            image_turn("turn_2", "B"),
        ],
        next_cursor: Some("older".to_string()),
        backwards_cursor: None,
    })]);

    let result = scan_composer_image_labels_for_plan(
        &mut backend,
        "thread_1",
        ComposerImageLabelScanPlan::AppendOnlySuffix {
            expected_appended_turn_count: 2,
            previous_newest_turn_id: Some("turn_2".to_string()),
            frontier: current_frontier.clone(),
        },
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(result.pages_scanned, 1);
    assert_eq!(result.observations.next_index(), 4);
    assert_eq!(result.frontier, current_frontier);
    assert_eq!(backend.calls.len(), 1);
}

#[test]
fn suffix_scan_for_empty_cached_history_scans_full_current_history() {
    let mut backend = FakeScanBackend::new(vec![Ok(ThreadTurnsListResponse {
        data: vec![image_turn("turn_2", "B"), image_turn("turn_1", "A")],
        next_cursor: None,
        backwards_cursor: None,
    })]);

    let result = scan_composer_image_labels_for_plan(
        &mut backend,
        "thread_1",
        ComposerImageLabelScanPlan::AppendOnlySuffix {
            expected_appended_turn_count: 2,
            previous_newest_turn_id: None,
            frontier: ComposerImageLabelHistoryFrontier::empty(),
        },
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(result.pages_scanned, 1);
    assert_eq!(result.observations.next_index(), 2);
    assert_eq!(
        result.frontier,
        frontier_desc(&[image_turn("turn_2", "B"), image_turn("turn_1", "A")])
    );
}

#[test]
fn suffix_scan_fails_when_cached_boundary_is_missing() {
    let mut backend = FakeScanBackend::new(vec![Ok(ThreadTurnsListResponse {
        data: vec![image_turn("turn_4", "D"), image_turn("turn_3", "C")],
        next_cursor: None,
        backwards_cursor: None,
    })]);

    let error = scan_composer_image_labels_for_plan(
        &mut backend,
        "thread_1",
        ComposerImageLabelScanPlan::AppendOnlySuffix {
            expected_appended_turn_count: 2,
            previous_newest_turn_id: Some("turn_2".to_string()),
            frontier: ComposerImageLabelHistoryFrontier::empty(),
        },
        Duration::from_secs(5),
    )
    .unwrap_err();

    assert_eq!(
        error,
        ComposerImageLabelScanError::AppendOnlyBoundaryMissing {
            previous_newest_turn_id: "turn_2".to_string(),
        }
    );
}

#[test]
fn suffix_scan_fails_when_appended_count_changed() {
    let mut backend = FakeScanBackend::new(vec![Ok(ThreadTurnsListResponse {
        data: vec![
            image_turn("turn_5", "E"),
            image_turn("turn_4", "D"),
            image_turn("turn_3", "C"),
        ],
        next_cursor: Some("older".to_string()),
        backwards_cursor: None,
    })]);

    let error = scan_composer_image_labels_for_plan(
        &mut backend,
        "thread_1",
        ComposerImageLabelScanPlan::AppendOnlySuffix {
            expected_appended_turn_count: 1,
            previous_newest_turn_id: Some("turn_3".to_string()),
            frontier: ComposerImageLabelHistoryFrontier::empty(),
        },
        Duration::from_secs(5),
    )
    .unwrap_err();

    assert_eq!(
        error,
        ComposerImageLabelScanError::AppendOnlySuffixChanged {
            expected_appended_turn_count: 1,
            observed_appended_turn_count: 2,
        }
    );
}

#[test]
fn scan_ignores_generated_image_result_payloads() {
    let mut backend = FakeScanBackend::new(vec![Ok(ThreadTurnsListResponse {
        data: vec![image_generation_turn("turn_1", "Image ZZ:")],
        next_cursor: None,
        backwards_cursor: None,
    })]);

    let result =
        scan_composer_image_labels(&mut backend, "thread_1", Duration::from_secs(5)).unwrap();

    assert_eq!(result.observations.next_index(), 0);
    assert_eq!(
        result.frontier,
        frontier_desc(&[image_generation_turn("turn_1", "Image ZZ:")])
    );
}

#[test]
fn validation_reads_not_loaded_pages_and_marks_cache_valid() {
    let cached_frontier = frontier_desc(&[
        not_loaded_turn("turn_3", TurnStatus::Completed),
        not_loaded_turn("turn_2", TurnStatus::Completed),
        not_loaded_turn("turn_1", TurnStatus::Interrupted),
    ]);
    let mut backend = FakeScanBackend::new(vec![
        Ok(ThreadTurnsListResponse {
            data: vec![
                not_loaded_turn("turn_3", TurnStatus::Completed),
                not_loaded_turn("turn_2", TurnStatus::Completed),
            ],
            next_cursor: Some("older".to_string()),
            backwards_cursor: None,
        }),
        Ok(ThreadTurnsListResponse {
            data: vec![not_loaded_turn("turn_1", TurnStatus::Interrupted)],
            next_cursor: None,
            backwards_cursor: Some("newer".to_string()),
        }),
    ]);

    let result = validate_composer_image_label_frontier(
        &mut backend,
        "thread_1",
        Some(&cached_frontier),
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(result.pages_scanned, 2);
    assert_eq!(
        result.outcome,
        ComposerImageLabelFrontierValidationOutcome::CacheValid {
            frontier: cached_frontier
        }
    );
    assert_eq!(backend.calls.len(), 2);
    assert_eq!(
        backend.calls[0].1,
        composer_image_label_frontier_validation_page_options(None)
    );
    assert_eq!(
        backend.calls[1].1,
        composer_image_label_frontier_validation_page_options(Some("older"))
    );
}

#[test]
fn validation_detects_append_only_suffix_across_pages() {
    let cached_frontier = frontier_desc(&[
        not_loaded_turn("turn_2", TurnStatus::Completed),
        not_loaded_turn("turn_1", TurnStatus::Completed),
    ]);
    let current_turns = vec![
        not_loaded_turn("turn_4", TurnStatus::Completed),
        not_loaded_turn("turn_3", TurnStatus::Failed),
        not_loaded_turn("turn_2", TurnStatus::Completed),
        not_loaded_turn("turn_1", TurnStatus::Completed),
    ];
    let current_frontier = frontier_desc(&current_turns);
    let mut backend = FakeScanBackend::new(vec![
        Ok(ThreadTurnsListResponse {
            data: current_turns[..2].to_vec(),
            next_cursor: Some("older".to_string()),
            backwards_cursor: None,
        }),
        Ok(ThreadTurnsListResponse {
            data: current_turns[2..].to_vec(),
            next_cursor: None,
            backwards_cursor: Some("newer".to_string()),
        }),
    ]);

    let result = validate_composer_image_label_frontier(
        &mut backend,
        "thread_1",
        Some(&cached_frontier),
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(
        result.outcome,
        ComposerImageLabelFrontierValidationOutcome::AppendOnly {
            appended_turn_count: 2,
            previous_newest_turn_id: Some("turn_2".to_string()),
            frontier: current_frontier,
        }
    );
}

#[test]
fn validation_treats_current_history_after_empty_cache_as_append_only() {
    let current_turns = vec![not_loaded_turn("turn_1", TurnStatus::Completed)];
    let current_frontier = frontier_desc(&current_turns);
    let mut backend = FakeScanBackend::new(vec![Ok(ThreadTurnsListResponse {
        data: current_turns,
        next_cursor: None,
        backwards_cursor: None,
    })]);

    let result = validate_composer_image_label_frontier(
        &mut backend,
        "thread_1",
        Some(&ComposerImageLabelHistoryFrontier::empty()),
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(
        result.outcome,
        ComposerImageLabelFrontierValidationOutcome::AppendOnly {
            appended_turn_count: 1,
            previous_newest_turn_id: None,
            frontier: current_frontier,
        }
    );
}

#[test]
fn validation_detects_unknown_mutation_when_cached_newest_is_missing() {
    let cached_frontier = frontier_desc(&[
        not_loaded_turn("turn_3", TurnStatus::Completed),
        not_loaded_turn("turn_2", TurnStatus::Completed),
        not_loaded_turn("turn_1", TurnStatus::Completed),
    ]);
    let current_turns = vec![
        not_loaded_turn("turn_2", TurnStatus::Completed),
        not_loaded_turn("turn_1", TurnStatus::Completed),
    ];
    let current_frontier = frontier_desc(&current_turns);
    let mut backend = FakeScanBackend::new(vec![Ok(ThreadTurnsListResponse {
        data: current_turns,
        next_cursor: None,
        backwards_cursor: None,
    })]);

    let result = validate_composer_image_label_frontier(
        &mut backend,
        "thread_1",
        Some(&cached_frontier),
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(
        result.outcome,
        ComposerImageLabelFrontierValidationOutcome::UnknownMutation {
            frontier: current_frontier,
        }
    );
}

#[test]
fn validation_detects_unknown_mutation_when_cached_tail_status_changes() {
    let cached_frontier = frontier_desc(&[not_loaded_turn("turn_1", TurnStatus::InProgress)]);
    let current_turns = vec![not_loaded_turn("turn_1", TurnStatus::Completed)];
    let current_frontier = frontier_desc(&current_turns);
    let mut backend = FakeScanBackend::new(vec![Ok(ThreadTurnsListResponse {
        data: current_turns,
        next_cursor: None,
        backwards_cursor: None,
    })]);

    let result = validate_composer_image_label_frontier(
        &mut backend,
        "thread_1",
        Some(&cached_frontier),
        Duration::from_secs(5),
    )
    .unwrap();

    assert_eq!(
        result.outcome,
        ComposerImageLabelFrontierValidationOutcome::UnknownMutation {
            frontier: current_frontier,
        }
    );
}

#[test]
fn validation_fails_on_backend_error() {
    let mut backend = FakeScanBackend::new(vec![Err("backend unavailable".to_string())]);

    let error = validate_composer_image_label_frontier(
        &mut backend,
        "thread_1",
        Some(&ComposerImageLabelHistoryFrontier::empty()),
        Duration::from_secs(5),
    )
    .unwrap_err();

    assert_eq!(
        error,
        ComposerImageLabelFrontierValidationError::Backend("backend unavailable".to_string())
    );
}

#[test]
fn validation_fails_when_not_loaded_page_returns_items() {
    let mut backend = FakeScanBackend::new(vec![Ok(ThreadTurnsListResponse {
        data: vec![image_turn("turn_1", "A")],
        next_cursor: None,
        backwards_cursor: None,
    })]);

    let error = validate_composer_image_label_frontier(
        &mut backend,
        "thread_1",
        Some(&ComposerImageLabelHistoryFrontier::empty()),
        Duration::from_secs(5),
    )
    .unwrap_err();

    assert_eq!(
        error,
        ComposerImageLabelFrontierValidationError::UnexpectedLoadedTurn {
            turn_id: "turn_1".to_string(),
            items_view: beryl_backend::TurnItemsView::Full,
            item_count: 1,
        }
    );
}

#[test]
fn validation_fails_when_page_limit_is_exceeded() {
    let mut backend = FakeScanBackend::new(vec![Ok(ThreadTurnsListResponse {
        data: vec![not_loaded_turn("turn_1", TurnStatus::Completed)],
        next_cursor: Some("older".to_string()),
        backwards_cursor: None,
    })]);

    let error = validate_composer_image_label_frontier_with_page_limit(
        &mut backend,
        "thread_1",
        Some(&ComposerImageLabelHistoryFrontier::empty()),
        Duration::from_secs(5),
        1,
    )
    .unwrap_err();

    assert_eq!(
        error,
        ComposerImageLabelFrontierValidationError::PageLimitExceeded { page_limit: 1 }
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

impl ComposerImageLabelHistoryBackend for FakeScanBackend {
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
        items_view: beryl_backend::TurnItemsView::Full,
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
        items_view: beryl_backend::TurnItemsView::Full,
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
        items_view: beryl_backend::TurnItemsView::Full,
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

fn image_generation_turn(id: &str, result: &str) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status: TurnStatus::Completed,
        items_view: beryl_backend::TurnItemsView::Full,
        error: None,
        items: vec![ThreadItem::ImageGeneration(ImageGenerationItem {
            id: format!("{id}_image"),
            status: Some("completed".to_string()),
            revised_prompt: Some(format!("Prompt for {id}")),
            result: Some(result.to_string()),
            saved_path: Some(format!("C:/tmp/{id}.png")),
        })],
    }
}

fn not_loaded_turn(id: &str, status: TurnStatus) -> TurnInfo {
    TurnInfo {
        id: id.to_string(),
        status,
        items_view: beryl_backend::TurnItemsView::NotLoaded,
        error: None,
        items: Vec::new(),
    }
}

fn frontier_desc(turns: &[TurnInfo]) -> ComposerImageLabelHistoryFrontier {
    ComposerImageLabelHistoryFrontier::from_turns_desc(turns)
}
