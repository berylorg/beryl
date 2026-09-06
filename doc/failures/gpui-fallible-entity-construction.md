# Inferring Required Recovery From A Fallible Constructor

## Scope And Invalidated Approach

Phase 285 was declared blocked because the range-input constructor returns `Result` while GPUI
entity builders require a value. That API mismatch alone does not establish a supported runtime
failure or justify adding a dependency API. The initial assessment failed to classify the errors
before requiring recovery machinery.

## Evidence

- `../zed-fork/crates/gpui/src/gpui.rs`, `AppContext::{new, insert_entity}`, accepts only
  `FnOnce(&mut Context<T>) -> T`. The implementations in `crates/gpui/src/app.rs` reserve an entity,
  build it through that context, and insert the returned value; there is no fallible-builder API.
- `../gpui-text-input/src/range_widget.rs`, `RangeTextInput::{new, new_internal}`, requires
  `Context<RangeTextInput>` and returns `Result`. Private construction checks dependency-owned
  budgets and checked capacity arithmetic, builds geometry and clipboard owners, and admits the
  initial interaction target. Those checks primarily reject inconsistent configuration or local
  budget allocations; they are not live shared-resource or OS-memory-pressure admission.
- `crates/beryl-app/src/main_window/conversation_composer_owner/construction.rs` uses `expect`
  inside the range-input entity builders. `conversation_composer_mount.rs` similarly expects
  successful composer construction inside its entity builder.

- Ordinary initial realization requests bounded pages without reading storage. Empty and nonempty
  drafts exercise different initial local budget paths. The clipboard instance counter can also
  overflow after its `u64` identity space is exhausted; this is not an ordinary operating failure.
- Constructor allocations are infallible Rust allocations. The returned `Result` does not provide
  allocator out-of-memory recovery.
- Beryl's configuration constructor does not prove every dependency condition. For example, it
  accepts positive page-byte limits below the dependency's minimum of four. No completed shell
  configurator exists yet, so supported configuration coverage remains implementation work.

## Correction And Remaining Evidence

On 2026-09-05, the Operator questioned the need for fallible handling. Focused source review found
no demonstrated supported runtime failure requiring a new GPUI API, and the blocker was withdrawn
before source edits. Verify Beryl's supported configuration and empty/nonempty realization paths;
constructor assertions may enforce internal invariants once that evidence supports them. Do not
duplicate all dependency validation merely to remove a `Result` or prove hypothetical failures
recoverable.

Expected host, seed, storage, and native-window failures still require their ordinary explicit
outcomes and exact custody. If concrete supported inputs can trigger widget construction failure,
address that specific boundary. The existing canonical range-input readiness check remains useful
for first publication. Phase 285 subsequently verified empty and populated selected editors,
minimum supported page budgets, exact preparation rejection, and existing activation/recovery
paths while retaining constructor assertions. No fallible GPUI API was required.
