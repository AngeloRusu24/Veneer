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
- `crates/windows-platform` — Windows capability detection and documented API
  adapters (implementation follows in later milestones).

## Development

Prerequisites: Rust 1.85+, Node.js 22+, npm, and the Windows build tools and
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

- No system settings are changed yet.
- Global display filters and animated desktop wallpapers require a separate
  feasibility decision; the documented wallpaper API supports static images
  and slideshows, not an arbitrary animated desktop renderer.
- DWM attributes are per-window. Support and appearance vary by Windows build;
  no support for styling arbitrary third-party windows is assumed.
- Performance, crash recovery, rollback, and multi-monitor behavior have not
  yet been implemented or measured.
