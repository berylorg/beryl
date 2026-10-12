use super::*;

pub(super) fn run_table_fixture(
    count: u8,
    chunks: usize,
    prepare: impl FnOnce(&std::path::Path) + 'static,
    scenario: impl for<'a> FnOnce(
        std::rc::Rc<std::cell::RefCell<RunningProcessOwner>>,
        FaultController,
        &'a mut AsyncApp,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>>
    + 'static,
) {
    let failure = std::rc::Rc::new(std::cell::RefCell::new(None));
    let result = failure.clone();
    run_mounted_with_prepared_home_retained(count, chunks, prepare, move |owner, faults, cx| {
        Box::pin(async move {
            let window = windows(&owner)[0];
            let panel = window
                .read_with(cx, |root, _| root.test_transcript_panel())
                .unwrap();
            let outcome = {
                let mut future = scenario(owner, faults, cx);
                std::future::poll_fn(|context| {
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        future.as_mut().poll(context)
                    })) {
                        Ok(std::task::Poll::Pending) => std::task::Poll::Pending,
                        Ok(std::task::Poll::Ready(())) => std::task::Poll::Ready(Ok(())),
                        Err(payload) => std::task::Poll::Ready(Err(payload)),
                    }
                })
                .await
            };
            if let Err(payload) = outcome {
                *result.borrow_mut() = Some(payload);
                if window.read_with(cx, |_, _| ()).is_ok() {
                    retire_code_fixture(window, &panel, cx).await;
                }
            }
        })
    });
    if let Some(payload) = failure.borrow_mut().take() {
        std::panic::resume_unwind(payload);
    }
}

pub(super) fn large_table() -> String {
    let mut text = format!(
        "|{}|\r\n|{}|\r\n",
        (0..12)
            .map(|column| format!(" head{column} "))
            .collect::<Vec<_>>()
            .join("|"),
        (0..12).map(|_| " :-----: ").collect::<Vec<_>>().join("|")
    );
    for row in 0..24 {
        text.push_str(&format!(
            "|{}|\r\n",
            (0..12)
                .map(|column| format!(" **r{row}c{column}** {} ", "x".repeat(24)))
                .collect::<Vec<_>>()
                .join("|")
        ));
    }
    text
}

pub(super) fn small_table() -> String {
    format!(
        "| head0 | head1 | head2 |\r\n| :----- | -----: | :---: |\r\n| **bold\\|** | | `tail*` |\r\n| | | |\r\n| last | | |\r\n{}",
        resource_padding_rows()
    )
}

pub(super) fn resource_padding_rows() -> String {
    format!("| {} | | |\r\n", "x".repeat(3000)).repeat(3)
}

pub(super) fn key(native: &mut gpui::Window, app: &mut gpui::App, value: &str, held: bool) {
    native.dispatch_input_for_test(
        PlatformInput::KeyDown(gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse(value).unwrap(),
            is_held: held,
        }),
        app,
    );
    native.draw_and_present_for_test(app);
}

pub(super) fn click(
    native: &mut gpui::Window,
    app: &mut gpui::App,
    position: gpui::Point<gpui::Pixels>,
    shift: bool,
) {
    native.dispatch_input_for_test(
        PlatformInput::MouseDown(gpui::MouseDownEvent {
            button: gpui::MouseButton::Left,
            position,
            modifiers: gpui::Modifiers {
                shift,
                ..Default::default()
            },
            click_count: 1,
            ..Default::default()
        }),
        app,
    );
    native.dispatch_input_for_test(
        PlatformInput::MouseUp(gpui::MouseUpEvent {
            button: gpui::MouseButton::Left,
            position,
            click_count: 1,
            ..Default::default()
        }),
        app,
    );
    native.draw_and_present_for_test(app);
}

pub(super) fn wheel(
    native: &mut gpui::Window,
    app: &mut gpui::App,
    position: gpui::Point<gpui::Pixels>,
    x: f32,
    y: f32,
) {
    native.dispatch_input_for_test(
        PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
            position,
            delta: gpui::ScrollDelta::Pixels(point(px(x), px(y))),
            touch_phase: gpui::TouchPhase::Moved,
            ..Default::default()
        }),
        app,
    );
    native.draw_and_present_for_test(app);
}

pub(super) fn cell_point(
    panel: &SyndicTranscriptPanel,
    row: u64,
    column: u64,
) -> gpui::Point<gpui::Pixels> {
    let (bounds, header, height, width) = table_geometry(panel).unwrap();
    let diagnostics = panel.diagnostic_snapshot();
    let table = &diagnostics.table_panels[0];
    assert!(table.realized_columns.contains(&column));
    assert!(row == 0 || table.realized_rows.contains(&(row - 1)));
    let offsets = table.offsets;
    let origin = if row == 0 {
        header.origin + point(px(column as f32 * width - offsets.0), px(0.0))
    } else {
        bounds.origin
            + point(
                px(column as f32 * width - offsets.0),
                px((row as f32 - 1.0) * height - offsets.1),
            )
    };
    let visible = gpui::Bounds::new(origin, gpui::size(px(width), px(height)))
        .intersect(if row == 0 { &header } else { &bounds });
    assert!(visible.size.width > px(0.0) && visible.size.height > px(0.0));
    visible.center()
}

