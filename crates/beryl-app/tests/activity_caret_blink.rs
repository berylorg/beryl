#[allow(dead_code)]
#[path = "../src/shell/render/transcript/activity_caret.rs"]
mod activity_caret;

use std::time::Duration;

use activity_caret::{
    ActivityCaretBlinkState, ActivityCaretMotion, windows_caret_blink_interval_from_millis,
};

const BLINK_INTERVAL: Duration = Duration::from_millis(530);

#[test]
fn blink_starts_visible_and_toggles_opacity_without_generation_churn() {
    let mut state = ActivityCaretBlinkState::default();

    assert!(state.sync(
        true,
        ActivityCaretMotion::Blink {
            interval: BLINK_INTERVAL
        },
    ));
    let schedule = state.blink_schedule().unwrap();
    let generation = schedule.generation;
    assert_eq!(schedule.interval, BLINK_INTERVAL);
    assert_eq!(state.opacity(), 1.0);

    assert!(state.advance(generation));
    assert_eq!(state.opacity(), 0.0);
    assert_eq!(state.blink_schedule().unwrap().generation, generation);

    assert!(state.advance(generation));
    assert_eq!(state.opacity(), 1.0);
    assert_eq!(state.blink_schedule().unwrap().generation, generation);
}

#[test]
fn disabled_blink_renders_steady_and_does_not_schedule_blink() {
    let mut state = ActivityCaretBlinkState::default();

    assert!(state.sync(true, ActivityCaretMotion::for_blink_interval(None),));

    assert_eq!(state.opacity(), 1.0);
    assert_eq!(state.blink_schedule(), None);
    assert!(!state.advance(state.generation()));
    assert_eq!(state.opacity(), 1.0);
}

#[test]
fn stopping_caret_invalidates_pending_blink_generation() {
    let mut state = ActivityCaretBlinkState::default();
    state.sync(
        true,
        ActivityCaretMotion::Blink {
            interval: BLINK_INTERVAL,
        },
    );
    let generation = state.blink_schedule().unwrap().generation;

    assert!(state.sync(
        false,
        ActivityCaretMotion::Blink {
            interval: BLINK_INTERVAL,
        },
    ));

    assert_eq!(state.blink_schedule(), None);
    assert!(!state.advance(generation));
    assert_eq!(state.opacity(), 1.0);
}

#[test]
fn changing_blink_interval_resets_visibility_and_invalidates_prior_generation() {
    let mut state = ActivityCaretBlinkState::default();
    state.sync(
        true,
        ActivityCaretMotion::Blink {
            interval: BLINK_INTERVAL,
        },
    );
    let generation = state.blink_schedule().unwrap().generation;
    assert!(state.advance(generation));
    assert_eq!(state.opacity(), 0.0);

    let updated_interval = Duration::from_millis(700);
    assert!(state.sync(
        true,
        ActivityCaretMotion::Blink {
            interval: updated_interval,
        },
    ));

    let schedule = state.blink_schedule().unwrap();
    assert_ne!(schedule.generation, generation);
    assert_eq!(schedule.interval, updated_interval);
    assert_eq!(state.opacity(), 1.0);
    assert!(!state.advance(generation));
}

#[test]
fn windows_caret_blink_interval_policy_treats_disabled_values_as_steady() {
    assert_eq!(windows_caret_blink_interval_from_millis(0), None);
    assert_eq!(windows_caret_blink_interval_from_millis(u32::MAX), None);
    assert_eq!(
        windows_caret_blink_interval_from_millis(700),
        Some(Duration::from_millis(700)),
    );
}
