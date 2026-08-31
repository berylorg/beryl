---
name: multi-agent-vcs
description: Safely use Git when multiple agents concurrently edit disjoint paths in one working tree. Use whenever agents share a Git checkout.
---

# Protocol

Treat every unassigned path and pre-existing change as another agent's work. Edit only explicitly owned, disjoint paths. Stop if ownership overlaps or is unclear.

Scope inspection to owned paths:

```sh
git status --short -- <owned-paths>
git diff -- <owned-paths>
git diff --cached -- <owned-paths>
```

Do not investigate unrelated changes.

Commit all and only the intended paths:

```sh
# Required only for untracked files and rename destinations
git add -- <exact-new-paths>

git commit --only -m "<message>" -- <exact-commit-paths>
```

Include both source and destination paths for renames. Reinspect scoped diffs immediately before committing. `--only` commits named working-tree paths and excludes unrelated staged changes.

Never use bare `git commit`, `git commit -a`, `git commit --amend`, `git add .`, or `git add -A`. While agents share the checkout, do not run repository-wide `reset`, `restore`, `checkout`, `switch`, `stash`, `clean`, `merge`, `rebase`, `cherry-pick`, `revert`, or `pull`.

On an index-lock or moved-ref failure, never remove locks or repair state. Reinspect owned paths and retry; stop if the repository is mid-operation or ownership is uncertain.
