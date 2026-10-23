# Button Designer

A native, cross-platform button designer built in Rust with `eframe` / `egui`.
It keeps the direct canvas workflow of the referenced web editor while using a
quieter desktop layout and a print-oriented export path.

## Features

- Round button sizes from 25 mm to 75 mm
- Drag-to-position text, shape, and image layers
- Shift-click or drag a marquee to select several objects; move, scale, rotate,
  duplicate, align, distribute, and group compositions together
- Numeric position and size in millimeters, proportional sizing, centering, and flips
- Snap to artwork, trim, safe-area, and object edges and centers with temporary guides
- Align layers to the artwork page or relative to another layer
- Proportional resizing from selection corner handles
- Editable text, color, opacity, size, and visibility
- Named and locked layers, plus an internal clipboard that carries embedded artwork
- Text alignment and spacing, shape/text outlines, rounded rectangles, and image crop controls
- Persistent, searchable Font Browser for installed faces and styles on each text layer
- Star, heart, circle, diamond, and bolt shapes
- Solid, sunburst, confetti, grid, and cover-cropped photo backgrounds
- Layer ordering, duplication, deletion, and visibility controls
- Bounded undo and redo history
- Trim-accurate preview plus a single labeled finished-edge guide on the canvas
- Trim, bleed, and safe-area guides; export review with resolution checks and an A4 placement preview
- PNG, JPEG, and WebP image import
- SVG image import (flattened with an explicit notice) and SVG export with fidelity warnings
- Self-contained, human-readable `.buttonstudio` project save/load
- Embedded project artwork previews for macOS Finder and Quick Look
- Transparent PNG export sized at 300 DPI
- CSV series printing with personalized text and QR variables
- Series entry ranges, copies, personalized preview, and explicit invalid-row reporting
- Built-in layouts and reusable custom templates, palettes, and object styles
- MCP server for programmatic design editing, export, and visual snapshots

## MCP server

The main `button-studio` executable exposes the designer over the Model Context
Protocol using stdio. A normal launch starts the GUI and MCP together; MCP edits
are synchronized to the visible canvas, and GUI edits are available to MCP
tools. Run MCP without a window with:

```sh
cargo run -- --headless
```

The included [`.vscode/mcp.json`](.vscode/mcp.json) registers this headless mode
as `button-studio` in VS Code. Start or debug it from VS Code's MCP server
controls. A headless session starts with a blank 59 mm button. The server
provides tools to:

- create, open, inspect, and save designs
- add text, shape, QR code, and embedded image objects
- use an imported image as a cover-cropped background
- update object position, rotation, color, opacity, visibility, content, and size
- duplicate, delete, and reorder objects
- change button and background settings
- export PNG, printable PDF sheets, and `.buttonstudio` projects
- return an inline PNG snapshot for visual inspection

Paths may be absolute or relative to the MCP server process. Position values
are normalized relative to the artwork radius: `[0, 0]` is centered and each
axis ranges from `-1` to `1`. Colors use `#RRGGBB` or `#RRGGBBAA`.

## Series printing

Open the **Series** tool and import a comma-, semicolon-, or tab-separated file whose first row contains unique column names:

```csv
name;team;url
Ada;Research;https://example.com/42
Linus;Kernel;https://example.com/7
```

Each column becomes a variable such as `{{name}}` or `{{url}}`. Select a text or QR layer and click a variable to append it to that layer, or type placeholders directly into its content. With series data loaded, **Export PDF** creates one button per CSV entry, keeps the standard A4 placement, and adds pages as needed. Remove the series data to return PDF export to its normal fill-the-page behavior.

## Run

Install the current stable Rust toolchain, then run:

```sh
cargo run --release
```

The same source builds on macOS, Windows, and Linux. The Linux build requires a
working desktop environment with WGPU support and an XDG desktop portal for
native file dialogs.

