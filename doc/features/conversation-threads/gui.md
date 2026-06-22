# Conversation Threads GUI

This is a normative supplemental GUI composition file for `design.md`. It owns conversation-thread slot mounts, layout relationships, and widget composition. Product behavior, activation authority, branching, editing, title precedence, failure states, and backend requirements remain in `design.md`.

## Branch Breadcrumbs

Mount-into: main-window.toolbar

Selected-branch parent breadcrumbs mount in the leading toolbar group after the Workspaces control. Breadcrumb buttons use content-sized `command-button` geometry under normal space and truncate only inside the bounded breadcrumb trail.

The breadcrumb trail leaves the trailing toolbar controls reachable. Graph and Settings controls remain in the trailing toolbar group after flexible space.

## Thread Strip Controls

Mount-into: main-window.thread-strip

The thread strip contains the New Thread control, backward and forward thread-navigation controls, and the active thread title selector.

New Thread is a normal text-labeled `command-button` that uses shared app-wide horizontal padding and content-sized width. Backward and forward controls are compact icon-like `command-button` controls with square geometry.

The active thread title selector occupies the remaining strip space after leading controls. It presents the selected thread title, disabled or pending activation states, and stable in-button progress fill without changing control geometry.

The strip does not render static runtime-context labels before the active title selector.

## Thread Selector Popup

Mount-into: main-window.overlays

The thread selector is an anchored popup opened from the active thread title selector. It uses the project-local `column-browser` widget for member, root, and recursive branch columns.

The selector is bounded to the main OS window. When the column trail exceeds available width, the selector owns horizontal scrolling through the `column-browser` contract. Each column owns vertical scrolling for its own rows.

Opening the selector closes the graph overlay and graph context menus so only one column-browser interaction path is active.

## Thread Context Menu Commands

Mount-into: main-window.overlays

Conversation-thread actions that originate from transcript content, such as branch, edit, and update-title commands, appear in the transcript context menu. They use the built-in `context-menu`, `tooltip`, and `disabled-command-tooltip` contracts supplied through the transcript GUI.
