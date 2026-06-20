# Menu smoke report 2026-06-18

## Scope

- Windows host: executed `scripts/windows_menu_smoke.ps1` against
  `target/debug/j3text.exe` with temporary `APPDATA` and temporary document files.
- Linux host: not executed in this Windows environment. WSL, bash, Docker, and
  Podman were unavailable. `cargo check --target x86_64-unknown-linux-gnu`
  reached GTK sys crates but failed because `pkg-config` cross sysroot is not
  configured for GTK4.
- Existing Linux behavioral baseline remains `docs/linux-menu-parity-report.md`.

## Windows result

The Windows smoke opened the real application window, used the real Win32 file
dialogs for Open and Save As, used the app encoding dialogs for Save/Reopen/
Change Encoding, exercised menu command dispatch for all command groups, and
used temporary app data plus executable-adjacent settings backup/restore so
user settings were not touched.

Physical foreground `SendKeys` and native menu mouse selection were unreliable in
this automation session, so the final smoke drives menu items through the same
`WM_COMMAND` path used by native menu selection. File dialogs and app-owned modal
dialogs are completed through their Win32 child controls.

| Menu | Feature | Windows action | Linux existing action | Problem | Cause | Change | Recheck |
| -- | -- | -- | -- | -- | -- | -- | -- |
| File | Save | Startup file loaded, Save command persisted content | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |
| File | Open | Native Open dialog accepted a path and loaded text | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |
| File | Save As | Native Save As dialog accepted a target, encoding OK saved text | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |
| File | Close dirty | Unsaved prompt Cancel kept the tab, No discarded it | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |
| File | Recent | Recent first item executed after save/open history update | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |
| File | Exit | File Exit closed the process | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |
| Edit | Undo/Redo/Cut/Copy/Paste/Select All | All edit commands executed and text state changed as expected | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |
| Find | Find/Replace/Next/Prev/All/One/All/Close | Find and replace controls executed command path and replacement result was visible | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |
| View | Commands/Line Numbers/Marks/Word Wrap | Toggle commands executed twice to restore state | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |
| View | Theme | System, Light, Classic Dark, Sepia Teal, Graphite, Forest, Steel Blue executed | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |
| Tabs | New/Move/Close Others/Close All | Clean tab commands executed and returned to a valid tab set | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |
| Tabs | Open in New Window | File-backed tab launched a second process and source tab command completed | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |
| Text | Reopen/Change Encoding | Reopen accepted current encoding; Change Encoding to UTF-8 BOM saved BOM bytes | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |
| Text | Line Ends | CRLF, LF, CR, and CRLF restore executed | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |
| Settings | Font | Native font dialog opened and cancelled | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |
| Settings | Tab Size | 2/4/8 tab sizes executed/restored | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |
| Settings | Shortcuts | Set dialog opened/cancelled for all 18 shortcut commands; Off and Default executed for all | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |
| Help | About | About dialog opened and closed | Covered by Linux smoke | No app issue | n/a | Added Windows smoke coverage | PASS |

## Commands run

```text
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo check --target x86_64-unknown-linux-gnu
powershell.exe -NoProfile -STA -ExecutionPolicy Bypass -File scripts\windows_menu_smoke.ps1
git diff --check
```

## Notes

- No runtime menu behavior bug was found in this pass.
- Source changes outside the smoke script were Clippy warning cleanups observed
  during verification.
- Added tool/script: `scripts/windows_menu_smoke.ps1`, used to run repeatable
  Windows menu smoke coverage. No external tool was installed.
- Linux revalidation still requires a Linux desktop/GTK4 environment with
  `xdotool`, or a configured GTK4 cross sysroot for compile-only checks.