## Verify

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
```

CI runs formatting, Clippy, and the test suite on pushes to `main` and pull requests.

## Releases

Prepare a stable `MAJOR.MINOR.PATCH` release from a clean working tree:

```sh
scripts/release.py 0.1.1
git push origin main --follow-tags
```

The release script updates the crate and lockfile versions, commits the change,
and creates the matching `vMAJOR.MINOR.PATCH` tag. Pushing that commit and tag
builds macOS, Windows, and Linux release archives. On GitHub, the workflow
uploads all three archives to a GitHub Release whose title matches the tag and
generates release notes. On Gitea, it publishes them to the matching Gitea
release. The macOS app archive uses the ad-hoc signing configured by the app
bundle script. Gitea Actions must be enabled, with runners registered for the
`ubuntu-latest`, `windows-latest`, and `macos-latest` labels.

## Shortcuts

| Action | Shortcut |
| --- | --- |
| Undo | `Cmd/Ctrl+Z` |
| Redo | `Cmd+Shift+Z` or `Ctrl+Y` |
| Save project | `Cmd/Ctrl+S` |
| Open project | `Cmd/Ctrl+O` |
| Export PNG | `Cmd/Ctrl+E` |
| Duplicate layer | `Cmd/Ctrl+D` |
| Copy / paste objects | `Cmd/Ctrl+C` / `Cmd/Ctrl+V` |
| Paste style | `Cmd/Ctrl+Shift+V` |
| Group / ungroup | `Cmd/Ctrl+G` / `Cmd/Ctrl+Shift+G` |
| Nudge / larger nudge | Arrow keys / `Shift` + arrow keys |
| Text / shapes / import image | `T` / `S` / `I` |
| Bring to front / send to back | `Cmd/Ctrl+PageUp` / `Cmd/Ctrl+PageDown` |
| Delete layer | `Delete` or `Backspace` |

Object shortcuts apply when a text field does not have keyboard focus. The object
clipboard works inside Button Designer; text fields retain their normal system
clipboard behavior.

Open **Library** for built-in layouts and reusable templates, palettes, and styles.
Open **Export & review** to choose PNG, SVG, an A4 PDF sheet, or a personalized
series PDF. The review shows physical dimensions, print warnings, and sheet
placement. For series, choose the entry range and copies, inspect personalized
previews, and fix invalid entries or explicitly skip them with a row-number report.

## Files

Project files store design data as versioned JSON. Imported PNG, JPEG, and WebP
artwork is embedded in an asset table as base64 while preserving its original
encoded format. Image layers and photo backgrounds reference assets by the
SHA-256 digest of their bytes, so identical artwork imported repeatedly or used
in both roles is stored only once. Saving removes assets that are no longer used.
Base64 keeps projects easy to inspect but increases the encoded image payload by
roughly one third compared with the original binary data.

Each newly saved project also contains a derived 512 x 512 PNG under its
top-level `preview` key. The preview is not editable design state and is ignored
when loading, so projects remain backward compatible and other platforms can
continue treating them as JSON. Older projects gain a preview the next time they
are saved.

Version 7 and older path-based projects remain supported. Button Designer reads
each linked image when opening a legacy project, migrates it to the embedded
asset table in memory, and writes the self-contained format on the next save.
Missing legacy images are reported with their path. New-format projects never
need the original image files for previewing, reopening, or export. Selected
system fonts remain linked to their original paths; a missing font is reported
instead of silently changing the design.

PNG exports include the circular artwork and wrap/bleed at 300 DPI, with transparency
outside the artwork circle. A 59 mm button has a 69.8 mm artwork diameter including
5.4 mm of wrap/bleed per side and exports at 824 × 824 pixels. The product preview
shows the finished 59 mm face. PDF sheets place the full artwork at its physical
size; print at 100% rather than scaling to fit the page.

SVG import renders artwork to a self-contained PNG at 2048 pixels on the longest
side. SVG export keeps supported content as vectors; designs using features that
cannot be represented faithfully use an embedded image with an explicit notice.
Keep the `.buttonstudio` project for fully editable design state.

Archivo Black is bundled under the SIL Open Font License; see
[`assets/fonts/OFL.txt`](assets/fonts/OFL.txt).

## macOS app bundle

Finder thumbnails and Space-bar Quick Look require the app's thumbnail and
preview extensions in addition to the embedded PNG. Build the app bundle with:

```sh
scripts/build-macos-app.sh
```

The result is `target/macos/Button Designer.app`. The script uses ad-hoc
signing by default for build verification. For an installable or distributable
build, pass one signing identity for the host and both extensions:

```sh
CODE_SIGN_IDENTITY="Developer ID Application: Example (TEAMID)" \
	scripts/build-macos-app.sh
```

Install the normally signed app in `/Applications` and open it once so Launch
Services registers the `.buttonstudio` document type and its Quick Look
providers.
