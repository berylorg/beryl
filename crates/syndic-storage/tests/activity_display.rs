#![cfg(feature = "test-faults")]

#[path = "activity_display/fixture.rs"]
mod fixture;
mod support;
use fixture::*;
use syndic_storage::*;

#[test]
fn large_diffs_yield_and_count_split_prefixes_long_lines_and_final_lines_exactly() {
    let f = Fixture::new();
    let diff = format!(
        "{}\n+one\r\n-two\n+++ b/file\n--- a/file\n+tail",
        "x".repeat(131_071)
    );
    let changes = vec![
        change("src/lib.rs", &diff),
        change(
            "src/lib.rs",
            "--- a/file\n+++ b/file\n@@ -1,2 +1,2 @@\n+++content\n---content\n+other\n-deleted",
        ),
    ];
    f.complete(
        ProviderItemV1::FileChange(ProviderFileChangeV1 {
            status: ProviderPatchStatusV1::InProgress,
            changes: changes.clone(),
        }),
        ProviderItemV1::FileChange(ProviderFileChangeV1 {
            status: ProviderPatchStatusV1::Failed,
            changes,
        }),
    );
    let (source, steps) = f.extract(&f.entry());
    assert!(steps > 30);
    assert_eq!(source.status(), ActivityDisplayStatus::FinishedError);
    let ActivityDisplayValue::FileChange(files) = source.value() else {
        panic!("expected file projection")
    };
    assert_eq!(
        (files.change_count(), files.additions(), files.deletions()),
        (2, 4, 3)
    );
    assert_eq!(files.sole_path(), Some("src/lib.rs"));
}

#[test]
fn distinct_or_oversized_paths_never_become_a_truncated_sole_path() {
    for paths in [vec!["a".to_owned(), "b".to_owned()], vec!["界".repeat(400)]] {
        let f = Fixture::new();
        let changes = paths
            .iter()
            .map(|path| change(path, "+x"))
            .collect::<Vec<_>>();
        f.complete(
            ProviderItemV1::FileChange(ProviderFileChangeV1 {
                status: ProviderPatchStatusV1::InProgress,
                changes: changes.clone(),
            }),
            ProviderItemV1::FileChange(ProviderFileChangeV1 {
                status: ProviderPatchStatusV1::Completed,
                changes,
            }),
        );
        let (source, _) = f.extract(&f.entry());
        let ActivityDisplayValue::FileChange(files) = source.value() else {
            panic!("expected file projection")
        };
        assert_eq!(files.change_count(), paths.len() as u64);
        assert_eq!(files.additions(), paths.len() as u64);
        assert_eq!(files.sole_path(), None);
    }
}

#[test]
fn command_first_nonempty_line_is_utf8_bounded_and_omits_output() {
    let f = Fixture::new();
    let command = format!(
        "\r\n{}\n\u{2003}\n{}\nsecret next line",
        " ".repeat(70_000),
        "界".repeat(700)
    );
    f.complete(
        command_value(&command, ProviderCommandStatusV1::InProgress),
        command_value(&command, ProviderCommandStatusV1::Completed),
    );
    let (source, steps) = f.extract(&f.entry());
    assert!(steps > 16);
    assert_eq!(source.status(), ActivityDisplayStatus::FinishedOk);
    let ActivityDisplayValue::Command(text) = source.value() else {
        panic!("expected command projection")
    };
    assert!(text.truncated());
    assert!(text.text().len() <= 1024);
    assert!(!text.text().contains("secret"));
    assert!(text.text().chars().all(|character| character == '界'));
}

#[test]
fn reused_command_ranges_read_original_bytes_without_resuming_or_retaining_payloads() {
    let f = Fixture::new();
    let start = command_value("echo exact", ProviderCommandStatusV1::InProgress);
    let reference = f.start_with_text_reference(start);
    let mut completed = match command_value("unused", ProviderCommandStatusV1::Completed) {
        ProviderItemV1::CommandExecution(value) => value,
        _ => unreachable!(),
    };
    completed.command = ProviderTextV1::Reused(reference);
    f.finish(ProviderItemV1::CommandExecution(completed));
    let (source, _) = f.extract(&f.entry());
    let ActivityDisplayValue::Command(text) = source.value() else {
        panic!("expected command projection")
    };
    assert_eq!(text.text(), "echo exact");
    assert!(!text.truncated());
}

