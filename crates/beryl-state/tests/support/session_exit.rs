use crate::support;
use beryl_home_store::{CommandOutcome, HomeStore};
use beryl_model::{
    RootId, RuntimeId, SyndicThreadId, VirtualDesktopId, WindowBounds, WindowDisplayState,
    WindowId, WindowPlacement,
};
use beryl_state::{
    CreateClaimedWindow, ExitWindowPlacement, InitializeThreadlessWindow, MinimalSessionBootstrap,
    RememberedTarget, ReplaceWindowClaim, SessionState,
};

pub fn placement(seed: i32) -> WindowPlacement {
    WindowPlacement::new(
        WindowBounds::new(seed, -seed, 900, 700).unwrap(),
        WindowDisplayState::Maximized,
        None,
        Some(VirtualDesktopId::from_bytes([3; 16])),
    )
}

pub fn snapshot(home: &HomeStore, session: &SessionState) -> MinimalSessionBootstrap {
    session.minimal_bootstrap(home).unwrap().unwrap()
}

pub fn committed(outcome: CommandOutcome) {
    assert!(
        matches!(
            outcome,
            CommandOutcome::Committed {
                later_failure: None,
                ..
            }
        ),
        "{outcome:?}"
    );
}

pub fn seed(home: &HomeStore, session: &SessionState, count: usize) {
    let first = WindowId::from_bytes(0u128.to_be_bytes());
    committed(support::execute(
        home,
        session.initialize_threadless(
            session.revision(home).unwrap(),
            InitializeThreadlessWindow::new(first, placement(0)),
        ),
    ));
    if count == 1 {
        return;
    }
    let target = RememberedTarget::new(RuntimeId::from_bytes([1; 16]), RootId::from_bytes([2; 16]));
    let current = snapshot(home, session);
    committed(support::execute(
        home,
        session.replace_claim(
            session.revision(home).unwrap(),
            ReplaceWindowClaim::new(
                current.header().revision(),
                first,
                current.windows()[0].revision(),
                None,
                target,
                SyndicThreadId::from_bytes(0u128.to_be_bytes()),
            ),
        ),
    ));
    for ordinal in 1..count {
        let current = snapshot(home, session);
        committed(support::execute(
            home,
            session.create_claimed_window(
                session.revision(home).unwrap(),
                CreateClaimedWindow::new(
                    current.header().revision(),
                    WindowId::from_bytes((ordinal as u128).to_be_bytes()),
                    target,
                    SyndicThreadId::from_bytes((ordinal as u128).to_be_bytes()),
                    placement(ordinal as i32),
                ),
            ),
        ));
    }
}

pub fn entries(before: &MinimalSessionBootstrap) -> Vec<ExitWindowPlacement> {
    before
        .windows()
        .iter()
        .enumerate()
        .rev()
        .map(|(index, window)| {
            ExitWindowPlacement::new(
                window.window_id(),
                window.revision(),
                if index == 0 {
                    window.placement().clone()
                } else {
                    placement(700 + index as i32)
                },
            )
        })
        .collect()
}

pub fn assert_same(before: &MinimalSessionBootstrap, after: &MinimalSessionBootstrap) {
    assert_eq!(before.header(), after.header());
    assert_eq!(before.windows(), after.windows());
}
