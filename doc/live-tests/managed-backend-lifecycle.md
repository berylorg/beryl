# Managed Backend Lifecycle Live Tests

## WSL Workspace Close

Use this checklist when a local WSL distro with `codex` on the login-shell `PATH` is available.

1. Start Beryl and open a workspace whose runtime is a WSL-Linux workspace member.
2. Confirm the workspace reaches the ready state.
3. In the target distro, list matching backend processes:
   ```sh
   ps -eo pid,ppid,pgid,comm,args | grep '[c]odex app-server'
   ```
4. Close Beryl through the window close button or in-app quit command.
5. In the same distro, repeat the process listing.
6. The expected result is no remaining `codex app-server` process for the closed Beryl session.

If a process remains, capture its `pid`, `ppid`, `pgid`, full command line, distro name, and the Beryl runtime mode used for the workspace before killing it manually.
