# ADR 0002: Pure core, thin platform and UI layers

Status: accepted

## Context
The Tauri crate cannot be built in every environment, and Win32 calls are hard
to test. Logic buried in either would be untested.

## Decision
Decision logic is pure and platform-independent: `theme-schema` (parsing),
`rule-engine` (cascade), `windows-platform::plan` (what to write) and
`core::Controller` (orchestration, generic over a `FrameBackend`). Win32 calls
live behind `FrameBackend`/`Autostart`; Tauri only forwards commands.

## Consequences
- Orchestration is tested with a fake backend; real-window tests cover DWM.
- A failing module cannot corrupt the others: each is a separate crate.
- Slightly more indirection than a single crate.
