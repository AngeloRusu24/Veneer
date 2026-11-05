# ADR 0005: Feasibility of global changes with documented APIs

Status: accepted (verified against Microsoft's public API surface; re-check per Windows release)

| Feature | Documented path | Decision |
|---|---|---|
| Static wallpaper, per monitor | `IDesktopWallpaper` (COM, Win32 Shell) | **Implement** (`windows-platform::wallpaper`) |
| Animated wallpaper (GIF/video/shader) | None. The API accepts images and slideshows only; hosting a window behind the icons relies on undocumented desktop internals | **Not offered** |
| Cursors | `SetSystemCursor` / `SystemParametersInfo(SPI_SETCURSORS)` are documented, but replacing a system cursor is process-session wide and `SPI_SETCURSORS` resets to the user's registry scheme | **Implement** (`windows-platform::cursors`); restore reloads the configured scheme |
| App light/dark mode | Values under `HKCU\...\Themes\Personalize` are widely used but not formally specified; `WM_SETTINGCHANGE` is documented | **Implement, best effort** (`windows-platform::appearance`); warn that Windows may ignore or change this convention |
| System accent color | No supported setter (`DwmGetColorizationColor` is read-only; the setting is owned by Personalization) | **Not offered** |
| Global color filters (hue, contrast, blue light) | Magnification API color effects apply to the magnified screen only; there is no supported full-screen compositor filter | **Not offered for now** |

## Consequences
- Every global change must go through `state-journal` (capture before change).
- Anything depending on unspecified registry values is labelled best effort.

Update (M8c): the wallpaper is a theme-level field (`wallpaper.path`), not a rule property: it is global, while rules depend on a window's context. Applying a theme replaces the previous one completely, wallpaper included.

Update (M9): appearance (`appearance.apps` / `appearance.system`) and cursors
(`cursors`) are also theme-level global settings. They use independent journal
files so each module can recover without overwriting another module's entry.
Appearance is deliberately best-effort: the Personalize registry values are
used by Windows but are not a formal stable setter API. Cursor replacement
uses documented User32 APIs; restoring reloads the user's configured scheme.
