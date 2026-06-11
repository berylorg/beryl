---
name: capture-repeatable-lessons
description: Use when a repeatable task failure, operator correction, live-test problem, or recurring implementation mistake has been discovered and fixed or needs to be preserved as reusable guidance. Routes lessons into project instructions, project-owned docs, project-scoped skills, global skills, or operator instructions; decides when to update an existing skill, create a new skill, or promote a project skill for wider reuse.
---

# Capture Repeatable Lessons

## Purpose

Preserve practical lessons from corrected failures without turning every one-off mistake into permanent process clutter.

Use this skill to decide what should be remembered, where it should live, and how to phrase it as positive reusable procedure.

## Decision Workflow

1. Confirm the lesson is repeatable.
   Preserve it only when the task or failure mode is likely to recur, the corrected approach is understood, and the guidance can improve future work. Do not preserve ordinary one-off debugging details, transient environment facts, secrets, or raw blame narratives.

2. Inspect current instructions.
   Read active user, project, and workspace instructions for existing conventions about plans, design docs, incident logs, exploration memory, skills, or other durable records. Follow those conventions. If no convention exists for a project-local artifact, ask before creating a new project documentation structure.

3. Prefer positive procedure.
   Write the durable lesson as "how to do it right" before writing "what went wrong." Failure history is useful as evidence, but future agents need a concrete workflow, checklist, invariant, or decision rule.

4. Update before creating.
   Search existing relevant skills and project guidance first. Update an existing artifact when the lesson fits its scope. Create a new skill only when no existing skill has a natural home for the procedure.

5. Keep scope narrow at first.
   New auto-created skills should be project-scoped by default. Early lessons often contain hidden project assumptions. Promote only after the lesson is clearly reusable outside the current project.

## Where To Put The Lesson

Use project-owned docs or instructions for project-specific contracts, architecture, implementation plans, local failure history, and facts that should not affect other projects.

Use a project-scoped skill for reusable procedure that is still tied to the current project, framework, UI toolkit, workflow, or codebase conventions.

Use a global skill when the procedure is broadly reusable across projects and does not depend on project-specific paths, names, architecture, or policies.

Use operator-level instructions for stable personal preferences that should affect all future work, especially trigger rules such as "consider preserving repeatable lessons after corrected failures."

Use environment-local files only for machine-specific paths, tool locations, private directories, or promotion targets. Do not put machine-local facts inside portable skills.

## Project-Scoped Skills

When local instructions define a project skill directory, use that directory.

When no project convention exists, propose `.codex/skills` as the default project-scoped location and ask before creating it.

For project-scoped skills:

- Name skills with lowercase letters, digits, and hyphens.
- Keep the skill focused on one repeatable task family.
- Put trigger conditions in the frontmatter `description`.
- Keep `SKILL.md` concise and imperative.
- Include only non-obvious practical procedure.
- Avoid project history unless a short example prevents misapplication.
- Do not duplicate authoritative project design or planning policy inside the skill.

## Promotion

Read optional operator config at `~/.codex/capture-repeatable-lessons.md` before promoting or creating non-project-scoped skills.

The config may define promotion targets such as an operator-global skill library or a git-backed shared skill worktree. If the config is missing or does not define a suitable target, ask before promoting.

When promoting a project-scoped skill:

- Remove project-specific paths, names, and assumptions.
- Replace project contracts with generic wording such as "follow the current project's planning convention."
- Split project-specific additions into a project wrapper skill when needed.
- Do not symlink, install, or copy promoted skills into active Codex discovery paths unless the operator explicitly asks.

## Creating Or Updating Skills

When creating or substantially updating a skill, use the available skill creation workflow if one is present.

Before editing:

- Identify concrete examples of future tasks that should trigger the skill.
- Decide whether the lesson belongs in `SKILL.md` or a bundled resource.
- Keep bundled resources only when they are directly useful.

After editing:

- Validate the skill if validation tooling is available.
- Forward-test with a subagent when the skill is complex enough that trigger wording or procedure quality is uncertain.
- Report what was preserved, where it was preserved, and why that scope was chosen.

## Do Not Preserve

Do not create or update durable learning artifacts when:

- The issue was a one-off typo, command failure, or transient state.
- The fix is not understood.
- The lesson would encode secrets or private local data.
- The lesson merely repeats existing instructions.
- The correct destination is an active implementation plan rather than reusable knowledge.
- The operator asked only for discussion and did not ask to modify files.
