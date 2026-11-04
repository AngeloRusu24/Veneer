# ADR 0004: Autostart via the HKCU Run key, tray-resident

Status: accepted

## Decision
Autostart writes a per-user value in
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (documented, no admin, shown
in Windows' Startup apps). The app starts with `--minimized`, lives in the
notification-area tray, and closing the window only hides it; "Esci" quits and
reverts every applied style.

## Alternatives rejected
Scheduled tasks and services need elevation and are more invasive.