#[test]
fn live_patch_replacement_uses_exact_latest_changes_and_stale_or_cancelled_work_is_terminal() {
    let f = Fixture::new();
    f.start(ProviderItemV1::FileChange(ProviderFileChangeV1 {
        status: ProviderPatchStatusV1::InProgress,
        changes: vec![change("old", "+old")],
    }));
    let old = f.entry();
    let mut stale = f.storage.prepare_activity_display(&f.home, &old).unwrap();
    f.delta(ProviderItemDeltaV1::FileChangePatchUpdated {
        changes: vec![change("new", "+a\n+b\n-c")],
    });
    assert!(matches!(
        stale.step(&f.home, &f.storage, &|| false),
        Err(ActivityDisplayReadError::Stale)
    ));
    assert!(matches!(
        stale.step(&f.home, &f.storage, &|| false),
        Err(ActivityDisplayReadError::Terminal)
    ));
    let entry = f.entry();
    let (source, _) = f.extract(&entry);
    assert_eq!(source.status(), ActivityDisplayStatus::Running);
    let ActivityDisplayValue::FileChange(files) = source.value() else {
        panic!("expected file projection")
    };
    assert_eq!(
        (
            files.change_count(),
            files.additions(),
            files.deletions(),
            files.sole_path()
        ),
        (1, 2, 1, Some("new"))
    );
    let mut cancelled = f.storage.prepare_activity_display(&f.home, &entry).unwrap();
    assert!(matches!(
        cancelled.step(&f.home, &f.storage, &|| true),
        Err(ActivityDisplayReadError::Cancelled)
    ));
    assert!(matches!(
        cancelled.step(&f.home, &f.storage, &|| false),
        Err(ActivityDisplayReadError::Terminal)
    ));
}

#[test]
fn reasoning_uses_only_summary_deltas_and_never_raw_reasoning() {
    let f = Fixture::new();
    f.start(ProviderItemV1::Reasoning(ProviderReasoningV1 {
        summary: vec![],
    }));
    f.delta(ProviderItemDeltaV1::ReasoningSummaryText {
        summary_index: 0,
        delta: ProviderTextV1::inline("Exact\nsummary"),
    });
    f.delta(ProviderItemDeltaV1::ReasoningTextObserved { content_index: 0 });
    let (source, _) = f.extract(&f.entry());
    let ActivityDisplayValue::Reasoning(text) = source.value() else {
        panic!("expected reasoning projection")
    };
    assert_eq!(text.text(), "Exact summary");
}

#[test]
fn tool_status_and_dynamic_success_follow_their_exact_protocol_enumerations() {
    for (status, expected) in [
        (
            ProviderToolCallStatusV1::Failed,
            ActivityDisplayStatus::FinishedError,
        ),
        (
            ProviderToolCallStatusV1::Completed,
            ActivityDisplayStatus::FinishedOk,
        ),
    ] {
        let f = Fixture::new();
        let value = |status| {
            ProviderItemV1::McpToolCall(ProviderMcpToolCallV1 {
                server: ProviderTextV1::inline("server"),
                tool: ProviderTextV1::inline("exact_mcp_tool"),
                status,
                arguments: ProviderStructuredValueV1::Null,
                app_context: None,
                mcp_app_resource_uri: None,
                plugin_id: None,
                result: None,
                error: None,
                duration_ms: None,
            })
        };
        f.complete(value(ProviderToolCallStatusV1::InProgress), value(status));
        assert_eq!(f.extract(&f.entry()).0.status(), expected);
    }
    let f = Fixture::new();
    let mut arguments =
        ProviderStructuredValueV1::List(vec![ProviderStructuredValueV1::Null; 5_000]);
    for _ in 0..127 {
        arguments = ProviderStructuredValueV1::List(vec![arguments]);
    }
    let value = |status, success| {
        ProviderItemV1::DynamicToolCall(ProviderDynamicToolCallV1 {
            namespace: None,
            tool: ProviderTextV1::inline("exact_tool"),
            arguments: arguments.clone(),
            status,
            content_items: Some(vec![ProviderDynamicToolOutputV1::InputText {
                text: ProviderTextV1::inline("private output".repeat(10_000)),
            }]),
            success,
            duration_ms: None,
        })
    };
    f.complete(
        value(ProviderToolCallStatusV1::InProgress, None),
        value(ProviderToolCallStatusV1::Completed, Some(false)),
    );
    let (source, steps) = f.extract(&f.entry());
    assert!(steps > 4);
    assert_eq!(source.status(), ActivityDisplayStatus::FinishedError);
    let ActivityDisplayValue::Tool(tool) = source.value() else {
        panic!("expected tool name")
    };
    assert_eq!(tool.text(), "exact_tool");
}

