# WSL managed backend launch

## Failed approach

- Beryl's original WSL managed-launch command used `wsl.exe --distribution <DISTRO> --cd <PATH> --exec codex app-server --listen stdio://`.
- That approach assumes `codex` is already on the pre-shell `PATH` that `wsl.exe --exec` uses for direct `execvpe(...)` lookup.
- On Debian, the common user-local install path is `$HOME/bin/codex`, and `/etc/profile` plus `~/.profile` add `$HOME/bin` only after a Bash login shell starts.
- In that configuration, direct `wsl.exe --exec codex ...` fails before the app-server handshake starts, even though `codex` works in an interactive WSL terminal session.

## Course adjustment

- Managed WSL launch uses `wsl.exe --distribution <DISTRO> --cd <PATH> --exec /bin/bash -lc 'exec codex app-server ...'`.
- `wsl.exe --cd` remains responsible for selecting the workspace before Bash starts.
- The Bash login shell applies user-local `PATH` setup and then `exec`s `codex app-server`, preserving the single backend child process Beryl expects after startup while still allowing multiple authenticated backend client connections.
