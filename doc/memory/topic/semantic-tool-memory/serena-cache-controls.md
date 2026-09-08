# Reason For Investigation

Determine whether Serena and its Rust language server expose cache eviction controls that bound
long-lived memory while retaining navigation across Beryl and its local dependency workspaces.

# Outcome

Serena 1.7.0 keeps two whole dictionaries of per-file, content-hash-keyed symbol results: raw LSP
results and processed document symbols. Both are loaded from and saved to complete pickle files.
The inspected implementation has version invalidation but no configurable capacity, TTL, or
eviction loop. Serialized size does not measure heap cost; heap profiling is needed to attribute
Python process memory to these dictionaries.

rust-analyzer's `lru.capacity` bounds retained syntax trees, not all semantic queries, source
text, macro processes, allocator residency, or total process memory. Disabling cache priming and
reducing worker count limit eager work and concurrency; neither is a hard memory limit. Do not
invent per-query LRU names or trim required dependency workspaces as a substitute for measurement.

`cargo.extraEnv` supports `CARGO_BUILD_JOBS=1` and `CARGO_PROFILE_DEV_DEBUG=0` for Cargo work started
by the analyzer, including independently loaded sibling workspaces. These reduce compilation
pressure, not Serena dictionary retention. Beryl's root build settings remain separately owned
by its [technology decisions](../../../design.md#implementation-technology).

Serena's `restart_language_server` recreates the server from the active cached `ProjectConfig`.
It does not reread `project.yml`; changed initialization options require a full Serena service
relaunch. Live adoption and memory effects must be measured after that relaunch.

# Sources

- Installed Serena distribution, version 1.7.0, inspected 2026-09-09. Relevant files:
  `solidlsp/ls.py`, `SolidLanguageServer` initialization at lines 550–563 and cache persistence at
  2970–3098; `solidlsp/util/cache.py`, `load_cache` and `save_cache` at lines 9 and 21.
  `solidlsp/ls.py` SHA-256:
  `890c5a689b95ad7ece7c21226040013325e2df877a7ca730e4376e0dd77eba31`.
- Installed Serena lifecycle source: `serena/tools/symbol_tools.py`,
  `RestartLanguageServerTool.apply` at lines 28–33; `serena/agent.py`,
  `reset_language_server_manager` at 1434–1438; `serena/project.py`,
  `create_language_server_manager` at 491–539. The latter's SHA-256:
  `19b779af83fdc3f9c6480fded0decf9c326c4e6ab7419feeecb5205a1999b64`.
- Serena maintainers, [project configuration](https://oraios.github.io/serena/02-usage/050_configuration.html),
  accessed 2026-09-09; project-specific language-server initialization scope.
- rust-analyzer project, [configuration reference](https://rust-analyzer.github.io/book/configuration.html),
  accessed 2026-09-09; `lru.capacity`, `cachePriming`, `numThreads`, and `cargo.extraEnv` semantics.
- Rust project, [Cargo configuration](https://doc.rust-lang.org/cargo/reference/config.html#buildjobs)
  and [profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#debug), accessed 2026-09-09;
  supported compilation-job and debug-information settings.

# Refresh Triggers

Recheck cache ownership and reload behavior when Serena changes version or the source hashes
change. Recheck LRU semantics on a rust-analyzer upgrade. Attribute memory with a heap profile
before claiming dictionary eviction would recover a particular amount of RAM.
