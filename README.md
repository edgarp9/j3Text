# j3Text

A focused Windows plain-text editor with tabs, encoding control, search and
replace, themes, and conservative file saving.

## Features

- Tab-based editing for multiple plain-text documents.
- Open, save, Save As, recent files, dirty-file prompts, and read-only handling.
- Encoding detection and manual encoding changes for UTF-8, UTF-8 BOM,
  UTF-16, Korean/Japanese/Chinese code pages, Windows code pages, and several
  legacy encodings.
- Line ending control for CRLF, LF, and CR.
- Find, replace, find-all results, and result navigation.
- Command palette, configurable keyboard shortcuts, line numbers, visible
  whitespace marks, word wrap, themes, font settings, and tab size settings.
- Conservative save behavior that writes through temporary files and checks for
  external file changes before replacing the target.

## Status

j3Text was created as an in-house tool with AI assistance. The project is still
early and the test coverage is not sufficient yet. Some automated and manual
regression checks exist, but the application should be treated as experimental
until more testing is added.

## Requirements

- Windows 10 version 1709 or later.
- Rust toolchain for building from source.

## Build

From the repository root:

```powershell
cd src
cargo build --release
```

The release executable is generated under:

```text
src/target/release/
```

## Test

```powershell
cd src
cargo test
```

Current tests do not cover every UI workflow, platform dialog, file-system edge
case, or encoding scenario. Please use caution with important files.

## License

This project is licensed under the GNU General Public License v3.0. See
[LICENSE](LICENSE) for details.

## Third-Party Notices

This project uses icons from
[Google Fonts Icons](https://fonts.google.com/icons), also known as Material
Symbols and Icons. Google makes these icons available under the
[Apache License Version 2.0](https://www.apache.org/licenses/LICENSE-2.0).

Thank you to Google and the Material Symbols team for making these icons
available.
