# Composer Clipboard Cutover

## The Retained Clipboard Was Not Draft-Independent

Checkpoint 1 originally classified `shell/composer_clipboard.rs` as a target-compatible helper to preserve when independent of removed storage. The final zero-match audit showed that it still imported the archived app-local `composer_draft.rs`, stored the removed `PendingNewThread` identity, and exposed old draft image payload types. Its test also loaded the archived draft source directly by path.

Keeping the file would therefore preserve an obsolete pre-thread draft state and an implicit adapter to the removed app-local draft model. Surgically inventing a string-based replacement draft or asset identity during the removal checkpoint would move the same problem behind new names.

The correction was to archive the clipboard source and its complete test byte-for-byte. Clipboard behavior returns only through the target durable-draft and content-addressed-asset boundaries in their owning implementation checkpoints.
