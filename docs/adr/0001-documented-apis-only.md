# ADR 0001: Only documented Windows APIs

Status: accepted

## Context
Deep desktop customization is usually done with code injection or undocumented
hooks. These break across Windows updates and can trip security software.

## Decision
Veneer uses only documented Win32/DWM/registry APIs. When a feature cannot be
done through a supported path it is reported as unavailable (or implemented as
a clearly warned best effort), never worked around with injection.

## Consequences
- Styling of third-party windows is not guaranteed; DWM attributes are per-window.
- Global filters and animated wallpapers are deferred pending a documented path.
- The app stays safe to run and easy to explain.