pub(super) fn table_geometry(
    panel: &SyndicTranscriptPanel,
) -> Option<(
    gpui::Bounds<gpui::Pixels>,
    gpui::Bounds<gpui::Pixels>,
    f32,
    f32,
)> {
    panel
        .table_viewport_for_test()
        .map(|(_, bounds, header, height, width)| (bounds, header, height, width))
        .or_else(|| panel.inline_table_viewport_for_test())
}

pub(super) async fn ready(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
    ready_with_visibility(window, panel, cx, true).await;
}

pub(super) async fn ready_for_resize(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
) {
    ready_with_visibility(window, panel, cx, false).await;
}

async fn ready_with_visibility(
    window: WindowHandle<MainWindowShellRoot>,
    panel: &Entity<SyndicTranscriptPanel>,
    cx: &mut AsyncApp,
    minimize: bool,
) {
    with_native(window, cx, |native, app| {
        native.draw_and_present_for_test(app);
        if minimize {
            native.minimize_window();
        }
    })
    .unwrap();
    let mut observations = 0_u32;
    wait_until_with_visibility(
        window,
        cx,
        |cx| {
            panel
                .read_with(cx, |panel, _| {
                    let diagnostics = panel.diagnostic_snapshot();
                    if observations % 50 == 0 {
                        let snapshot = panel.snapshot();
                        eprintln!("native Table query state: {}", panel.table_query_state_for_test());
                        eprintln!("native Table readiness: records={}, metadata={:?}, tables={:?}, demand={:?}, current={}, measured={}, resident={:?}", snapshot.records.len(), snapshot.resources.metadata.iter().map(|metadata|metadata.kind.clone()).collect::<Vec<_>>(), diagnostics.table_panels, panel.resource_demand().and_then(|demand| demand.indexed), panel.table_commands_current_for_test(), panel.measured_frame_for_test().is_some(), diagnostics.resident_data);
                    }
                    observations += 1;
                    diagnostics
                        .table_panels
                        .first()
                        .is_some_and(|table| table.cells > 0 && !table.pending)
                        && table_geometry(panel)
                            .is_some_and(|(bounds, _, _, _)| bounds.size.height > px(0.0))
                        && panel.measured_frame_for_test().is_some()
                        && panel.table_commands_current_for_test()
                })
                .unwrap()
        },
        "Table must publish admitted cells and measured native geometry",
        minimize,
    )
    .await;
    if minimize {
        window
            .update(cx, |_, native, _| native.minimize_window())
            .unwrap();
    }
}

pub(super) fn output_directory() -> std::path::PathBuf {
    let ledger = std::env::var_os("BERYL_NATIVE_RETAINED_FIXTURE_LEDGER")
        .expect("absolute retained fixture ledger");
    assert!(std::path::Path::new(&ledger).is_absolute());
    let path = tempfile::Builder::new()
        .prefix("native-table-save-")
        .tempdir()
        .unwrap()
        .keep();
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ledger)
        .unwrap();
    std::io::Write::write_all(&mut file, format!("{}\n", path.display()).as_bytes()).unwrap();
    eprintln!("retained native Table Save output: {}", path.display());
    path
}

pub(super) fn table_appearance(
    state: &beryl_state::BerylState,
) -> beryl_state::PreparedThemeAppearance {
    use beryl_state::*;
    let mut source =
        String::from("schema = 1\nid = \"native-table-test\"\nname = \"Native Table Test\"\n");
    for (_, role) in canonical_theme_schema().roles() {
        if role.property(ThemePropertyId::FontFamily).is_some() {
            source.push_str(&format!(
                "\n[[role]]\nid = \"{}\"\nfont_family = \".SystemUIFont\"\n",
                role.id().as_str()
            ));
            if matches!(
                role.id().as_str(),
                "table-panel.cell" | "table-panel.header-cell"
            ) {
                source.push_str("font_size = 26\n");
            }
        }
    }
    let document =
        ThemeDocument::parse_bytes(source.as_bytes(), ThemeParseMode::StrictCandidate).unwrap();
    let service = state.themes();
    let active = InstalledThemeId::new("native-table-test").unwrap();
    PreparedThemeAppearance::installed(
        service.settings_identity(beryl_model::DomainRevision::new(1).unwrap(), None),
        &active,
        ThemeDocumentIdentity::new(
            service.manifest(ThemeManifestGeneration::INITIAL),
            active.clone(),
            ThemeDocumentRevision::new(std::num::NonZeroU64::new(1).unwrap()),
            source.len() as u64,
            ThemeDocumentDigest::of_bytes(source.as_bytes()),
        ),
        ThemeResolver::new(document.definition()).unwrap().resolve(),
    )
    .unwrap()
}
