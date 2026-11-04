# ADR 0005: Feasibility of global changes with documented APIs

Status: accepted (verified against Microsoft's public API surface; re-check per Windows release)

| Feature | Documented path | Decision |
|---|---|---|
| Static wallpaper, per monitor | `IDesktopWallpaper` (COM, Win32 Shell) | **Implement** (`windows-platform::wallpaper`) |
| Animated wallpaper (GIF/video/shader) | None. The API accepts images and slideshows only; hosting a window behind the icons relies on undocumented desktop internals | **Not offered** |
| Cursors | `SetSystemCursor` / `SystemParametersInfo(SPI_SETCURSORS)` are documented, but replacing a system cursor is process-session wide and `SPI_SETCURSORS` resets to the user's registry scheme | **Candidate**, needs its own journal entry and test |
| App light/dark mode | Values under `HKCU\...\Themes\Personalize` are widely used but not formally specified | **Candidate, best effort**, warn in UI that it may break |
| System accent color | No supported setter (`DwmGetColorizationColor` is read-only; the setting is owned by Personalization) | **Not offered** |
| Global color filters (hue, contrast, blue light) | Magnification API color effects apply to the magnified screen only; there is no supported full-screen compositor filter | **Not offered for now** |

## Consequences
- Every global change must go through `state-journal` (capture before change).
- Anything depending on unspecified registry values is labelled best effort.

Update (M8c): the wallpaper is a theme-level field (wallpaper.path), not a rule property: it is global, while rules depend on a window's context. Applying a theme replaces the previous one completely, wallpaper included.
