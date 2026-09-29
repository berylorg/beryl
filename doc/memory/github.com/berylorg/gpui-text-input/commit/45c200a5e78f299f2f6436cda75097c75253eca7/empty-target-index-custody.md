# Reason For Investigation

An untouched empty acquired composer reaches ready shutdown flush but never widget quiescence.
Determine whether the retained index intent comes from fixture scheduling or widget ownership.

# Outcome

The widget and exact geometry owner disagree about an ongoing index job. The stranded Index state
is observed; the originating immediate-target transition below is inferred from inspected source
and was not directly traced. `prepare_local_target_replacement` accepts an active index and passes
`preview_target_replacement_release`, which retires only target jobs. An empty origin takes the
end-of-source shortcut in `prepare_target_replacement_from_checkpoint_inner`, yielding Complete.
`commit_prepared_transition` preserves the active index when no inputs change. The widget's
`commit_widget_transition_state` nevertheless clears `active_geometry` on TargetComplete and
retains `pending_index_intent` because no completed index exists. `prepare_start_index` then
returns Busy. `service_pending_index_intent` retains the intent; prepaint discards its error.

The acquired fixture reproduced this after removing its text insertion. Extra input notifications
did not help. Temporary probes captured Geometry(Busy), then confirmed the underlying retained job
was Index (binding 1, revision 0, presentation 1, epoch 2, job 12). All probes were removed.
An explicit empty replacement changes mutation/binding state; its success does not qualify an
untouched empty editor. The restored fixture uses saved nonempty text.

This establishes a widget ownership defect, not growing memory measurements or whole-shell recovery
completion. A correction must keep both job owners coherent and settle the original index custody;
ignoring pending intent, fabricating an edit, or clearing it without settlement is insufficient.

# Sources

- Repository: https://github.com/berylorg/gpui-text-input.git, requested and resolved commit
  `45c200a5e78f299f2f6436cda75097c75253eca7`, inspected 2026-09-29 on Windows.
- Beryl root Cargo.toml/Cargo.lock pin that revision. `cargo +stable tree --locked -p beryl-app
  --depth 1 --prefix none` confirms verification uses the matching sibling checkout through
  `.cargo/local.toml`; no dependency or manifest changes were made.
- `src/range_geometry/exact/transition.rs`: local target preparation, EOF shortcut, target-only
  release preview, transition commit and index admission.
- `src/range_widget/transition.rs`: target selection and widget transition commit;
  `geometry.rs`: pending index service; `render.rs`: prepaint error handling;
  `range_widget.rs`: exact quiescence predicate; `ime.rs`: empty replacement mutation path.
- Beryl `crates/beryl-app/tests/initial_composer/resident_retirement.rs` and
  `restored_native.rs::prepared_fixture_with_history`.
- Reproduction: `cargo +stable nextest run --locked -p beryl-app --features test-faults
  --test initial_composer -E 'test(resident_retirement)' --test-threads 1 --no-fail-fast` after
  temporarily removing the helper's `replace_text_in_range` call. Run
  `b54350be-af18-4ef2-b789-1ebc09bb62e3`: acquired fails, saved-text restored passes.
- Repeated notifications: `e1b55038-1994-4757-8ffc-9e1fdc253716`, same failure. Empty replacement
  control: `71fb9d13-2bda-4efd-8381-37158a47996f`, both pass.
- Temporary panic probes: `77dd7651-e162-4ac6-a94b-e037f1968c3f` captured Geometry(Busy);
  `20288098-d350-4d2f-8dcb-e968d87b9636` captured the retained Index job.
