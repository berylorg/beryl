# Phase 9 Stale CAS Revival

## Invalidated approach

The first Phase 9 reverse-index implementation permanently assigned each CAS thread id to one
Syndic thread, but retained only that first-owner identity. A later valid-binding publication by
the same Syndic thread could therefore reuse a CAS thread already recorded as stale or abandoned.

## Why it failed

Stale binding history is non-authorizing provenance. The accepted execution contract prohibits a
stale, abandoned, unloaded, or uncommitted CAS thread from becoming a valid execution projection
again, including for its original Syndic owner. Cross-thread uniqueness alone does not prove that
the external thread remains eligible for execution.

## Required correction

The permanent CAS-thread reverse record must also carry one-way retirement provenance. Publishing
stale or abandoned provenance retires that CAS thread atomically; later valid or active authority
must reject every retired id. Reopen validation must prove the retirement revision names matching
stale history and that no later valid or active binding revives the id.

