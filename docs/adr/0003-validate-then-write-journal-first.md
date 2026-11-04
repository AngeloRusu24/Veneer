# ADR 0003: Validate everything before the first write; journal first

Status: accepted

## Context
A half-applied theme or a crash mid-change must never leave the system altered
with no way back.

## Decision
`Controller::apply_theme` parses, checks build and policy, and resolves rules
before touching anything. Re-applying restores first, so the stored original is
never an already-styled state. Global changes (future) go through
`state-journal`: write the original to disk (atomic save) before changing, and
recover on next start. DWM attributes need no journal because they disappear
with the window.

## Consequences
- Errors never leave partial state.
- DWM colors cannot be read back, so restore uses the system default value.
