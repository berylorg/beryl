# GUI Integration

# Windows

## Settings Application Window

The Settings application window is Beryl's preheated auxiliary window for app-wide settings. Its title is `Beryl Settings · <build-id>`, it has dedicated chrome rather than the main workspace toolbar, and it is hidden when inactive. Its content uses a left settings-navigation sidebar beside one flexible selected-page pane; the sidebar and page body may scroll independently, while the page header and apply/action areas remain reachable. The window relies on the external settings-window minimum size of `800x520` logical pixels and does not use outer content scrolling.

### Slots

#### Slot: settings-window.body

This slot is the sole content region of the Settings application window. Inserted GUI provides the Settings navigation and selected-page composition within the window's dedicated settings layout.
