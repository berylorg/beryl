# Reason For Investigation

Beryl needed opt-in Windows process-memory counter APIs for diagnostic milestones without polling from render-loop paths.

# Outcome

Useful. The migrated finding records the windows crate features, Process Status APIs, counter meanings, initialization requirements, and the constraint that these calls belong only in explicit diagnostics.

# Sources

- Legacy note segment: doc/deps/windows/0.61.3.md.
- Source identity: crates.io package windows 0.61.3.
- Workspace dependency context: Cargo.toml and Cargo.lock in this repository at migration time.
- Additional upstream files, commands, feature flags, local use sites, and follow-up sources are listed in the migrated legacy details below.

# Migrated Legacy Details

## windows 0.61.3

### Beryl App Process Memory Counters

Additional verification: 2026-05-10.

`beryl-app` uses `windows` 0.61.3 for opt-in Windows process-memory milestone diagnostics. The app crate needs these explicit features:

- `Win32_System_ProcessStatus`
- `Win32_System_Threading`

Symbols examined for this use:

- `GetCurrentProcess`
- `GetProcessMemoryInfo`
- `PROCESS_MEMORY_COUNTERS`
- `PROCESS_MEMORY_COUNTERS_EX`

Memory-counter gotchas:

- `GetProcessMemoryInfo` is a synchronous current-process API call; use it only at explicit diagnostic milestones and not as a render-loop poll.
- `PROCESS_MEMORY_COUNTERS_EX::PrivateUsage` is the closest in-process equivalent to Windows Private Bytes.
- `PROCESS_MEMORY_COUNTERS_EX::WorkingSetSize` is the current working set.
- `PROCESS_MEMORY_COUNTERS_EX::PagefileUsage` is the commit-like page-file usage counter comparable to WMI `Win32_Process.PageFileUsage`.
- Initialize `PROCESS_MEMORY_COUNTERS_EX::cb` to `size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32` and pass the structure pointer cast to `*mut PROCESS_MEMORY_COUNTERS`.

Memory-counter source entrypoints:

- `windows-0.61.3/src/Windows/Win32/System/ProcessStatus/mod.rs`
- `windows-0.61.3/src/Windows/Win32/System/Threading/mod.rs`

Memory-counter commands and files consulted:

- `rg "pub unsafe fn GetProcessMemoryInfo|struct PROCESS_MEMORY_COUNTERS_EX|pub struct PROCESS_MEMORY_COUNTERS_EX" <cargo-registry> -n`
- `Get-Content windows-0.61.3/src/Windows/Win32/System/ProcessStatus/mod.rs`
- `cargo check --workspace --all-targets`