#[test]
fn live_reused_diff_ranges_preserve_exact_counts_and_cancel_between_quanta() {
    let f = Fixture::new();
    let reference = f.start_with_text_reference(ProviderItemV1::FileChange(ProviderFileChangeV1 {
        status: ProviderPatchStatusV1::InProgress,
        changes: vec![change("old", "+one\n-two\n+three")],
    }));
    f.delta(ProviderItemDeltaV1::FileChangePatchUpdated {
        changes: vec![ProviderFileUpdateChangeV1 {
            path: ProviderTextV1::inline("new"),
            diff: ProviderTextV1::Reused(reference),
            kind: ProviderPatchChangeKindV1::Add,
        }],
    });
    let entry = f.entry();
    let (source, _) = f.extract(&entry);
    let ActivityDisplayValue::FileChange(files) = source.value() else {
        panic!("expected file projection")
    };
    assert_eq!(
        (
            files.change_count(),
            files.additions(),
            files.deletions(),
            files.sole_path()
        ),
        (1, 2, 1, Some("new"))
    );
    let mut preparation = f.storage.prepare_activity_display(&f.home, &entry).unwrap();
    assert!(matches!(
        preparation.step(&f.home, &f.storage, &|| false).unwrap(),
        ActivityDisplayStep::Pending
    ));
    assert!(matches!(
        preparation.step(&f.home, &f.storage, &|| true),
        Err(ActivityDisplayReadError::Cancelled)
    ));
}

#[test]
fn admitted_reused_tool_ranges_refuse_misaligned_utf8_incomplete_scalars_and_wrong_digests() {
    use sha2::{Digest, Sha256};
    let value = |tool, status| {
        ProviderItemV1::McpToolCall(ProviderMcpToolCallV1 {
            server: ProviderTextV1::inline("server"),
            tool,
            status,
            arguments: ProviderStructuredValueV1::Null,
            app_context: None,
            mcp_app_resource_uri: None,
            plugin_id: None,
            result: None,
            error: None,
            duration_ms: None,
        })
    };
    for (start, end, wrong_digest, expected) in [
        (1, 3, false, "invalid Activity source UTF-8 scalar"),
        (0, 1, false, "incomplete Activity source UTF-8 scalar"),
        (0, 3, true, "reused Activity source digest disagrees"),
    ] {
        let f = Fixture::new();
        f.start(value(
            ProviderTextV1::inline("tool"),
            ProviderToolCallStatusV1::InProgress,
        ));
        let prior = f.delta_with_text_reference(ProviderItemDeltaV1::McpToolCallProgress {
            message: ProviderTextV1::inline("éx"),
        });
        let digest = if wrong_digest {
            [0; 32]
        } else {
            Sha256::digest(&"éx".as_bytes()[start..end]).into()
        };
        let reference = ProviderTextReferenceV1::new(
            prior.start() + start as u64,
            prior.start() + end as u64,
            digest,
        )
        .unwrap();
        f.finish(value(
            ProviderTextV1::Reused(reference),
            ProviderToolCallStatusV1::Completed,
        ));
        let entry = f.entry();
        let mut preparation = f.storage.prepare_activity_display(&f.home, &entry).unwrap();
        let mut rejected = false;
        for _ in 0..1000 {
            match preparation.step(&f.home, &f.storage, &|| false) {
                Ok(ActivityDisplayStep::Pending) => {}
                Err(ActivityDisplayReadError::Invalid(message)) => {
                    assert_eq!(message, expected);
                    rejected = true;
                    break;
                }
                _ => panic!("invalid reused tool text must not produce a row"),
            }
        }
        assert!(rejected);
        assert!(matches!(
            preparation.step(&f.home, &f.storage, &|| false),
            Err(ActivityDisplayReadError::Terminal)
        ));
    }
}
