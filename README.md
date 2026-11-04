# Veneer

Veneer is an experimental Windows 10/11 desktop appearance manager. Its design
uses documented Microsoft APIs only. Features that Windows does not expose
through a supported API will be identified as unavailable rather than
implemented with undocumented hooks or process injection.

> **Early development:** this repository currently contains only the initial
> application and Rust workspace scaffold. It does not yet change Windows
> settings or style third-party application windows.

## Project layout

- `apps/desktop` — Tauri and Svelte configuration UI.
- `crates/ipc` — versioned request/response types shared across process
  boundaries.
- `crates/theme-schema` — versioned JSON theme format: strict parsing and
  validation (pure, no I/O). Example: `themes/examples/midnight-work.json`.
- `crates/rule-engine` — deterministic, CSS-like cascade that resolves which
  style applies to a window at a given time (pure).
- `crates/windows-platform` — Windows build detection and DWM window-frame
  styling. Planning (`plan`) is portable; applying (`dwm`) is Windows-only and
  records previous values so changes can be restored.

## Development

Prerequisites: Rust 1.88+, Node.js 22+, npm, and the Windows build tools and
WebView2 runtime required by Tauri.

```powershell
npm --prefix apps/desktop install
npm --prefix apps/desktop run tauri dev
```

In another terminal:

```powershell
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Current limitations

- The theme, rule and DWM crates are not wired into the app yet, so no
  system settings are changed end to end.
- DWM color attributes cannot be read back; restore uses the system default.
- Applying a style twice to the same window would snapshot the already-styled
  state; the planned state journal must keep the first, original snapshot.
- Global display filters and animated desktop wallpapers require a separate
  feasibility decision; the documented wallpaper API supports static images
  and slideshows, not an arbitrary animated desktop renderer.
- DWM attributes are per-window. Support and appearance vary by Windows build;
  no support for styling arbitrary third-party windows is assumed.
- Performance, crash recovery, rollback, and multi-monitor behavior have not
  yet been implemented or measured.
