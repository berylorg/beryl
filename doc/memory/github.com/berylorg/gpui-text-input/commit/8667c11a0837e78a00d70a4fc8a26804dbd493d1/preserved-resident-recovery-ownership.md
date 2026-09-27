# Reason For Investigation

Interrupted Exit recovery preserves the native window and resident editor while replacing the
home generation. After Beryl detaches its service, publication adapters and callbacks, can the
pinned range widget still retain old home capabilities through presentation or deferred work?

# Outcome

The inspected widget ownership contains bounded presentation data, protocol identities, local
coordination and GUI references. No retained storage handle, app service callback or execution
capability was found in that ownership. This supports preserving the widget during retirement;
it does not prove fresh binding validity, completed host cleanup or whole-home retirement.

- `RangeTextInputConfig` contains bindings, limits, presentation configuration and a settlement
  coordinator. The coordinator's shared mutex contains capacity, operation counters and typed
  mutation/history slots. It contains no callback or service. The old binding and operation keys
  remain historical identities; their presence cannot authorize replacement-generation dispatch.
- `RangeResidency`, `ObjectResidency` and `CoherentRangeSurface` retain bounded page data, request
  keys, geometry, selection and scroll facts. Text pages own strings and atom facts. Object pages
  own inline-object facts whose presentation is text, metrics, colors and semantic values, not an
  app-provided renderer or callback. Shared object display strings and geometry arrays own data.
- `ExactGeometryOwner` retains layout/style inputs and bounded geometry state. Its layout binding
  contains numeric identities, positions, metrics and limits. No storage reader or worker task is
  installed into the owner. Edit and clipboard coordinators retain protocol state; quiescence
  additionally excludes active semantic operations and dispatched mutation/clipboard work.
- Scrollbar interaction closures are constructed inside the widget. They capture local
  `Rc<Cell<...>>` scroll state and a weak widget entity. The visibility callback only notifies the
  widget; scroll application refuses a disabled widget. Focus-out subscription code captures no
  app service and requests a local target transition before emitting a widget event.
- Realization scheduling either sets a flag and notifies or defers a closure capturing the frame
  generation. The deferred closure checks mounted state, the pending flag and exact frame
  generation before servicing geometry. It does not capture a storage adapter. The app's own
  deferred pump similarly re-enters the resident; `RecoveryFenced` fails `can_pump` and `begin_flight`.

`is_quiescent` is a live observation, not a permanent freeze. It excludes pending geometry,
response custody, layout/presentation/rebind intents, realization continuation, page requests,
restoration, surface candidates, semantic work, queued requests and attached object surfaces.
It does not require resident presentation, historical keys, the settlement coordinator or adopted
prepublication custody to be absent. Disabling editing does not promise that later focus/layout
activity cannot queue local presentation work. Beryl's retirement acceptance and status query
recheck quiescence; callers cannot promote a previous observation into enduring recovery authority.
This investigation does not establish recovery progress after every possible resize/focus event.

Adopted prepublication custody is the significant retained ownership exception to an empty-work
reading of quiescence. It contains page IDs and cleanup tokens plus a ledger. The ledger owns a
fixed boxed array of typed cleanup records behind `Arc<Mutex<_>>` and a weak `WindowTextSystem`.
Records contain only IDs, keys and cleanup-effect values. Releasing or dropping adopted custody
marks tokens ready; it does not call storage. `service` returns typed effects and `acknowledge`
updates records. Therefore retaining this ledger does not retain a failed-generation storage
handle, but neither dropping it nor observing widget quiescence proves that a host consumed its
cleanup effects. Host/source ownership must be checked separately.

In Beryl, the resident guards exclude active flights, pending dispatch/activation/clipboard/marker
work and attached image surfaces. The mount additionally rejects retained native-lineage
environment, session, candidate, effect, cleanup-ledger, source and host-result ownership, and
requires tracked workers to drain. Detached bundle retirement requires exclusive service ownership.
These app checks remain necessary: widget-only evidence cannot replace them. Resident observation
and event subscriptions capture the resident through GPUI; they do not separately capture the old
service. The service and clipboard callback fields are explicitly detached before retirement is
accepted. Fresh storage validation and widget rebinding were not implemented or qualified here.

# Sources

Inspected on 2026-09-28. Canonical repository:
[berylorg/gpui-text-input](https://github.com/berylorg/gpui-text-input), exact commit
`8667c11a0837e78a00d70a4fc8a26804dbd493d1`, as pinned by Beryl `Cargo.toml` and `Cargo.lock`.
The Windows local Cargo configuration patches to the sibling checkout at that same commit;
tracked source had no changes. Beryl's app enables the workspace dependency without additional
text-input features. No widget source was changed and no new runtime test was run for this audit.

Relevant widget files and symbols at that commit:

- `src/range_widget.rs`: `RangeTextInput`, `RangeScrollbar`, `new_internal`, `is_quiescent`,
  `is_semantically_quiescent`, `set_enabled`, adopted-custody installation/release.
- `src/range_widget/types.rs`: `RangeTextInputConfig`, `RangeSettlementCoordinator`,
  `RangeSettlementState`, `RangeSettlementSlot`.
- `src/residency.rs`, `src/object_residency.rs`, `src/range_widget/surface.rs`:
  page retention and coherent surface fields.
- `src/range_source/page.rs`, `src/range_source/object/page.rs`,
  `src/range_source/object/presentation.rs`: page and inline-object presentation ownership.
- `src/range_geometry/exact.rs`, `src/range_geometry/exact/types.rs`, `src/range_edit.rs`,
  `src/range_clipboard.rs`, `src/widget/theme.rs`: retained geometry, protocol and styling fields.
- `src/range_widget/realization.rs`, `src/range_widget/render.rs`,
  `src/range_widget/interaction.rs`: continuation capture and disabled interaction checks.
- `src/range_widget/prepublication/adopted_custody.rs`, `prepublication/cleanup.rs`,
  `prepublication/cleanup/record.rs`, `prepublication/session/custody.rs`,
  `prepublication/types.rs` under `src/range_widget`: ledger, token and environment ownership.

The layout-binding definition was checked in
[berylorg/zed-fork](https://github.com/berylorg/zed-fork), commit
`d2665f256d20c62ccf1a07c39f549ac05671e11e`,
`crates/gpui/src/text_system/streaming_layout/inputs.rs::StreamingLayoutBinding`.

Local use sites at Beryl commit `544cd89e`:
`crates/beryl-app/src/main_window/conversation_composer_owner.rs`,
`conversation_composer_owner/{construction,lifecycle,recovery}.rs`, and
`conversation_composer_mount/close/{recovery,recovery_resources}.rs` under the same `main_window`
directory. Controlling interpretation came from Beryl's shell-lifecycle interrupted-Exit ownership
and backend-runtime same-home recovery contracts; this note supplies source evidence only.

Resolution checks used `git rev-parse HEAD`, `git status --short`, `git remote get-url origin`,
manifest/lockfile inspection, targeted Serena symbol bodies and bounded `rg` ownership/callback
searches. Refresh this note when the pin, retained widget types, callback captures, quiescence
predicate or app recovery guards change.
