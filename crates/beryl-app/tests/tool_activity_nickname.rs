use std::time::{Duration, Instant};

#[path = "../src/shell/tool_activity_nickname.rs"]
mod tool_activity_nickname;

use tool_activity_nickname::{ToolActivityNicknameResolutionTarget, ToolActivityNicknameResolver};

#[test]
fn resolver_deduplicates_eligible_thread_ids_and_respects_retry_backoff() {
    let now = Instant::now();
    let mut resolver = ToolActivityNicknameResolver::default();
    resolver.mark_in_flight_for_test("child_busy");
    resolver.mark_retry_for_test("child_backoff", now + Duration::from_secs(1), 1);
    resolver.mark_retry_for_test("child_ready", now - Duration::from_millis(1), 2);

    let batch = resolver.eligible_resolution_batch_for_test(
        vec![
            target(" child_a "),
            target("child_a"),
            target(""),
            target("child_busy"),
            target("child_backoff"),
            target("child_ready"),
            target("child_b"),
        ],
        now,
    );

    assert_eq!(
        target_ids(batch),
        vec![
            "child_a".to_string(),
            "child_ready".to_string(),
            "child_b".to_string()
        ]
    );
}

#[test]
fn resolver_limits_each_resolution_batch() {
    let resolver = ToolActivityNicknameResolver::default();
    let batch = resolver.eligible_resolution_batch_for_test(
        (0..12)
            .map(|index| target(format!("child_{index}")))
            .collect(),
        Instant::now(),
    );

    assert_eq!(batch.len(), 8);
    assert_eq!(batch[0].thread_id, "child_0");
    assert_eq!(batch[7].thread_id, "child_7");
}

fn target(thread_id: impl Into<String>) -> ToolActivityNicknameResolutionTarget {
    ToolActivityNicknameResolutionTarget {
        thread_id: thread_id.into(),
        requires_nickname: true,
    }
}

fn target_ids(targets: Vec<ToolActivityNicknameResolutionTarget>) -> Vec<String> {
    targets.into_iter().map(|target| target.thread_id).collect()
}
